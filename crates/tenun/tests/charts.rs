//! The data behind the bar-family charts: histogram bins, scatter rows and waterfall periods.
//! The expected numbers are worked out by hand from the rows below.

use serde_json::{Value, json};
use std::path::PathBuf;
use tenun_core::Engine;
use tenun_core::engine::QueryResult;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile, tile_query};
use tenun_core::workspace::{Source, SourceKind, Workspace};

const SALES: &str = "date,product,region,amount,cost
2026-01-05,Arabica,North,100,40
2026-01-20,Robusta,South,50,20
2026-03-10,Arabica,North,200,80
2026-03-15,Liberica,South,30,10
2026-04-01,Robusta,North,10,5
";

fn engine(tag: &str) -> Engine {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("tenun-charts-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sales.csv"), SALES).unwrap();
    // The numbers 0 to 99, the first half in group "lo", the second in group "hi".
    let mut nums = String::from("n,grp\n");
    for i in 0..100 {
        nums.push_str(&format!("{i},{}\n", if i < 50 { "lo" } else { "hi" }));
    }
    std::fs::write(dir.join("nums.csv"), nums).unwrap();
    let source = |id: &str, file: &str| Source {
        id: id.into(),
        name: file.into(),
        kind: SourceKind::Csv { path: file.into() },
    };
    let ws = Workspace {
        name: "Charts".into(),
        sources: vec![source("s1", "sales.csv"), source("s2", "nums.csv")],
        dashboards: vec![json!({ "id": "d1", "title": "T", "refresh": 0, "tiles": [
            { "id": "hist", "type": "histogram", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
              "measures": [{ "table": "s2/nums", "column": "n", "agg": "sum" }], "dims": [] },
            { "id": "grp", "type": "bar", "title": "", "x": 6, "y": 0, "w": 6, "h": 4,
              "measures": [{ "table": "s2/nums", "column": "n", "agg": "sum" }],
              "dims": [{ "table": "s2/nums", "column": "grp" }] },
        ]})],
        ..Workspace::default()
    };
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    e.replace(ws, Some(dir.join("T.tenun")));
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    e
}

fn tile(ty: &str, measures: Value, dims: Value, extra: Value) -> Tile {
    let mut t = json!({ "id": "t", "type": ty, "measures": measures, "dims": dims });
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        t[k] = v;
    }
    serde_json::from_value(t).unwrap()
}

fn run(e: &Engine, t: &Tile) -> QueryResult {
    run_tile(e, t, &[]).unwrap()
}

fn n_col() -> Value {
    json!([{ "table": "s2/nums", "column": "n", "agg": "sum" }])
}

fn counts(r: &QueryResult) -> Vec<(f64, f64)> {
    r.rows
        .iter()
        .map(|row| (row[0].as_f64().unwrap(), row[1].as_f64().unwrap()))
        .collect()
}

#[test]
fn histogram_picks_a_nice_width_and_counts_every_bin() {
    let e = engine("hist");
    let r = run(&e, &tile("histogram", n_col(), json!([]), json!({})));
    // 0 to 99, n = 100: q1 = 24.75 and q3 = 74.25, so 2 · 49.5 · 100^(−1/3) = 21.3. The width is
    // kept to 5 bins at least (99 / 5 = 19.8), and 19.8 rounds up to the nice number 20.
    let bins = r.bins.unwrap();
    assert_eq!((bins.start, bins.width), (0.0, 20.0));
    assert_eq!(
        counts(&r),
        vec![
            (0.0, 20.0),
            (20.0, 20.0),
            (40.0, 20.0),
            (60.0, 20.0),
            (80.0, 20.0)
        ]
    );
    assert_eq!(r.columns[0].role, "dimension");
    assert_eq!(r.columns[1].role, "measure");
    // The v1 JSON shape: `bins` appears only on histograms.
    let js = serde_json::to_value(&r).unwrap();
    assert_eq!(js["bins"]["width"], 20.0);
    let bar = run(
        &e,
        &tile(
            "bar",
            n_col(),
            json!([{ "table": "s2/nums", "column": "grp" }]),
            json!({}),
        ),
    );
    assert!(serde_json::to_value(&bar).unwrap().get("bins").is_none());
}

#[test]
fn histogram_width_can_be_set_and_empty_bins_stay() {
    let e = engine("hist-width");
    let r = run(
        &e,
        &tile(
            "histogram",
            n_col(),
            json!([]),
            json!({ "bins": { "width": 25 } }),
        ),
    );
    assert_eq!(r.bins.unwrap().width, 25.0);
    assert_eq!(
        counts(&r),
        vec![(0.0, 25.0), (25.0, 25.0), (50.0, 25.0), (75.0, 25.0)]
    );
    // A width of 30 starts at 0 and ends with a short bin: 90 to 99 holds 10 values.
    let r = run(
        &e,
        &tile(
            "histogram",
            n_col(),
            json!([]),
            json!({ "bins": { "width": 30 } }),
        ),
    );
    assert_eq!(
        counts(&r),
        vec![(0.0, 30.0), (30.0, 30.0), (60.0, 30.0), (90.0, 10.0)]
    );
    // Zero or a negative width means "auto".
    let r = run(
        &e,
        &tile(
            "histogram",
            n_col(),
            json!([]),
            json!({ "bins": { "width": 0 } }),
        ),
    );
    assert_eq!(r.bins.unwrap().width, 20.0);
    // A width that would make thousands of bars is made wider (200 bars at most).
    let r = run(
        &e,
        &tile(
            "histogram",
            n_col(),
            json!([]),
            json!({ "bins": { "width": 0.001 } }),
        ),
    );
    assert!(r.rows.len() <= 200);
}

#[test]
fn histogram_follows_filters_and_total_matches_the_rows() {
    let e = engine("hist-filter");
    let t = tile("histogram", n_col(), json!([]), json!({}));
    let f = DashFilter {
        filter: serde_json::from_value(json!({
            "table": "s2/nums", "column": "grp", "op": "in", "values": ["lo"]
        }))
        .unwrap(),
        from_tile: "other".into(),
        to: None,
    };
    // Values 0 to 49: 2 · 24.5 · 50^(−1/3) = 13.3, limited to 49 / 5 = 9.8, then rounded up to 10.
    let r = run_tile(&e, &t, &[f]).unwrap();
    assert_eq!(r.bins.unwrap().width, 10.0);
    assert_eq!(counts(&r).iter().map(|c| c.1).sum::<f64>(), 50.0);
    // A tile that sits behind its own filter does not filter itself.
    let own = DashFilter {
        filter: serde_json::from_value(json!({
            "table": "s2/nums", "column": "n", "op": "range", "values": [0, 20]
        }))
        .unwrap(),
        from_tile: "t".into(),
        to: None,
    };
    assert_eq!(run_tile(&e, &t, &[own]).unwrap().rows.len(), 5);
}

#[test]
fn histogram_says_what_is_wrong() {
    let e = engine("hist-bad");
    let text = tile(
        "histogram",
        json!([{ "table": "s2/nums", "column": "grp", "agg": "count" }]),
        json!([]),
        json!({}),
    );
    let err = run_tile(&e, &text, &[]).unwrap_err().to_string();
    assert!(err.contains("not a number column"), "{err}");
    let none = tile("histogram", json!([]), json!([]), json!({}));
    assert!(tile_query(&none, &[]).is_none());
}

#[test]
fn clicking_a_bin_filters_the_other_tiles_offline() {
    let e = engine("hist-cross");
    let snap = snapshot::build(&e, "d1", &[]).unwrap();
    // Bin 20 to 40 holds 20 to 39, all in "lo": the group tile then shows lo = 20 + … + 39 = 590.
    let rows = &snap["cross"]["hist"]["20"]["grp"]["rows"];
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0][0], "lo");
    assert_eq!(rows[0][1], 590.0);
    // The bin 40 to 60 holds 40 to 59: lo has 40 to 49 (445) and hi has 50 to 59 (545).
    let rows = &snap["cross"]["hist"]["40"]["grp"]["rows"];
    assert_eq!(rows.as_array().unwrap().len(), 2);
}

#[test]
fn scatter_asks_for_two_values_a_third_for_the_size_and_two_groups() {
    let e = engine("scatter");
    let amount = json!({ "table": "s1/sales", "column": "amount", "agg": "sum" });
    let cost = json!({ "table": "s1/sales", "column": "cost", "agg": "sum" });
    let by = json!([{ "table": "s1/sales", "column": "product" }, { "table": "s1/sales", "column": "region" }]);
    // Two values: across and up. A third (build 13) sets the size of the dot; a fourth is not asked for.
    let two = tile("scatter", json!([amount, cost]), by.clone(), json!({}));
    assert_eq!(tile_query(&two, &[]).unwrap().measures.len(), 2);
    let t = tile(
        "scatter",
        json!([amount, cost, amount, cost]),
        by,
        json!({}),
    );
    let q = tile_query(&t, &[]).unwrap();
    assert_eq!(
        (q.measures.len(), q.dimensions.len(), q.limit),
        (3, 2, Some(2000))
    );
    let r = run(&e, &t);
    // Columns: dot, colour, x, y, size. Arabica in the North sold 100 + 200 at a cost of 40 + 80.
    assert_eq!(r.columns.len(), 5);
    let arabica = r.rows.iter().find(|row| row[0] == "Arabica").unwrap();
    assert_eq!(
        (
            arabica[1].as_str(),
            arabica[2].as_f64(),
            arabica[3].as_f64()
        ),
        (Some("North"), Some(300.0), Some(120.0))
    );
    assert_eq!(r.rows.len(), 4);
}

#[test]
fn waterfall_by_month_fills_the_quiet_month() {
    let e = engine("waterfall");
    let amount = json!([{ "table": "s1/sales", "column": "amount", "agg": "sum" }]);
    let month = json!([{ "table": "s1/sales", "column": "date", "grain": "month" }]);
    let t = tile("waterfall", amount, month, json!({}));
    let r = run(&e, &t);
    let rows: Vec<(String, f64)> = r
        .rows
        .iter()
        .map(|x| (snapshot::js_key(&x[0]), x[1].as_f64().unwrap()))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("2026-01-01".into(), 150.0),
            ("2026-02-01".into(), 0.0),
            ("2026-03-01".into(), 230.0),
            ("2026-04-01".into(), 10.0),
        ]
    );
    // Only the first dimension counts for a waterfall.
    let two = tile(
        "waterfall",
        json!([{ "table": "s1/sales", "column": "amount", "agg": "sum" }, { "table": "s1/sales", "column": "cost", "agg": "sum" }]),
        json!([{ "table": "s1/sales", "column": "date", "grain": "month" }, { "table": "s1/sales", "column": "region" }]),
        json!({}),
    );
    let q = tile_query(&two, &[]).unwrap();
    assert_eq!((q.measures.len(), q.dimensions.len()), (1, 1));
}
