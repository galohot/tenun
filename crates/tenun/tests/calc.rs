//! Calculated columns (build 02): formulas that become real columns of a table's view.
//!
//! Three small CSV files stand for an order sheet, the stores and the regions. The numbers are
//! small enough to add up by hand.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::calc::CalcRefused;
use tenun_core::catalog::DType;
use tenun_core::engine::AddSource;
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::workspace::{CalcColumn, FieldRef, TableSetup};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-calc-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Orders: 100, 250, 40, 75.5, 10, and an order for store 9, which no store has.
fn write_files(d: &Path) {
    std::fs::write(
        d.join("orders.csv"),
        "id,date,store_id,amount,note\n1,2026-01-15,1,100,a\n2,2026-01-20,1,250,b\n\
         3,2026-02-03,2,40,c\n4,2026-03-31,3,75.5,d\n5,2026-03-05,2,10,e\n6,2026-03-06,9,5,f\n",
    )
    .unwrap();
    std::fs::write(
        d.join("stores.csv"),
        "id,name,region_id,rent\n1,Kopi A,10,500\n2,Kopi B,10,300\n3,Kopi C,20,200\n",
    )
    .unwrap();
    std::fs::write(d.join("regions.csv"), "id,name\n10,Java\n20,Bali\n").unwrap();
    std::fs::write(
        d.join("lines.csv"),
        "id,order_id,qty\n1,1,2\n2,1,3\n3,2,1\n",
    )
    .unwrap();
}

struct World {
    e: Engine,
    d: PathBuf,
    orders: String,
    stores: String,
    regions: String,
    lines: String,
}

fn field(t: &str, c: &str) -> FieldRef {
    FieldRef {
        table: t.to_string(),
        column: c.to_string(),
    }
}

fn world(tag: &str) -> World {
    let d = dir(tag);
    write_files(&d);
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    for f in ["orders.csv", "stores.csv", "regions.csv", "lines.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    let snap = e.snapshot();
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    let id = |name: &str| {
        snap.tables
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("no table {name}"))
            .id
            .clone()
    };
    let (orders, stores, regions, lines) = (id("orders"), id("stores"), id("regions"), id("lines"));
    e.add_relationship(field(&orders, "store_id"), field(&stores, "id"))
        .unwrap();
    e.add_relationship(field(&stores, "region_id"), field(&regions, "id"))
        .unwrap();
    e.add_relationship(field(&lines, "order_id"), field(&orders, "id"))
        .unwrap();
    World {
        e,
        d,
        orders,
        stores,
        regions,
        lines,
    }
}

/// What the column menu does: puts a name on a column and keeps the rest of the setup.
fn label(e: &Engine, table: &str, column: &str, label: &str) {
    let mut setup = e
        .snapshot()
        .workspace
        .tables
        .get(table)
        .cloned()
        .unwrap_or_default();
    setup.columns.entry(column.into()).or_default().label = Some(label.into());
    e.set_table_setup(table, Some(setup)).unwrap();
}

fn calc(name: &str, formula: &str) -> CalcColumn {
    CalcColumn {
        name: name.into(),
        formula: formula.into(),
        sql: false,
        description: None,
    }
}

fn sql(name: &str, formula: &str) -> CalcColumn {
    CalcColumn {
        sql: true,
        ..calc(name, formula)
    }
}

/// Sets the calculated columns of a table and keeps the rest of its setup.
fn set(e: &Engine, table: &str, cols: Vec<CalcColumn>) -> anyhow::Result<bool> {
    let mut setup = e
        .snapshot()
        .workspace
        .tables
        .get(table)
        .cloned()
        .unwrap_or_default();
    setup.calc = cols;
    e.set_table_setup(table, Some(setup))
}

fn refused(r: anyhow::Result<bool>) -> CalcRefused {
    r.expect_err("the setup should be refused")
        .downcast::<CalcRefused>()
        .expect("a refusal of a calculated column")
}

fn q(v: Value) -> SemanticQuery {
    serde_json::from_value(v).unwrap()
}

fn column(e: &Engine, table: &str, name: &str) -> tenun_core::catalog::Column {
    e.catalog()
        .table(table)
        .unwrap()
        .column(name)
        .unwrap_or_else(|| panic!("no column {name}"))
        .clone()
}

/// The values of one column, in the order of the file.
fn values(e: &Engine, table: &str, col: &str) -> Vec<Value> {
    let p = e.preview(table, 100).unwrap();
    let i = p.columns.iter().position(|c| c.name == col).unwrap();
    p.rows.iter().map(|r| r[i].clone()).collect()
}

#[test]
fn a_calculated_column_is_a_column_of_the_table() {
    let w = world("basic");
    set(
        &w.e,
        &w.orders,
        vec![calc("Revenue incl. tax", "[amount] * 1.11")],
    )
    .unwrap();
    let c = column(&w.e, &w.orders, "Revenue incl. tax");
    assert!(c.calc && c.error.is_none());
    assert_eq!(c.dtype, DType::Number);
    // It comes after the columns of the file.
    let t = w.e.catalog().table(&w.orders).unwrap().clone();
    assert_eq!(t.columns.last().unwrap().name, "Revenue incl. tax");
    assert_eq!(t.columns.len(), 6);
    // Its values are the formula's.
    let v: Vec<f64> = values(&w.e, &w.orders, "Revenue incl. tax")
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect();
    assert!(
        (v[0] - 111.0).abs() < 1e-9 && (v[1] - 277.5).abs() < 1e-9,
        "{v:?}"
    );
    // A chart can add it up.
    let r =
        w.e.query(&q(json!({
            "measures": [{"table": w.orders, "column": "Revenue incl. tax", "agg": "sum"}]
        })))
        .unwrap();
    let total = r.rows[0][0].as_f64().unwrap();
    assert!((total - 480.5 * 1.11).abs() < 1e-9, "{total}");
    // The row count did not change.
    assert_eq!(t.rows, Some(6));
    // The column profile reads it like any column.
    let p = w.e.profile(&w.orders, "Revenue incl. tax").unwrap();
    assert_eq!((p.rows, p.distinct), (6, 6));
    assert_eq!(p.blank, 0.0);
}

#[test]
fn a_tile_groups_by_a_calculated_column_and_a_filter_on_it_works() {
    let w = world("group");
    set(
        &w.e,
        &w.orders,
        vec![calc("Size", r#"IF([amount] >= 100, "Large", "Small")"#)],
    )
    .unwrap();
    let r =
        w.e.query(&q(json!({
            "measures": [{"table": w.orders, "column": "amount", "agg": "sum"}],
            "dimensions": [{"table": w.orders, "column": "Size"}],
            "sort": [{"by": 0, "desc": false}]
        })))
        .unwrap();
    assert_eq!(
        r.rows,
        [
            [json!("Large"), json!(350.0)],
            [json!("Small"), json!(130.5)]
        ]
    );
    let f =
        w.e.query(&q(json!({
            "measures": [{"table": w.orders, "column": null, "agg": "count"}],
            "filters": [{"table": w.orders, "column": "Size", "op": "in", "values": ["Large"]}]
        })))
        .unwrap();
    assert_eq!(f.rows[0][0].as_f64(), Some(2.0));
}

#[test]
fn a_related_column_joins_right_and_is_blank_for_an_unmatched_key() {
    let w = world("related");
    set(
        &w.e,
        &w.orders,
        vec![
            calc("Store name", "[stores].[name]"),
            // Two hops: orders to stores to regions.
            calc("Region", "[regions].[name]"),
        ],
    )
    .unwrap();
    assert_eq!(
        values(&w.e, &w.orders, "Store name"),
        [
            json!("Kopi A"),
            json!("Kopi A"),
            json!("Kopi B"),
            json!("Kopi C"),
            json!("Kopi B"),
            Value::Null
        ]
    );
    assert_eq!(
        values(&w.e, &w.orders, "Region"),
        [
            json!("Java"),
            json!("Java"),
            json!("Java"),
            json!("Bali"),
            json!("Java"),
            Value::Null
        ]
    );
    // A chart by the new column adds the orders of each region; the unmatched one is blank.
    let r =
        w.e.query(&q(json!({
            "measures": [{"table": w.orders, "column": "amount", "agg": "sum"}],
            "dimensions": [{"table": w.orders, "column": "Region"}],
            "sort": [{"by": 1, "desc": true}]
        })))
        .unwrap();
    let rows: Vec<(Value, f64)> = r
        .rows
        .iter()
        .map(|x| (x[0].clone(), x[1].as_f64().unwrap()))
        .collect();
    assert_eq!(
        rows,
        [
            (json!("Java"), 400.0),
            (json!("Bali"), 75.5),
            (Value::Null, 5.0)
        ]
    );
}

#[test]
fn a_calculated_column_may_use_an_earlier_one_and_never_a_later_one() {
    let w = world("order");
    // The second uses the first.
    set(
        &w.e,
        &w.orders,
        vec![
            calc("Net", "[amount] * 0.9"),
            calc("Net rounded", "ROUND([Net], 0)"),
        ],
    )
    .unwrap();
    assert_eq!(values(&w.e, &w.orders, "Net rounded")[0], json!(90.0));
    // The first uses the second: refused, and the error underlines the name.
    let r = refused(set(
        &w.e,
        &w.orders,
        vec![
            calc("Net rounded", "ROUND([Net], 0)"),
            calc("Net", "[amount] * 0.9"),
        ],
    ));
    assert_eq!((r.index, r.name.as_str()), (0, "Net rounded"));
    assert!(
        r.error.message.contains("comes after this one"),
        "{}",
        r.error.message
    );
    assert_eq!((r.error.at, r.error.len), (6, 5));
    // A column cannot use itself.
    let r = refused(set(&w.e, &w.orders, vec![calc("Loop", "[Loop] + 1")]));
    assert!(
        r.error.message.contains("cannot use itself"),
        "{}",
        r.error.message
    );
    // Nothing was saved: the earlier setup is still there.
    assert!(column(&w.e, &w.orders, "Net rounded").error.is_none());
}

#[test]
fn a_chain_across_two_tables_is_built_in_the_order_it_needs() {
    let w = world("chain");
    // Orders read a calculated column of stores, which reads a calculated column of regions.
    // The tables were added in the opposite order.
    set(
        &w.e,
        &w.regions,
        vec![calc("Label", r#"UPPER([name]) & "!""#)],
    )
    .unwrap();
    set(
        &w.e,
        &w.stores,
        vec![calc("Place", r#"[regions].[Label] & " / " & [name]"#)],
    )
    .unwrap();
    set(&w.e, &w.orders, vec![calc("Where", "[stores].[Place]")]).unwrap();
    assert_eq!(values(&w.e, &w.orders, "Where")[0], json!("JAVA! / Kopi A"));
    assert_eq!(values(&w.e, &w.orders, "Where")[3], json!("BALI! / Kopi C"));
    assert_eq!(values(&w.e, &w.orders, "Where")[5], Value::Null);
    // A change of regions reaches the orders: the file is saved and the source loads again.
    std::fs::write(w.d.join("regions.csv"), "id,name\n10,Jawa\n20,Bali\n").unwrap();
    let id = w.e.catalog().table(&w.regions).unwrap().source_id.clone();
    w.e.load(&id);
    assert_eq!(values(&w.e, &w.orders, "Where")[0], json!("JAWA! / Kopi A"));
    assert!(
        w.e.snapshot().errors.is_empty(),
        "{:?}",
        w.e.snapshot().errors
    );
}

#[test]
fn a_view_with_a_related_column_over_a_repeated_key_keeps_its_row_count() {
    let w = world("repeat");
    set(&w.e, &w.orders, vec![calc("Store name", "[stores].[name]")]).unwrap();
    // The stores file now has store 1 twice. The first row of a key is the one that counts.
    std::fs::write(
        w.d.join("stores.csv"),
        "id,name,region_id,rent\n1,Kopi A,10,500\n1,Kopi A again,10,500\n2,Kopi B,10,300\n3,Kopi C,20,200\n",
    )
    .unwrap();
    let id = w.e.catalog().table(&w.stores).unwrap().source_id.clone();
    w.e.load(&id);
    let t = w.e.catalog().table(&w.orders).unwrap().clone();
    assert_eq!(t.rows, Some(6));
    assert_eq!(values(&w.e, &w.orders, "Store name").len(), 6);
    assert_eq!(values(&w.e, &w.orders, "Store name")[0], json!("Kopi A"));
    // No total doubled.
    let r =
        w.e.query(&q(json!({
            "measures": [{"table": w.orders, "column": "amount", "agg": "sum"}]
        })))
        .unwrap();
    assert_eq!(r.rows[0][0].as_f64().unwrap(), 480.5);
}

#[test]
fn a_failing_calculated_column_leaves_the_rest_of_the_table_loaded() {
    let w = world("fail");
    set(
        &w.e,
        &w.orders,
        vec![
            calc("Tagged", r#"[note] & "!""#),
            calc("Double", "[amount] * 2"),
        ],
    )
    .unwrap();
    assert_eq!(values(&w.e, &w.orders, "Tagged")[0], json!("a!"));
    // The file loses the column the formula reads.
    std::fs::write(
        w.d.join("orders.csv"),
        "id,date,store_id,amount\n1,2026-01-15,1,100\n2,2026-01-20,1,250\n",
    )
    .unwrap();
    let id = w.e.catalog().table(&w.orders).unwrap().source_id.clone();
    w.e.load(&id);
    let snap = w.e.snapshot();
    // The source loaded; the table says which column is blank and why.
    assert!(!snap.errors.contains_key(&id), "{:?}", snap.errors);
    let said = snap
        .table_errors
        .get(&w.orders)
        .expect("an error on the table");
    assert!(
        said.contains("Tagged") && said.contains("No column [note]"),
        "{said}"
    );
    let tagged = column(&w.e, &w.orders, "Tagged");
    assert!(tagged.calc && tagged.error.is_some());
    assert_eq!(
        values(&w.e, &w.orders, "Tagged"),
        [Value::Null, Value::Null]
    );
    // The other calculated column and the file's columns are fine.
    assert_eq!(
        values(&w.e, &w.orders, "Double"),
        [json!(200.0), json!(500.0)]
    );
    let amounts: Vec<f64> = values(&w.e, &w.orders, "amount")
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert_eq!(amounts, [100.0, 250.0]);
}

#[test]
fn a_reload_keeps_the_calculated_columns() {
    let w = world("reload");
    set(&w.e, &w.orders, vec![calc("Double", "[amount] * 2")]).unwrap();
    let id = w.e.catalog().table(&w.orders).unwrap().source_id.clone();
    w.e.load(&id);
    assert_eq!(column(&w.e, &w.orders, "Double").dtype, DType::Number);
    assert_eq!(values(&w.e, &w.orders, "Double")[0], json!(200.0));
    // And so does a workspace that is saved and opened again.
    let file = w.d.join("w.tenun");
    w.e.save(Some(file.clone())).unwrap();
    let e2 = Engine::new(Secrets::new(w.d.join("secrets2.json"))).unwrap();
    e2.open(&file).unwrap();
    assert_eq!(values(&e2, &w.orders, "Double")[0], json!(200.0));
}

#[test]
fn a_table_has_a_limit_of_calculated_columns() {
    let w = world("limit");
    let many: Vec<CalcColumn> = (0..51)
        .map(|i| calc(&format!("C{i}"), "[amount] + 1"))
        .collect();
    let err = set(&w.e, &w.orders, many).expect_err("too many");
    assert!(
        err.to_string().contains("up to 50 calculated columns"),
        "{err}"
    );
    let fifty: Vec<CalcColumn> = (0..50)
        .map(|i| calc(&format!("C{i}"), "[amount] + 1"))
        .collect();
    set(&w.e, &w.orders, fifty).unwrap();
    assert!(column(&w.e, &w.orders, "C49").error.is_none());
    assert_eq!(values(&w.e, &w.orders, "C49")[0].as_f64(), Some(101.0));
}

#[test]
fn a_name_the_table_has_already_leaves_the_column_out_and_says_so() {
    // A file made by hand: the calculated column has the name of a column of the table.
    let w = world("dup-name");
    let mut ws = w.e.snapshot().workspace;
    ws.tables.insert(
        w.orders.clone(),
        tenun_core::workspace::TableSetup {
            calc: vec![calc("amount", "[id] * 2"), calc("Fine", "[id] * 3")],
            ..Default::default()
        },
    );
    w.e.replace(ws, Some(w.d.join("dup.tenun")));
    let t = w.e.catalog().table(&w.orders).unwrap().clone();
    // One `amount`, the file's. The other calculated column is there.
    assert_eq!(t.columns.iter().filter(|c| c.name == "amount").count(), 1);
    assert!(t.column("Fine").is_some_and(|c| c.error.is_none()));
    assert_eq!(values(&w.e, &w.orders, "amount")[0].as_f64(), Some(100.0));
    let note = t
        .notes
        .iter()
        .find(|n| n.starts_with("A calculated column was not made: amount"));
    assert!(note.is_some_and(|n| n.contains("already")), "{:?}", t.notes);
    // The note is made again with each pass, not added to.
    let id = t.source_id.clone();
    w.e.load(&id);
    let again = w.e.catalog().table(&w.orders).unwrap().clone();
    assert_eq!(
        again
            .notes
            .iter()
            .filter(|n| n.contains("was not made"))
            .count(),
        1
    );
}

#[test]
fn deleting_the_columns_gives_the_table_back_as_it_was() {
    let w = world("delete");
    let before: Vec<String> =
        w.e.catalog()
            .table(&w.orders)
            .unwrap()
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect();
    set(
        &w.e,
        &w.orders,
        vec![
            calc("Double", "[amount] * 2"),
            calc("Triple", "[Double] + [amount]"),
        ],
    )
    .unwrap();
    // One goes: the other that used it is blank, with words.
    set(&w.e, &w.orders, vec![calc("Triple", "[Double] + [amount]")]).unwrap();
    assert!(column(&w.e, &w.orders, "Triple").error.is_some());
    assert!(
        w.e.catalog()
            .table(&w.orders)
            .unwrap()
            .column("Double")
            .is_none()
    );
    // All of them go: the table has its own columns and no setup is left in the file.
    set(&w.e, &w.orders, vec![]).unwrap();
    let t = w.e.catalog().table(&w.orders).unwrap().clone();
    let after: Vec<String> = t.columns.iter().map(|c| c.name.clone()).collect();
    assert_eq!(after, before);
    assert!(!t.sql_ref.contains("__calc_"), "{}", t.sql_ref);
    assert!(!w.e.snapshot().workspace.tables.contains_key(&w.orders));
    assert!(w.e.snapshot().errors.is_empty());
    // The tiles that asked for the column are told, not the table.
    let err =
        w.e.query(&q(
            json!({"measures": [{"table": w.orders, "column": "Double", "agg": "sum"}]}),
        ))
        .unwrap_err();
    assert!(err.to_string().contains("Double"), "{err}");
}

// ---- raw SQL (Advanced, V4-D62) ----------------------------------------------------------------

#[test]
fn a_sql_column_is_one_expression_that_runs_inside_try() {
    let w = world("sql");
    set(
        &w.e,
        &w.orders,
        vec![
            sql("Half", "round(amount / 2.0, 1)"),
            // A cell that cannot be cast is blank, not a failed table.
            sql("Note number", "CAST(note AS INTEGER)"),
            // A `;` inside a text is only a character.
            sql("Tagged", "note || ';' || 'x'"),
        ],
    )
    .unwrap();
    assert_eq!(column(&w.e, &w.orders, "Half").dtype, DType::Number);
    assert_eq!(values(&w.e, &w.orders, "Half")[0], json!(50.0));
    assert_eq!(values(&w.e, &w.orders, "Note number")[0], Value::Null);
    assert_eq!(values(&w.e, &w.orders, "Tagged")[0], json!("a;x"));
    assert!(w.e.snapshot().errors.is_empty());
}

#[test]
fn a_sql_column_cannot_read_a_file_or_another_table() {
    let w = world("sql-attack");
    let attacks = [
        // A scalar subquery that reads a local file.
        "1 + (SELECT length(content) FROM read_text('/etc/hostname'))",
        "(FROM read_text('/etc/hostname'))",
        "length(read_text('/etc/hostname'))",
        r#""read_text"('/etc/hostname')"#,
        "query_table('orders')",
        "(SELECT count(*) FROM query_table('orders'))",
        "getenv('HOME')",
        "current_setting('home_directory')",
        "WITH x AS (SELECT 1) SELECT 1",
        "amount; DROP VIEW orders",
        "amount -- and the rest",
        "amount /* x */",
        "amount IN (SELECT 1)",
        "EXISTS(SELECT 1)",
        "glob('*')",
        "sum(amount)",
        "row_number() OVER ()",
        "amount, note",
        "orders.amount",
        "E'x'",
        "$$x$$",
    ];
    for a in attacks {
        let r = refused(set(&w.e, &w.orders, vec![sql("Bad", a)]));
        assert_eq!(r.name, "Bad", "{a}");
        // Nothing reached the table.
        assert!(
            w.e.catalog()
                .table(&w.orders)
                .unwrap()
                .column("Bad")
                .is_none(),
            "{a}"
        );
    }
    // The underline sits on the word that is refused.
    let r = refused(set(&w.e, &w.orders, vec![sql("Bad", "1 + (SELECT 2)")]));
    assert_eq!((r.error.at, r.error.len), (5, 6), "{}", r.error.message);
    assert!(
        r.error.message.starts_with("SELECT is not available"),
        "{}",
        r.error.message
    );
    let r = refused(set(&w.e, &w.orders, vec![sql("Bad", "getenv('HOME')")]));
    assert!(r.error.message.contains("getenv"), "{}", r.error.message);
}

// ---- tables that read each other (V4-D63) -----------------------------------------------------

#[test]
fn two_tables_that_read_each_other_are_refused_with_both_columns() {
    let d = dir("cycle");
    std::fs::write(d.join("a.csv"), "id,v,b_id\n1,10,7\n2,20,8\n").unwrap();
    std::fs::write(d.join("b.csv"), "id,w\n7,70\n8,80\n").unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    for f in ["a.csv", "b.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    let id = |n: &str| {
        e.catalog()
            .tables
            .iter()
            .find(|t| t.name == n)
            .unwrap()
            .id
            .clone()
    };
    let (a, b) = (id("a"), id("b"));
    // Both sides are unique, so the link reads either way.
    e.add_relationship(field(&a, "b_id"), field(&b, "id"))
        .unwrap();
    set(&e, &a, vec![calc("X", "[b].[w] + 1")]).unwrap();
    assert_eq!(values(&e, &a, "X"), [json!(71.0), json!(81.0)]);
    // B reading A, which reads B, is a circle: refused, and the words name both columns.
    let r = refused(set(&e, &b, vec![calc("Y", "[a].[v] + 1")]));
    assert_eq!(r.name, "Y");
    let m = &r.error.message;
    assert!(
        m.contains("b › Y reads a") && m.contains("a › X reads b"),
        "{m}"
    );
    assert!(m.contains("Two tables cannot read each other"), "{m}");
    // Nothing was saved.
    assert!(e.catalog().table(&b).unwrap().column("Y").is_none());
    // B may read A when A does not read B.
    set(&e, &a, vec![]).unwrap();
    set(&e, &b, vec![calc("Y", "[a].[v] + 1")]).unwrap();
    assert_eq!(values(&e, &b, "Y"), [json!(11.0), json!(21.0)]);
}

#[test]
fn a_column_that_reads_another_table_cannot_be_a_link_key() {
    let w = world("key");
    set(&w.e, &w.orders, vec![calc("Store name", "[stores].[name]")]).unwrap();
    // It cannot become the key of a link.
    let err =
        w.e.add_relationship(field(&w.orders, "Store name"), field(&w.stores, "name"))
            .expect_err("a related column is not a key");
    assert!(err.to_string().contains("reads another table"), "{err}");
    // A calculated column that reads only its own table can.
    set(
        &w.e,
        &w.orders,
        vec![
            calc("Store name", "[stores].[name]"),
            calc("Region key", "[store_id] * 0 + 10"),
        ],
    )
    .unwrap();
    assert!(
        w.e.add_relationship(field(&w.orders, "Region key"), field(&w.regions, "id"))
            .is_ok()
    );
    // The link is checked on the calculated key after a reload too.
    let link =
        w.e.snapshot()
            .workspace
            .relationships
            .last()
            .unwrap()
            .id
            .clone();
    let source = w.e.catalog().table(&w.orders).unwrap().source_id.clone();
    w.e.load(&source);
    let health = w.e.snapshot().health;
    assert!(
        health.contains_key(&link),
        "{:?}",
        health.keys().collect::<Vec<_>>()
    );
    assert_eq!(health[&link].kind, "one-many");
    // A column that is a key stays: deleting it is refused.
    let err = set(&w.e, &w.orders, vec![calc("Store name", "[stores].[name]")]).expect_err("a key");
    assert!(err.to_string().contains("key of a link"), "{err}");
    // And a key cannot start to read another table.
    let err = set(
        &w.e,
        &w.orders,
        vec![
            calc("Store name", "[stores].[name]"),
            calc("Region key", "[stores].[region_id]"),
        ],
    )
    .expect_err("a key that reads another table");
    assert!(err.to_string().contains("reads another table"), "{err}");
}

// ---- names that change (V4-D25, V4-D46) --------------------------------------------------------

#[test]
fn renaming_a_column_rewrites_the_formulas_that_use_it() {
    let w = world("rename-col");
    // The amount is shown as "Sales". A formula of its table and a formula of another table use it.
    label(&w.e, &w.orders, "amount", "Sales");
    set(
        &w.e,
        &w.orders,
        vec![calc("Double", "[Sales] * 2 + [orders].[Sales]")],
    )
    .unwrap();
    set(
        &w.e,
        &w.lines,
        vec![calc("Order sales", "[orders].[Sales] + [qty]")],
    )
    .unwrap();
    assert_eq!(values(&w.e, &w.lines, "Order sales")[0], json!(102.0));
    // From now on the amount is shown as "Revenue".
    label(&w.e, &w.orders, "amount", "Revenue");
    let tables = w.e.snapshot().workspace.tables;
    assert_eq!(
        tables[&w.orders].calc[0].formula,
        "[Revenue] * 2 + [orders].[Revenue]"
    );
    assert_eq!(
        tables[&w.lines].calc[0].formula,
        "[orders].[Revenue] + [qty]"
    );
    assert!(column(&w.e, &w.orders, "Double").error.is_none());
    assert_eq!(values(&w.e, &w.orders, "Double")[0], json!(300.0));
    assert_eq!(values(&w.e, &w.lines, "Order sales")[0], json!(102.0));
    // A formula that uses the real name keeps working when the name shown changes: the text stays.
    label(&w.e, &w.orders, "store_id", "Shop");
    set(
        &w.e,
        &w.orders,
        vec![calc("Plain", "[store_id] + 1"), calc("Shown", "[Shop] + 1")],
    )
    .unwrap();
    label(&w.e, &w.orders, "store_id", "Outlet");
    let calc = w.e.snapshot().workspace.tables[&w.orders].calc.clone();
    assert_eq!(calc[0].formula, "[store_id] + 1");
    assert_eq!(calc[1].formula, "[Outlet] + 1");
    assert!(column(&w.e, &w.orders, "Plain").error.is_none());
    assert!(column(&w.e, &w.orders, "Shown").error.is_none());
}

#[test]
fn renaming_a_link_rewrites_the_formulas_that_use_it() {
    let w = world("rename-link");
    let first = w.e.snapshot().workspace.relationships[0].id.clone();
    w.e.rename_link(&first, Some("Sold at".into())).unwrap();
    set(
        &w.e,
        &w.orders,
        vec![calc("Store name", r#"[Sold at].[name] & "!""#)],
    )
    .unwrap();
    assert_eq!(values(&w.e, &w.orders, "Store name")[0], json!("Kopi A!"));
    w.e.rename_link(&first, Some("Bought at".into())).unwrap();
    let saved = w.e.snapshot().workspace.tables[&w.orders].calc.clone();
    assert_eq!(saved[0].formula, r#"[Bought at].[name] & "!""#);
    assert!(column(&w.e, &w.orders, "Store name").error.is_none());
    assert_eq!(values(&w.e, &w.orders, "Store name")[0], json!("Kopi A!"));
    // The batch call follows two links that swap their names.
    let second = w.e.snapshot().workspace.relationships[1].id.clone();
    w.e.rename_link(&second, Some("Region of".into())).unwrap();
    set(
        &w.e,
        &w.stores,
        vec![calc("Place", r#"[Region of].[name]"#)],
    )
    .unwrap();
    w.e.rename_links(
        [
            (first.clone(), Some("Region of".to_string())),
            (second.clone(), Some("Bought at".to_string())),
        ]
        .into(),
    )
    .unwrap();
    let tables = w.e.snapshot().workspace.tables;
    assert_eq!(tables[&w.stores].calc[0].formula, "[Bought at].[name]");
    assert_eq!(
        tables[&w.orders].calc[0].formula,
        r#"[Region of].[name] & "!""#
    );
    assert_eq!(values(&w.e, &w.stores, "Place")[0], json!("Java"));
}

// ---- the day (V4-D34) ---------------------------------------------------------------------------

#[test]
fn a_column_with_today_is_built_again_when_the_day_changes() {
    let w = world("today");
    let day = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    w.e.set_today(Some(day("2026-10-11")));
    set(&w.e, &w.orders, vec![calc("Age", "DAYS(TODAY(), [date])")]).unwrap();
    assert_eq!(
        column(&w.e, &w.orders, "Age").as_of.as_deref(),
        Some("2026-10-11")
    );
    let before = values(&w.e, &w.orders, "Age")[0].as_f64().unwrap();
    // Nothing happens while the day stays.
    assert!(!w.e.refresh_dates());
    w.e.set_today(Some(day("2026-10-12")));
    assert!(w.e.refresh_dates());
    let after = values(&w.e, &w.orders, "Age")[0].as_f64().unwrap();
    assert_eq!(after - before, 1.0);
    assert_eq!(
        column(&w.e, &w.orders, "Age").as_of.as_deref(),
        Some("2026-10-12")
    );
    assert!(!w.e.refresh_dates());
    // A column without TODAY has no day.
    set(&w.e, &w.orders, vec![calc("Double", "[amount] * 2")]).unwrap();
    assert!(column(&w.e, &w.orders, "Double").as_of.is_none());
    assert!(!w.e.refresh_dates());
}

// ---- words that leave the computer (V4-D52, V4-D54) ----------------------------------------------

#[test]
fn a_blank_column_says_so_on_the_tile_and_the_public_note_has_no_formula() {
    use tenun_core::snapshot::{DashFilter, Tile, run_tile, run_tile_public};
    let w = world("public");
    set(
        &w.e,
        &w.orders,
        vec![calc("Marked", r#"[amount] & "s3cr3t""#)],
    )
    .unwrap();
    // The file loses the column the formula reads.
    std::fs::write(
        w.d.join("orders.csv"),
        "id,date,store_id,note\n1,2026-01-15,1,a\n2,2026-01-20,1,b\n",
    )
    .unwrap();
    let id = w.e.catalog().table(&w.orders).unwrap().source_id.clone();
    w.e.load(&id);
    let tile: Tile = serde_json::from_value(json!({
        "id": "t1", "type": "bar", "title": "", "x": 0, "y": 0, "w": 4, "h": 3,
        "measures": [{"table": w.orders, "column": null, "agg": "count"}],
        "dims": [{"table": w.orders, "column": "Marked"}]
    }))
    .unwrap();
    let filters: Vec<DashFilter> = Vec::new();
    // In the app: the table, the column and the reason.
    let mine = run_tile(&w.e, &tile, &filters).unwrap();
    assert_eq!(mine.notes.len(), 1, "{:?}", mine.notes);
    assert!(
        mine.notes[0].starts_with("orders: Marked could not be computed"),
        "{:?}",
        mine.notes
    );
    assert!(
        mine.notes[0].contains("No column [amount]"),
        "{:?}",
        mine.notes
    );
    // Out of the computer: the table and the column, and no formula text.
    let out = run_tile_public(&w.e, &tile, &filters).unwrap();
    assert_eq!(out.notes, ["orders: Marked could not be computed."]);
    let text = serde_json::to_string(&out).unwrap();
    assert!(!text.contains("s3cr3t") && !text.contains(".csv"), "{text}");

    // The snapshot of a dashboard, which is what an export and a published page are made of.
    w.e.set_dashboards(vec![json!({
        "id": "d1", "title": "D", "refresh": 0,
        "tiles": [{
            "id": "t1", "type": "bar", "title": "", "x": 0, "y": 0, "w": 4, "h": 3,
            "measures": [{"table": w.orders, "column": null, "agg": "count"}],
            "dims": [{"table": w.orders, "column": "Marked"}]
        }]
    })]);
    let snap = tenun_core::snapshot::build(&w.e, "d1", &[]).unwrap();
    let shipped = serde_json::to_string(&snap).unwrap();
    assert!(
        shipped.contains("orders: Marked could not be computed."),
        "{shipped}"
    );
    assert!(
        !shipped.contains("s3cr3t") && !shipped.contains("No column") && !shipped.contains(".csv"),
        "{shipped}"
    );
    // The exported HTML holds the same.
    let html = tenun::export_html(&snap);
    assert!(!html.contains("s3cr3t") && !html.contains("No column [amount]"));
}

// ---- the Coffee sample -----------------------------------------------------------------------

fn coffee(tag: &str) -> Engine {
    let dir = dir(tag);
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sample");
    for f in ["coffee.xlsx", "stores.csv", "customers.sqlite"] {
        std::fs::copy(src.join(f), dir.join(f)).unwrap();
    }
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    e.replace(tenun::sample::workspace(), Some(dir.join("Coffee.tenun")));
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    e
}

#[test]
fn the_coffee_sample_gets_two_columns_that_chart_filter_and_export() {
    use tenun_core::snapshot::{DashFilter, Tile, run_tile};
    let e = coffee("coffee");
    let orders = "s1/Orders";
    set(
        &e,
        orders,
        vec![
            calc("Revenue incl. tax", "[amount] * 1.11"),
            // Orders to stores, in the CSV file: a column of another source.
            calc("Region", "[stores].[region]"),
        ],
    )
    .unwrap();
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    let t = e.catalog().table(orders).unwrap().clone();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(&names[names.len() - 2..], ["Revenue incl. tax", "Region"]);
    assert_eq!(column(&e, orders, "Region").dtype, DType::Text);

    // The chart: the revenue of each region adds up to what the plain column gives by region.
    let by = |measure: Value, dim: Value| -> Vec<(String, f64)> {
        let r = e
            .query(&q(json!({ "measures": [measure], "dimensions": [dim] })))
            .unwrap();
        let mut rows: Vec<(String, f64)> = r
            .rows
            .iter()
            .map(|x| {
                (
                    x[0].as_str().unwrap_or("").to_string(),
                    x[1].as_f64().unwrap(),
                )
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows
    };
    let calc_by_region = by(
        json!({"table": orders, "column": "Revenue incl. tax", "agg": "sum"}),
        json!({"table": orders, "column": "Region"}),
    );
    let plain_by_region = by(
        json!({"table": orders, "column": "amount", "agg": "sum"}),
        json!({"table": "s2/stores", "column": "region"}),
    );
    assert!(calc_by_region.len() >= 2, "{calc_by_region:?}");
    assert_eq!(calc_by_region.len(), plain_by_region.len());
    for (a, b) in calc_by_region.iter().zip(&plain_by_region) {
        assert_eq!(a.0, b.0);
        assert!((a.1 - b.1 * 1.11).abs() < 1e-6, "{a:?} {b:?}");
    }

    // The filter: a click on one region keeps the orders of that region.
    let one = &plain_by_region[0];
    let filtered = e
        .query(&q(json!({
            "measures": [{"table": orders, "column": "amount", "agg": "sum"}],
            "filters": [{"table": orders, "column": "Region", "op": "in", "values": [one.0]}]
        })))
        .unwrap();
    assert!((filtered.rows[0][0].as_f64().unwrap() - one.1).abs() < 1e-6);

    // The export: the Excel file of a table tile holds the new columns.
    let tile = json!({
        "id": "t", "type": "table", "totals": true,
        "measures": [{"table": orders, "column": "Revenue incl. tax", "agg": "sum"}],
        "dims": [{"table": orders, "column": "Region"}],
    });
    let t: Tile = serde_json::from_value(tile.clone()).unwrap();
    let res = run_tile(&e, &t, &Vec::<DashFilter>::new()).unwrap();
    let bytes = tenun::xlsx::build(
        &tile,
        &res,
        &["Region".into(), "Revenue".into()],
        "Revenue by region",
    )
    .unwrap();
    let mut wb: calamine::Xlsx<_> =
        calamine::open_workbook_from_rs(std::io::Cursor::new(bytes)).unwrap();
    use calamine::{DataType, Reader};
    let name = wb.sheet_names()[0].clone();
    let rows: Vec<Vec<calamine::Data>> = wb
        .worksheet_range(&name)
        .unwrap()
        .rows()
        .map(|r| r.to_vec())
        .collect();
    let total = rows.last().unwrap()[1].as_f64().unwrap();
    let expect: f64 = plain_by_region.iter().map(|r| r.1).sum::<f64>() * 1.11;
    assert!((total - expect).abs() < 1e-6, "{total} {expect}");

    // The workspace is saved and opened again: three sources load one after the other, and the
    // column that reads the stores file finds them in the end, with no error.
    let file = e.workspace_path().unwrap();
    e.save(None).unwrap();
    let again = Engine::new(Secrets::new(file.with_file_name("secrets2.json"))).unwrap();
    again.open(&file).unwrap();
    assert!(
        again.snapshot().errors.is_empty(),
        "{:?}",
        again.snapshot().errors
    );
    let regions = values(&again, orders, "Region");
    assert!(regions.iter().all(|v| v.is_string()), "{:?}", &regions[..3]);
}

// ---- the server -------------------------------------------------------------------------------

async fn send(
    running: &tenun::Running,
    method: &'static str,
    path: &str,
    body: Value,
) -> (u16, Value) {
    let url = format!("http://{}{path}", running.addr);
    tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let req = match method {
            "PUT" => agent.put(url),
            _ => agent.post(url),
        };
        let mut res = req
            .header("content-type", "application/json")
            .header("x-tenun", "1")
            .send(body.to_string())
            .unwrap();
        let status = res.status().as_u16();
        let text = res.body_mut().read_to_string().unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_setup_route_answers_a_bad_formula_as_a_normal_answer_with_utf16_places() {
    let d = dir("http");
    write_files(&d);
    let running = tenun::start(tenun::Options {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        workspace: Some(d.join("http.tenun")),
        picker: None,
    })
    .await
    .unwrap();
    let e = &running.state.engine;
    e.add_source(AddSource::File {
        path: d.join("orders.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let table = e.snapshot().tables[0].id.clone();

    // A good column: the new snapshot comes back.
    let (status, snap) = send(
        &running,
        "PUT",
        "/api/tables/setup",
        json!({"table": table, "setup": {"calc": [{"name": "Double", "formula": "[amount] * 2"}]}}),
    )
    .await;
    assert_eq!(status, 200);
    assert!(snap.get("ok").is_none(), "{snap}");
    assert_eq!(
        snap["workspace"]["tables"][&table]["calc"][0]["name"],
        json!("Double")
    );
    let cols = snap["tables"][0]["columns"].as_array().unwrap();
    let double = cols.iter().find(|c| c["name"] == "Double").unwrap();
    assert_eq!(double["calc"], json!(true));

    // A bad one is a 200 with `ok: false`, the column, and a place counted in UTF-16 units: the
    // emoji takes two.
    let formula = r#""😀" & [nope]"#;
    let (status, bad) = send(
        &running,
        "PUT",
        "/api/tables/setup",
        json!({"table": table, "setup": {"calc": [{"name": "Double", "formula": "[amount] * 2"},
                                                  {"name": "Broken", "formula": formula}]}}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(bad["ok"], json!(false));
    assert_eq!(
        (bad["calc"].clone(), bad["name"].clone()),
        (json!(1), json!("Broken"))
    );
    let (at, len) = (
        bad["error"]["at"].as_u64().unwrap() as usize,
        bad["error"]["len"].as_u64().unwrap() as usize,
    );
    let units: Vec<u16> = formula.encode_utf16().collect();
    assert_eq!(String::from_utf16(&units[at..at + len]).unwrap(), "[nope]");
    assert!(
        bad["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("No column [nope]"),
        "{bad}"
    );
    // Nothing was saved.
    assert!(
        e.catalog()
            .table(&table)
            .unwrap()
            .column("Broken")
            .is_none()
    );

    // The check route knows the column being edited, and SQL.
    let (_, r) = send(
        &running,
        "POST",
        "/api/formula/check",
        json!({"formula": "[Double] + 1", "kind": "column", "table": table, "before": "Double"}),
    )
    .await;
    assert_eq!(r["ok"], json!(false), "a column cannot use itself: {r}");
    let (_, r) = send(
        &running,
        "POST",
        "/api/formula/check",
        json!({"formula": "[Double] + 1", "kind": "column", "table": table}),
    )
    .await;
    assert_eq!(r["ok"], json!(true), "{r}");
    let (_, r) = send(
        &running,
        "POST",
        "/api/formula/check",
        json!({"formula": "round(amount / 3.0, 2)", "kind": "column", "table": table, "sql": true}),
    )
    .await;
    assert_eq!(r["ok"], json!(true), "{r}");
    assert_eq!(r["dtype"], json!("number"));
    assert_eq!(r["preview"][0], json!(33.33));
    let (_, r) = send(
        &running,
        "POST",
        "/api/formula/check",
        json!({"formula": "getenv('HOME')", "kind": "column", "table": table, "sql": true}),
    )
    .await;
    assert_eq!(r["ok"], json!(false));
    assert_eq!(
        (r["error"]["at"].clone(), r["error"]["len"].clone()),
        (json!(0), json!(6))
    );
}

// ---- review fixes ------------------------------------------------------------------------------

fn source_of(e: &Engine, table: &str) -> String {
    e.catalog().table(table).unwrap().source_id.clone()
}

#[test]
fn a_blank_column_keeps_its_type_across_a_reload() {
    let w = world("keeps-type");
    set(
        &w.e,
        &w.orders,
        vec![calc("Broken", "LEN([note]) * 2 + [amount]")],
    )
    .unwrap();
    assert_eq!(column(&w.e, &w.orders, "Broken").dtype, DType::Number);
    // The file loses the column the formula reads. The column is blank, and still a number.
    std::fs::write(
        w.d.join("orders.csv"),
        "id,date,store_id,amount\n1,2026-01-15,1,100\n2,2026-01-20,1,250\n",
    )
    .unwrap();
    w.e.load(&source_of(&w.e, &w.orders));
    let c = column(&w.e, &w.orders, "Broken");
    assert!(c.error.is_some());
    assert_eq!(c.dtype, DType::Number, "{c:?}");
    // A chart that adds it up still draws: a blank sum, not an error about text.
    let r =
        w.e.query(&q(json!({
            "measures": [{"table": w.orders, "column": "Broken", "agg": "sum"}]
        })))
        .unwrap();
    assert!(
        r.rows[0][0].is_null() || r.rows[0][0].as_f64() == Some(0.0),
        "{:?}",
        r.rows
    );
    // It still has the type after another reload, and the type is the real one when it heals.
    w.e.load(&source_of(&w.e, &w.orders));
    assert_eq!(column(&w.e, &w.orders, "Broken").dtype, DType::Number);
    std::fs::write(
        w.d.join("orders.csv"),
        "id,date,store_id,amount,note\n1,2026-01-15,1,100,abc\n",
    )
    .unwrap();
    w.e.load(&source_of(&w.e, &w.orders));
    assert!(column(&w.e, &w.orders, "Broken").error.is_none());
    assert_eq!(values(&w.e, &w.orders, "Broken")[0].as_f64(), Some(106.0));
}

#[test]
fn a_source_that_fails_blanks_only_the_columns_that_read_it() {
    let w = world("failing-source");
    set(
        &w.e,
        &w.orders,
        vec![
            calc("Store name", "[stores].[name]"),
            calc("Double", "[amount] * 2"),
        ],
    )
    .unwrap();
    let stores_source = source_of(&w.e, &w.stores);
    // The stores file goes away and the source is read again.
    std::fs::rename(w.d.join("stores.csv"), w.d.join("stores.moved")).unwrap();
    w.e.load(&stores_source);
    let sum = |col: &str| {
        w.e.query(&q(json!({
            "measures": [{"table": w.orders, "column": col, "agg": "sum"}]
        })))
    };
    // Every tile of the orders still works. Only the column that read the stores is blank.
    assert_eq!(sum("amount").unwrap().rows[0][0].as_f64(), Some(480.5));
    assert_eq!(sum("Double").unwrap().rows[0][0].as_f64(), Some(961.0));
    assert!(column(&w.e, &w.orders, "Store name").error.is_some());
    assert_eq!(values(&w.e, &w.orders, "Store name")[0], Value::Null);
    let snap = w.e.snapshot();
    assert!(snap.table_errors[&w.orders].contains("Store name"));
    assert!(snap.errors.contains_key(&stores_source));
    // The file comes back: the column is whole again.
    std::fs::rename(w.d.join("stores.moved"), w.d.join("stores.csv")).unwrap();
    w.e.load(&stores_source);
    assert!(column(&w.e, &w.orders, "Store name").error.is_none());
    assert_eq!(values(&w.e, &w.orders, "Store name")[0], json!("Kopi A"));
    assert!(w.e.snapshot().table_errors.is_empty());
    // A source that is removed does the same.
    w.e.remove_source(&stores_source);
    assert_eq!(sum("amount").unwrap().rows[0][0].as_f64(), Some(480.5));
    assert!(column(&w.e, &w.orders, "Store name").error.is_some());
}

#[test]
fn a_swap_of_two_names_keeps_every_formula_true() {
    let w = world("swap");
    label(&w.e, &w.orders, "amount", "X");
    label(&w.e, &w.orders, "store_id", "Y");
    set(&w.e, &w.orders, vec![calc("Mix", "[X] * 1000 + [Y]")]).unwrap();
    let before = values(&w.e, &w.orders, "Mix");
    // One save gives each column the other's name.
    let mut setup = w.e.snapshot().workspace.tables[&w.orders].clone();
    setup.columns.get_mut("amount").unwrap().label = Some("Y".into());
    setup.columns.get_mut("store_id").unwrap().label = Some("X".into());
    w.e.set_table_setup(&w.orders, Some(setup)).unwrap();
    let saved = w.e.snapshot().workspace.tables[&w.orders].calc[0]
        .formula
        .clone();
    // The first name now means the other column, so the text swaps with it.
    assert_eq!(saved, "[Y] * 1000 + [X]");
    assert!(column(&w.e, &w.orders, "Mix").error.is_none());
    assert_eq!(values(&w.e, &w.orders, "Mix"), before);
}

#[test]
fn a_column_written_after_the_name_of_a_link_follows_a_rename() {
    let w = world("link-rename-col");
    let link = w.e.snapshot().workspace.relationships[0].id.clone();
    w.e.rename_link(&link, Some("Sold at".into())).unwrap();
    label(&w.e, &w.stores, "name", "Shop name");
    set(&w.e, &w.orders, vec![calc("Shop", "[Sold at].[Shop name]")]).unwrap();
    assert_eq!(values(&w.e, &w.orders, "Shop")[0], json!("Kopi A"));
    label(&w.e, &w.stores, "name", "Outlet");
    let saved = w.e.snapshot().workspace.tables[&w.orders].calc[0]
        .formula
        .clone();
    assert_eq!(saved, "[Sold at].[Outlet]");
    assert!(column(&w.e, &w.orders, "Shop").error.is_none());
    assert_eq!(values(&w.e, &w.orders, "Shop")[0], json!("Kopi A"));
}

#[test]
fn a_name_shown_that_two_columns_would_answer_to_is_refused_on_every_save() {
    let w = world("label-clash");
    set(&w.e, &w.orders, vec![calc("Margin", "[amount] * 0.2")]).unwrap();
    let with_label = |column: &str, text: &str| {
        let mut setup = w.e.snapshot().workspace.tables[&w.orders].clone();
        setup.columns.entry(column.into()).or_default().label = Some(text.into());
        w.e.set_table_setup(&w.orders, Some(setup))
    };
    // A calculated column, whatever the case and the spaces.
    for text in ["Margin", "margin", "  MARGIN "] {
        let err = with_label("note", text).expect_err(text);
        assert!(
            err.to_string().contains("already used by another column"),
            "{err}"
        );
    }
    // A real column of the file, by its real name.
    let err = with_label("note", "Amount").expect_err("a real column");
    assert!(err.to_string().contains("already used"), "{err}");
    // Another column's name shown.
    with_label("note", "Memo").unwrap();
    let err = with_label("date", "memo").expect_err("a name shown");
    assert!(err.to_string().contains("already used"), "{err}");
    // Nothing was saved: the column still answers to its own names only.
    let setup = &w.e.snapshot().workspace.tables[&w.orders];
    assert!(!setup.columns.contains_key("date"));
    // A column may be given its own real name, and a name nobody uses.
    with_label("date", "date").unwrap();
    with_label("date", "When").unwrap();
    // An old file whose names already clash can still be saved when the save does not touch them.
    let mut ws = w.e.snapshot().workspace;
    let t = ws.tables.get_mut(&w.orders).unwrap();
    t.columns.entry("id".into()).or_default().label = Some("When".into());
    w.e.replace(ws, Some(w.d.join("old.tenun")));
    label(&w.e, &w.orders, "store_id", "Shop");
}

#[test]
fn an_error_is_placed_on_the_text_that_was_sent() {
    let w = world("place");
    label(&w.e, &w.orders, "amount", "Sales");
    // One save renames the column and adds a column that has a slip after it. The engine reads
    // the text with the new name, which is longer; the place is moved back onto the sent text.
    let mut setup = w.e.snapshot().workspace.tables[&w.orders].clone();
    setup.columns.get_mut("amount").unwrap().label = Some("A much longer name".into());
    setup.calc = vec![calc("Bad", "[Sales] * [nope]")];
    let err = refused(w.e.set_table_setup(&w.orders, Some(setup)));
    assert_eq!(err.formula, "[Sales] * [nope]");
    assert_eq!(
        (err.error.at, err.error.len),
        (10, 6),
        "{}",
        err.error.message
    );
    let reply = err.reply();
    assert_eq!(reply["error"]["at"], json!(10));
}

#[test]
fn an_unpivot_rename_follows_formulas_and_is_refused_for_sql() {
    let d = dir("unpivot-rename");
    std::fs::write(
        d.join("sales.csv"),
        "product,jan,feb\nTea,10,20\nCoffee,30,40\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.add_source(AddSource::File {
        path: d.join("sales.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let t = e.catalog().tables[0].id.clone();
    let unpivot = |value: &str| TableSetup {
        transforms: vec![tenun_core::workspace::Transform::Unpivot {
            keep: vec!["product".into()],
            name: "Month".into(),
            value: value.into(),
        }],
        ..TableSetup::default()
    };
    e.set_table_setup(&t, Some(unpivot("Sales"))).unwrap();
    let mut setup = e.snapshot().workspace.tables[&t].clone();
    setup.calc = vec![calc("Double", "[Sales] * 2")];
    e.set_table_setup(&t, Some(setup)).unwrap();
    assert_eq!(values(&e, &t, "Double")[0].as_f64(), Some(20.0));
    // The value column gets another name: the formula follows it.
    let mut setup = e.snapshot().workspace.tables[&t].clone();
    setup.transforms = unpivot("Amount").transforms;
    e.set_table_setup(&t, Some(setup)).unwrap();
    let saved = e.snapshot().workspace.tables[&t].calc[0].formula.clone();
    assert_eq!(saved, "[Amount] * 2");
    assert!(column(&e, &t, "Double").error.is_none());
    assert_eq!(values(&e, &t, "Double")[0].as_f64(), Some(20.0));
    // A SQL column is not rewritten, so the rename that would break it is refused, by name.
    let mut setup = e.snapshot().workspace.tables[&t].clone();
    setup.calc.push(sql("Triple", "\"Amount\" * 3"));
    e.set_table_setup(&t, Some(setup)).unwrap();
    let mut setup = e.snapshot().workspace.tables[&t].clone();
    setup.transforms = unpivot("Revenue").transforms;
    let err = e
        .set_table_setup(&t, Some(setup))
        .expect_err("the SQL would break");
    let said = err.to_string();
    assert!(said.contains("Triple") && said.contains("Amount"), "{said}");
    // Nothing changed.
    assert_eq!(values(&e, &t, "Triple")[0].as_f64(), Some(30.0));
}

#[test]
fn a_formula_that_reads_a_column_of_today_through_another_table_is_as_of_the_day_too() {
    let w = world("as-of-through");
    let day = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    w.e.set_today(Some(day("2026-10-11")));
    set(&w.e, &w.orders, vec![calc("Age", "DAYS(TODAY(), [date])")]).unwrap();
    // The lines read the age of their order, and the next column reads that one.
    set(
        &w.e,
        &w.lines,
        vec![
            calc("Order age", "[orders].[Age] + 1"),
            calc("Older", "[Order age] + 1"),
        ],
    )
    .unwrap();
    for c in ["Order age", "Older"] {
        assert_eq!(
            column(&w.e, &w.lines, c).as_of.as_deref(),
            Some("2026-10-11"),
            "{c}"
        );
    }
    let before = values(&w.e, &w.lines, "Older")[0].as_f64().unwrap();
    // The day changes: the orders, then the lines that read them, are built again.
    w.e.set_today(Some(day("2026-10-12")));
    assert!(w.e.refresh_dates());
    assert_eq!(
        values(&w.e, &w.lines, "Older")[0].as_f64().unwrap() - before,
        1.0
    );
    assert_eq!(
        column(&w.e, &w.lines, "Older").as_of.as_deref(),
        Some("2026-10-12")
    );
}

#[test]
fn building_a_snapshot_builds_a_column_of_today_again() {
    let w = world("snapshot-today");
    let day = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    w.e.set_today(Some(day("2026-10-11")));
    set(&w.e, &w.orders, vec![calc("Age", "DAYS(TODAY(), [date])")]).unwrap();
    w.e.set_dashboards(vec![json!({
        "id": "d1", "title": "D", "refresh": 0,
        "tiles": [{
            "id": "t1", "type": "kpi", "title": "", "x": 0, "y": 0, "w": 3, "h": 2,
            "measures": [{"table": w.orders, "column": "Age", "agg": "max"}],
            "dims": []
        }]
    })]);
    let first = tenun_core::snapshot::build(&w.e, "d1", &[]).unwrap();
    let age = |v: &Value| v["base"]["t1"]["rows"][0][0].as_f64().unwrap();
    // The next day, with no load and no refresh: the page that is made is of that day.
    w.e.set_today(Some(day("2026-10-12")));
    let second = tenun_core::snapshot::build(&w.e, "d1", &[]).unwrap();
    assert_eq!(age(&second) - age(&first), 1.0, "{first} {second}");
    assert_eq!(
        column(&w.e, &w.orders, "Age").as_of.as_deref(),
        Some("2026-10-12")
    );
}

#[test]
fn a_hostile_file_cannot_read_a_file_or_the_environment_when_it_is_opened() {
    let w = world("hostile");
    let secret = w.d.join("secret.txt");
    std::fs::write(&secret, "TOP SECRET CONTENT").unwrap();
    let mut ws = w.e.snapshot().workspace;
    ws.tables.insert(
        w.orders.clone(),
        TableSetup {
            calc: vec![
                sql(
                    "Leak",
                    &format!("(SELECT content FROM read_text('{}'))", secret.display()),
                ),
                sql("Env", "getenv('HOME')"),
                sql("Bomb", "repeat('x', 1000000000)"),
                sql("Loop", "list_reduce(list_value(note, note), lambda x)"),
                sql("Fine", "amount * 2"),
            ],
            ..TableSetup::default()
        },
    );
    // The file is opened: nothing is run for the columns that are refused.
    w.e.replace(ws, Some(w.d.join("hostile.tenun")));
    let t = w.e.catalog().table(&w.orders).unwrap().clone();
    for bad in ["Leak", "Env", "Bomb", "Loop"] {
        let c = t.column(bad).unwrap_or_else(|| panic!("{bad}"));
        assert!(c.error.is_some(), "{bad}");
        assert!(
            values(&w.e, &w.orders, bad).iter().all(Value::is_null),
            "{bad}"
        );
    }
    assert!(t.column("Fine").is_some_and(|c| c.error.is_none()));
    let text = serde_json::to_string(&w.e.preview(&w.orders, 5).unwrap().rows).unwrap();
    assert!(!text.contains("TOP SECRET"), "{text}");
}

#[test]
fn a_file_that_opens_with_too_many_or_too_long_columns_builds_what_it_may() {
    let w = world("open-limits");
    let mut cols: Vec<CalcColumn> = (0..60)
        .map(|i| calc(&format!("C{i}"), "[amount] + 1"))
        .collect();
    cols.push(calc("Long", &format!("[amount]{}", " + 1".repeat(5000))));
    let mut ws = w.e.snapshot().workspace;
    ws.tables.insert(
        w.orders.clone(),
        TableSetup {
            calc: cols,
            ..TableSetup::default()
        },
    );
    w.e.replace(ws, Some(w.d.join("many.tenun")));
    let t = w.e.catalog().table(&w.orders).unwrap().clone();
    assert!(t.column("C49").is_some_and(|c| c.error.is_none()));
    assert!(t.column("C50").is_none());
    assert!(t.column("Long").is_none());
    let said = t.notes.join(" ");
    assert!(
        said.contains("11 calculated columns after the first 50"),
        "{said}"
    );
    // The file keeps them all.
    assert_eq!(w.e.snapshot().workspace.tables[&w.orders].calc.len(), 61);
    // An oversized column alone says why.
    let mut ws = w.e.snapshot().workspace;
    ws.tables.get_mut(&w.orders).unwrap().calc =
        vec![calc("Long", &format!("[amount]{}", " + 1".repeat(5000)))];
    w.e.replace(ws, Some(w.d.join("long.tenun")));
    let t = w.e.catalog().table(&w.orders).unwrap().clone();
    assert!(t.column("Long").is_none());
    assert!(
        t.notes
            .iter()
            .any(|n| n.contains("Long") && n.contains("10000 characters")),
        "{:?}",
        t.notes
    );
}

#[test]
fn a_function_that_makes_a_big_result_is_refused_when_a_column_is_saved() {
    let w = world("bomb-save");
    for text in [
        "repeat(note, 1000000000)",
        "lpad(note, 1000000000, 'x')",
        "range(1000000000)",
        "list_reduce(list_value(note, note), lambda x)",
    ] {
        let r = refused(set(&w.e, &w.orders, vec![sql("Bomb", text)]));
        assert!(
            r.error.message.contains("not available"),
            "{text}: {}",
            r.error.message
        );
    }
}

#[test]
fn tenun_snapshot_goes_on_when_a_calculated_column_is_blank_and_says_so_in_public_words() {
    let w = world("cli");
    set(
        &w.e,
        &w.orders,
        vec![calc("Marked", r#"[amount] & "s3cr3t""#)],
    )
    .unwrap();
    w.e.set_dashboards(vec![json!({
        "id": "d1", "title": "D", "refresh": 0,
        "tiles": [{
            "id": "t1", "type": "kpi", "title": "", "x": 0, "y": 0, "w": 3, "h": 2,
            "measures": [{"table": w.orders, "column": null, "agg": "count"}],
            "dims": []
        }]
    })]);
    let file = w.d.join("cli.tenun");
    w.e.save(Some(file.clone())).unwrap();
    // The file loses the column the formula reads.
    std::fs::write(
        w.d.join("orders.csv"),
        "id,date,store_id,note\n1,2026-01-15,1,a\n",
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tenun"))
        .args(["snapshot", file.to_str().unwrap(), "--json"])
        .arg(w.d.join("snap.json"))
        .env("HOME", &w.d)
        .env("XDG_CONFIG_HOME", w.d.join("cfg"))
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(
        err.contains("orders: Marked could not be computed."),
        "{err}"
    );
    assert!(
        !err.contains("s3cr3t") && !err.contains("No column"),
        "{err}"
    );
    assert!(w.d.join("snap.json").is_file());
    // The same words in the engine, and no source error.
    let snap = w.e.snapshot();
    assert!(snap.errors.is_empty());
    w.e.load(&source_of(&w.e, &w.orders));
    assert_eq!(
        w.e.public_table_problems(),
        ["orders: Marked could not be computed."]
    );
}
