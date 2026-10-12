//! New tables from other tables (build 04, V4-D6): append, merge and group.
//!
//! Orders (id, store_id, region, amount, date): 1 store 1 Java 100 Jan; 2 store 1 Java 50 Jan; 3 store 2
//! Bali 200 Feb; 4 store 3 Java 30 Feb; 5 store 9 Bali 70 Feb. In all 450.
//! Stores (id, name, region, sqm): 1 Kopi A Java 100; 2 Kopi B Bali 50; 3 Kopi C Java 80; and store 3 again
//! as Kopi C2 with 999, a repeated key. Store 9 is not there.
//!
//! Every expected number below is worked out from those rows by hand.

use serde_json::{Value, json};
use std::path::PathBuf;
use tenun_core::Engine;
use tenun_core::engine::{AddSource, QueryResult};
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::workspace::{
    CalcColumn, DerivedSpec, FieldRef, GroupValue, Keep, SourceKind, TableSetup,
};

const ORDERS: &str = "id,store_id,region,amount,date\n1,1,Java,100,2026-01-05\n2,1,Java,50,2026-01-20\n\
    3,2,Bali,200,2026-02-03\n4,3,Java,30,2026-02-10\n5,9,Bali,70,2026-02-15\n";
const STORES: &str = "id,name,region,sqm\n1,Kopi A,Java,100\n2,Kopi B,Bali,50\n3,Kopi C,Java,80\n3,Kopi C2,Java,999\n";
/// The store as text (a value that is not a number makes the column text), and a region.
const TARGETS: &str = "store,region,target\n1,Java,500\n2,Bali,300\nx,Java,10\n";
const JAN: &str = "id,amount,note\n1,10,a\n2,20,b\n";
const FEB: &str = "id,amount,extra\n3,30,x\n4,n/a,y\n";

struct W {
    e: Engine,
    d: PathBuf,
}

fn world(tag: &str) -> W {
    let d = std::env::temp_dir().join(format!("tenun-derived-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    for (f, text) in [
        ("orders.csv", ORDERS),
        ("stores.csv", STORES),
        ("targets.csv", TARGETS),
        ("jan.csv", JAN),
        ("feb.csv", FEB),
    ] {
        std::fs::write(d.join(f), text).unwrap();
    }
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    for f in [
        "orders.csv",
        "stores.csv",
        "targets.csv",
        "jan.csv",
        "feb.csv",
    ] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    assert!(e.snapshot().errors.is_empty());
    W { e, d }
}

impl W {
    fn id(&self, name: &str) -> String {
        self.e
            .snapshot()
            .tables
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("no table {name}"))
            .id
            .clone()
    }

    fn add(&self, name: &str, spec: DerivedSpec) -> anyhow::Result<String> {
        self.e.add_source(AddSource::Derived {
            name: name.into(),
            spec,
        })
    }

    fn merge(&self, keep: Keep) -> DerivedSpec {
        DerivedSpec::Merge {
            left: self.id("orders"),
            right: self.id("stores"),
            on: vec![("store_id".into(), "id".into())],
            keep,
        }
    }

    fn q(&self, v: Value) -> QueryResult {
        let q: SemanticQuery = serde_json::from_value(v).unwrap();
        self.e.query(&q).unwrap()
    }

    fn sum(&self, table: &str, column: &str) -> f64 {
        let r = self.q(json!({ "measures": [{ "table": table, "column": column, "agg": "sum" }] }));
        r.rows[0][0].as_f64().unwrap()
    }

    /// `(group, value)` of every row, sorted by group, with a blank group as "".
    fn by(&self, table: &str, value: &str, agg: &str, group: &str) -> Vec<(String, f64)> {
        let r = self.q(json!({
            "measures": [{ "table": table, "column": value, "agg": agg }],
            "dimensions": [{ "table": table, "column": group }]
        }));
        let mut v: Vec<(String, f64)> = r
            .rows
            .iter()
            .map(|row| {
                (
                    row[0].as_str().map(str::to_string).unwrap_or_default(),
                    row[1].as_f64().unwrap(),
                )
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    fn columns(&self, table: &str) -> Vec<String> {
        let snap = self.e.snapshot();
        let t = snap.tables.iter().find(|t| t.id == table).unwrap();
        t.columns.iter().map(|c| c.name.clone()).collect()
    }

    fn notes(&self, table: &str) -> Vec<String> {
        let snap = self.e.snapshot();
        snap.tables
            .iter()
            .find(|t| t.id == table)
            .unwrap()
            .notes
            .clone()
    }

    fn rows(&self, table: &str) -> i64 {
        let snap = self.e.snapshot();
        snap.tables
            .iter()
            .find(|t| t.id == table)
            .unwrap()
            .rows
            .unwrap()
    }
}

fn pairs(v: &[(&str, f64)]) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = v.iter().map(|(k, n)| (k.to_string(), *n)).collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn group_value(name: &str, formula: &str) -> GroupValue {
    GroupValue {
        name: name.into(),
        formula: formula.into(),
    }
}

// ---- append ------------------------------------------------------------------------------------

#[test]
fn an_append_matches_columns_by_name_says_where_each_row_came_from_and_leaves_gaps_blank() {
    let w = world("append");
    let src = w
        .add(
            "Months",
            DerivedSpec::Append {
                tables: vec![w.id("jan"), w.id("feb")],
            },
        )
        .unwrap();
    let t = format!("{src}/Months");
    assert_eq!(w.columns(&t), ["source", "id", "amount", "note", "extra"]);
    assert_eq!(w.rows(&t), 4);
    // feb has "n/a" in amount, so the column is text; note is blank on feb's rows, extra on jan's.
    let snap = w.e.snapshot();
    let table = snap.tables.iter().find(|x| x.id == t).unwrap();
    let amount = table.columns.iter().find(|c| c.name == "amount").unwrap();
    assert_eq!(serde_json::to_value(amount.dtype).unwrap(), json!("text"));
    assert_eq!(
        w.by(&t, "id", "count", "source"),
        pairs(&[("jan", 2.0), ("feb", 2.0)])
    );
    assert_eq!(
        w.by(&t, "note", "count", "source"),
        pairs(&[("jan", 2.0), ("feb", 0.0)])
    );
    // The same shape twice: the numbers add up. Targets 500 + 300 + 10, twice.
    let again = w
        .add(
            "Targets twice",
            DerivedSpec::Append {
                tables: vec![w.id("targets"), w.id("targets")],
            },
        )
        .unwrap_err();
    assert_eq!(
        again.to_string(),
        "targets is picked twice. Pick each table once."
    );
    let one = w.add(
        "Too few",
        DerivedSpec::Append {
            tables: vec![w.id("jan")],
        },
    );
    assert_eq!(
        one.unwrap_err().to_string(),
        "Pick two or more tables to append."
    );
    // A table with a column called source keeps it, and the new column is "source 2".
    let src2 = w
        .add(
            "Both",
            DerivedSpec::Append {
                tables: vec![t.clone(), w.id("jan")],
            },
        )
        .unwrap();
    assert_eq!(w.columns(&format!("{src2}/Both"))[0], "source 2");
    assert_eq!(w.rows(&format!("{src2}/Both")), 6);
}

// ---- merge -------------------------------------------------------------------------------------

#[test]
fn a_merge_keeps_every_left_row_once_and_says_how_many_found_a_match() {
    let w = world("merge");
    let src = w.add("Orders with stores", w.merge(Keep::Left)).unwrap();
    let t = format!("{src}/Orders with stores");
    // The right key (id) goes: it is store_id again. The region of the stores clashes with the region
    // of the orders, so it is "stores region".
    assert_eq!(
        w.columns(&t),
        [
            "id",
            "store_id",
            "region",
            "amount",
            "date",
            "name",
            "stores region",
            "sqm"
        ]
    );
    // Store 3 is twice in stores; the order of store 3 is still one row, with the first of them.
    assert_eq!(w.rows(&t), 5);
    assert_eq!(w.sum(&t, "amount"), 450.0);
    assert_eq!(
        w.by(&t, "amount", "sum", "name"),
        pairs(&[
            ("Kopi A", 150.0),
            ("Kopi B", 200.0),
            ("Kopi C", 30.0),
            ("", 70.0)
        ])
    );
    assert_eq!(w.sum(&t, "sqm"), 100.0 + 100.0 + 50.0 + 80.0);
    assert_eq!(
        w.notes(&t),
        [
            "4 of 5 rows of orders found a match in stores.",
            "stores repeats 1 key. The first row of each is used, so no row of orders is counted twice."
        ]
    );
    // Only the rows that match: order 5 (store 9) goes.
    let src = w.add("Matched orders", w.merge(Keep::Inner)).unwrap();
    let t = format!("{src}/Matched orders");
    assert_eq!(w.rows(&t), 4);
    assert_eq!(w.sum(&t, "amount"), 380.0);
    assert_eq!(
        w.notes(&t)[0],
        "4 of 5 rows of orders found a match in stores. The other row is left out."
    );
}

#[test]
fn a_merge_on_two_keys_of_two_types_matches_as_text() {
    let w = world("two-keys");
    let spec = DerivedSpec::Merge {
        left: w.id("orders"),
        right: w.id("targets"),
        on: vec![
            ("store_id".into(), "store".into()),
            ("region".into(), "region".into()),
        ],
        keep: Keep::Left,
    };
    let src = w.add("Orders and targets", spec).unwrap();
    let t = format!("{src}/Orders and targets");
    // Both keys of targets go; target has no twin in orders.
    assert_eq!(
        w.columns(&t),
        ["id", "store_id", "region", "amount", "date", "target"]
    );
    // Orders 1 and 2 (1, Java) find 500; order 3 (2, Bali) finds 300; 4 (3, Java) and 5 (9, Bali) none.
    assert_eq!(w.sum(&t, "target"), 500.0 + 500.0 + 300.0);
    assert_eq!(
        w.notes(&t),
        ["3 of 5 rows of orders found a match in targets."]
    );
}

#[test]
fn a_merge_that_cannot_be_made_says_what_to_change() {
    let w = world("merge-words");
    let none = DerivedSpec::Merge {
        left: w.id("orders"),
        right: w.id("stores"),
        on: vec![],
        keep: Keep::Left,
    };
    assert_eq!(
        w.add("x", none).unwrap_err().to_string(),
        "Pick the columns that match: one from each table."
    );
    let bad = DerivedSpec::Merge {
        left: w.id("orders"),
        right: w.id("stores"),
        on: vec![("store_id".into(), "code".into())],
        keep: Keep::Left,
    };
    assert_eq!(
        w.add("x", bad).unwrap_err().to_string(),
        "stores has no column code."
    );
    let same = DerivedSpec::Merge {
        left: w.id("orders"),
        right: w.id("orders"),
        on: vec![("id".into(), "id".into())],
        keep: Keep::Left,
    };
    assert_eq!(
        w.add("x", same).unwrap_err().to_string(),
        "Pick two different tables to merge."
    );
    assert_eq!(
        w.add(" ", w.merge(Keep::Left)).unwrap_err().to_string(),
        "Give the new table a name."
    );
    // Nothing failed was kept.
    assert!(
        !w.e.snapshot()
            .workspace
            .sources
            .iter()
            .any(|s| matches!(s.kind, SourceKind::Derived { .. }))
    );
}

// ---- group -------------------------------------------------------------------------------------

#[test]
fn a_group_has_one_row_for_each_group_and_a_value_for_each_formula() {
    let w = world("group");
    let spec = DerivedSpec::Group {
        table: w.id("orders"),
        by: vec!["region".into()],
        values: vec![
            group_value("Total", "SUM([amount])"),
            group_value("Stores", "COUNTD([store_id])"),
            group_value("Big", "SUMIF([amount], [amount] > 60)"),
            group_value(
                "Share of big",
                "SUMIF([amount], [amount] > 60) / SUM([amount])",
            ),
        ],
    };
    let src = w.add("By region", spec).unwrap();
    let t = format!("{src}/By region");
    assert_eq!(
        w.columns(&t),
        ["region", "Total", "Stores", "Big", "Share of big"]
    );
    assert_eq!(w.rows(&t), 2);
    // Java: 100 + 50 + 30 = 180, stores 1 and 3, big 100. Bali: 200 + 70 = 270, stores 2 and 9, big 270.
    assert_eq!(
        w.by(&t, "Total", "sum", "region"),
        pairs(&[("Java", 180.0), ("Bali", 270.0)])
    );
    assert_eq!(
        w.by(&t, "Stores", "sum", "region"),
        pairs(&[("Java", 2.0), ("Bali", 2.0)])
    );
    assert_eq!(
        w.by(&t, "Big", "sum", "region"),
        pairs(&[("Java", 100.0), ("Bali", 270.0)])
    );
    let share = w.by(&t, "Share of big", "sum", "region");
    assert!((share[1].1 - 100.0 / 180.0).abs() < 1e-9, "{share:?}");
    assert!((share[0].1 - 1.0).abs() < 1e-9, "{share:?}");
    // No group column: one row, the whole.
    let all = w
        .add(
            "All orders",
            DerivedSpec::Group {
                table: w.id("orders"),
                by: vec![],
                values: vec![
                    group_value("Total", "SUM([amount])"),
                    group_value("Rows", "COUNTROWS([orders])"),
                ],
            },
        )
        .unwrap();
    let t = format!("{all}/All orders");
    assert_eq!(w.rows(&t), 1);
    assert_eq!(w.sum(&t, "Total"), 450.0);
    assert_eq!(w.sum(&t, "Rows"), 5.0);
    // Words for a value that is not an aggregate, a name used twice, a column that is not there.
    let bad = |values: Vec<GroupValue>, by: Vec<&str>| {
        w.add(
            "Bad",
            DerivedSpec::Group {
                table: w.id("orders"),
                by: by.into_iter().map(String::from).collect(),
                values,
            },
        )
        .unwrap_err()
        .to_string()
    };
    assert!(
        bad(vec![group_value("x", "[amount]")], vec!["region"]).starts_with("x: "),
        "a formula error names the value"
    );
    assert_eq!(
        bad(vec![group_value("region", "SUM([amount])")], vec!["region"]),
        "Two columns would be called region. Give each value its own name."
    );
    assert_eq!(bad(vec![], vec!["city"]), "orders has no column city.");
}

#[test]
fn a_group_of_a_merge_is_a_chain_and_a_ring_is_refused() {
    let w = world("chain");
    let m = w.add("Orders with stores", w.merge(Keep::Left)).unwrap();
    let merged = format!("{m}/Orders with stores");
    let g = w
        .add(
            "By store",
            DerivedSpec::Group {
                table: merged.clone(),
                by: vec!["name".into()],
                values: vec![group_value("Total", "SUM([amount])")],
            },
        )
        .unwrap();
    let by_store = format!("{g}/By store");
    assert_eq!(
        w.by(&by_store, "Total", "sum", "name"),
        pairs(&[
            ("Kopi A", 150.0),
            ("Kopi B", 200.0),
            ("Kopi C", 30.0),
            ("", 70.0)
        ])
    );
    // The merge may not read the group, which reads the merge.
    let ring = DerivedSpec::Merge {
        left: w.id("orders"),
        right: by_store.clone(),
        on: vec![("region".into(), "name".into())],
        keep: Keep::Left,
    };
    assert_eq!(
        w.e.set_derived(&m, ring).unwrap_err().to_string(),
        "Orders with stores would read itself. Pick other tables."
    );
    // A change that works: only the matches. The group follows.
    w.e.set_derived(&m, w.merge(Keep::Inner)).unwrap();
    assert_eq!(
        w.by(&by_store, "Total", "sum", "name"),
        pairs(&[("Kopi A", 150.0), ("Kopi B", 200.0), ("Kopi C", 30.0)])
    );
}

// ---- reloads, files and the rest of the app -----------------------------------------------------

#[test]
fn a_reload_of_an_input_rebuilds_the_new_tables_that_read_it() {
    let w = world("reload");
    let m = w.add("Orders with stores", w.merge(Keep::Left)).unwrap();
    let g = w
        .add(
            "By store",
            DerivedSpec::Group {
                table: format!("{m}/Orders with stores"),
                by: vec!["name".into()],
                values: vec![group_value("Total", "SUM([amount])")],
            },
        )
        .unwrap();
    // A new order of store 2, 100.
    std::fs::write(
        w.d.join("orders.csv"),
        format!("{ORDERS}6,2,Bali,100,2026-03-01\n"),
    )
    .unwrap();
    let orders_src = w.id("orders").split('/').next().unwrap().to_string();
    let ids = w.e.load(&orders_src);
    // The tables that loaded again: orders, the merge, the group.
    assert!(ids.contains(&format!("{m}/Orders with stores")), "{ids:?}");
    assert!(ids.contains(&format!("{g}/By store")), "{ids:?}");
    assert_eq!(
        w.by(&format!("{g}/By store"), "Total", "sum", "name"),
        pairs(&[
            ("Kopi A", 150.0),
            ("Kopi B", 300.0),
            ("Kopi C", 30.0),
            ("", 70.0)
        ])
    );
    assert_eq!(
        w.notes(&format!("{m}/Orders with stores"))[0],
        "5 of 6 rows of orders found a match in stores."
    );
    // A calculated column of the orders is in the merge as soon as it is made.
    let setup = TableSetup {
        calc: vec![CalcColumn {
            name: "double".into(),
            formula: "[amount] * 2".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    w.e.set_table_setup(&w.id("orders"), Some(setup)).unwrap();
    let merged = format!("{m}/Orders with stores");
    assert!(
        w.columns(&merged).contains(&"double".to_string()),
        "{:?}",
        w.columns(&merged)
    );
    assert_eq!(w.sum(&merged, "double"), 1100.0);
}

#[test]
fn a_missing_input_is_an_error_in_words_and_the_rest_loads() {
    let w = world("missing");
    let m = w.add("Orders with stores", w.merge(Keep::Left)).unwrap();
    let a = w
        .add(
            "Months",
            DerivedSpec::Append {
                tables: vec![w.id("jan"), w.id("feb")],
            },
        )
        .unwrap();
    let path = w.d.join("w.tenun");
    w.e.save(Some(path.clone())).unwrap();
    // The stores file goes; the workspace opens again.
    std::fs::remove_file(w.d.join("stores.csv")).unwrap();
    w.e.open(&path).unwrap();
    let snap = w.e.snapshot();
    assert_eq!(
        snap.errors.get(&m).map(String::as_str),
        Some(
            "The table stores is not there any more. Open the new table and pick its tables again."
        )
    );
    assert!(snap.errors.get(&a).is_none(), "{:?}", snap.errors);
    assert_eq!(w.rows(&format!("{a}/Months")), 4);
    assert_eq!(w.sum(&w.id("orders"), "amount"), 450.0);
    // Removing a source tells the new tables that read it.
    let jan_src = w.id("jan").split('/').next().unwrap().to_string();
    w.e.remove_source(&jan_src);
    let snap = w.e.snapshot();
    assert!(
        snap.errors.get(&a).is_some_and(|e| e.contains("jan")),
        "{:?}",
        snap.errors
    );
}

#[test]
fn a_file_with_new_tables_opens_in_the_order_they_read_each_other_and_takes_part_in_links() {
    let w = world("file");
    let m = w.add("Orders with stores", w.merge(Keep::Left)).unwrap();
    let g = w
        .add(
            "By region",
            DerivedSpec::Group {
                table: format!("{m}/Orders with stores"),
                by: vec!["region".into()],
                values: vec![group_value("Total", "SUM([amount])")],
            },
        )
        .unwrap();
    // A file written by another hand lists the group first.
    let path = w.d.join("w.tenun");
    w.e.save(Some(path.clone())).unwrap();
    let mut file: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let sources = file["sources"].as_array_mut().unwrap();
    let at = sources.iter().position(|s| s["id"] == json!(g)).unwrap();
    let group = sources.remove(at);
    sources.insert(0, group);
    assert_eq!(file["sources"][0]["kind"], json!("derived"));
    assert_eq!(file["sources"][0]["spec"]["op"], json!("group"));
    std::fs::write(&path, serde_json::to_string_pretty(&file).unwrap()).unwrap();
    w.e.open(&path).unwrap();
    assert!(
        w.e.snapshot().errors.is_empty(),
        "{:?}",
        w.e.snapshot().errors
    );
    let by_region = format!("{g}/By region");
    assert_eq!(
        w.by(&by_region, "Total", "sum", "region"),
        pairs(&[("Java", 180.0), ("Bali", 270.0)])
    );
    // A link from the merge to the group by region: the total of the region beside each order.
    w.e.add_relationship(
        FieldRef {
            table: format!("{m}/Orders with stores"),
            column: "region".into(),
        },
        FieldRef {
            table: by_region.clone(),
            column: "region".into(),
        },
    )
    .unwrap();
    let r = w.q(json!({
        "measures": [{ "table": format!("{m}/Orders with stores"), "column": "amount", "agg": "sum" }],
        "dimensions": [{ "table": by_region, "column": "Total" }]
    }));
    let mut got: Vec<(f64, f64)> = r
        .rows
        .iter()
        .map(|x| (x[0].as_f64().unwrap(), x[1].as_f64().unwrap()))
        .collect();
    got.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(got, [(180.0, 180.0), (270.0, 270.0)]);
}

#[test]
fn a_look_before_the_table_is_made_shows_columns_rows_and_notes() {
    let w = world("preview");
    let p = w.e.preview_derived(&w.merge(Keep::Left)).unwrap();
    let names: Vec<&str> = p.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "id",
            "store_id",
            "region",
            "amount",
            "date",
            "name",
            "stores region",
            "sqm"
        ]
    );
    assert_eq!(p.rows.len(), 5);
    assert_eq!(p.total, 5);
    assert_eq!(p.notes.len(), 2);
    // Nothing was added.
    assert!(
        !w.e.snapshot()
            .workspace
            .sources
            .iter()
            .any(|s| matches!(s.kind, SourceKind::Derived { .. }))
    );
    let bad = DerivedSpec::Append { tables: vec![] };
    assert!(w.e.preview_derived(&bad).is_err());
}

#[test]
fn a_group_value_may_read_a_related_table_and_follows_the_link() {
    let w = world("related");
    std::fs::write(w.d.join("shops.csv"), "id,sqm\n1,100\n2,50\n3,80\n").unwrap();
    w.e.add_source(AddSource::File {
        path: w.d.join("shops.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let link =
        w.e.add_relationship(
            FieldRef {
                table: w.id("orders"),
                column: "store_id".into(),
            },
            FieldRef {
                table: w.id("shops"),
                column: "id".into(),
            },
        )
        .unwrap();
    let g = w
        .add(
            "Space by region",
            DerivedSpec::Group {
                table: w.id("orders"),
                by: vec!["region".into()],
                values: vec![group_value("Space", "SUM([shops].[sqm])")],
            },
        )
        .unwrap();
    let t = format!("{g}/Space by region");
    // Each order reads the floor space of its shop. Java: orders 1 and 2 (shop 1, 100 each) and 4 (shop 3,
    // 80): 280. Bali: order 3 (shop 2, 50) and order 5 (shop 9, none): 50.
    assert_eq!(
        w.by(&t, "Space", "sum", "region"),
        pairs(&[("Java", 280.0), ("Bali", 50.0)])
    );
    // The rows come in the order of the groups.
    let r = w.q(json!({
        "measures": [{ "table": t, "column": "Space", "agg": "sum" }],
        "dimensions": [{ "table": t, "column": "region" }],
        "sort": []
    }));
    assert_eq!(r.rows.len(), 2);
    // Without the link the value cannot be read, and the table says so; with it again, it is back.
    w.e.remove_relationship(&link);
    let err = w.e.snapshot().errors.get(&g).cloned().unwrap_or_default();
    assert!(err.starts_with("Space: "), "{err}");
    w.e.add_relationship(
        FieldRef {
            table: w.id("orders"),
            column: "store_id".into(),
        },
        FieldRef {
            table: w.id("shops"),
            column: "id".into(),
        },
    )
    .unwrap();
    assert!(w.e.snapshot().errors.get(&g).is_none());
    assert_eq!(w.sum(&t, "Space"), 330.0);
}

#[test]
fn a_new_table_takes_a_calculated_column_and_any_name() {
    let w = world("calc-name");
    let name = "Toko \"x\"; DROP TABLE orders; --/by region";
    let g = w
        .add(
            name,
            DerivedSpec::Group {
                table: w.id("orders"),
                by: vec!["region".into()],
                values: vec![group_value("Total", "SUM([amount])")],
            },
        )
        .unwrap();
    let t = format!("{g}/{name}");
    assert_eq!(w.rows(&t), 2);
    // The orders are still there.
    assert_eq!(w.sum(&w.id("orders"), "amount"), 450.0);
    let setup = TableSetup {
        calc: vec![CalcColumn {
            name: "Thousands".into(),
            formula: "[Total] / 1000".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    w.e.set_table_setup(&t, Some(setup)).unwrap();
    assert!((w.sum(&t, "Thousands") - 0.45).abs() < 1e-9);
    // A reload of the orders keeps the column of the new table.
    let orders_src = w.id("orders").split('/').next().unwrap().to_string();
    w.e.load(&orders_src);
    assert!((w.sum(&t, "Thousands") - 0.45).abs() < 1e-9);
}
