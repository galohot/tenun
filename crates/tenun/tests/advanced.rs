//! The engine side of the Advanced charts: box plots, indexed lines, small multiples, manual order
//! and heat map / funnel questions. The expected numbers are worked out by hand from the rows below.

use serde_json::{Value, json};
use std::path::PathBuf;
use tenun_core::Engine;
use tenun_core::engine::QueryResult;
use tenun_core::query::{Agg, Measure, SemanticQuery};
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile, tile_query};
use tenun_core::workspace::{Source, SourceKind, Workspace};

const SALES: &str = "date,product,amount,cost
2026-01-05,Arabica,100,40
2026-01-20,Robusta,50,20
2026-02-10,Arabica,200,60
2026-02-15,Robusta,30,10
2026-03-01,Arabica,300,120
2026-03-02,Liberica,5,5
";

// Group A: 1..9 and one far value (100). Group B: five close values.
const VALS: &str = "grp,v
A,1
A,2
A,3
A,4
A,5
A,6
A,7
A,8
A,9
A,100
B,10
B,11
B,12
B,13
B,14
";

// Fifteen regions, three products. Region Rnn sells 100 + nn of product a, and 10 of b and c.
fn regions() -> String {
    let mut s = String::from("region,product,qty\n");
    for i in 1..=15 {
        s.push_str(&format!(
            "R{i:02},a,{}\nR{i:02},b,10\nR{i:02},c,5\n",
            100 + i
        ));
    }
    s
}

fn engine(tag: &str) -> Engine {
    let dir: PathBuf = std::env::temp_dir().join(format!("tenun-adv-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sales.csv"), SALES).unwrap();
    std::fs::write(dir.join("vals.csv"), VALS).unwrap();
    std::fs::write(dir.join("regions.csv"), regions()).unwrap();
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
        name: "Adv".into(),
        sources: vec![
            source("s1", "sales.csv"),
            source("s2", "vals.csv"),
            source("s3", "regions.csv"),
            source("s4", "nums.csv"),
        ],
        dashboards: vec![json!({ "id": "d1", "title": "T", "refresh": 0, "tiles": [
            { "id": "bx", "type": "box", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
              "measures": [{ "table": "s2/vals", "column": "v", "agg": "sum" }],
              "dims": [{ "table": "s2/vals", "column": "grp" }] },
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

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

fn vals() -> Value {
    json!([{ "table": "s2/vals", "column": "v", "agg": "sum" }])
}

fn grp() -> Value {
    json!([{ "table": "s2/vals", "column": "grp" }])
}

fn amount(agg: &str) -> Value {
    json!({ "table": "s1/sales", "column": "amount", "agg": agg })
}

fn cost() -> Value {
    json!({ "table": "s1/sales", "column": "cost", "agg": "sum" })
}

fn month() -> Value {
    json!([{ "table": "s1/sales", "column": "date", "grain": "month" }])
}

#[test]
fn box_stats_match_duckdb_quantiles() {
    let e = engine("box");
    let res = run(&e, &tile("box", vals(), grp(), json!({})));
    assert_eq!(res.rows.len(), 2);
    // Columns: group, min, q1, median, q3, max, lower whisker, upper whisker, count.
    assert_eq!(res.columns.len(), 9);
    assert_eq!(res.columns[3].name, "Median");

    // The same numbers straight from DuckDB: one query for each group, filtered to that group.
    for (row, g) in res.rows.iter().zip(["A", "B"]) {
        assert_eq!(row[0], json!(g));
        let q = SemanticQuery {
            measures: vec![Measure {
                table: "s2/vals".into(),
                column: Some("v".into()),
                agg: Agg::Count,
                per: None,
                via: None,
                named: None,
            }],
            dimensions: vec![],
            filters: vec![
                serde_json::from_value(json!({
                    "table": "s2/vals", "column": "grp", "op": "in", "values": [g]
                }))
                .unwrap(),
            ],
            sort: vec![],
            limit: None,
        };
        let (direct, _) = e
            .query_custom(&q, |sc| {
                let (x, _) = sc.expr("s2/vals", "v", None)?;
                Ok((
                    format!(
                        "SELECT min({x}), quantile_cont({x}, 0.25), quantile_cont({x}, 0.5), \
                         quantile_cont({x}, 0.75), max({x}), count(*){}",
                        sc.from_where
                    ),
                    sc.params.clone(),
                ))
            })
            .unwrap();
        let d = &direct[0];
        for i in 0..5 {
            near(num(&row[1 + i]), num(&d[i]));
        }
        near(num(&row[8]), num(&d[5]));
    }
    // By hand: A has 1..9 and 100. Quartiles of ten values: q1 = 3.25, median = 5.5, q3 = 7.75.
    let a = &res.rows[0];
    near(num(&a[1]), 1.0);
    near(num(&a[2]), 3.25);
    near(num(&a[3]), 5.5);
    near(num(&a[4]), 7.75);
    near(num(&a[5]), 100.0);
    near(num(&a[8]), 10.0);
}

#[test]
fn box_whiskers_and_outliers() {
    let e = engine("whisk");
    let res = run(&e, &tile("box", vals(), grp(), json!({})));
    // Group A: IQR = 7.75 - 3.25 = 4.5, so the fences are -3.5 and 14.5. The whiskers reach 1 and 9
    // and 100 is the one outlier.
    let a = &res.rows[0];
    near(num(&a[6]), 1.0);
    near(num(&a[7]), 9.0);
    let dots = res.outliers.as_ref().unwrap();
    assert_eq!(dots, &vec![vec![json!("A"), json!(100.0)]]);
    // Group B has no outliers and its whiskers are its minimum and maximum.
    let b = &res.rows[1];
    near(num(&b[6]), 10.0);
    near(num(&b[7]), 14.0);
}

#[test]
fn box_without_a_group_is_one_box_and_says_what_is_wrong() {
    let e = engine("one");
    let res = run(&e, &tile("box", vals(), json!([]), json!({})));
    assert_eq!(res.rows.len(), 1);
    assert_eq!(res.rows[0][0], json!("All"));
    near(num(&res.rows[0][8]), 15.0);
    let t = tile(
        "box",
        json!([{ "table": "s2/vals", "column": "grp", "agg": "count" }]),
        json!([]),
        json!({}),
    );
    let err = run_tile(&e, &t, &[]).unwrap_err().to_string();
    assert!(err.contains("not a number column"), "{err}");
}

#[test]
fn box_follows_filters_and_orders_by_median() {
    let e = engine("order");
    let t = tile(
        "box",
        vals(),
        grp(),
        json!({ "sort": { "by": "value", "desc": true } }),
    );
    let res = run(&e, &t);
    // B has the higher median (12 against 5.5).
    assert_eq!(res.rows[0][0], json!("B"));
    let only_a = DashFilter {
        filter: serde_json::from_value(json!({
            "table": "s2/vals", "column": "grp", "op": "in", "values": ["A"]
        }))
        .unwrap(),
        from_tile: "other".into(),
    };
    let res = run_tile(&e, &t, &[only_a]).unwrap();
    assert_eq!(res.rows.len(), 1);
    assert_eq!(res.rows[0][0], json!("A"));
}

#[test]
fn indexed_lines_start_at_100() {
    let e = engine("idx");
    let t = tile(
        "line",
        json!([amount("sum"), cost()]),
        month(),
        json!({ "layout": "indexed" }),
    );
    let res = run(&e, &t);
    // Amount by month: 150, 230, 305. Cost: 60, 70, 125.
    let col = |c: usize| -> Vec<f64> { res.rows.iter().map(|r| num(&r[c])).collect() };
    let a = col(1);
    near(a[0], 100.0);
    near(a[1], 230.0 / 150.0 * 100.0);
    near(a[2], 305.0 / 150.0 * 100.0);
    let c = col(2);
    near(c[0], 100.0);
    near(c[2], 125.0 / 60.0 * 100.0);
    // The real values are kept for the tooltip.
    let real = res.real.as_ref().unwrap();
    near(num(&real[0][1]), 150.0);
    near(num(&real[2][2]), 125.0);
    // Without the layout nothing changes.
    let plain = run(
        &e,
        &tile("line", json!([amount("sum"), cost()]), month(), json!({})),
    );
    assert!(plain.real.is_none());
    near(num(&plain.rows[0][1]), 150.0);
}

#[test]
fn indexed_split_series_and_a_quiet_start() {
    let e = engine("idx2");
    // One value split by product: Liberica sells only in March (0 before it), so it starts there.
    let t = tile(
        "line",
        json!([amount("sum")]),
        json!([
            { "table": "s1/sales", "column": "date", "grain": "month" },
            { "table": "s1/sales", "column": "product" }
        ]),
        json!({ "layout": "indexed" }),
    );
    let res = run(&e, &t);
    let at = |month: &str, product: &str| {
        res.rows
            .iter()
            .find(|r| r[0].as_str().unwrap().starts_with(month) && r[1] == json!(product))
            .map(|r| r[2].clone())
            .unwrap()
    };
    near(num(&at("2026-01", "Arabica")), 100.0);
    near(num(&at("2026-03", "Arabica")), 300.0);
    assert!(at("2026-02", "Liberica").is_null());
    near(num(&at("2026-03", "Liberica")), 100.0);
}

#[test]
fn repeat_adds_the_dimension_and_keeps_twelve_charts() {
    let e = engine("rep");
    let t = tile(
        "bar",
        json!([{ "table": "s3/regions", "column": "qty", "agg": "sum" }]),
        json!([{ "table": "s3/regions", "column": "product" }]),
        json!({ "repeat": { "table": "s3/regions", "column": "region" } }),
    );
    // The question has the repeat field after the group.
    let q = tile_query(&t, &[]).unwrap();
    assert_eq!(q.dimensions.len(), 2);
    assert_eq!(q.dimensions[1].column, "region");
    let res = run(&e, &t);
    let rep = res.repeat.as_ref().unwrap();
    assert_eq!(rep.keys.len(), 12);
    assert_eq!(rep.more, 3);
    // The biggest regions first: R15 sells 115 + 10 + 5.
    assert_eq!(rep.keys[0], json!("R15"));
    assert_eq!(rep.keys[11], json!("R04"));
    // Every row belongs to a chart that is drawn: 12 charts of 3 bars.
    assert_eq!(res.rows.len(), 36);
    assert!(res.rows.iter().all(|r| rep.keys.contains(&r[1])));
}

#[test]
fn repeat_top_n_works_inside_each_chart() {
    let e = engine("repn");
    let t = tile(
        "bar",
        json!([{ "table": "s3/regions", "column": "qty", "agg": "sum" }]),
        json!([{ "table": "s3/regions", "column": "product" }]),
        json!({ "repeat": { "table": "s3/regions", "column": "region" }, "top": { "n": 1 } }),
    );
    let res = run(&e, &t);
    let rep = res.repeat.as_ref().unwrap();
    assert!(res.other.unwrap());
    // Each chart: product a, and the rest as "Other" (10 + 5).
    for key in &rep.keys {
        let mine: Vec<_> = res.rows.iter().filter(|r| &r[1] == key).collect();
        assert_eq!(mine.len(), 2, "{key}");
        assert_eq!(mine[0][0], json!("a"));
        assert_eq!(mine[1][0], json!("\u{0}other"));
        near(num(&mine[1][2]), 15.0);
    }
}

#[test]
fn repeat_by_a_few_values_keeps_all_and_a_line_reads_in_time() {
    let e = engine("rep2");
    // Amount by month, one chart for each product: three charts, Arabica first (the biggest).
    let t = tile(
        "line",
        json!([amount("sum")]),
        month(),
        json!({ "repeat": { "table": "s1/sales", "column": "product" } }),
    );
    let res = run(&e, &t);
    let rep = res.repeat.as_ref().unwrap();
    assert_eq!(
        rep.keys,
        vec![json!("Arabica"), json!("Robusta"), json!("Liberica")]
    );
    assert_eq!(rep.more, 0);
    // Every chart has all three months (the quiet ones filled with 0).
    assert_eq!(res.rows.len(), 9);
}

#[test]
fn manual_order_puts_groups_where_they_were_dragged() {
    let e = engine("man");
    let t = tile(
        "bar",
        json!([amount("sum")]),
        json!([{ "table": "s1/sales", "column": "product" }]),
        json!({ "sort": { "by": "manual", "order": ["Liberica", "Robusta"] } }),
    );
    let res = run(&e, &t);
    let names: Vec<&str> = res.rows.iter().map(|r| r[0].as_str().unwrap()).collect();
    // Arabica is not in the list, so it follows the ones that are.
    assert_eq!(names, vec!["Liberica", "Robusta", "Arabica"]);
}

#[test]
fn funnel_puts_the_biggest_stage_first_and_heat_map_is_a_pivot_question() {
    let e = engine("fun");
    let f = tile(
        "funnel",
        json!([amount("sum")]),
        json!([{ "table": "s1/sales", "column": "product" }]),
        json!({}),
    );
    let res = run(&e, &f);
    let names: Vec<&str> = res.rows.iter().map(|r| r[0].as_str().unwrap()).collect();
    assert_eq!(names, vec!["Arabica", "Robusta", "Liberica"]);

    let h = tile(
        "heatmap",
        json!([amount("sum")]),
        json!([{ "table": "s1/sales", "column": "product" }]),
        json!({ "cols": { "table": "s1/sales", "column": "date", "grain": "month" } }),
    );
    let q = tile_query(&h, &[]).unwrap();
    assert_eq!(q.dimensions.len(), 2);
    assert_eq!(q.measures.len(), 1);
    let res = run(&e, &h);
    // Arabica: 100 in January, 200 in February, 300 in March.
    let cell = |p: &str, m: &str| {
        res.rows
            .iter()
            .find(|r| r[0] == json!(p) && r[1].as_str().unwrap().starts_with(m))
            .map(|r| num(&r[2]))
    };
    assert_eq!(cell("Arabica", "2026-02"), Some(200.0));
    assert_eq!(cell("Liberica", "2026-03"), Some(5.0));
}

#[test]
fn clicking_a_heat_map_column_filters_the_other_tiles_offline() {
    let e = engine("cross");
    let h = json!({ "id": "hm", "type": "heatmap", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
        "measures": [{ "table": "s1/sales", "column": "amount", "agg": "sum" }],
        "dims": [{ "table": "s1/sales", "column": "product" }],
        "cols": { "table": "s1/sales", "column": "date", "grain": "month" } });
    let k = json!({ "id": "k", "type": "bar", "title": "", "x": 6, "y": 0, "w": 6, "h": 4,
        "measures": [{ "table": "s1/sales", "column": "amount", "agg": "sum" }],
        "dims": [{ "table": "s1/sales", "column": "product" }] });
    e.set_dashboards(vec![
        json!({ "id": "d2", "title": "H", "refresh": 0, "tiles": [h, k] }),
    ]);
    let snap = snapshot::build(&e, "d2", &[]).unwrap();
    let cross = &snap["cross"]["hm"];
    // A click on a product row, and one on a month column (its key starts with the column prefix).
    assert!(cross["Arabica"]["k"].is_object());
    let col = cross
        .as_object()
        .unwrap()
        .keys()
        .find(|k| k.ends_with("2026-02-01"))
        .unwrap();
    let rows = cross[col]["k"]["rows"].as_array().unwrap();
    // February only: Arabica 200, Robusta 30.
    let total: f64 = rows.iter().map(|r| num(&r[1])).sum();
    near(total, 230.0);
}
