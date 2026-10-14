//! More in the tooltip (v5 build 10, V5-D12): up to three more values for each group of a chart, and the last 12
//! finished periods of its first value. Each equals the same question asked on its own.
//!
//! The data: two stores, A and B, one order a day from Mon Jun 1 to Sun Sep 20, 2026 (16 weeks); A sells 10 + the
//! week number, B sells 5. Then one order on Mon Sep 21 (a week that has just begun).

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, Tile};
use tenun_core::workspace::Workspace;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-tips-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn lab(d: &Path) -> (Engine, String) {
    let mut csv = String::from("date,store,amount\n");
    let start = chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
    for day in 0..112 {
        let date = start + chrono::Duration::days(day);
        let week = day / 7;
        csv.push_str(&format!("{date},A,{}\n{date},B,5\n", 10 + week));
    }
    csv.push_str("2026-09-21,A,100\n");
    std::fs::write(d.join("orders.csv"), csv).unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.replace(
        Workspace {
            name: "Tips".into(),
            ..Workspace::default()
        },
        Some(d.join("Tips.tenun")),
    );
    let id = e
        .add_source(AddSource::File {
            path: d.join("orders.csv").to_string_lossy().into_owned(),
        })
        .unwrap();
    let t = e
        .snapshot()
        .tables
        .iter()
        .find(|t| t.source_id == id)
        .unwrap()
        .id
        .clone();
    (e, t)
}

fn ask(e: &Engine, q: Value) -> Vec<Vec<Value>> {
    e.query(&serde_json::from_value::<SemanticQuery>(q).unwrap())
        .unwrap()
        .rows
}

#[test]
fn extra_values_and_the_trend_equal_their_own_questions() {
    let (e, t) = lab(&dir("tips"));
    let tile: Tile = serde_json::from_value(json!({ "id": "b", "type": "bar",
        "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": t, "column": "store" }],
        "tooltip": {
            "extra": [{ "table": t, "column": null, "agg": "count" }, { "table": t, "column": "amount", "agg": "avg" }],
            "trend": { "table": t, "column": "date", "grain": "week" }
        } }))
    .unwrap();
    let res = snapshot::run_tile(&e, &tile, &[]).unwrap();
    let tips = res.tips.expect("tips");
    // The extra values: the count of orders and the average, asked on their own.
    let own = ask(
        &e,
        json!({ "measures": [{ "table": t, "column": null, "agg": "count" }, { "table": t, "column": "amount", "agg": "avg" }],
                "dimensions": [{ "table": t, "column": "store" }], "sort": [{ "by": 0 }] }),
    );
    for r in &own {
        let tip = &tips[r[0].as_str().unwrap()];
        assert_eq!(tip.extra, r[1..].to_vec(), "{}", r[0]);
    }
    // The trend: the 12 finished weeks before the week of Sep 21, as a line of that store alone gives them.
    let line = ask(
        &e,
        json!({ "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
                "dimensions": [{ "table": t, "column": "date", "grain": "week" }],
                "filters": [{ "table": t, "column": "store", "op": "in", "values": ["A"] }],
                "sort": [{ "by": 0 }] }),
    );
    let finished: Vec<Option<f64>> = line[line.len() - 13..line.len() - 1]
        .iter()
        .map(|r| r[1].as_f64())
        .collect();
    assert_eq!(tips["A"].trend.as_deref(), Some(finished.as_slice()));
    // Week 5 to week 16 of A: 7 orders of 10 + the week each.
    assert_eq!(finished[0], Some(7.0 * 14.0));
    // A filter reaches the tips too: in August only, A had 31 orders.
    let aug: Vec<snapshot::DashFilter> =
        serde_json::from_value(json!([{ "table": t, "column": "date",
        "op": "range", "values": ["2026-08-01", "2026-09-01"], "fromTile": "f" }]))
        .unwrap();
    let res = snapshot::run_tile(&e, &tile, &aug).unwrap();
    assert_eq!(res.tips.unwrap()["A"].extra[0], json!(31.0));
}

#[test]
fn an_export_holds_the_tips() {
    let (e, t) = lab(&dir("export"));
    e.set_dashboards(vec![
        json!({ "id": "d1", "title": "T", "refresh": 0, "tiles": [
        { "id": "b", "type": "bar", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
          "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
          "dims": [{ "table": t, "column": "store" }],
          "tooltip": { "extra": [{ "table": t, "column": null, "agg": "count" }] } },
        { "id": "c", "type": "bar", "title": "", "x": 6, "y": 0, "w": 6, "h": 4,
          "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
          "dims": [{ "table": t, "column": "date", "grain": "month" }] }
    ] }),
    ]);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    assert_eq!(p["base"]["b"]["tips"]["B"]["extra"], json!([112.0]));
    // A click on a month of the other chart is worked out ahead with its tips: June is 30 days of B.
    let june = &p["cross"]["c"]["2026-06-01"]["b"]["tips"]["B"]["extra"];
    assert_eq!(june, &json!([30.0]));
}
