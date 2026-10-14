//! Closing and opening values (v5 build 07, V5-D9): the rows on the last (first) date of each period that has any row,
//! by the table's main date. A store with no row on that day counts as nothing, and the tile says so.
//!
//! The data, by hand (kg of beans). Stores: A Java, B Java, C Bali.
//! Jan 30: A 10, B 20 · Jan 31: A 12, C 5 (B has no row) → January closes on Jan 31 at 17, opens on Jan 30 at 30.
//! Feb 1: A 11 · Feb 27: C 6 · Feb 28: A 14, B 22 (C has no row) → February closes on Feb 28 at 36, opens at 11.
//! Mar 10: A 9 · Mar 14: A 8, C 7 → March (data up to Mar 14) closes at 15, opens at 9.

use chrono::NaiveDate;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, Tile};
use tenun_core::workspace::{FieldRef, NamedMeasure, Workspace};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-closing-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Lab {
    e: Engine,
    stores: String,
    stock: String,
}

fn lab(d: &Path) -> Lab {
    std::fs::write(
        d.join("stores.csv"),
        "store,region\nA,Java\nB,Java\nC,Bali\n",
    )
    .unwrap();
    std::fs::write(
        d.join("stock.csv"),
        "date,store,kg\n2026-01-30,A,10\n2026-01-30,B,20\n2026-01-31,A,12\n2026-01-31,C,5\n\
         2026-02-01,A,11\n2026-02-27,C,6\n2026-02-28,A,14\n2026-02-28,B,22\n\
         2026-03-10,A,9\n2026-03-14,A,8\n2026-03-14,C,7\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.replace(
        Workspace {
            name: "Stock".into(),
            ..Workspace::default()
        },
        Some(d.join("Stock.tenun")),
    );
    let mut ids = Vec::new();
    for f in ["stores.csv", "stock.csv"] {
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
    let field = |t: &str, c: &str| FieldRef {
        table: t.into(),
        column: c.into(),
    };
    e.add_relationship(field(&ids[1], "store"), field(&ids[0], "store"))
        .unwrap();
    Lab {
        e,
        stores: ids[0].clone(),
        stock: ids[1].clone(),
    }
}

fn near(v: &Value, want: f64) -> bool {
    v.as_f64().is_some_and(|x| (x - want).abs() < 1e-9)
}

impl Lab {
    fn measure(&self, name: &str, formula: &str) -> String {
        let mut list = self.e.measures();
        list.push(NamedMeasure {
            name: name.into(),
            formula: formula.replace("[K]", &format!("[{}]", self.stock)),
            ..NamedMeasure::default()
        });
        self.e.set_measures(list, false).unwrap();
        self.e.measures().last().unwrap().id.clone()
    }

    fn ask(&self, measure: Value, dims: Value) -> Vec<Vec<Value>> {
        let q: SemanticQuery = serde_json::from_value(json!({
            "measures": [measure], "dimensions": dims, "sort": [{ "by": 0 }, { "by": 1 }]
        }))
        .unwrap();
        self.e.query(&q).unwrap().rows
    }

    fn month(&self) -> Value {
        json!({ "table": self.stock, "column": "date", "grain": "month" })
    }
}

#[test]
fn the_closing_value_of_a_month_is_its_last_day_with_data() {
    let l = lab(&dir("month"));
    let closing = l.measure("Closing stock", "CLOSING(SUM([K].[kg]))");
    let m = json!({ "named": closing });
    let by_month: Vec<f64> = l
        .ask(m.clone(), json!([l.month()]))
        .iter()
        .map(|r| r[1].as_f64().unwrap())
        .collect();
    // The naive sum would be 47, 53 and 24.
    assert_eq!(by_month, [17.0, 36.0, 15.0]);
    // Each store on its month's last day; a store with no row that day is blank, and the region is the sum of its
    // stores on the same day.
    let rows = l.ask(
        m.clone(),
        json!([l.month(), { "table": l.stores, "column": "store" }]),
    );
    let at = |month: &str, store: &str| {
        rows.iter()
            .find(|r| r[0].as_str().unwrap().starts_with(month) && r[1] == json!(store))
            .map(|r| r[2].clone())
    };
    assert!(near(&at("2026-01", "A").unwrap(), 12.0));
    assert!(
        at("2026-01", "B").unwrap().is_null(),
        "B has no row on Jan 31"
    );
    assert!(near(&at("2026-02", "B").unwrap(), 22.0));
    let java: Vec<f64> = l
        .ask(
            m.clone(),
            json!([l.month(), { "table": l.stores, "column": "region" }]),
        )
        .iter()
        .filter(|r| r[1] == json!("Java"))
        .map(|r| r[2].as_f64().unwrap())
        .collect();
    assert_eq!(java, [12.0, 36.0, 8.0]);
    // A year closes on its last day with data; with no time group, the last day of all.
    let year = json!({ "table": l.stock, "column": "date", "grain": "year" });
    assert!(near(&l.ask(m.clone(), json!([year]))[0][1], 15.0));
    assert!(near(&l.ask(m.clone(), json!([]))[0][0], 15.0));
    // Opening: the first day of each period.
    let opening = l.measure("Opening stock", "OPENING(SUM([K].[kg]))");
    let first: Vec<f64> = l
        .ask(json!({ "named": opening }), json!([l.month()]))
        .iter()
        .map(|r| r[1].as_f64().unwrap())
        .collect();
    assert_eq!(first, [30.0, 11.0, 9.0]);
    // The picker's closing value of a column is the same measure.
    let plain = json!({ "table": l.stock, "column": "kg", "agg": "closing" });
    let picked: Vec<f64> = l
        .ask(plain, json!([l.month()]))
        .iter()
        .map(|r| r[1].as_f64().unwrap())
        .collect();
    assert_eq!(picked, [17.0, 36.0, 15.0]);
    // A stock does not add up over time: no running total of it.
    assert!(!l.e.measure_info()[&closing].additive);
}

#[test]
fn a_number_tile_says_which_day_and_compares_with_the_same_day_before() {
    let l = lab(&dir("kpi"));
    let closing = l.measure("Closing stock", "CLOSING(SUM([K].[kg]))");
    l.e.set_today(Some(NaiveDate::from_ymd_opt(2026, 3, 20).unwrap()));
    let k: Tile = serde_json::from_value(json!({ "id": "k", "type": "kpi",
        "measures": [{ "named": closing }], "dims": [l.month()] }))
    .unwrap();
    let kpi = snapshot::run_tile(&l.e, &k, &[]).unwrap().kpi.unwrap();
    // March has data up to the 14th: the stock on Mar 14, against the stock on Feb 14 (the last row by then, Feb 1).
    assert_eq!(kpi.value, Some(15.0));
    assert_eq!(kpi.period, "On Mar 14, 2026");
    assert_eq!(kpi.versus.as_deref(), Some("Feb 14, 2026"));
    assert_eq!(kpi.prev, Some(11.0));
    // A finished month: its last day with data, against the month before's.
    let feb = json!([{ "table": l.stock, "column": "date", "op": "range",
        "values": ["2026-01-01", "2026-03-01"], "fromTile": "f" }]);
    let feb: Vec<snapshot::DashFilter> = serde_json::from_value(feb).unwrap();
    let kpi = snapshot::run_tile(&l.e, &k, &feb).unwrap().kpi.unwrap();
    assert_eq!(kpi.value, Some(36.0));
    assert_eq!(kpi.period, "On Feb 28, 2026");
    assert_eq!(kpi.versus.as_deref(), Some("Jan 31, 2026"));
    assert_eq!(kpi.prev, Some(17.0));
}

#[test]
fn a_store_with_no_row_on_the_last_day_counts_as_nothing_and_the_tile_says_so() {
    let l = lab(&dir("gap"));
    let closing = l.measure("Closing stock", "CLOSING(SUM([K].[kg]))");
    let t: Tile = serde_json::from_value(json!({ "id": "t", "type": "table",
        "measures": [{ "named": closing }],
        "dims": [l.month(), { "table": l.stores, "column": "store" }] }))
    .unwrap();
    let res = snapshot::run_tile(&l.e, &t, &[]).unwrap();
    let note = res
        .notes
        .iter()
        .find(|n| n.contains("no row on the last day"))
        .unwrap_or_else(|| panic!("{:?}", res.notes));
    assert!(note.contains("February 2026: 1 of 3"), "{note}");
    assert!(note.contains("January 2026: 1 of 3"), "{note}");
    // A table with no date cannot have a closing value: said in words.
    let mut list = l.e.measures();
    list.push(NamedMeasure {
        name: "Bad".into(),
        formula: format!("CLOSING(COUNTROWS([{}]))", l.stores),
        ..NamedMeasure::default()
    });
    l.e.set_measures(list, false).unwrap();
    let bad = l.e.measures().last().unwrap().id.clone();
    let q: SemanticQuery =
        serde_json::from_value(json!({ "measures": [{ "named": bad }], "dimensions": [] }))
            .unwrap();
    let err = l.e.query(&q).unwrap_err().to_string();
    assert!(err.contains("has no date"), "{err}");
}
