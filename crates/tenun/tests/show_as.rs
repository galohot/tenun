//! "Show value as", ratio measures, median and p90, and same period last year.
//! The expected numbers are worked out by hand from the rows below.

use serde_json::{Value, json};
use std::path::PathBuf;
use tenun_core::Engine;
use tenun_core::engine::QueryResult;
use tenun_core::query::{Param, SemanticQuery};
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile};
use tenun_core::workspace::{FieldRef, Relationship, Source, SourceKind, Workspace};

const OTHER: &str = "\u{0}other";

// Sums by month: 2025-01 10 · 02 50 · 03 90 · 2026-01 60 · 02 70 · 03 170. The total is 450.
// Tea 220 (North 170, South 50) · Coffee 230 (North 130, South 100).
const ORDERS: &str = "date,cat,region,amount,qty
2025-01-15,Tea,North,10,1
2025-02-10,Tea,North,20,2
2025-02-20,Coffee,South,30,3
2025-03-05,Coffee,North,40,4
2025-03-25,Tea,South,50,0
2026-01-10,Tea,North,60,1
2026-02-12,Coffee,South,70,7
2026-03-03,Tea,North,80,8
2026-03-28,Coffee,North,90,9
";

const PRODUCTS: &str = "cat,weight\nTea,2\nCoffee,5\n";

// A zero below the line: y is 5 over 0, z is 0 over 0.
const RATIO: &str = "g,a,b\nx,10,4\ny,5,0\nz,0,0\n";

// Mondays: 2025-01-06 and 2026-01-05 are 364 days apart.
const WEEKLY: &str = "d,v\n2025-01-06,1\n2025-01-13,2\n2026-01-05,10\n2026-01-12,20\n";

// 2024 is a leap year.
const LEAP: &str = "d,v\n2024-02-28,1\n2024-02-29,2\n2024-03-01,3\n2025-02-28,10\n2025-03-01,30\n";

fn engine(tag: &str) -> Engine {
    let dir: PathBuf = std::env::temp_dir().join(format!("tenun-sa-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (f, text) in [
        ("orders.csv", ORDERS),
        ("products.csv", PRODUCTS),
        ("ratio.csv", RATIO),
        ("weekly.csv", WEEKLY),
        ("leap.csv", LEAP),
    ] {
        std::fs::write(dir.join(f), text).unwrap();
    }
    let source = |id: &str, file: &str| Source {
        id: id.into(),
        name: file.into(),
        kind: SourceKind::Csv { path: file.into() },
    };
    let ws = Workspace {
        name: "ShowAs".into(),
        sources: vec![
            source("o", "orders.csv"),
            source("p", "products.csv"),
            source("r", "ratio.csv"),
            source("w", "weekly.csv"),
            source("l", "leap.csv"),
        ],
        dashboards: vec![json!({ "id": "d1", "title": "T", "refresh": 0, "tiles": [
            { "id": "a", "type": "line", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
              "measures": amount("sum"), "dims": month(),
              "compare": "prev_year", "showAs": ["running"] },
            { "id": "b", "type": "bar", "title": "", "x": 6, "y": 0, "w": 6, "h": 4,
              "measures": [{ "table": "o/orders", "column": "amount", "agg": "sum",
                             "per": { "table": "p/products", "column": "weight", "agg": "sum" } }],
              "dims": cat() }
        ]})],
        relationships: vec![Relationship {
            id: "rel1".into(),
            from: FieldRef {
                table: "o/orders".into(),
                column: "cat".into(),
            },
            to: FieldRef {
                table: "p/products".into(),
                column: "cat".into(),
            },
        }],
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

fn amount(agg: &str) -> Value {
    json!([{ "table": "o/orders", "column": "amount", "agg": agg }])
}

fn cat() -> Value {
    json!([{ "table": "o/orders", "column": "cat" }])
}

fn region() -> Value {
    json!({ "table": "o/orders", "column": "region" })
}

fn month() -> Value {
    json!([{ "table": "o/orders", "column": "date", "grain": "month" }])
}

fn by_month_and_region() -> Value {
    json!([{ "table": "o/orders", "column": "date", "grain": "month" }, region()])
}

fn show(how: &str) -> Value {
    json!({ "showAs": [how] })
}

/// `(first column, last column)` of every row.
fn pairs(r: &QueryResult) -> Vec<(String, Option<f64>)> {
    r.rows
        .iter()
        .map(|row| {
            (
                snapshot::js_key(&row[0]),
                row.last().and_then(Value::as_f64),
            )
        })
        .collect()
}

fn near(a: Option<f64>, b: f64) {
    let a = a.unwrap_or(f64::NAN);
    assert!((a - b).abs() < 1e-9, "{a} is not {b}");
}

fn value_of(r: &QueryResult, key: &str) -> Option<f64> {
    pairs(r)
        .into_iter()
        .find(|(k, _)| k == key)
        .unwrap_or_else(|| panic!("no row {key}"))
        .1
}

#[test]
fn percent_of_total_adds_up_to_one() {
    let e = engine("pcttotal");
    let r = run(&e, &tile("bar", amount("sum"), cat(), show("pct_total")));
    near(value_of(&r, "Tea"), 220.0 / 450.0);
    near(value_of(&r, "Coffee"), 230.0 / 450.0);
    assert_eq!(r.columns[1].name, "amount (% of total)");
    let js = serde_json::to_value(&r).unwrap();
    assert_eq!(js["columns"][1]["unit"], "percent");
    assert!(js["columns"][0].get("unit").is_none());
    let sum: f64 = pairs(&r).iter().map(|p| p.1.unwrap()).sum();
    near(Some(sum), 1.0);

    // With a top N the share is of the rows shown, "Other" included.
    let top = run(
        &e,
        &tile(
            "bar",
            amount("sum"),
            cat(),
            json!({ "showAs": ["pct_total"], "top": { "n": 1 } }),
        ),
    );
    assert_eq!(top.rows.len(), 2);
    near(value_of(&top, "Coffee"), 230.0 / 450.0);
    near(value_of(&top, OTHER), 220.0 / 450.0);
}

#[test]
fn percent_of_group_needs_a_split() {
    let e = engine("pctgroup");
    let dims = json!([{ "table": "o/orders", "column": "cat" }, region()]);
    let r = run(&e, &tile("bar", amount("sum"), dims, show("pct_group")));
    let share = |c: &str, g: &str| {
        r.rows
            .iter()
            .find(|row| row[0] == c && row[1] == g)
            .and_then(|row| row[2].as_f64())
    };
    near(share("Tea", "North"), 170.0 / 220.0);
    near(share("Tea", "South"), 50.0 / 220.0);
    near(share("Coffee", "North"), 130.0 / 230.0);
    near(share("Coffee", "South"), 100.0 / 230.0);
    // Without a split it is the share of the total.
    let plain = run(&e, &tile("bar", amount("sum"), cat(), show("pct_group")));
    near(value_of(&plain, "Tea"), 220.0 / 450.0);
    assert_eq!(plain.columns[1].name, "amount (% of total)");
}

#[test]
fn running_total_ends_at_the_total() {
    let e = engine("running");
    let r = run(&e, &tile("line", amount("sum"), month(), show("running")));
    assert_eq!(r.columns[1].name, "amount (running total)");
    let v = pairs(&r);
    assert_eq!(v.len(), 15); // 2025-01 to 2026-03, gaps filled
    near(v[0].1, 10.0);
    near(v[1].1, 60.0);
    near(v[2].1, 150.0);
    near(v[3].1, 150.0); // a quiet month keeps the level
    near(v.last().unwrap().1, 450.0);

    // The same total as the number tile.
    let kpi = run(&e, &tile("kpi", amount("sum"), json!([]), json!({})));
    near(kpi.kpi.unwrap().value, 450.0);

    // With a split every group runs on its own.
    let split = run(
        &e,
        &tile(
            "area",
            amount("sum"),
            by_month_and_region(),
            show("running"),
        ),
    );
    let last = |g: &str| {
        split
            .rows
            .iter()
            .rfind(|row| row[1] == g)
            .and_then(|row| row[2].as_f64())
    };
    near(last("North"), 300.0);
    near(last("South"), 150.0);
}

#[test]
fn year_to_date_starts_over_in_january() {
    let e = engine("ytd");
    let r = run(&e, &tile("line", amount("sum"), month(), show("ytd")));
    assert_eq!(r.columns[1].name, "amount (year to date)");
    near(value_of(&r, "2025-03-01"), 150.0);
    near(value_of(&r, "2025-12-01"), 150.0);
    near(value_of(&r, "2026-01-01"), 60.0); // reset
    near(value_of(&r, "2026-02-01"), 130.0);
    near(value_of(&r, "2026-03-01"), 300.0);
    // A category is not a date: the value stays as it is.
    let c = run(&e, &tile("bar", amount("sum"), cat(), show("ytd")));
    near(value_of(&c, "Tea"), 220.0);
    assert_eq!(c.columns[1].name, "amount");
}

#[test]
fn moving_average_waits_for_a_full_window() {
    let e = engine("moving");
    let r = run(&e, &tile("line", amount("sum"), month(), show("moving_3")));
    assert_eq!(r.columns[1].name, "amount (3-period average)");
    let v = pairs(&r);
    assert_eq!(v[0].1, None);
    assert_eq!(v[1].1, None);
    near(v[2].1, (10.0 + 50.0 + 90.0) / 3.0);
    near(v[3].1, (50.0 + 90.0 + 0.0) / 3.0);
    // 7 and 12 points: nothing before the 7th and the 12th month.
    let seven = pairs(&run(
        &e,
        &tile("line", amount("sum"), month(), show("moving_7")),
    ));
    assert!(seven[..6].iter().all(|p| p.1.is_none()) && seven[6].1.is_some());
    let twelve = pairs(&run(
        &e,
        &tile("line", amount("sum"), month(), show("moving_12")),
    ));
    assert!(twelve[..11].iter().all(|p| p.1.is_none()) && twelve[11].1.is_some());
}

#[test]
fn change_and_percent_change() {
    let e = engine("change");
    let r = run(&e, &tile("line", amount("sum"), month(), show("change")));
    let v = pairs(&r);
    assert_eq!(v[0].1, None);
    near(v[1].1, 40.0);
    near(v[2].1, 40.0);
    near(v[3].1, -90.0);

    let p = run(
        &e,
        &tile("line", amount("sum"), month(), show("pct_change")),
    );
    assert_eq!(p.columns[1].name, "amount (% change)");
    assert_eq!(
        serde_json::to_value(&p).unwrap()["columns"][1]["unit"],
        "percent"
    );
    let v = pairs(&p);
    assert_eq!(v[0].1, None);
    near(v[1].1, 4.0); // 10 to 50
    near(v[3].1, -1.0); // 90 to 0
    assert_eq!(v[4].1, None); // 0 to 0: no base to divide by
}

#[test]
fn rank_one_is_the_biggest() {
    let e = engine("rank");
    let r = run(&e, &tile("bar", amount("sum"), cat(), show("rank")));
    near(value_of(&r, "Coffee"), 1.0);
    near(value_of(&r, "Tea"), 2.0);
    assert_eq!(
        serde_json::to_value(&r).unwrap()["columns"][1]["unit"],
        "rank"
    );
    // With a split, each group has its own ranking.
    let split = run(
        &e,
        &tile(
            "bar",
            amount("sum"),
            json!([{ "table": "o/orders", "column": "cat" }, region()]),
            show("rank"),
        ),
    );
    let rank = |c: &str, g: &str| {
        split
            .rows
            .iter()
            .find(|row| row[0] == c && row[1] == g)
            .and_then(|row| row[2].as_f64())
    };
    // North: Tea 170 beats Coffee 130. South: Coffee 100 beats Tea 50.
    near(rank("Tea", "North"), 1.0);
    near(rank("Coffee", "North"), 2.0);
    near(rank("Coffee", "South"), 1.0);
    near(rank("Tea", "South"), 2.0);
}

#[test]
fn values_not_shown_another_way_are_untouched() {
    let e = engine("plain");
    let base = run(&e, &tile("bar", amount("sum"), cat(), json!({})));
    for extra in [
        json!({ "showAs": ["value"] }),
        json!({ "showAs": [null] }),
        json!({ "showAs": ["no_such_way"] }),
        json!({ "showAs": [] }),
    ] {
        let r = run(&e, &tile("bar", amount("sum"), cat(), extra));
        assert_eq!(r.rows, base.rows);
        assert_eq!(r.columns[1].name, "amount");
    }
    // A number tile ignores it.
    let k = run(
        &e,
        &tile("kpi", amount("sum"), json!([]), show("pct_total")),
    );
    near(k.kpi.unwrap().value, 450.0);
    // The second value of two is shown on its own.
    let two = json!([
        { "table": "o/orders", "column": "amount", "agg": "sum" },
        { "table": "o/orders", "column": "qty", "agg": "sum" }
    ]);
    let r = run(
        &e,
        &tile("bar", two, cat(), json!({ "showAs": [null, "pct_total"] })),
    );
    near(r.rows[0][1].as_f64(), 230.0); // Coffee amount
    near(r.rows[0][2].as_f64(), 23.0 / 35.0); // Coffee qty over all qty
    assert_eq!(r.columns[1].name, "amount");
    assert_eq!(r.columns[2].name, "qty (% of total)");
}

#[test]
fn a_ratio_divides_in_one_select() {
    let e = engine("ratio");
    let m = json!([{ "table": "r/ratio", "column": "a", "agg": "sum",
                    "per": { "table": "r/ratio", "column": "b", "agg": "sum" } }]);
    let g = json!([{ "table": "r/ratio", "column": "g" }]);
    let r = run(
        &e,
        &tile("table", m.clone(), g.clone(), json!({ "totals": true })),
    );
    assert_eq!(r.columns[1].name, "a per b");
    near(value_of(&r, "x"), 2.5);
    // A zero below the line is a gap, not an error and not infinity.
    assert_eq!(value_of(&r, "y"), None);
    assert_eq!(value_of(&r, "z"), None);
    // The total is the ratio of the totals (15 over 4), never a sum of ratios (2.5).
    near(r.totals.unwrap().grand[0].as_f64(), 15.0 / 4.0);

    // Both sides may come from reachable tables: amount of an order over the weight of its product.
    let across = json!([{ "table": "o/orders", "column": "amount", "agg": "sum",
                         "per": { "table": "p/products", "column": "weight", "agg": "sum" } }]);
    let r = run(&e, &tile("bar", across, cat(), json!({})));
    near(value_of(&r, "Tea"), 220.0 / 10.0); // 5 Tea orders of weight 2
    near(value_of(&r, "Coffee"), 230.0 / 20.0); // 4 Coffee orders of weight 5

    // A count of rows works as the bottom too.
    let rows = json!([{ "table": "o/orders", "column": "amount", "agg": "sum",
                       "per": { "table": "o/orders", "column": null, "agg": "count" } }]);
    let r = run(&e, &tile("bar", rows, cat(), json!({})));
    near(value_of(&r, "Tea"), 220.0 / 5.0);

    // A quiet month in a filled chart is a gap for a ratio, where a sum would be 0.
    let monthly = json!([{ "table": "o/orders", "column": "amount", "agg": "sum",
                          "per": { "table": "o/orders", "column": "qty", "agg": "sum" } }]);
    let r = run(&e, &tile("line", monthly, month(), json!({})));
    assert_eq!(pairs(&r)[3].1, None);
}

#[test]
fn median_and_p90_match_duckdb() {
    let e = engine("quantiles");
    let q =
        |agg: &str| run(&e, &tile("kpi", amount(agg), json!([]), json!({}))).rows[0][0].as_f64();
    // Straight from DuckDB over the same rows.
    let sql = SemanticQuery {
        measures: serde_json::from_value(amount("sum")).unwrap(),
        ..SemanticQuery::default()
    };
    let (rows, _) = e
        .query_custom(&sql, |sc| {
            let (x, _) = sc.expr("o/orders", "amount", None)?;
            Ok((
                format!(
                    "SELECT median({x}), quantile_cont({x}, 0.9), stddev_samp({x}), var_samp({x}){}",
                    sc.from_where
                ),
                Vec::<Param>::new(),
            ))
        })
        .unwrap();
    let direct = |i: usize| rows[0][i].as_f64();
    near(q("median"), 50.0);
    near(q("p90"), 82.0);
    near(q("median"), direct(0).unwrap());
    near(q("p90"), direct(1).unwrap());
    near(q("stdev"), direct(2).unwrap());
    near(q("variance"), direct(3).unwrap());
    near(q("variance"), 750.0);
    // Per group, and a text column is refused in words.
    let by_cat = run(&e, &tile("bar", amount("median"), cat(), json!({})));
    near(value_of(&by_cat, "Tea"), 50.0); // 10 20 50 60 80
    let text = json!([{ "table": "o/orders", "column": "cat", "agg": "median" }]);
    let err = run_tile(&e, &tile("kpi", text, json!([]), json!({})), &[]).unwrap_err();
    assert!(err.to_string().contains("cat cannot be"), "{err}");
}

fn date_filter(lo: &str, hi: &str) -> DashFilter {
    serde_json::from_value(json!({
        "table": "o/orders", "column": "date", "op": "between",
        "values": [lo, hi], "fromTile": "other"
    }))
    .unwrap()
}

#[test]
fn same_period_last_year_lines_up_the_months() {
    let e = engine("compare");
    let t = tile(
        "line",
        amount("sum"),
        month(),
        json!({ "compare": "prev_year" }),
    );
    let r = run(&e, &t);
    let c = r.compare.as_ref().unwrap();
    assert_eq!(c.label, "Same period last year");
    // Only the periods the chart shows: Jan to Mar 2026 hold the values of Jan to Mar 2025.
    let got: Vec<(String, f64)> = c
        .rows
        .iter()
        .map(|r| (snapshot::js_key(&r[0]), r[1].as_f64().unwrap()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("2026-01-01".into(), 10.0),
            ("2026-02-01".into(), 50.0),
            ("2026-03-01".into(), 90.0)
        ]
    );
    // The chart itself is what it was.
    assert_eq!(r.rows.len(), 15);

    // A date filter on the chart moves back by the same year.
    let f = date_filter("2026-01-01", "2026-03-31");
    let r = run_tile(&e, &t, &[f.clone()]).unwrap();
    assert_eq!(r.rows.len(), 3);
    let c = r.compare.unwrap();
    assert_eq!(c.rows.len(), 3);
    near(c.rows[2][1].as_f64(), 90.0);

    // The previous period is the month before.
    let prev = tile(
        "line",
        amount("sum"),
        month(),
        json!({ "compare": "prev_period" }),
    );
    let r = run_tile(&e, &prev, &[f]).unwrap();
    let c = r.compare.unwrap();
    assert_eq!(c.label, "Previous period");
    // 2026-02 sits over the month before it (2026-01, 60) and 2026-03 over 2026-02 (70). 2026-01
    // would sit over 2025-12, which has no data at all, so it has no point.
    assert_eq!(c.rows.len(), 2);
    assert_eq!(snapshot::js_key(&c.rows[0][0]), "2026-02-01");
    near(c.rows[0][1].as_f64(), 60.0);
    near(c.rows[1][1].as_f64(), 70.0);

    // A chart that cannot compare says nothing.
    assert!(
        run(
            &e,
            &tile(
                "line",
                amount("sum"),
                cat(),
                json!({ "compare": "prev_year" })
            )
        )
        .compare
        .is_none()
    );
    assert!(
        run(
            &e,
            &tile(
                "kpi",
                amount("sum"),
                month(),
                json!({ "compare": "prev_year" })
            )
        )
        .compare
        .is_none()
    );
}

#[test]
fn compare_follows_the_workspace_language() {
    let e = engine("compare-id");
    e.set_locale(tenun_core::workspace::Locale::Id);
    let t = tile(
        "bar",
        amount("sum"),
        month(),
        json!({ "compare": "prev_year" }),
    );
    assert_eq!(
        run(&e, &t).compare.unwrap().label,
        "Periode yang sama tahun lalu"
    );
}

#[test]
fn compare_weeks_meet_on_the_same_weekday() {
    let e = engine("compare-week");
    let dims = json!([{ "table": "w/weekly", "column": "d", "grain": "week" }]);
    let m = json!([{ "table": "w/weekly", "column": "v", "agg": "sum" }]);
    let r = run(&e, &tile("bar", m, dims, json!({ "compare": "prev_year" })));
    let c = r.compare.unwrap();
    // 2025-01-06 plus 364 days is 2026-01-05, a Monday as well.
    let got: Vec<(String, f64)> = c
        .rows
        .iter()
        .map(|r| (snapshot::js_key(&r[0]), r[1].as_f64().unwrap()))
        .collect();
    assert_eq!(
        got,
        vec![("2026-01-05".into(), 1.0), ("2026-01-12".into(), 2.0)]
    );
}

#[test]
fn compare_leap_day_has_no_partner() {
    let e = engine("compare-leap");
    let dims = json!([{ "table": "l/leap", "column": "d", "grain": "day" }]);
    let m = json!([{ "table": "l/leap", "column": "v", "agg": "sum" }]);
    let r = run(
        &e,
        &tile("line", m, dims, json!({ "compare": "prev_year" })),
    );
    let c = r.compare.unwrap();
    let got: Vec<(String, f64)> = c
        .rows
        .iter()
        .map(|r| (snapshot::js_key(&r[0]), r[1].as_f64().unwrap()))
        .collect();
    // 2024-02-29 has no 2025 twin and is dropped. It does not land on 2025-02-28, which keeps
    // the value of 2024-02-28.
    assert_eq!(
        got,
        vec![("2025-02-28".into(), 1.0), ("2025-03-01".into(), 3.0)]
    );
}

#[test]
fn compare_series_is_shown_the_same_way() {
    let e = engine("compare-running");
    let f = date_filter("2026-01-01", "2026-03-31");
    let t = tile(
        "line",
        amount("sum"),
        month(),
        json!({ "compare": "prev_year", "showAs": ["running"] }),
    );
    let r = run_tile(&e, &t, &[f]).unwrap();
    near(r.rows[2][1].as_f64(), 60.0 + 70.0 + 170.0);
    let c = r.compare.unwrap();
    // 10, 50, 90 of last year add up to 150.
    near(c.rows[2][1].as_f64(), 150.0);
}

#[test]
fn the_snapshot_carries_the_new_results() {
    let e = engine("snapshot");
    let snap = snapshot::build(&e, "d1", &[]).unwrap();
    assert_eq!(snap["version"], 2);
    assert_eq!(
        snap["base"]["a"]["compare"]["label"],
        "Same period last year"
    );
    assert_eq!(
        snap["base"]["a"]["columns"][1]["name"],
        "amount (running total)"
    );
    // The table behind the bottom of the ratio goes into the file too.
    let ids: Vec<&str> = snap["tables"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["id"].as_str())
        .collect();
    assert!(ids.contains(&"p/products"), "{ids:?}");
}
