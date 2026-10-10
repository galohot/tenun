//! End-to-end checks of the engine on the sample data: three sources of three
//! kinds, joined by the query compiler. Expected totals were computed
//! independently from the source files (Python, openpyxl + csv + sqlite3).

use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tenun_core::Engine;
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::workspace::FieldRef;

fn sample_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tenun-it-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sample");
    for f in ["coffee.xlsx", "stores.csv", "customers.sqlite"] {
        std::fs::copy(src.join(f), dir.join(f)).unwrap();
    }
    dir
}

fn engine(tag: &str) -> (Engine, PathBuf) {
    let dir = sample_dir(tag);
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    e.replace(tenun::sample::workspace(), Some(dir.join("Coffee.tenun")));
    (e, dir)
}

fn q(v: serde_json::Value) -> SemanticQuery {
    serde_json::from_value(v).unwrap()
}

fn total_by(e: &Engine, table: &str, column: &str) -> Vec<(serde_json::Value, f64)> {
    let r = e
        .query(&q(json!({
            "measures": [{ "table": "s1/Orders", "column": "amount", "agg": "sum" }],
            "dimensions": [{ "table": table, "column": column }]
        })))
        .unwrap();
    r.rows
        .into_iter()
        .map(|row| (row[0].clone(), row[1].as_f64().unwrap()))
        .collect()
}

#[test]
fn each_source_has_a_load_time() {
    let (e, dir) = engine("loaded-at");
    let snap = e.snapshot();
    for id in ["s1", "s2", "s3"] {
        let t = snap.loaded_at.get(id).expect("load time");
        assert!(t.ends_with('Z') && t.len() == 20, "{t}");
    }
    // A source that cannot be read keeps its last good time and gains an error.
    std::fs::rename(dir.join("stores.csv"), dir.join("stores.moved")).unwrap();
    e.load("s2");
    let snap = e.snapshot();
    assert!(snap.errors.contains_key("s2"));
    assert!(snap.loaded_at.contains_key("s2"));
    // Removing a source drops its time.
    e.remove_source("s2");
    assert!(!e.snapshot().loaded_at.contains_key("s2"));
}

#[test]
fn loads_three_kinds_of_source() {
    let (e, _) = engine("load");
    let snap = e.snapshot();
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    let rows = |id: &str| snap.tables.iter().find(|t| t.id == id).and_then(|t| t.rows);
    assert_eq!(rows("s1/Orders"), Some(17491));
    assert_eq!(rows("s1/Products"), Some(10));
    assert_eq!(rows("s2/stores"), Some(9));
    assert_eq!(rows("s3/customers"), Some(400));
    let orders = snap.tables.iter().find(|t| t.id == "s1/Orders").unwrap();
    let date = orders.columns.iter().find(|c| c.name == "date").unwrap();
    assert_eq!(date.raw_type, "DATE");
}

#[test]
fn joins_excel_to_csv_and_sqlite() {
    let (e, _) = engine("join");
    let by_region = total_by(&e, "s2/stores", "region");
    let get = |rows: &[(serde_json::Value, f64)], k: serde_json::Value| {
        rows.iter().find(|r| r.0 == k).map(|r| r.1)
    };
    assert_eq!(get(&by_region, json!("Java")), Some(430813.0));
    assert_eq!(
        get(&by_region, json!("Bali & Nusa Tenggara")),
        Some(150526.0)
    );
    assert_eq!(get(&by_region, json!("Sumatra")), Some(64516.0));
    assert_eq!(get(&by_region, json!("Sulawesi")), Some(39980.0));

    // Orders without a loyalty member keep their amount under (blank).
    let by_segment = total_by(&e, "s3/customers", "segment");
    assert_eq!(get(&by_segment, json!("Gold")), Some(119954.0));
    assert_eq!(get(&by_segment, serde_json::Value::Null), Some(310438.0));
    let sum: f64 = by_segment.iter().map(|r| r.1).sum();
    assert_eq!(sum, 685835.0);
}

#[test]
fn filters_cross_sources() {
    let (e, _) = engine("filter");
    let r = e
        .query(&q(json!({
            "measures": [{ "table": "s1/Orders", "column": "amount", "agg": "sum" }],
            "dimensions": [],
            "filters": [
                { "table": "s2/stores", "column": "region", "op": "in", "values": ["Java"] },
                { "table": "s1/Orders", "column": "date", "grain": "year", "op": "in", "values": ["2026-01-01"] }
            ]
        })))
        .unwrap();
    assert_eq!(r.rows[0][0].as_f64(), Some(220696.0));
}

#[test]
fn suggests_the_sample_relationships() {
    let (e, dir) = engine("suggest");
    let mut ws = tenun::sample::workspace();
    ws.relationships.clear();
    e.replace(ws, Some(dir.join("Coffee.tenun")));
    let s = e.suggest().unwrap();
    let has = |a: &str, ac: &str, b: &str, bc: &str| {
        s.iter().any(|x| {
            x.from
                == FieldRef {
                    table: a.into(),
                    column: ac.into(),
                }
                && x.to
                    == FieldRef {
                        table: b.into(),
                        column: bc.into(),
                    }
        })
    };
    assert!(has("s1/Orders", "store_id", "s2/stores", "id"), "{s:#?}");
    assert!(
        has("s1/Orders", "customer_id", "s3/customers", "id"),
        "{s:#?}"
    );
    assert!(
        has("s1/Orders", "product", "s1/Products", "product"),
        "{s:#?}"
    );
}

#[test]
fn relationship_direction_comes_from_the_data() {
    let (e, dir) = engine("direction");
    let mut ws = tenun::sample::workspace();
    ws.relationships.clear();
    e.replace(ws, Some(dir.join("Coffee.tenun")));
    // Drawn from the unique side to the many side: stored the other way round.
    e.add_relationship(
        FieldRef {
            table: "s2/stores".into(),
            column: "id".into(),
        },
        FieldRef {
            table: "s1/Orders".into(),
            column: "store_id".into(),
        },
    )
    .unwrap();
    let r = &e.snapshot().workspace.relationships[0];
    assert_eq!(r.from.table, "s1/Orders");
    assert_eq!(r.to.table, "s2/stores");
    // Neither side unique: refused.
    let err = e
        .add_relationship(
            FieldRef {
                table: "s1/Orders".into(),
                column: "product".into(),
            },
            FieldRef {
                table: "s2/stores".into(),
                column: "region".into(),
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("unique"));
}

#[test]
fn saved_file_has_relative_paths_and_no_secrets() {
    let (e, dir) = engine("save");
    let p = e.save(None).unwrap();
    let text = std::fs::read_to_string(p).unwrap();
    assert!(text.contains("\"path\": \"coffee.xlsx\""));
    assert!(!text.contains(dir.to_string_lossy().as_ref()));
    assert!(!text.to_lowercase().contains("password"));
}

#[test]
fn edited_csv_and_excel_show_up() {
    let (e, dir) = engine("refresh");
    // CSV is a view: the next query reads the new file.
    let csv = std::fs::read_to_string(dir.join("stores.csv")).unwrap();
    std::fs::write(dir.join("stores.csv"), csv.replace(",Java,", ",Jawa,")).unwrap();
    let by_region = total_by(&e, "s2/stores", "region");
    assert!(by_region.iter().any(|r| r.0 == json!("Jawa")));

    // Excel is copied into memory: a reload picks up the new workbook.
    let mut wb = rust_xlsxwriter::Workbook::new();
    let s = wb.add_worksheet().set_name("Orders").unwrap();
    for (c, h) in [
        "order_id",
        "date",
        "store_id",
        "customer_id",
        "product",
        "quantity",
        "amount",
    ]
    .iter()
    .enumerate()
    {
        s.write_string(0, c as u16, *h).unwrap();
    }
    s.write_number(1, 0, 1.0).unwrap();
    s.write_string(1, 1, "2026-10-01").unwrap();
    s.write_number(1, 2, 1.0).unwrap();
    s.write_number(1, 4, 0.0).unwrap();
    s.write_number(1, 6, 999.0).unwrap();
    wb.save(dir.join("coffee.xlsx")).unwrap();
    let tables = e.load("s1");
    assert_eq!(
        tables,
        vec!["s1/Orders".to_string()],
        "the Products sheet is gone"
    );
    let total = e
        .query(&q(
            json!({ "measures": [{ "table": "s1/Orders", "column": "amount", "agg": "sum" }] }),
        ))
        .unwrap();
    assert_eq!(total.rows[0][0].as_f64(), Some(999.0));
}

#[test]
fn watcher_reports_a_saved_file() {
    let dir = sample_dir("watch");
    let (tx, rx) = std::sync::mpsc::channel();
    let mut w = tenun_core::watch::FileWatcher::new(move |paths| {
        let _ = tx.send(paths);
    })
    .unwrap();
    let target = dir.join("stores.csv");
    w.set(std::slice::from_ref(&target), &[]);
    std::thread::sleep(Duration::from_millis(200));
    // Save the way spreadsheet apps do: write a temp file, rename it over.
    let tmp = dir.join(".~stores.csv");
    std::fs::write(&tmp, "id,name\n1,a\n").unwrap();
    std::fs::rename(&tmp, &target).unwrap();
    let start = Instant::now();
    let got = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("no change event within 3 s");
    assert!(got.contains(&tenun_core::watch::canonical(&target)));
    assert!(start.elapsed() < Duration::from_secs(3));
}
