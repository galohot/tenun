//! Engine shaping on a small sales file: date parts, filled periods, top N with
//! "Other", sorting, several values and blank groups. The expected numbers are
//! worked out by hand from the rows below.

use serde_json::{Value, json};
use std::path::PathBuf;
use tenun_core::Engine;
use tenun_core::engine::QueryResult;
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile};
use tenun_core::workspace::{Source, SourceKind, Workspace};

const OTHER: &str = "\u{0}other";

/// Jan has two rows; Feb has none; Mar has two (one with a blank region); a
/// last row has no date at all. 2026-01-05 is a Monday.
const SALES: &str = "date,ts,product,region,amount,cost
2026-01-05,2026-01-05 09:30:00,Arabica,North,100,40
2026-01-20,2026-01-20 09:10:00,Robusta,South,50,20
2026-03-10,2026-03-10 14:00:00,Arabica,North,200,80
2026-03-15,2026-03-15 14:45:00,Liberica,,30,10
2026-04-01,2026-04-01 23:59:00,Excelsa,South,10,5
2026-05-02,2026-05-02 09:00:00,Arabica,North,300,100
2026-06-30,2026-06-30 12:00:00,Robusta,South,70,30
2026-06-30,2026-06-30 12:30:00,Geisha,North,20,8
2026-06-30,2026-06-30 08:00:00,Kopi,North,15,6
,,Arabica,North,5,2
";

fn engine(tag: &str) -> Engine {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("tenun-shape-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sales.csv"), SALES).unwrap();
    // 26 years of daily rows: too many periods to fill.
    std::fs::write(dir.join("long.csv"), "day,n\n2000-01-01,1\n2026-01-01,2\n").unwrap();
    let source = |id: &str, file: &str| Source {
        id: id.into(),
        name: file.into(),
        kind: SourceKind::Csv { path: file.into() },
    };
    let tile = |id: &str, ty: &str, measures: Value, dims: Value, extra: Value| {
        let mut t = json!({ "id": id, "type": ty, "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
                            "measures": measures, "dims": dims });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            t[k] = v;
        }
        t
    };
    let sum = json!([{ "table": "s1/sales", "column": "amount", "agg": "sum" }]);
    let ws = Workspace {
        name: "Shaping".into(),
        sources: vec![source("s1", "sales.csv"), source("s2", "long.csv")],
        dashboards: vec![json!({ "id": "d1", "title": "T", "refresh": 0, "tiles": [
            tile("by_product", "bar", sum.clone(),
                 json!([{ "table": "s1/sales", "column": "product" }]),
                 json!({ "top": { "n": 3 } })),
            tile("by_region", "bar", sum.clone(),
                 json!([{ "table": "s1/sales", "column": "region" }]), json!({})),
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

const AMOUNT: fn(&str) -> Value =
    |agg| json!([{ "table": "s1/sales", "column": "amount", "agg": agg }]);

fn product() -> Value {
    json!([{ "table": "s1/sales", "column": "product" }])
}

fn date(grain: &str) -> Value {
    json!([{ "table": "s1/sales", "column": "date", "grain": grain }])
}

/// `(first column as text, value column k)` of every row.
fn pairs(r: &QueryResult, k: usize) -> Vec<(String, Option<f64>)> {
    r.rows
        .iter()
        .map(|row| (snapshot::js_key(&row[0]), row[k].as_f64()))
        .collect()
}

#[test]
fn top_n_with_other_adds_up_to_the_total() {
    let e = engine("topsum");
    let all = run(&e, &tile("bar", AMOUNT("sum"), product(), json!({})));
    let total: f64 = all.rows.iter().map(|r| r[1].as_f64().unwrap()).sum();
    assert_eq!(total, 800.0);
    assert!(all.other.is_none() && all.other_of.is_none());

    let top = run(
        &e,
        &tile(
            "bar",
            AMOUNT("sum"),
            product(),
            json!({ "top": { "n": 3 } }),
        ),
    );
    assert_eq!(
        pairs(&top, 1),
        vec![
            ("Arabica".into(), Some(605.0)),
            ("Robusta".into(), Some(120.0)),
            ("Liberica".into(), Some(30.0)),
            (OTHER.into(), Some(45.0)), // Geisha 20 + Kopi 15 + Excelsa 10
        ]
    );
    assert_eq!(
        top.rows.iter().map(|r| r[1].as_f64().unwrap()).sum::<f64>(),
        total
    );
    assert_eq!(top.other, Some(true));
    assert_eq!(
        top.other_of,
        Some(vec![json!("Arabica"), json!("Robusta"), json!("Liberica")])
    );
    // The v1 JSON names.
    let js = serde_json::to_value(&top).unwrap();
    assert_eq!(js["other"], true);
    assert_eq!(js["otherOf"][0], "Arabica");
    assert!(serde_json::to_value(&all).unwrap().get("otherOf").is_none());
}

#[test]
fn top_n_ranks_and_totals_averages_correctly() {
    let e = engine("topavg");
    let r = run(
        &e,
        &tile(
            "bar",
            AMOUNT("avg"),
            product(),
            json!({ "top": { "n": 2 } }),
        ),
    );
    // Arabica (100+200+300+5)/4, Robusta (50+70)/2. Other is the average of the
    // remaining rows (30+10+20+15)/4, never a sum of averages.
    assert_eq!(
        pairs(&r, 1),
        vec![
            ("Arabica".into(), Some(151.25)),
            ("Robusta".into(), Some(60.0)),
            (OTHER.into(), Some(18.75)),
        ]
    );
}

#[test]
fn top_n_can_leave_the_rest_out() {
    let e = engine("noother");
    let r = run(
        &e,
        &tile(
            "bar",
            AMOUNT("sum"),
            product(),
            json!({ "top": { "n": 3, "other": false } }),
        ),
    );
    assert_eq!(r.rows.len(), 3);
    assert!(r.other.is_none());
    // Fewer groups than N changes nothing.
    let r = run(
        &e,
        &tile(
            "bar",
            AMOUNT("sum"),
            product(),
            json!({ "top": { "n": 6 } }),
        ),
    );
    assert_eq!(r.rows.len(), 6);
    assert!(r.other.is_none());
}

#[test]
fn top_n_keeps_the_split() {
    let e = engine("topsplit");
    let dims = json!([{ "table": "s1/sales", "column": "product" }, { "table": "s1/sales", "column": "region" }]);
    let r = run(
        &e,
        &tile("bar", AMOUNT("sum"), dims, json!({ "top": { "n": 2 } })),
    );
    // Other has one row per region of the groups it replaces: Liberica (blank), Excelsa (South), Geisha and Kopi (North).
    let other: Vec<_> = r.rows.iter().filter(|row| row[0] == OTHER).collect();
    let mut by_region: Vec<(String, f64)> = other
        .iter()
        .map(|row| (snapshot::js_key(&row[1]), row[2].as_f64().unwrap()))
        .collect();
    by_region.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        by_region,
        vec![
            ("\u{0}null".to_string(), 30.0),
            ("North".to_string(), 35.0),
            ("South".to_string(), 10.0)
        ]
    );
    assert_eq!(r.rows.last().unwrap()[0], OTHER);
    assert_eq!(
        r.rows
            .iter()
            .map(|row| row[2].as_f64().unwrap())
            .sum::<f64>(),
        800.0
    );
    // Rows of the kept groups keep their split.
    assert!(
        r.rows
            .iter()
            .any(|row| row[0] == "Robusta" && row[1] == "South" && row[2] == 120.0)
    );
}

#[test]
fn a_date_ignores_top_n() {
    let e = engine("topdate");
    let r = run(
        &e,
        &tile(
            "bar",
            AMOUNT("sum"),
            date("month"),
            json!({ "top": { "n": 2 } }),
        ),
    );
    assert!(r.other.is_none());
    assert!(r.rows.len() > 2);
}

#[test]
fn blank_values_stay_their_own_group() {
    let e = engine("blank");
    let region = json!([{ "table": "s1/sales", "column": "region" }]);
    let r = run(&e, &tile("bar", AMOUNT("sum"), region.clone(), json!({})));
    assert_eq!(
        pairs(&r, 1),
        vec![
            ("North".into(), Some(640.0)),
            ("South".into(), Some(130.0)),
            ("\u{0}null".into(), Some(30.0))
        ]
    );
    // A blank group inside "Other".
    let r = run(
        &e,
        &tile("bar", AMOUNT("sum"), region, json!({ "top": { "n": 2 } })),
    );
    assert_eq!(
        pairs(&r, 1).last().unwrap(),
        &(OTHER.to_string(), Some(30.0))
    );
    // A blank group among the top groups.
    let r = run(
        &e,
        &tile(
            "bar",
            AMOUNT("sum"),
            json!([{ "table": "s1/sales", "column": "region" }]),
            json!({ "sort": { "by": "value", "desc": false }, "top": { "n": 1 } }),
        ),
    );
    assert_eq!(pairs(&r, 1)[0], ("North".to_string(), Some(640.0)));
    assert_eq!(r.other_of, Some(vec![json!("North")]));
    // A blank date is its own period, after the real ones.
    let r = run(&e, &tile("line", AMOUNT("sum"), date("month"), json!({})));
    assert_eq!(
        pairs(&r, 1).last().unwrap(),
        &("\u{0}null".to_string(), Some(5.0))
    );
}

#[test]
fn clicks_on_other_and_on_blank_filter_every_other_tile() {
    let e = engine("cross");
    let snap = snapshot::build(&e, "d1", &[]).unwrap();
    // Other of products = Excelsa (South 10), Geisha and Kopi (North 20 + 15).
    let other = &snap["cross"]["by_product"][OTHER]["by_region"];
    let rows: Vec<(String, f64)> = other["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (snapshot::js_key(&r[0]), r[1].as_f64().unwrap()))
        .collect();
    assert_eq!(
        rows,
        vec![("North".to_string(), 35.0), ("South".to_string(), 10.0)]
    );
    // The blank region leaves Liberica (30) in the product tile; the others are out.
    let blank = &snap["cross"]["by_region"]["\u{0}null"]["by_product"];
    assert_eq!(blank["rows"].as_array().unwrap().len(), 1);
    assert_eq!(blank["rows"][0][0], "Liberica");
    assert_eq!(blank["rows"][0][1], 30.0);
}

#[test]
fn missing_periods_are_zero_for_sums_and_counts_and_a_gap_otherwise() {
    let e = engine("densify");
    let sum = run(&e, &tile("line", AMOUNT("sum"), date("month"), json!({})));
    let keys: Vec<String> = sum.rows.iter().map(|r| snapshot::js_key(&r[0])).collect();
    assert_eq!(
        keys,
        [
            "2026-01-01",
            "2026-02-01",
            "2026-03-01",
            "2026-04-01",
            "2026-05-01",
            "2026-06-01",
            "\u{0}null"
        ]
    );
    assert_eq!(sum.rows[1][1].as_f64(), Some(0.0));
    assert_eq!(sum.rows[2][1].as_f64(), Some(230.0));
    let count = run(
        &e,
        &tile(
            "bar",
            json!([{ "table": "s1/sales", "column": null, "agg": "count" }]),
            date("month"),
            json!({}),
        ),
    );
    assert_eq!(count.rows[1][1].as_f64(), Some(0.0));
    let avg = run(&e, &tile("area", AMOUNT("avg"), date("month"), json!({})));
    assert_eq!(avg.rows.len(), 7);
    assert!(avg.rows[1][1].is_null());
    // Tables and number tiles are left as they are.
    let table = run(&e, &tile("table", AMOUNT("sum"), date("month"), json!({})));
    assert_eq!(table.rows.len(), 6);
}

#[test]
fn a_split_fills_each_period_with_each_split_value() {
    let e = engine("densify-split");
    let dims = json!([{ "table": "s1/sales", "column": "date", "grain": "month" }, { "table": "s1/sales", "column": "region" }]);
    let r = run(&e, &tile("bar", AMOUNT("sum"), dims, json!({})));
    // 6 months × (North, South, blank) + the blank-date row.
    assert_eq!(r.rows.len(), 19);
    let get = |month: &str, region: &str| {
        r.rows
            .iter()
            .find(|row| snapshot::js_key(&row[0]) == month && snapshot::js_key(&row[1]) == region)
            .map(|row| row[2].as_f64())
    };
    assert_eq!(get("2026-02-01", "North"), Some(Some(0.0)));
    assert_eq!(get("2026-01-01", "\u{0}null"), Some(Some(0.0)));
    assert_eq!(get("2026-03-01", "North"), Some(Some(200.0)));
    assert_eq!(get("2026-03-01", "\u{0}null"), Some(Some(30.0)));
}

#[test]
fn a_filled_result_never_passes_5000_rows() {
    let e = engine("densify-cap");
    let dims = json!([{ "table": "s2/long", "column": "day", "grain": "day" }]);
    let m = json!([{ "table": "s2/long", "column": "n", "agg": "sum" }]);
    let r = run(&e, &tile("line", m, dims, json!({})));
    assert_eq!(r.rows.len(), 2); // 9,497 days would be filled
}

#[test]
fn a_chart_sorted_by_value_is_not_filled() {
    let e = engine("densify-sort");
    let r = run(
        &e,
        &tile(
            "bar",
            AMOUNT("sum"),
            date("month"),
            json!({ "sort": { "by": "value" } }),
        ),
    );
    assert_eq!(r.rows.len(), 6);
    let r = run(
        &e,
        &tile(
            "bar",
            AMOUNT("sum"),
            date("month"),
            json!({ "sort": { "by": "label", "desc": true } }),
        ),
    );
    assert_eq!(snapshot::js_key(&r.rows[0][0]), "2026-06-01");
    assert_eq!(r.rows[4][1].as_f64(), Some(0.0)); // February, fifth from the top
}

#[test]
fn date_parts_fill_their_whole_range() {
    let e = engine("parts");
    let month = run(
        &e,
        &tile("bar", AMOUNT("sum"), date("month_of_year"), json!({})),
    );
    assert_eq!(month.rows.len(), 13); // 12 months + the blank date
    let v = pairs(&month, 1);
    assert_eq!(v[0], ("1".into(), Some(150.0)));
    assert_eq!(v[1], ("2".into(), Some(0.0)));
    assert_eq!(v[2], ("3".into(), Some(230.0)));
    assert_eq!(v[11], ("12".into(), Some(0.0)));
    assert_eq!(month.columns[0].dtype, tenun_core::catalog::DType::Number);

    // Monday is 1.
    let wd = run(&e, &tile("bar", AMOUNT("sum"), date("weekday"), json!({})));
    let v = pairs(&wd, 1);
    assert_eq!(
        &v[..7],
        &[
            ("1".to_string(), Some(100.0)),
            ("2".to_string(), Some(355.0)),
            ("3".to_string(), Some(10.0)),
            ("4".to_string(), Some(0.0)),
            ("5".to_string(), Some(0.0)),
            ("6".to_string(), Some(300.0)),
            ("7".to_string(), Some(30.0)),
        ]
    );

    // Hour needs a datetime column.
    let hr = json!([{ "table": "s1/sales", "column": "ts", "grain": "hour" }]);
    let h = run(&e, &tile("bar", AMOUNT("sum"), hr, json!({})));
    assert_eq!(h.rows.len(), 25);
    let v = pairs(&h, 1);
    assert_eq!(v[8], ("8".into(), Some(15.0)));
    assert_eq!(v[9], ("9".into(), Some(450.0)));
    assert_eq!(v[10], ("10".into(), Some(0.0)));
    assert_eq!(v[12], ("12".into(), Some(90.0)));
    assert_eq!(v[14], ("14".into(), Some(230.0)));
    assert_eq!(v[23], ("23".into(), Some(10.0)));

    // A part is not an unfinished period, and a number tile shrugs it off.
    assert!(h.partial.is_none());
    let kpi = run(&e, &tile("kpi", AMOUNT("sum"), date("weekday"), json!({})));
    assert_eq!(kpi.kpi.unwrap().value, Some(800.0));
}

#[test]
fn a_cross_filter_on_a_date_part_uses_the_same_number() {
    let e = engine("parts-filter");
    let t = tile("bar", AMOUNT("sum"), product(), json!({}));
    let f = DashFilter {
        filter: serde_json::from_value(json!({
            "table": "s1/sales", "column": "date", "grain": "weekday", "op": "in", "values": [2]
        }))
        .unwrap(),
        from_tile: "other".into(),
    };
    let r = run_tile(&e, &t, &[f]).unwrap();
    assert_eq!(
        pairs(&r, 1),
        vec![
            ("Arabica".into(), Some(200.0)),
            ("Robusta".into(), Some(120.0)),
            ("Geisha".into(), Some(20.0)),
            ("Kopi".into(), Some(15.0)),
        ]
    );
}

#[test]
fn range_keeps_the_low_end_and_drops_the_high_end() {
    let e = engine("range");
    let q: SemanticQuery = serde_json::from_value(json!({
        "measures": AMOUNT("sum"),
        "filters": [{ "table": "s1/sales", "column": "amount", "op": "range", "values": [20, 100] }]
    }))
    .unwrap();
    // 50, 30, 20 and 70 count; 100 and 10 do not.
    assert_eq!(e.query(&q).unwrap().rows[0][0].as_f64(), Some(170.0));
}

#[test]
fn a_chart_takes_up_to_four_values_and_no_split() {
    let e = engine("many");
    let ms = json!([
        { "table": "s1/sales", "column": "amount", "agg": "sum" },
        { "table": "s1/sales", "column": "cost", "agg": "sum" },
    ]);
    let dims = json!([{ "table": "s1/sales", "column": "date", "grain": "month" }, { "table": "s1/sales", "column": "region" }]);
    let r = run(&e, &tile("line", ms, dims, json!({})));
    assert_eq!(r.columns.len(), 3);
    assert_eq!(r.columns[2].name, "cost");
    assert_eq!(r.rows.len(), 7);
    assert_eq!(r.rows[0][1].as_f64(), Some(150.0));
    assert_eq!(r.rows[0][2].as_f64(), Some(60.0));
    assert_eq!(r.rows[1][2].as_f64(), Some(0.0));

    let five = json!([
        { "table": "s1/sales", "column": "amount", "agg": "sum" },
        { "table": "s1/sales", "column": "cost", "agg": "sum" },
        { "table": "s1/sales", "column": "amount", "agg": "max" },
        { "table": "s1/sales", "column": "cost", "agg": "max" },
        { "table": "s1/sales", "column": "amount", "agg": "min" },
    ]);
    assert_eq!(
        run(&e, &tile("bar", five.clone(), product(), json!({})))
            .columns
            .len(),
        5
    );
    // A table keeps every value, and a number tile uses the first.
    assert_eq!(
        run(&e, &tile("table", five.clone(), product(), json!({})))
            .columns
            .len(),
        6
    );
    assert_eq!(
        run(&e, &tile("kpi", five, json!([]), json!({})))
            .kpi
            .unwrap()
            .value,
        Some(800.0)
    );
}

#[test]
fn sort_options_order_the_groups() {
    let e = engine("sort");
    let names = |extra: Value| -> Vec<String> {
        run(&e, &tile("bar", AMOUNT("sum"), product(), extra))
            .rows
            .iter()
            .map(|r| snapshot::js_key(&r[0]))
            .collect()
    };
    // No sort: biggest first.
    assert_eq!(
        names(json!({})),
        [
            "Arabica", "Robusta", "Liberica", "Geisha", "Kopi", "Excelsa"
        ]
    );
    assert_eq!(
        names(json!({ "sort": { "by": "value" } })),
        names(json!({}))
    );
    assert_eq!(
        names(json!({ "sort": { "by": "value", "desc": false } })),
        [
            "Excelsa", "Kopi", "Geisha", "Liberica", "Robusta", "Arabica"
        ]
    );
    assert_eq!(
        names(json!({ "sort": { "by": "label" } })),
        [
            "Arabica", "Excelsa", "Geisha", "Kopi", "Liberica", "Robusta"
        ]
    );
    assert_eq!(
        names(json!({ "sort": { "by": "label", "desc": true } })),
        [
            "Robusta", "Liberica", "Kopi", "Geisha", "Excelsa", "Arabica"
        ]
    );
    // The UI orders a manual sort; the engine keeps its default.
    assert_eq!(
        names(json!({ "sort": { "by": "manual", "order": ["Kopi"] } })),
        names(json!({}))
    );
    // Top N picks by value whatever the order, and Other stays last.
    let r = run(
        &e,
        &tile(
            "bar",
            AMOUNT("sum"),
            product(),
            json!({ "sort": { "by": "label", "desc": true }, "top": { "n": 2 } }),
        ),
    );
    assert_eq!(
        pairs(&r, 1).iter().map(|p| p.0.clone()).collect::<Vec<_>>(),
        ["Robusta", "Arabica", OTHER]
    );
}
