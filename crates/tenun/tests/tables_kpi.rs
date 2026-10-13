//! Number-tile pace, table and pivot totals, and static tiles in snapshots.
//! The expected numbers are worked out by hand from the rows below.

use serde_json::{Value, json};
use std::path::PathBuf;
use tenun_core::Engine;
use tenun_core::engine::QueryResult;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile, tile_query};
use tenun_core::workspace::{Source, SourceKind, Workspace};

// October has data up to the 14th: 14 of its 31 days.
const SALES: &str = "date,cat,channel,amount
2026-09-05,Coffee,Shop,100
2026-09-20,Tea,Online,50
2026-10-02,Coffee,Online,200
2026-10-03,Coffee,Shop,40
2026-10-08,Tea,Shop,30
2026-10-14,Snack,Online,10
";

// A finished month: data to the last day.
const DONE: &str = "date,amount
2026-08-01,5
2026-08-31,7
";

// 40 columns for a pivot: group G0 holds the even numbers 2..40 (420), G1 the odd ones (400).
fn wide() -> String {
    let mut s = String::from("grp,code,v\n");
    for i in 1..=40 {
        s.push_str(&format!("G{},c{i:02},{i}\n", i % 2));
    }
    s
}

fn engine(tag: &str) -> Engine {
    let dir: PathBuf = std::env::temp_dir().join(format!("tenun-tk-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sales.csv"), SALES).unwrap();
    std::fs::write(dir.join("done.csv"), DONE).unwrap();
    std::fs::write(dir.join("wide.csv"), wide()).unwrap();
    let source = |id: &str, file: &str| Source {
        id: id.into(),
        name: file.into(),
        kind: SourceKind::Csv { path: file.into() },
    };
    let ws = Workspace {
        name: "TK".into(),
        sources: vec![
            source("s1", "sales.csv"),
            source("s2", "done.csv"),
            source("s3", "wide.csv"),
        ],
        dashboards: vec![
            json!({ "id": "d2", "title": "W", "refresh": 0, "tiles": [
                { "id": "wp", "type": "pivot", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
                  "measures": [{ "table": "s3/wide", "column": "v", "agg": "sum" }],
                  "dims": [{ "table": "s3/wide", "column": "grp" }],
                  "cols": { "table": "s3/wide", "column": "code" } },
                { "id": "wb", "type": "bar", "title": "", "x": 6, "y": 0, "w": 6, "h": 4,
                  "measures": [{ "table": "s3/wide", "column": "v", "agg": "sum" }],
                  "dims": [{ "table": "s3/wide", "column": "grp" }] },
            ]}),
            json!({ "id": "d1", "title": "T", "refresh": 0, "tiles": [
                { "id": "k", "type": "kpi", "title": "", "x": 0, "y": 0, "w": 3, "h": 2,
                  "measures": [{ "table": "s1/sales", "column": "amount", "agg": "sum" }],
                  "dims": [{ "table": "s1/sales", "column": "date", "grain": "month" }] },
                { "id": "note", "type": "text", "title": "Notes", "x": 3, "y": 0, "w": 3, "h": 1,
                  "measures": [], "dims": [], "text": "# Hello\n\nSome **bold** text." },
                { "id": "pic", "type": "image", "title": "", "x": 6, "y": 0, "w": 3, "h": 2,
                  "measures": [], "dims": [], "image": { "data": "data:image/png;base64,AAAA", "alt": "Logo" } },
                { "id": "bycat", "type": "bar", "title": "", "x": 0, "y": 2, "w": 6, "h": 4,
                  "measures": [{ "table": "s1/sales", "column": "amount", "agg": "sum" }],
                  "dims": [{ "table": "s1/sales", "column": "cat" }] },
                { "id": "pv", "type": "pivot", "title": "", "x": 6, "y": 2, "w": 6, "h": 4,
                  "measures": [{ "table": "s1/sales", "column": "amount", "agg": "sum" }],
                  "dims": [{ "table": "s1/sales", "column": "cat" }],
                  "cols": { "table": "s1/sales", "column": "channel" } },
            ]}),
        ],
        ..Workspace::default()
    };
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    e.replace(ws, Some(dir.join("T.tenun")));
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    e
}

fn tile(ty: &str, extra: Value) -> Tile {
    let mut t = json!({
        "id": "t", "type": ty,
        "measures": [{ "table": "s1/sales", "column": "amount", "agg": "sum" }],
        "dims": [{ "table": "s1/sales", "column": "cat" }],
    });
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        t[k] = v;
    }
    serde_json::from_value(t).unwrap()
}

fn run(e: &Engine, t: &Tile) -> QueryResult {
    run_tile(e, t, &[]).unwrap()
}

fn channel() -> Value {
    json!({ "table": "s1/sales", "column": "channel" })
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

#[test]
fn number_tile_says_how_much_of_the_period_has_data() {
    let e = engine("elapsed");
    // October: data to the 14th, 31 days in the month.
    let r = run(
        &e,
        &tile(
            "kpi",
            json!({ "dims": [{ "table": "s1/sales", "column": "date", "grain": "month" }] }),
        ),
    );
    let kpi = r.kpi.unwrap();
    assert!(kpi.partial);
    assert!((kpi.elapsed.unwrap() - 14.0 / 31.0).abs() < 1e-9);
    assert_eq!(kpi.value, Some(280.0));

    // A finished month: 1.
    let r = run(
        &e,
        &tile(
            "kpi",
            json!({
                "measures": [{ "table": "s2/done", "column": "amount", "agg": "sum" }],
                "dims": [{ "table": "s2/done", "column": "date", "grain": "month" }]
            }),
        ),
    );
    let kpi = r.kpi.unwrap();
    assert!(!kpi.partial);
    assert_eq!(kpi.elapsed, Some(1.0));
    assert_eq!(kpi.value, Some(12.0));

    // "All data" has no period, so no `elapsed` key at all (the v1 JSON shape).
    let r = run(&e, &tile("kpi", json!({ "dims": [] })));
    assert_eq!(r.kpi.as_ref().unwrap().elapsed, None);
    let js = serde_json::to_value(&r).unwrap();
    assert!(js["kpi"].get("elapsed").is_none());
    assert!(js.get("totals").is_none());
}

#[test]
fn table_total_row_is_computed_not_summed() {
    let e = engine("tabletotal");
    let avg = json!([
        { "table": "s1/sales", "column": "amount", "agg": "sum" },
        { "table": "s1/sales", "column": "amount", "agg": "avg" },
    ]);
    // Off by default.
    assert!(
        run(&e, &tile("table", json!({ "measures": avg })))
            .totals
            .is_none()
    );
    let r = run(
        &e,
        &tile("table", json!({ "measures": avg, "totals": true })),
    );
    let t = r.totals.unwrap();
    // 430 over 6 rows. The average is 71.67, not the sum of the three group averages.
    assert_eq!(num(&t.grand[0]), 430.0);
    assert!((num(&t.grand[1]) - 430.0 / 6.0).abs() < 1e-9);
    assert!(t.rows.is_none() && t.cols.is_none());
    // The cells add up to the grand total for a sum.
    let col: f64 = r.rows.iter().map(|row| num(&row[1])).sum();
    assert_eq!(col, 430.0);
}

#[test]
fn pivot_totals_match_queries_run_by_hand() {
    let e = engine("pivot");
    let t = tile("pivot", json!({ "cols": channel() }));
    let q = tile_query(&t, &[]).unwrap();
    assert_eq!(q.dimensions.len(), 2);
    assert_eq!(q.limit, Some(5000));
    let r = run(&e, &t);
    // Coffee: Shop 140, Online 200 · Tea: Online 50, Shop 30 · Snack: Online 10.
    assert_eq!(r.rows.len(), 5);
    let cell = |cat: &str, ch: &str| {
        r.rows
            .iter()
            .find(|row| row[0] == cat && row[1] == ch)
            .map(|row| num(&row[2]))
    };
    assert_eq!(cell("Coffee", "Shop"), Some(140.0));
    assert_eq!(cell("Coffee", "Online"), Some(200.0));
    assert_eq!(cell("Snack", "Shop"), None);

    let totals = r.totals.unwrap();
    assert_eq!(num(&totals.grand[0]), 430.0);
    let rows = totals.rows.unwrap();
    let of = |k: &str| rows.iter().find(|row| row[0] == k).map(|row| num(&row[1]));
    assert_eq!(of("Coffee"), Some(340.0));
    assert_eq!(of("Tea"), Some(80.0));
    assert_eq!(of("Snack"), Some(10.0));
    let cols = totals.cols.unwrap();
    let col = |k: &str| cols.iter().find(|row| row[0] == k).map(|row| num(&row[1]));
    assert_eq!(col("Shop"), Some(170.0));
    assert_eq!(col("Online"), Some(260.0));
    // Both add up to the grand total, which is the same as the bar chart's total.
    assert_eq!(rows.iter().map(|r| num(&r[1])).sum::<f64>(), 430.0);
    assert_eq!(cols.iter().map(|r| num(&r[1])).sum::<f64>(), 430.0);

    // `totals: false` turns them all off.
    let r = run(
        &e,
        &tile("pivot", json!({ "cols": channel(), "totals": false })),
    );
    assert!(r.totals.is_none());
}

#[test]
fn pivot_with_several_values_and_a_filter() {
    let e = engine("pivot2");
    let two = json!([
        { "table": "s1/sales", "column": "amount", "agg": "sum" },
        { "table": "s1/sales", "column": null, "agg": "count" },
    ]);
    let t = tile("pivot", json!({ "cols": channel(), "measures": two }));
    let r = run(&e, &t);
    let totals = r.totals.unwrap();
    assert_eq!((num(&totals.grand[0]), num(&totals.grand[1])), (430.0, 6.0));
    // A cross-filter reaches the totals too: only Coffee.
    let f = DashFilter {
        filter: serde_json::from_value(json!({
            "table": "s1/sales", "column": "cat", "op": "in", "values": ["Coffee"]
        }))
        .unwrap(),
        from_tile: "other".into(),
        to: None,
    };
    let r = run_tile(&e, &t, &[f]).unwrap();
    let totals = r.totals.unwrap();
    assert_eq!((num(&totals.grand[0]), num(&totals.grand[1])), (340.0, 3.0));
}

#[test]
fn pivot_row_totals_follow_a_top_n() {
    let e = engine("pivottop");
    let r = run(
        &e,
        &tile("pivot", json!({ "cols": channel(), "top": { "n": 1 } })),
    );
    // Coffee stays; Tea and Snack fold into Other: 50 + 30 + 10 = 90.
    assert_eq!(r.other, Some(true));
    let totals = r.totals.unwrap();
    assert_eq!(num(&totals.grand[0]), 430.0);
    let rows = totals.rows.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0], "Coffee");
    assert_eq!(rows[1][0], "\u{0}other");
    assert_eq!(num(&rows[1][1]), 90.0);
}

#[test]
fn text_and_image_tiles_stay_in_the_snapshot() {
    let e = engine("static");
    for id in ["note", "pic"] {
        let t: Tile = serde_json::from_value(
            json!({ "id": id, "type": if id == "note" { "text" } else { "image" } }),
        )
        .unwrap();
        assert!(tile_query(&t, &[]).is_none());
    }
    let snap = snapshot::build(&e, "d1", &[]).unwrap();
    let ids: Vec<&str> = snap["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["k", "note", "pic", "bycat", "pv"]);
    // They carry their own content and have no result.
    assert_eq!(snap["tiles"][1]["text"], "# Hello\n\nSome **bold** text.");
    assert_eq!(snap["tiles"][2]["image"]["alt"], "Logo");
    assert!(snap["base"].get("note").is_none() && snap["base"].get("pic").is_none());
    assert!(snap["base"].get("pv").is_some());
    // Static tiles are not part of any cross-filter result.
    for per_value in snap["cross"].as_object().unwrap().values() {
        for per_tile in per_value.as_object().unwrap().values() {
            assert!(per_tile.get("note").is_none() && per_tile.get("pic").is_none());
        }
    }
    // A click on a column header of the pivot is stored under its own key.
    let pv = snap["cross"]["pv"].as_object().unwrap();
    assert!(pv.contains_key("Coffee"));
    assert!(pv.contains_key("\u{1}col:Shop"));
    assert!(pv.contains_key("\u{1}col:Online"));
    // Filtering the pivot by the Shop column leaves 170 in the bar chart.
    let bars = &snap["cross"]["pv"]["\u{1}col:Shop"]["bycat"]["rows"];
    let sum: f64 = bars.as_array().unwrap().iter().map(|r| num(&r[1])).sum();
    assert_eq!(sum, 170.0);
}

#[test]
fn a_wide_pivot_reads_its_first_30_columns_and_totals_all_of_them() {
    let e = engine("widepivot");
    let t: Tile = serde_json::from_value(json!({
        "id": "wp", "type": "pivot",
        "measures": [{ "table": "s3/wide", "column": "v", "agg": "sum" }],
        "dims": [{ "table": "s3/wide", "column": "grp" }],
        "cols": { "table": "s3/wide", "column": "code" },
    }))
    .unwrap();
    let r = run(&e, &t);
    // The cells cover the 30 biggest columns (c11 to c40). The totals cover all 40.
    let codes: std::collections::HashSet<&str> =
        r.rows.iter().map(|row| row[1].as_str().unwrap()).collect();
    assert_eq!(codes.len(), 30);
    assert!(codes.contains("c40") && codes.contains("c11") && !codes.contains("c10"));
    let totals = r.totals.unwrap();
    assert_eq!(totals.cols.unwrap().len(), 40);
    assert_eq!(num(&totals.grand[0]), 820.0);
    let rows = totals.rows.unwrap();
    let of = |g: &str| num(&rows.iter().find(|r| r[0] == g).unwrap()[1]);
    assert_eq!((of("G0"), of("G1")), (420.0, 400.0));

    // A column click in an exported file is stored for those 30 columns.
    let snap = snapshot::build(&e, "d2", &[]).unwrap();
    let pv = snap["cross"]["wp"].as_object().unwrap();
    assert!(pv.contains_key("\u{1}col:c40") && !pv.contains_key("\u{1}col:c01"));
}
