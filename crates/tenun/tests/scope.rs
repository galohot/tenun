//! Measures that set their own groups and filters, and related totals (v5 build 06, V5-D8): `TOTAL`, `UNFILTERED`, and
//! a total of a related table in a calculated column. Each test fails on the naive answer.
//!
//! The data, by hand. Stores: A Java, B Java, C Bali, D Bali. Orders: A 100 (Jan 10) and 50 (Feb 10), B 150 (Feb 12),
//! C 200 (Jan 15) and 80 (Feb 15); D has none. Targets: A 120, B 130, C 220, D 50.
//! Sales: A 150, B 150, C 280, D 0; Java 300, Bali 280; in all 580. Targets in all 520.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::workspace::{FieldRef, NamedMeasure, TableSetup, Workspace};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-scope-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Lab {
    e: Engine,
    stores: String,
    orders: String,
    targets: String,
}

fn lab(d: &Path) -> Lab {
    std::fs::write(
        d.join("stores.csv"),
        "store,region\nA,Java\nB,Java\nC,Bali\nD,Bali\n",
    )
    .unwrap();
    std::fs::write(
        d.join("orders.csv"),
        "date,store,amount\n2026-01-10,A,100\n2026-02-10,A,50\n2026-02-12,B,150\n2026-01-15,C,200\n2026-02-15,C,80\n",
    )
    .unwrap();
    std::fs::write(
        d.join("targets.csv"),
        "store,target\nA,120\nB,130\nC,220\nD,50\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.replace(
        Workspace {
            name: "Scope".into(),
            ..Workspace::default()
        },
        Some(d.join("Scope.tenun")),
    );
    let mut ids = Vec::new();
    for f in ["stores.csv", "orders.csv", "targets.csv"] {
        let id = e
            .add_source(AddSource::File {
                path: d.join(f).to_string_lossy().into_owned(),
            })
            .unwrap();
        ids.push(
            e.snapshot()
                .tables
                .iter()
                .find(|t| t.source_id == id)
                .unwrap()
                .id
                .clone(),
        );
    }
    let (stores, orders, targets) = (ids[0].clone(), ids[1].clone(), ids[2].clone());
    let field = |t: &str, c: &str| FieldRef {
        table: t.into(),
        column: c.into(),
    };
    for from in [&orders, &targets] {
        e.add_relationship(field(from, "store"), field(&stores, "store"))
            .unwrap();
    }
    Lab {
        e,
        stores,
        orders,
        targets,
    }
}

fn near(v: &Value, want: f64) -> bool {
    v.as_f64().is_some_and(|x| (x - want).abs() < 1e-9)
}

impl Lab {
    fn measure(&self, formula: &str) -> String {
        let mut list = self.e.measures();
        list.push(NamedMeasure {
            name: format!("m{}", list.len() + 1),
            formula: formula
                .replace("[O]", &format!("[{}]", self.orders))
                .replace("[S]", &format!("[{}]", self.stores))
                .replace("[T]", &format!("[{}]", self.targets)),
            ..NamedMeasure::default()
        });
        self.e.set_measures(list, false).unwrap();
        self.e.measures().last().unwrap().id.clone()
    }

    /// The rows of a question: its groups, then the measure.
    fn ask(&self, m: &str, dims: Value, filters: Value) -> Vec<Vec<Value>> {
        let q: SemanticQuery = serde_json::from_value(json!({
            "measures": [{ "named": m }], "dimensions": dims, "filters": filters,
            "sort": [{ "by": 0 }]
        }))
        .unwrap();
        self.e.query(&q).unwrap().rows
    }

    fn region_store(&self) -> Value {
        json!([{ "table": self.stores, "column": "region" }, { "table": self.stores, "column": "store" }])
    }
}

#[test]
fn a_share_of_its_region_divides_by_the_region_not_by_everything() {
    let l = lab(&dir("share"));
    let m = l.measure("SUM([O].[amount]) / TOTAL(SUM([O].[amount]), [S].[region])");
    let rows = l.ask(&m, l.region_store(), json!([]));
    let got: Vec<(String, f64)> = rows
        .iter()
        .map(|r| (r[1].as_str().unwrap().to_string(), r[2].as_f64().unwrap()))
        .collect();
    // Bali: C 280 of 280; Java: A 150 of 300, B 150 of 300. The naive answer divides by 580.
    assert_eq!(got.len(), 3, "{got:?}");
    for (store, want) in [("A", 0.5), ("B", 0.5), ("C", 1.0)] {
        let v = got.iter().find(|g| g.0 == store).unwrap().1;
        assert!((v - want).abs() < 1e-9, "{store}: {v}");
    }
    // A total over more of the data is the same for many groups: the measure is not added up.
    assert!(!l.e.measure_info()[&m].additive);
    // With no column, the total of everything the tile shows.
    let all = l.measure("SUM([O].[amount]) / TOTAL(SUM([O].[amount]))");
    let rows = l.ask(
        &all,
        json!([{ "table": l.stores, "column": "store" }]),
        json!([]),
    );
    assert!(near(&rows[0][1], 150.0 / 580.0), "{rows:?}");
    // A filter that keeps one region is as good as the group: Java's stores, each half of Java.
    let java = json!([{ "table": l.stores, "column": "region", "op": "in", "values": ["Java"] }]);
    let rows = l.ask(&m, json!([{ "table": l.stores, "column": "store" }]), java);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows.iter().all(|r| near(&r[1], 0.5)), "{rows:?}");
    // A column the tile is not grouped by: blank, said in words, never the grand total.
    let q: SemanticQuery = serde_json::from_value(json!({ "measures": [{ "named": m }],
        "dimensions": [{ "table": l.stores, "column": "store" }] }))
    .unwrap();
    let res = l.e.query(&q).unwrap();
    assert!(res.rows.iter().all(|r| r[1].is_null()), "{:?}", res.rows);
    assert!(
        res.parts
            .iter()
            .any(|p| p.note.contains("not grouped by it")),
        "{:?}",
        res.parts
    );
}

#[test]
fn unfiltered_drops_the_filters_it_names_or_all_of_them() {
    let l = lab(&dir("unfiltered"));
    let java_feb = json!([
        { "table": l.stores, "column": "region", "op": "in", "values": ["Java"] },
        { "table": l.orders, "column": "date", "op": "range", "values": ["2026-02-01", "2027-01-01"] }
    ]);
    let plain = l.measure("SUM([O].[amount])");
    let regions = l.measure("UNFILTERED(SUM([O].[amount]), [S].[region])");
    let all = l.measure("UNFILTERED(SUM([O].[amount]))");
    // Java in February: A 50 + B 150. All regions in February: + C 80. Everything: 580.
    assert!(near(
        &l.ask(&plain, json!([]), java_feb.clone())[0][0],
        200.0
    ));
    assert!(near(
        &l.ask(&regions, json!([]), java_feb.clone())[0][0],
        280.0
    ));
    assert!(near(&l.ask(&all, json!([]), java_feb.clone())[0][0], 580.0));
    // The tile's groups stay: by store, each store unfiltered, and only the stores the filters keep.
    let by_store = l.ask(
        &all,
        json!([{ "table": l.stores, "column": "store" }]),
        java_feb,
    );
    let stores: Vec<&str> = by_store.iter().map(|r| r[0].as_str().unwrap()).collect();
    assert_eq!(
        stores,
        ["A", "B"],
        "a store the filter takes away does not come back"
    );
    assert!(
        near(&by_store[0][1], 150.0) && near(&by_store[1][1], 150.0),
        "{by_store:?}"
    );
}

#[test]
fn a_total_of_another_table_counts_each_of_its_rows_once() {
    let l = lab(&dir("fanout"));
    let m = l.measure("SUM([O].[amount]) / TOTAL(SUM([T].[target]))");
    let rows = l.ask(
        &m,
        json!([{ "table": l.stores, "column": "region" }]),
        json!([]),
    );
    // Every target once: 520. Through the orders, A's and C's targets would count twice (860).
    let java = rows.iter().find(|r| r[0] == json!("Java")).unwrap();
    assert!(near(&java[1], 300.0 / 520.0), "{rows:?}");
    let t = l.measure("TOTAL(SUM([T].[target]))");
    // The same for every group: it does not add up across them, though a sum on its own would.
    assert!(!l.e.measure_info()[&t].additive);
    for r in l.ask(
        &t,
        json!([{ "table": l.stores, "column": "store" }]),
        json!([]),
    ) {
        assert!(near(&r[1], 520.0), "{r:?}");
    }
}

#[test]
fn a_column_adds_up_a_related_table_per_row() {
    let l = lab(&dir("related"));
    let setup: TableSetup = serde_json::from_value(json!({ "calc": [
        { "name": "sales", "formula": format!("SUM([{}].[amount])", l.orders) },
        { "name": "orders", "formula": format!("COUNTROWS([{}])", l.orders) },
        { "name": "average", "formula": format!("AVERAGE([{}].[amount])", l.orders) }
    ] }))
    .unwrap();
    l.e.set_table_setup(&l.stores, Some(setup)).unwrap();
    let q: SemanticQuery = serde_json::from_value(json!({
        "measures": [{ "table": l.stores, "column": "sales", "agg": "sum" },
                     { "table": l.stores, "column": "orders", "agg": "sum" },
                     { "table": l.stores, "column": "average", "agg": "max" }],
        "dimensions": [{ "table": l.stores, "column": "store" }], "sort": [{ "by": 0 }]
    }))
    .unwrap();
    let rows = l.e.query(&q).unwrap().rows;
    let want = [
        ("A", 150.0, 2.0, Some(75.0)),
        ("B", 150.0, 1.0, Some(150.0)),
        ("C", 280.0, 2.0, Some(140.0)),
        ("D", 0.0, 0.0, None),
    ];
    for (r, (store, sales, n, avg)) in rows.iter().zip(want) {
        assert_eq!(r[0], json!(store));
        assert!(near(&r[1], sales), "{store} sales {r:?}");
        assert!(near(&r[2], n), "{store} orders {r:?}");
        match avg {
            Some(a) => assert!(near(&r[3], a), "{store} average {r:?}"),
            None => assert!(
                r[3].is_null(),
                "a store with no orders has no average: {r:?}"
            ),
        }
    }
    // The column groups like any column: stores by their number of orders.
    let q: SemanticQuery = serde_json::from_value(json!({
        "measures": [{ "table": l.stores, "column": null, "agg": "count" }],
        "dimensions": [{ "table": l.stores, "column": "orders" }], "sort": [{ "by": 0 }]
    }))
    .unwrap();
    assert_eq!(l.e.query(&q).unwrap().rows.len(), 3, "0, 1 and 2 orders");
    // The one side cannot add up the table it points at; its own rows belong in a measure.
    let wrong: TableSetup = serde_json::from_value(json!({ "calc": [
        { "name": "x", "formula": format!("SUM([{}].[amount])", l.orders) }
    ] }))
    .unwrap();
    let err =
        l.e.set_table_setup(&l.targets, Some(wrong))
            .unwrap_err()
            .to_string();
    assert!(err.contains("points to"), "{err}");
}
