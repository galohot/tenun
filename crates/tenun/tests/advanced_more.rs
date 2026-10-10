//! The engine side of the second set of Advanced charts: bullet, compare two values, Sankey,
//! calendar, cohort, Gantt, bubble size, trend columns and expandable pivots. The expected numbers
//! are worked out by hand from the rows below.

use serde_json::{Value, json};
use std::path::PathBuf;
use tenun_core::Engine;
use tenun_core::engine::QueryResult;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile, tile_query};
use tenun_core::workspace::{Source, SourceKind, Workspace};

// Six customers. Jan: c1 c2 c3. Feb: c4 c5 join. Mar: c6 joins. Last month with data is March.
const ORDERS: &str = "customer,date,channel,category,status,amount
c1,2026-01-05,Web,Coffee,paid,100
c2,2026-01-10,Shop,Tea,paid,50
c3,2026-01-20,Web,Coffee,refunded,30
c1,2026-02-03,Web,Tea,paid,40
c4,2026-02-08,Shop,Coffee,paid,70
c5,2026-02-20,Web,Coffee,paid,10
c1,2026-03-04,Shop,Coffee,paid,60
c3,2026-03-09,Web,Tea,paid,20
c4,2026-03-15,Web,Coffee,paid,90
c6,2026-03-31,Shop,Tea,refunded,5
";

const REGIONS: &str = "region,actual,target
N,120,100
S,80,100
E,50,90
W,70,70
";

// Yearly figures. E has nothing in 2025. The data runs to the last day of 2026, so that year is over.
const YEARLY: &str = "region,year,amt
N,2024-06-01,80
N,2025-06-01,100
N,2026-12-31,130
S,2025-06-01,90
S,2026-12-31,50
E,2026-12-31,20
";

// The latest day is 2026-03-20; the window starts on 2025-04-01.
const DAYS: &str = "date,n
2025-02-10,9
2025-04-02,1
2025-06-15,2
2026-03-20,3
";

// The data runs to the last day of February 2026, so February is over. The twelve months run from
// March 2025. A third product has data only in March 2026, a month that has just begun.
const MONTHLY: &str = "product,date,qty
A,2025-01-10,1
A,2025-12-05,4
A,2026-02-02,6
B,2026-02-28,9
";

// The same with a month that is not over: the data stops on 4 March.
const MONTHLY_OPEN: &str = "product,date,qty
A,2025-12-05,4
A,2026-02-02,6
A,2026-03-04,3
";

const TASKS: &str = "task,start,end
Design,2026-01-05,2026-01-20
Build,2026-01-15,2026-03-01
Test,2026-03-01,2026-03-15
Ship,2026-03-20,2026-03-21
";

// Thirty channels in the first stage, two categories, one status: the stage folds.
fn many() -> String {
    let mut s = String::from("ch,cat,v\n");
    for i in 1..=30 {
        s.push_str(&format!(
            "ch{i:02},{},{}\n",
            if i % 2 == 0 { "x" } else { "y" },
            i
        ));
    }
    s
}

fn engine(tag: &str) -> Engine {
    let dir: PathBuf = std::env::temp_dir().join(format!("tenun-adm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (f, c) in [
        ("orders.csv", ORDERS.to_string()),
        ("regions.csv", REGIONS.to_string()),
        ("yearly.csv", YEARLY.to_string()),
        ("days.csv", DAYS.to_string()),
        ("monthly.csv", MONTHLY.to_string()),
        ("open.csv", MONTHLY_OPEN.to_string()),
        ("tasks.csv", TASKS.to_string()),
        ("many.csv", many()),
    ] {
        std::fs::write(dir.join(f), c).unwrap();
    }
    let source = |id: &str, file: &str| Source {
        id: id.into(),
        name: file.into(),
        kind: SourceKind::Csv { path: file.into() },
    };
    let ws = Workspace {
        name: "Adm".into(),
        sources: vec![
            source("s1", "orders.csv"),
            source("s2", "regions.csv"),
            source("s3", "yearly.csv"),
            source("s4", "days.csv"),
            source("s5", "monthly.csv"),
            source("s6", "tasks.csv"),
            source("s7", "many.csv"),
            source("s8", "open.csv"),
        ],
        dashboards: vec![json!({ "id": "d1", "title": "T", "refresh": 0, "tiles": [
            { "id": "sk", "type": "sankey", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
              "measures": [{ "table": "s1/orders", "column": "amount", "agg": "sum" }],
              "dims": [col("s1/orders", "channel"), col("s1/orders", "category"), col("s1/orders", "status")] },
            { "id": "ot", "type": "bar", "title": "", "x": 6, "y": 0, "w": 6, "h": 4,
              "measures": [{ "table": "s1/orders", "column": "amount", "agg": "sum" }],
              "dims": [col("s1/orders", "status")] },
        ]})],
        ..Workspace::default()
    };
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    e.replace(ws, Some(dir.join("T.tenun")));
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    e
}

fn col(table: &str, column: &str) -> Value {
    json!({ "table": table, "column": column })
}

fn by(table: &str, column: &str, grain: &str) -> Value {
    json!({ "table": table, "column": column, "grain": grain })
}

fn m(table: &str, column: &str, agg: &str) -> Value {
    json!({ "table": table, "column": column, "agg": agg })
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

fn text(v: &Value) -> &str {
    v.as_str().unwrap()
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn bullet_reads_the_value_and_its_target() {
    let e = engine("bullet");
    let t = tile(
        "bullet",
        json!([
            m("s2/regions", "actual", "sum"),
            m("s2/regions", "target", "sum")
        ]),
        json!([col("s2/regions", "region")]),
        json!({ "sort": { "by": "label" } }),
    );
    let r = run(&e, &t);
    assert_eq!(r.columns.len(), 3);
    let rows: Vec<(String, f64, f64)> = r
        .rows
        .iter()
        .map(|x| (text(&x[0]).into(), num(&x[1]), num(&x[2])))
        .collect();
    assert_eq!(
        rows,
        [
            ("E".into(), 50.0, 90.0),
            ("N".into(), 120.0, 100.0),
            ("S".into(), 80.0, 100.0),
            ("W".into(), 70.0, 70.0)
        ]
    );
    // The target in the tile alone: one value, no second column.
    let one = tile(
        "bullet",
        json!([m("s2/regions", "actual", "sum")]),
        json!([]),
        json!({}),
    );
    assert_eq!(tile_query(&one, &[]).is_some(), true);
    assert_eq!(run(&e, &one).rows[0].len(), 1);
}

#[test]
fn compare_two_values_orders_rows_by_the_gap() {
    let e = engine("dumbbell");
    let t = tile(
        "compare",
        json!([
            m("s2/regions", "target", "sum"),
            m("s2/regions", "actual", "sum")
        ]),
        json!([col("s2/regions", "region")]),
        json!({}),
    );
    let r = run(&e, &t);
    assert!(r.pair.is_none());
    // E 40, then N and S at 20 (in name order), then W at 0.
    let names: Vec<&str> = r.rows.iter().map(|x| text(&x[0])).collect();
    assert_eq!(names, ["E", "N", "S", "W"]);
    assert_eq!((num(&r.rows[0][1]), num(&r.rows[0][2])), (90.0, 50.0));
}

#[test]
fn compare_two_periods_takes_the_latest_two_by_default() {
    let e = engine("slope");
    let t = tile(
        "compare",
        json!([m("s3/yearly", "amt", "sum")]),
        json!([col("s3/yearly", "region"), by("s3/yearly", "year", "year")]),
        json!({}),
    );
    let r = run(&e, &t);
    let p = r.pair.as_ref().unwrap().periods.clone().unwrap();
    assert_eq!(p, [json!("2025-01-01"), json!("2026-01-01")]);
    // S falls by 40, N rises by 30, E has no 2025 value, so it has no gap and comes last.
    let rows: Vec<(&str, Option<f64>, Option<f64>)> = r
        .rows
        .iter()
        .map(|x| (text(&x[0]), x[1].as_f64(), x[2].as_f64()))
        .collect();
    assert_eq!(
        rows,
        [
            ("S", Some(90.0), Some(50.0)),
            ("N", Some(100.0), Some(130.0)),
            ("E", None, Some(20.0))
        ]
    );
    assert_eq!(r.columns.len(), 3);
    // The Inspector picks the periods: 2024 against 2025 has one region only.
    let t = tile(
        "compare",
        json!([m("s3/yearly", "amt", "sum")]),
        json!([col("s3/yearly", "region"), by("s3/yearly", "year", "year")]),
        json!({ "periods": ["2024-01-01", "2025-01-01"] }),
    );
    let r = run(&e, &t);
    assert_eq!(
        r.pair.as_ref().unwrap().periods.as_ref().unwrap()[0],
        json!("2024-01-01")
    );
    assert_eq!(text(&r.rows[0][0]), "N");
    assert_eq!((num(&r.rows[0][1]), num(&r.rows[0][2])), (80.0, 100.0));
}

#[test]
fn a_slope_chart_skips_the_period_that_is_not_over() {
    // The data stops on 4 March 2026. March is not over, so the default pair is December and February.
    let e = engine("slope-open");
    let t = tile(
        "compare",
        json!([m("s8/open", "qty", "sum")]),
        json!([col("s8/open", "product"), by("s8/open", "date", "month")]),
        json!({}),
    );
    let r = run(&e, &t);
    let p = r.pair.as_ref().unwrap().periods.clone().unwrap();
    assert_eq!(p, [json!("2025-12-01"), json!("2026-02-01")]);
    assert!(r.partial.is_none());
    // A pair the user picks keeps the open month and marks it.
    let t = tile(
        "compare",
        json!([m("s8/open", "qty", "sum")]),
        json!([col("s8/open", "product"), by("s8/open", "date", "month")]),
        json!({ "periods": ["2026-02-01", "2026-03-01"] }),
    );
    let r = run(&e, &t);
    let part = r.partial.as_ref().unwrap();
    assert_eq!(
        (part.key.as_str(), part.until.as_str()),
        ("2026-03-01", "2026-03-04")
    );
}

#[test]
fn sankey_links_add_up_between_stages() {
    let e = engine("sankey");
    let t = tile(
        "sankey",
        json!([m("s1/orders", "amount", "sum")]),
        json!([
            col("s1/orders", "channel"),
            col("s1/orders", "category"),
            col("s1/orders", "status")
        ]),
        json!({}),
    );
    let r = run(&e, &t);
    let link = |stage: u64, a: &str, b: &str| -> f64 {
        r.rows
            .iter()
            .find(|x| x[0].as_u64() == Some(stage) && text(&x[1]) == a && text(&x[2]) == b)
            .map(|x| num(&x[3]))
            .unwrap_or(-1.0)
    };
    assert_eq!(link(0, "Web", "Coffee"), 230.0);
    assert_eq!(link(0, "Web", "Tea"), 60.0);
    assert_eq!(link(0, "Shop", "Coffee"), 130.0);
    assert_eq!(link(0, "Shop", "Tea"), 55.0);
    assert_eq!(link(1, "Coffee", "paid"), 330.0);
    assert_eq!(link(1, "Coffee", "refunded"), 30.0);
    assert_eq!(link(1, "Tea", "paid"), 110.0);
    assert_eq!(link(1, "Tea", "refunded"), 5.0);
    // Each stage carries the same total.
    let stage = |s: u64| {
        r.rows
            .iter()
            .filter(|x| x[0].as_u64() == Some(s))
            .map(|x| num(&x[3]))
            .sum::<f64>()
    };
    near(stage(0), 475.0);
    near(stage(1), 475.0);
    assert!(r.other.is_none());
}

#[test]
fn sankey_folds_a_long_stage_into_other() {
    let e = engine("fold");
    let t = tile(
        "sankey",
        json!([m("s7/many", "v", "sum")]),
        json!([col("s7/many", "ch"), col("s7/many", "cat")]),
        json!({}),
    );
    let r = run(&e, &t);
    assert_eq!(r.other, Some(true));
    let firsts: std::collections::HashSet<String> =
        r.rows.iter().map(|x| x[1].to_string()).collect();
    // 24 channels stay, and "Other" makes 25.
    assert_eq!(firsts.len(), 25);
    let other: f64 = r
        .rows
        .iter()
        .filter(|x| text(&x[1]) == "\u{0}other")
        .map(|x| num(&x[3]))
        .sum();
    // The six smallest channels (ch01..ch06) fold: 1 + 2 + 3 + 4 + 5 + 6.
    near(other, 21.0);
    // Nothing is lost on the way.
    near(
        r.rows.iter().map(|x| num(&x[3])).sum::<f64>(),
        (1..=30).sum::<i32>() as f64,
    );
}

#[test]
fn sankey_nodes_filter_by_the_field_of_their_stage() {
    let e = engine("sankeyx");
    let snap = snapshot::build(&e, "d1", &[]).unwrap();
    let cross = &snap["cross"]["sk"];
    let web = cross
        .get(format!("{}0:Web", snapshot::STAGE_PREFIX))
        .expect("a node of stage 0");
    // Web is 290 of the status totals: paid 260 and refunded 30.
    let paid = web["ot"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == "paid")
        .unwrap();
    assert_eq!(paid[1], 260.0);
    // The same name in another stage is another filter.
    let refunded = cross
        .get(format!("{}2:refunded", snapshot::STAGE_PREFIX))
        .expect("a node of stage 2");
    assert_eq!(refunded["ot"]["rows"].as_array().unwrap().len(), 1);
}

#[test]
fn calendar_keeps_the_last_twelve_months_with_data() {
    let e = engine("cal");
    let t = tile(
        "calendar",
        json!([m("s4/days", "n", "sum")]),
        json!([by("s4/days", "date", "month")]),
        json!({}),
    );
    let q = tile_query(&t, &[]).unwrap();
    // Always by day, whatever grain the tile was saved with.
    assert_eq!(q.dimensions[0].grain, Some(tenun_core::query::Grain::Day));
    let r = run(&e, &t);
    assert_eq!(text(&r.rows[0][0]), "2025-04-02");
    assert_eq!(text(&r.rows.last().unwrap()[0]), "2026-03-20");
    // 2025-02-10 is older than the window. Days between the others are filled with 0.
    assert_eq!(r.rows.len(), 353);
    assert!(r.rows.iter().all(|x| text(&x[0]) != "2025-02-10"));
    let at = |d: &str| r.rows.iter().find(|x| text(&x[0]) == d).map(|x| num(&x[1]));
    assert_eq!(at("2025-06-15"), Some(2.0));
    assert_eq!(at("2025-07-01"), Some(0.0));
}

#[test]
fn cohort_share_of_each_cohort_that_is_active() {
    let e = engine("cohort");
    let t = tile(
        "cohort",
        json!([m("s1/orders", "customer", "count_distinct")]),
        json!([by("s1/orders", "date", "month")]),
        json!({}),
    );
    let r = run(&e, &t);
    assert_eq!(r.columns[2].unit, Some(tenun_core::query::Unit::Percent));
    let at = |c: &str, k: u64| -> Option<(f64, f64)> {
        r.rows
            .iter()
            .find(|x| text(&x[0]) == c && x[1].as_u64() == Some(k))
            .map(|x| (num(&x[2]), num(&x[3])))
    };
    // January: c1 c2 c3. February: c1 comes back. March: c1 and c3.
    assert_eq!(at("2026-01-01", 0), Some((1.0, 3.0)));
    let feb = at("2026-01-01", 1).unwrap();
    near(feb.0, 1.0 / 3.0);
    let mar = at("2026-01-01", 2).unwrap();
    near(mar.0, 2.0 / 3.0);
    // February joiners: c4 c5. In March only c4 buys.
    assert_eq!(at("2026-02-01", 0), Some((1.0, 2.0)));
    assert_eq!(at("2026-02-01", 1), Some((0.5, 2.0)));
    // March has one customer, c6, and no later period has data: there is nothing after offset 0.
    assert_eq!(at("2026-03-01", 0), Some((1.0, 1.0)));
    assert_eq!(at("2026-03-01", 1), None);
    assert_eq!(at("2026-01-01", 3), None);
    assert_eq!(r.rows.len(), 3 + 2 + 1);
}

#[test]
fn cohort_leaves_out_a_period_that_is_not_over() {
    let e = engine("cohortpart");
    let t = tile(
        "cohort",
        json!([m("s1/orders", "customer", "count_distinct")]),
        json!([by("s1/orders", "date", "month")]),
        json!({}),
    );
    // The data now ends on 2026-03-15, in the middle of March: March is not a period yet.
    let upto = DashFilter {
        filter: serde_json::from_value(json!({
            "table": "s1/orders", "column": "date", "op": "lte", "values": ["2026-03-15"]
        }))
        .unwrap(),
        from_tile: "other".into(),
    };
    let r = run_tile(&e, &t, &[upto]).unwrap();
    let cohorts: Vec<&str> = r.rows.iter().map(|x| text(&x[0])).collect();
    assert!(cohorts.iter().all(|c| *c != "2026-03-01"), "{cohorts:?}");
    // January has offsets 0 and 1 (not 2); February has offset 0 only.
    assert_eq!(r.rows.len(), 2 + 1);
    assert!(r.rows.iter().all(|x| x[1].as_u64().unwrap() <= 1));
}

#[test]
fn cohort_can_add_up_a_value_instead() {
    let e = engine("cohortsum");
    let t = tile(
        "cohort",
        json!([
            m("s1/orders", "customer", "count_distinct"),
            m("s1/orders", "amount", "sum")
        ]),
        json!([by("s1/orders", "date", "month")]),
        json!({}),
    );
    let r = run(&e, &t);
    assert_eq!(r.columns[2].unit, None);
    let at = |c: &str, k: u64| {
        r.rows
            .iter()
            .find(|x| text(&x[0]) == c && x[1].as_u64() == Some(k))
            .map(|x| num(&x[2]))
    };
    assert_eq!(at("2026-01-01", 0), Some(180.0));
    assert_eq!(at("2026-01-01", 1), Some(40.0));
    assert_eq!(at("2026-01-01", 2), Some(80.0));
    assert_eq!(at("2026-02-01", 0), Some(80.0));
    assert_eq!(at("2026-02-01", 1), Some(90.0));
    assert_eq!(at("2026-03-01", 0), Some(5.0));
}

#[test]
fn cohort_needs_a_date_and_an_entity() {
    let e = engine("cohorterr");
    let t = tile(
        "cohort",
        json!([m("s1/orders", "customer", "count_distinct")]),
        json!([col("s1/orders", "channel")]),
        json!({}),
    );
    assert!(run_tile(&e, &t, &[]).is_err());
    let t = tile(
        "cohort",
        json!([]),
        json!([by("s1/orders", "date", "month")]),
        json!({}),
    );
    assert!(run_tile(&e, &t, &[]).is_err());
}

#[test]
fn gantt_starts_with_the_task_that_starts_first() {
    let e = engine("gantt");
    let t = tile(
        "gantt",
        json!([m("s6/tasks", "start", "min"), m("s6/tasks", "end", "max")]),
        json!([col("s6/tasks", "task")]),
        json!({}),
    );
    let r = run(&e, &t);
    let rows: Vec<(&str, &str, &str)> = r
        .rows
        .iter()
        .map(|x| (text(&x[0]), text(&x[1]), text(&x[2])))
        .collect();
    assert_eq!(
        rows,
        [
            ("Design", "2026-01-05", "2026-01-20"),
            ("Build", "2026-01-15", "2026-03-01"),
            ("Test", "2026-03-01", "2026-03-15"),
            ("Ship", "2026-03-20", "2026-03-21"),
        ]
    );
    assert_eq!(tile_query(&t, &[]).unwrap().limit, Some(100));
}

#[test]
fn scatter_reads_a_third_value_for_the_size() {
    let e = engine("bubble");
    let t = tile(
        "scatter",
        json!([
            m("s2/regions", "actual", "sum"),
            m("s2/regions", "target", "sum"),
            m("s2/regions", "actual", "avg")
        ]),
        json!([col("s2/regions", "region")]),
        json!({}),
    );
    let r = run(&e, &t);
    assert_eq!(r.columns.len(), 4);
    assert_eq!(r.rows.len(), 4);
}

#[test]
fn a_trend_column_has_twelve_periods_for_each_row() {
    let e = engine("spark");
    let t = tile(
        "table",
        json!([m("s5/monthly", "qty", "sum")]),
        json!([col("s5/monthly", "product")]),
        json!({ "spark": by("s5/monthly", "date", "month") }),
    );
    let r = run(&e, &t);
    let sparks = r.sparks.as_ref().unwrap();
    assert_eq!(sparks.len(), 2);
    let want = |at: &[(usize, f64)]| -> Vec<Option<f64>> {
        (0..12)
            .map(|i| Some(at.iter().find(|(j, _)| *j == i).map_or(0.0, |(_, v)| *v)))
            .collect()
    };
    // March 2025 is the first period. The January 2025 row is older than that.
    assert_eq!(sparks["A"], want(&[(9, 4.0), (11, 6.0)]));
    assert_eq!(sparks["B"], want(&[(11, 9.0)]));
    // No trend column asked: no sparks.
    let plain = tile(
        "table",
        json!([m("s5/monthly", "qty", "sum")]),
        json!([col("s5/monthly", "product")]),
        json!({}),
    );
    assert!(run(&e, &plain).sparks.is_none());
}

#[test]
fn a_month_that_is_not_over_is_left_out_of_a_trend() {
    let e = engine("sparkopen");
    let t = tile(
        "table",
        json!([m("s8/open", "qty", "sum")]),
        json!([col("s8/open", "product")]),
        json!({ "spark": by("s8/open", "date", "month") }),
    );
    let line = run(&e, &t).sparks.unwrap()["A"].clone();
    // March has data up to the 4th only, so the twelve months end with February: 6 in the last place.
    assert_eq!(line.len(), 12);
    assert_eq!(line[11], Some(6.0));
    assert_eq!(line[9], Some(4.0));
}

#[test]
fn a_trend_of_averages_leaves_gaps_where_nothing_happened() {
    let e = engine("sparkavg");
    let t = tile(
        "table",
        json!([m("s5/monthly", "qty", "avg")]),
        json!([col("s5/monthly", "product")]),
        json!({ "spark": by("s5/monthly", "date", "month") }),
    );
    let line = run(&e, &t).sparks.unwrap()["B"].clone();
    assert_eq!(line.len(), 12);
    assert_eq!(line[11], Some(9.0));
    assert!(line[..11].iter().all(Option::is_none));
}

#[test]
fn an_expandable_pivot_has_a_subtotal_for_each_group() {
    let e = engine("expand");
    let t = tile(
        "pivot",
        json!([m("s1/orders", "amount", "sum")]),
        json!([col("s1/orders", "channel"), col("s1/orders", "category")]),
        json!({ "cols": col("s1/orders", "status"), "expand": true }),
    );
    let r = run(&e, &t);
    let tot = r.totals.as_ref().unwrap();
    // Row totals: two row values and a number for a row, one row value and a number for a group.
    let rows = tot.rows.as_ref().unwrap();
    let group = |c: &str| {
        rows.iter()
            .find(|x| x.len() == 2 && text(&x[0]) == c)
            .map(|x| num(&x[1]))
    };
    assert_eq!(group("Web"), Some(290.0));
    assert_eq!(group("Shop"), Some(185.0));
    let leaf = rows
        .iter()
        .find(|x| x.len() == 3 && text(&x[0]) == "Web" && text(&x[1]) == "Coffee")
        .unwrap();
    assert_eq!(num(&leaf[2]), 230.0);
    // The cells of a group, over each column.
    let cells = tot.groups.as_ref().unwrap();
    let cell = |c: &str, s: &str| {
        cells
            .iter()
            .find(|x| text(&x[0]) == c && text(&x[1]) == s)
            .map(|x| num(&x[2]))
    };
    assert_eq!(cell("Web", "paid"), Some(260.0));
    assert_eq!(cell("Web", "refunded"), Some(30.0));
    assert_eq!(cell("Shop", "paid"), Some(180.0));
    // Grand total unchanged.
    assert_eq!(num(&tot.grand[0]), 475.0);
    // Off by default: the same pivot without `expand` has no groups and no group totals.
    let plain = tile(
        "pivot",
        json!([m("s1/orders", "amount", "sum")]),
        json!([col("s1/orders", "channel"), col("s1/orders", "category")]),
        json!({ "cols": col("s1/orders", "status") }),
    );
    let p = run(&e, &plain);
    let ptot = p.totals.as_ref().unwrap();
    assert!(ptot.groups.is_none());
    assert!(ptot.rows.as_ref().unwrap().iter().all(|x| x.len() == 3));
}

#[test]
fn an_expandable_pivot_without_columns_still_has_group_totals() {
    let e = engine("expand2");
    let t = tile(
        "pivot",
        json!([m("s1/orders", "amount", "sum")]),
        json!([
            col("s1/orders", "channel"),
            col("s1/orders", "category"),
            col("s1/orders", "status")
        ]),
        json!({ "expand": true }),
    );
    let r = run(&e, &t);
    let rows = r.totals.as_ref().unwrap().rows.as_ref().unwrap();
    let find = |n: usize, a: &str| {
        rows.iter()
            .find(|x| x.len() == n + 1 && text(&x[0]) == a)
            .map(|x| num(&x[n]))
    };
    assert_eq!(find(1, "Web"), Some(290.0));
    // A group of two row fields: Web and Coffee.
    let wc = rows
        .iter()
        .find(|x| x.len() == 3 && text(&x[0]) == "Web" && text(&x[1]) == "Coffee")
        .unwrap();
    assert_eq!(num(&wc[2]), 230.0);
}

#[test]
fn old_tiles_ask_the_same_question() {
    // A scatter with two values still asks two; the third is only read when it is there.
    let t = tile(
        "scatter",
        json!([
            m("s2/regions", "actual", "sum"),
            m("s2/regions", "target", "sum")
        ]),
        json!([col("s2/regions", "region")]),
        json!({}),
    );
    assert_eq!(tile_query(&t, &[]).unwrap().measures.len(), 2);
}
