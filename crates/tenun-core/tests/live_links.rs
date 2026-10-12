//! Links into a live database table (V4-D51). An attached DuckDB file stands in: Tenun reads it
//! where it is, as it reads PostgreSQL and MySQL, so it is a table with no copy in memory.

use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tenun_core::Engine;
use tenun_core::formula::{self, Body, Context, Kind};
use tenun_core::links::Health;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{Tile, run_tile};
use tenun_core::workspace::{FieldRef, Relationship, Source, SourceKind, Workspace};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-live-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A database with orders and stores. Store 1 is in the table twice, in two regions: nothing in
/// a database stops it. Orders: store 1 has 100 and 250, store 2 has 40 and 10, store 3 has 75.5.
fn database(d: &std::path::Path) -> PathBuf {
    let path = d.join("shop.duckdb");
    let db = duckdb::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE orders (id INTEGER, store_id INTEGER, amount DOUBLE);
         INSERT INTO orders VALUES (1,1,100),(2,1,250),(3,2,40),(4,3,75.5),(5,2,10);
         CREATE TABLE stores (id INTEGER, region VARCHAR);
         INSERT INTO stores VALUES (1,'West'),(1,'East'),(2,'East'),(3,'West');",
    )
    .unwrap();
    path
}

fn engine(tag: &str) -> Engine {
    let d = dir(tag);
    let db = database(&d);
    // The link is put in the file by hand: Tenun does not make a link whose both sides repeat.
    let ws = Workspace {
        name: "Live".into(),
        sources: vec![Source {
            id: "s1".into(),
            name: "shop.duckdb".into(),
            kind: SourceKind::Duckdb {
                path: db.to_string_lossy().into_owned(),
            },
        }],
        relationships: vec![Relationship {
            id: "r1".into(),
            from: FieldRef {
                table: "s1/orders".into(),
                column: "store_id".into(),
            },
            to: FieldRef {
                table: "s1/stores".into(),
                column: "id".into(),
            },
            name: None,
        }],
        ..Workspace::default()
    };
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.replace(ws, Some(d.join("live.tenun")));
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    e
}

fn by_region(e: &Engine) -> Vec<(String, f64)> {
    let t: Tile = serde_json::from_value(json!({
        "id": "t", "type": "bar", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
        "measures": [{ "table": "s1/orders", "column": "amount", "agg": "sum" }],
        "dims": [{ "table": "s1/stores", "column": "region" }],
    }))
    .unwrap();
    let r = run_tile(e, &t, &[]).unwrap();
    let mut rows: Vec<(String, f64)> = r
        .rows
        .iter()
        .map(|r| (r[0].as_str().unwrap().to_string(), r[1].as_f64().unwrap()))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

fn wait_for_checks(e: &Engine) {
    let start = Instant::now();
    while e.checks_pending() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "the check never ended"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_live_table_never_doubles_a_total_before_during_or_after_its_check() {
    let e = engine("totals");
    // The store is in the table twice. The orders of store 1 count once, with the row that sorts
    // first by the key: the order of the rows of a key is not defined, so the numbers of that
    // store can sit in either region, but the total of all regions is the total of the orders.
    let total = |rows: &[(String, f64)]| rows.iter().map(|r| r.1).sum::<f64>();
    let during = by_region(&e);
    assert_eq!(total(&during), 475.5, "{during:?}");
    wait_for_checks(&e);
    let after = by_region(&e);
    assert_eq!(total(&after), 475.5, "{after:?}");

    // The check read the database in the background and found the repeated key.
    let snap = e.snapshot();
    let h = &snap.health["r1"];
    assert_eq!(h.duplicates, 1);
    assert_eq!(h.sample, [json!("1")]);
    assert_eq!(h.kind, "one-many");
    let note = h.note.as_deref().unwrap();
    assert!(
        note.starts_with(
            "stores in shop.duckdb has 2 rows with id 1. Tenun used one row of each. Which one can change."
        ),
        "{note}"
    );
    assert!(
        note.contains("2 of 5 orders rows meet a repeated key."),
        "{note}"
    );
    assert!(
        note.ends_with("Fix the table to remove this note."),
        "{note}"
    );
}

#[test]
fn a_formula_joins_a_live_table_one_row_per_key_too() {
    let e = engine("formula");
    wait_for_checks(&e);
    let snap = e.snapshot();
    let rels = snap.workspace.relationships.clone();
    let catalog = e.catalog();
    // Even a context that knows nothing about the data (every link unknown) joins one row per key.
    let unknown = Health::new();
    let cx = Context::new(&catalog, &rels, &unknown);
    let compiled =
        formula::compile("[stores].[region]", Kind::Column, Some("orders"), &cx).unwrap();
    let Body::Column(c) = compiled.body else {
        panic!("a column");
    };
    assert_eq!(c.joins.len(), 1);
    assert!(
        c.joins[0].sql_ref.contains("DISTINCT ON"),
        "{}",
        c.joins[0].sql_ref
    );
    // The preview of the whole column has one value for each order, not one for each match.
    let r = e
        .check_formula(&formula::CheckRequest {
            formula: "COUNTROWS([orders])".into(),
            kind: Kind::Measure,
            table: None,
        })
        .unwrap();
    assert_eq!(r.preview.unwrap(), [Value::from(5.0)]);
}

#[test]
fn the_check_reads_only_the_first_rows_of_a_live_table_and_says_so() {
    let e = engine("bounded");
    wait_for_checks(&e);
    e.set_live_check_rows(3);
    assert!(
        e.recheck_live_links(),
        "the answer changes when fewer rows are read"
    );
    let snap = e.snapshot();
    let note = snap.health["r1"].note.clone().unwrap_or_default();
    assert!(
        note.contains("Tenun checked only the first 3 rows."),
        "{note}"
    );
    // Nothing changes the second time: the answer is the same.
    assert!(!e.recheck_live_links());
    // And a load does not wait for it: the load returns at once and the link is unknown until
    // the background check has written its answer.
    e.slow_link_checks(Duration::from_millis(600));
    let start = Instant::now();
    e.load("s1");
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "the load waited for the check"
    );
    assert!(!e.snapshot().health.contains_key("r1"));
    assert!(e.checks_pending());
    // While it is unknown a total is still right.
    let total: f64 = by_region(&e).iter().map(|r| r.1).sum();
    assert_eq!(total, 475.5);
    wait_for_checks(&e);
    assert!(e.snapshot().health.contains_key("r1"));
}

#[test]
fn a_listener_is_told_when_a_background_check_has_written_its_answer() {
    let e = engine("listener");
    wait_for_checks(&e);
    let told = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let t = told.clone();
    e.on_links_checked(move || {
        t.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    e.load("s1");
    wait_for_checks(&e);
    // The listener is called just after the counter drops.
    let start = Instant::now();
    while told.load(std::sync::atomic::Ordering::SeqCst) == 0
        && start.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(told.load(std::sync::atomic::Ordering::SeqCst), 1);
}

fn calc_column(name: &str, formula: &str) -> tenun_core::workspace::CalcColumn {
    tenun_core::workspace::CalcColumn {
        name: name.into(),
        formula: formula.into(),
        sql: false,
        description: None,
    }
}

#[test]
fn a_calculated_column_works_on_a_live_table_and_reads_one_row_per_key_from_one() {
    use tenun_core::workspace::TableSetup;
    let e = engine("calc");
    wait_for_checks(&e);
    let setup = TableSetup {
        calc: vec![
            calc_column("Double", "[amount] * 2"),
            // The stores table repeats store 1. The view keeps one row of it for each order.
            calc_column("Region", "[stores].[region]"),
        ],
        ..TableSetup::default()
    };
    e.set_table_setup("s1/orders", Some(setup)).unwrap();
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    let count = |col: Option<&str>, agg: &str| -> f64 {
        let t: Tile = serde_json::from_value(json!({
            "id": "t", "type": "kpi", "title": "", "x": 0, "y": 0, "w": 3, "h": 2,
            "measures": [{ "table": "s1/orders", "column": col, "agg": agg }],
            "dims": [],
        }))
        .unwrap();
        run_tile(&e, &t, &[]).unwrap().rows[0][0].as_f64().unwrap()
    };
    // Five orders stay five, and nothing is added twice.
    assert_eq!(count(None, "count"), 5.0);
    assert_eq!(count(Some("Double"), "sum"), 951.0);
    assert_eq!(count(Some("Region"), "count"), 5.0);
    // The database is read again: the views are still there and still right.
    e.load("s1");
    wait_for_checks(&e);
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    assert_eq!(count(Some("Double"), "sum"), 951.0);
    assert_eq!(count(None, "count"), 5.0);
    // The table is still live: the column list shows the calculated columns after its own.
    let t = e.catalog().table("s1/orders").unwrap().clone();
    assert!(!t.prep);
    assert_eq!(t.rows, Some(5));
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["id", "store_id", "amount", "Double", "Region"]);
}
