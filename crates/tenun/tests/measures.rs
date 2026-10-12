//! Named measures (build 03): a formula of totals that is right at every grouping.
//!
//! Sales are many rows for each store and month; targets are one row for each store and month.
//! `SUM([sales].[amount]) / SUM([targets].[target])` must not count a target once for each sale,
//! so every number below is worked out by hand from the rows. A single SELECT that joined sales to
//! targets would give 1300 / 3200 = 0.406 for the whole instead of 700 / 1200 = 0.583, and
//! 600 / 1800 = 0.333 for Kopi A instead of 300 / 600 = 0.5.

use chrono::NaiveDate;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::engine::{AddSource, MeasureInUse, MeasureRefused, QueryResult};
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile};
use tenun_core::workspace::{FieldRef, NamedMeasure, TableSetup};

const OTHER: &str = "\u{0}other";

const SALES: &str = "id,date,store_id,product,amount\n\
    1,2026-01-05,1,Latte,100\n2,2026-01-10,1,Mocha,50\n3,2026-01-20,2,Latte,200\n\
    4,2026-02-03,1,Latte,150\n5,2026-02-14,2,Mocha,80\n6,2026-02-20,2,Latte,20\n\
    7,2026-02-25,3,Latte,60\n8,2026-01-12,4,Latte,40\n";
const STORES: &str = "id,name,region\n1,Kopi A,Java\n2,Kopi B,Java\n3,Kopi C,Bali\n4,Kopi D,\n";
const TARGETS: &str = "store_id,month,target\n1,2026-01-01,300\n1,2026-02-01,300\n\
    2,2026-01-01,200\n2,2026-02-01,200\n3,2026-02-01,100\n4,2026-01-01,100\n";

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-measures-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn field(t: &str, c: &str) -> FieldRef {
    FieldRef {
        table: t.to_string(),
        column: c.to_string(),
    }
}

struct World {
    e: Engine,
    d: PathBuf,
    sales: String,
    stores: String,
    targets: String,
}

fn world_of(tag: &str, sales: &str, stores: &str, targets: &str) -> World {
    let d = dir(tag);
    for (f, text) in [
        ("sales.csv", sales),
        ("stores.csv", stores),
        ("targets.csv", targets),
    ] {
        std::fs::write(d.join(f), text).unwrap();
    }
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    for f in ["sales.csv", "stores.csv", "targets.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    let snap = e.snapshot();
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    let id = |name: &str| {
        snap.tables
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("no table {name}"))
            .id
            .clone()
    };
    let (sales, stores, targets) = (id("sales"), id("stores"), id("targets"));
    e.add_relationship(field(&sales, "store_id"), field(&stores, "id"))
        .unwrap();
    e.add_relationship(field(&targets, "store_id"), field(&stores, "id"))
        .unwrap();
    World {
        e,
        d,
        sales,
        stores,
        targets,
    }
}

fn world(tag: &str) -> World {
    world_of(tag, SALES, STORES, TARGETS)
}

fn measure(name: &str, formula: &str) -> NamedMeasure {
    NamedMeasure {
        id: String::new(),
        name: name.into(),
        formula: formula.into(),
        ..Default::default()
    }
}

const ACHIEVEMENT: &str = "SUM([sales].[amount]) / SUM([targets].[target])";

impl World {
    /// Saves a measure and returns its id.
    fn add(&self, name: &str, formula: &str) -> String {
        let mut list = self.e.measures();
        list.push(measure(name, formula));
        self.e.set_measures(list, false).unwrap();
        self.e.measures().last().unwrap().id.clone()
    }

    fn q(&self, value: Value) -> QueryResult {
        let q: SemanticQuery = serde_json::from_value(value).unwrap();
        self.e.query(&q).unwrap()
    }

    fn dim(&self, table: &str, column: &str) -> Value {
        let t = match table {
            "sales" => &self.sales,
            "stores" => &self.stores,
            _ => &self.targets,
        };
        json!({ "table": t, "column": column })
    }

    fn month(&self) -> Value {
        json!({ "table": self.sales, "column": "date", "grain": "month" })
    }

    fn tile(&self, ty: &str, measures: Value, dims: Value, extra: Value) -> Tile {
        let mut t = json!({ "id": "t", "type": ty, "measures": measures, "dims": dims });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            t[k] = v;
        }
        serde_json::from_value(t).unwrap()
    }
}

fn named(id: &str) -> Value {
    json!({ "named": id })
}

fn near(a: &Value, b: f64) {
    let a = a.as_f64().unwrap_or(f64::NAN);
    assert!((a - b).abs() < 1e-9, "{a} is not {b}");
}

/// `(group, value)` of every row of a one-group result.
fn by(r: &QueryResult) -> Vec<(String, Option<f64>)> {
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

fn get(r: &QueryResult, key: &str) -> f64 {
    by(r)
        .into_iter()
        .find(|(k, _)| k == key)
        .unwrap_or_else(|| panic!("no row {key} in {:?}", r.rows))
        .1
        .unwrap_or(f64::NAN)
}

fn same(r: &QueryResult, key: &str, want: f64) {
    let got = get(r, key);
    assert!((got - want).abs() < 1e-9, "{key}: {got} is not {want}");
}

// ---- the numbers ----------------------------------------------------------------------

#[test]
fn the_whole_by_store_by_month_and_by_both_are_the_hand_numbers() {
    let w = world("numbers");
    let id = w.add("Achievement", ACHIEVEMENT);

    let total = w.q(json!({ "measures": [named(&id)] }));
    assert_eq!(total.columns[0].name, "Achievement");
    near(&total.rows[0][0], 700.0 / 1200.0);

    let stores = w.q(json!({ "measures": [named(&id)], "dimensions": [w.dim("stores", "name")] }));
    assert_eq!(stores.rows.len(), 4);
    same(&stores, "Kopi A", 0.5);
    same(&stores, "Kopi B", 0.75);
    same(&stores, "Kopi C", 0.6);
    same(&stores, "Kopi D", 0.4);

    // Sales by their date, targets by their main date: both by month.
    let months = w.q(json!({ "measures": [named(&id)], "dimensions": [w.month()] }));
    assert_eq!(months.rows.len(), 2);
    same(&months, "2026-01-01", 390.0 / 600.0);
    same(&months, "2026-02-01", 310.0 / 600.0);
    // Time reads left to right.
    assert_eq!(months.rows[0][0], json!("2026-01-01"));

    let both = w.q(json!({
        "measures": [named(&id)],
        "dimensions": [w.dim("stores", "name"), w.month()]
    }));
    // Kopi C has no January sales and no January target, so there is no such group.
    assert_eq!(both.rows.len(), 6);
    let at = |s: &str, m: &str| {
        both.rows
            .iter()
            .find(|r| r[0] == json!(s) && r[1] == json!(m))
            .unwrap_or_else(|| panic!("no group {s} {m}"))[2]
            .as_f64()
            .unwrap()
    };
    assert!((at("Kopi A", "2026-01-01") - 0.5).abs() < 1e-9);
    assert!((at("Kopi A", "2026-02-01") - 0.5).abs() < 1e-9);
    assert!((at("Kopi B", "2026-01-01") - 1.0).abs() < 1e-9);
    assert!((at("Kopi B", "2026-02-01") - 0.5).abs() < 1e-9);
    assert!((at("Kopi C", "2026-02-01") - 0.6).abs() < 1e-9);
    assert!((at("Kopi D", "2026-01-01") - 0.4).abs() < 1e-9);
    assert!(total.parts.is_empty() && stores.parts.is_empty() && months.parts.is_empty());
}

#[test]
fn a_blank_group_is_a_group_that_both_parts_find() {
    let w = world("blank");
    let id = w.add("Achievement", ACHIEVEMENT);
    // Kopi D has no region: that group is blank in the sales part and blank in the targets part.
    let r = w.q(json!({ "measures": [named(&id)], "dimensions": [w.dim("stores", "region")] }));
    assert_eq!(r.rows.len(), 3, "{:?}", r.rows);
    same(&r, "Java", 0.6);
    same(&r, "Bali", 0.6);
    same(&r, "\u{0}null", 0.4);
}

#[test]
fn a_group_only_one_part_can_reach_repeats_and_the_result_says_so() {
    let w = world("onepart");
    let id = w.add("Achievement", ACHIEVEMENT);
    let r = w.q(json!({ "measures": [named(&id)], "dimensions": [w.dim("sales", "product")] }));
    // Targets cannot be split by product: all 1200 stand against each product.
    same(&r, "Latte", 570.0 / 1200.0);
    same(&r, "Mocha", 130.0 / 1200.0);
    assert_eq!(r.parts.len(), 1, "{:?}", r.parts);
    assert_eq!(r.parts[0].measure, 0);
    assert_eq!(r.parts[0].note, "targets is not split by product.");
    // The words are also where the other notes are.
    assert!(r.parts.iter().all(|p| !p.note.is_empty()));

    let t = w.tile(
        "bar",
        json!([named(&id)]),
        json!([w.dim("sales", "product")]),
        json!({}),
    );
    let shown = run_tile(&w.e, &t, &[]).unwrap();
    assert!(
        shown
            .notes
            .contains(&"targets is not split by product.".to_string())
    );
    assert!(
        shown
            .public_notes
            .contains(&"targets is not split by product.".to_string())
    );
    // What leaves the computer carries the same words and no value.
    let out = snapshot::run_tile_public(&w.e, &t, &[]).unwrap();
    assert!(
        out.notes
            .contains(&"targets is not split by product.".to_string())
    );
}

#[test]
fn a_filter_on_a_column_only_one_part_reaches_filters_only_that_part() {
    let w = world("filter");
    let id = w.add("Achievement", ACHIEVEMENT);
    let product = json!({ "table": w.sales, "column": "product", "op": "in", "values": ["Latte"] });
    let r = w.q(json!({ "measures": [named(&id)], "filters": [product] }));
    // 570 of sales over all 1200 of targets (V4-D85a: and the words say the targets are not filtered).
    near(&r.rows[0][0], 570.0 / 1200.0);
    assert_eq!(r.parts.len(), 1);
    assert_eq!(r.parts[0].note, "targets is not filtered by product.");
    // The filter reached the tile, so it is not ignored by the tile.
    assert!(r.ignored_filters.is_empty());

    // A filter both parts reach filters both.
    let region = json!({ "table": w.stores, "column": "region", "op": "in", "values": ["Java"] });
    let r = w.q(json!({ "measures": [named(&id)], "filters": [region] }));
    near(&r.rows[0][0], 600.0 / 1000.0);
    assert!(r.parts.is_empty());
}

#[test]
fn an_unreached_filter_is_noted_like_an_unreached_group() {
    let w = world("noted");
    let id = w.add("Achievement", ACHIEVEMENT);
    let product = json!({ "table": w.sales, "column": "product", "op": "in", "values": ["Mocha"] });
    let region = json!({ "table": w.stores, "column": "region", "op": "in", "values": ["Java"] });
    let r = w.q(json!({
        "measures": [named(&id)],
        "dimensions": [w.dim("sales", "product")],
        "filters": [product, region]
    }));
    // Mocha sold 130 in Java; targets of Java stand against it, not split and not filtered by product.
    near(&r.rows[0][1], 130.0 / 1000.0);
    let notes: Vec<&str> = r.parts.iter().map(|p| p.note.as_str()).collect();
    assert_eq!(
        notes,
        [
            "targets is not split by product.",
            "targets is not filtered by product."
        ]
    );
}

#[test]
fn a_date_filter_is_read_from_the_main_date_of_the_other_table() {
    let w = world("datefilter");
    let id = w.add("Achievement", ACHIEVEMENT);
    let january = json!({ "table": w.sales, "column": "date", "op": "between",
                          "values": ["2026-01-01", "2026-01-31"] });
    let r = w.q(json!({ "measures": [named(&id)], "filters": [january] }));
    near(&r.rows[0][0], 390.0 / 600.0);
    assert!(r.parts.is_empty(), "{:?}", r.parts);
    // A click on a month of a chart makes this filter: both parts follow it.
    let click = json!({ "table": w.sales, "column": "date", "grain": "month", "op": "in",
                        "values": ["2026-02-01"] });
    let r = w.q(json!({ "measures": [named(&id)], "filters": [click] }));
    near(&r.rows[0][0], 310.0 / 600.0);
}

#[test]
fn the_main_date_can_be_picked_and_a_table_without_one_says_so() {
    // Targets with two dates: no default main date.
    let targets = "store_id,month,created,target\n1,2026-01-01,2026-01-02,300\n1,2026-02-01,2026-02-02,300\n\
        2,2026-01-01,2026-01-02,200\n2,2026-02-01,2026-02-02,200\n3,2026-02-01,2026-02-02,100\n\
        4,2026-01-01,2026-01-02,100\n";
    let w = world_of("maindate", SALES, STORES, targets);
    let id = w.add("Achievement", ACHIEVEMENT);
    let r = w.q(json!({ "measures": [named(&id)], "dimensions": [w.month()] }));
    // Targets cannot be split by time: the whole 1200 stands against each month, and it is said.
    same(&r, "2026-01-01", 390.0 / 1200.0);
    assert_eq!(
        r.parts[0].note,
        "targets has no main date, so it is not split by date."
    );

    // The user picks `month`; the numbers are the ones above.
    let setup = TableSetup {
        main_date: Some("month".into()),
        ..Default::default()
    };
    w.e.set_table_setup(&w.targets, Some(setup)).unwrap();
    let r = w.q(json!({ "measures": [named(&id)], "dimensions": [w.month()] }));
    same(&r, "2026-01-01", 390.0 / 600.0);
    same(&r, "2026-02-01", 310.0 / 600.0);
    assert!(r.parts.is_empty());
    // A column that is not a date cannot be the main date; a missing one cannot either.
    let bad = |c: &str| TableSetup {
        main_date: Some(c.into()),
        ..Default::default()
    };
    assert!(
        w.e.set_table_setup(&w.targets, Some(bad("target")))
            .is_err()
    );
    assert!(w.e.set_table_setup(&w.targets, Some(bad("nope"))).is_err());
    // The picked date is saved in the file, as `mainDate`, and a table without one writes nothing.
    let path = w.d.join("w.tenun");
    w.e.save(Some(path.clone())).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"mainDate\": \"month\""), "{text}");
}

#[test]
fn two_tables_with_no_links_are_tables_of_events_and_both_are_split_by_month() {
    // V4-D111. The first thing a person tries: sales, and a targets file for the whole company, no links.
    let d = dir("v4d111");
    std::fs::write(
        d.join("sales.csv"),
        "date,amount\n2026-01-05,100\n2026-01-20,50\n2026-02-10,240\n",
    )
    .unwrap();
    std::fs::write(
        d.join("targets.csv"),
        "month,target\n2026-01-01,300\n2026-02-01,400\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    for f in ["sales.csv", "targets.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    let snap = e.snapshot();
    let id = |name: &str| {
        snap.tables
            .iter()
            .find(|t| t.name == name)
            .unwrap()
            .id
            .clone()
    };
    let w = World {
        e,
        d,
        sales: id("sales"),
        stores: String::new(),
        targets: id("targets"),
    };
    assert!(w.e.snapshot().workspace.relationships.is_empty());
    let achievement = w.add("Achievement", ACHIEVEMENT);
    let r = w.q(json!({ "measures": [named(&achievement)], "dimensions": [w.month()] }));
    // January: 150 sold against 300. February: 240 against 400. Neither is split by nothing.
    same(&r, "2026-01-01", 150.0 / 300.0);
    same(&r, "2026-02-01", 240.0 / 400.0);
    assert!(r.parts.is_empty(), "{:?}", r.parts);
    // The whole is a ratio of totals.
    let all = w.q(json!({ "measures": [named(&achievement)] }));
    near(&all.rows[0][0], 390.0 / 700.0);
}

// ---- gaps, mixes and shapes ------------------------------------------------------------

#[test]
fn a_group_missing_from_a_part_is_zero_for_a_sum_and_blank_for_the_rest() {
    // Jan: both. Mar: targets only. Apr: sales only.
    let sales =
        "id,date,store_id,product,amount\n1,2026-01-05,1,Latte,10\n2,2026-04-02,1,Latte,30\n";
    let targets = "store_id,month,target\n1,2026-01-01,20\n1,2026-03-01,40\n";
    let w = world_of("gaps", sales, STORES, targets);
    let ratio = w.add("Achievement", ACHIEVEMENT);
    let gap = w.add("Gap", "SUM([sales].[amount]) - SUM([targets].[target])");
    let avg = w.add(
        "Mean",
        "AVERAGE([sales].[amount]) + AVERAGE([targets].[target])",
    );
    let r = w.q(json!({
        "measures": [named(&ratio), named(&gap), named(&avg)],
        "dimensions": [w.month()]
    }));
    assert_eq!(r.rows.len(), 3, "{:?}", r.rows);
    let row = |m: &str| r.rows.iter().find(|r| r[0] == json!(m)).unwrap().clone();
    // Jan 10 / 20.
    near(&row("2026-01-01")[1], 0.5);
    near(&row("2026-01-01")[2], -10.0);
    // Mar: no sales is 0, so 0 / 40, and 0 - 40.
    near(&row("2026-03-01")[1], 0.0);
    near(&row("2026-03-01")[2], -40.0);
    // Apr: no targets is 0, so 30 / 0 is blank, and 30 - 0 is 30.
    assert!(row("2026-04-01")[1].is_null());
    near(&row("2026-04-01")[2], 30.0);
    // An average of nothing stays blank: Mar and Apr each miss one of the two parts.
    assert!(row("2026-03-01")[3].is_null());
    assert!(row("2026-04-01")[3].is_null());
    near(&row("2026-01-01")[3], 10.0 + 20.0);
}

#[test]
fn a_plain_measure_beside_a_named_one_is_a_part_of_its_own() {
    let w = world("mixed");
    let id = w.add("Achievement", ACHIEVEMENT);
    let rows = json!({ "table": w.sales, "column": null, "agg": "count" });
    let amount = json!({ "table": w.sales, "column": "amount", "agg": "sum" });
    let r = w.q(json!({
        "measures": [named(&id), rows, amount],
        "dimensions": [w.dim("stores", "name")]
    }));
    let row = |s: &str| r.rows.iter().find(|r| r[0] == json!(s)).unwrap().clone();
    near(&row("Kopi A")[1], 0.5);
    near(&row("Kopi A")[2], 3.0);
    near(&row("Kopi A")[3], 300.0);
    near(&row("Kopi C")[2], 1.0);
    near(&row("Kopi D")[3], 40.0);
    // Each plain measure reads as it does alone: sales of a store are not multiplied by its targets.
    let alone =
        w.q(json!({ "measures": [amount_of(&w)], "dimensions": [w.dim("stores", "name")] }));
    same(&alone, "Kopi A", 300.0);
}

fn amount_of(w: &World) -> Value {
    json!({ "table": w.sales, "column": "amount", "agg": "sum" })
}

#[test]
fn one_total_on_one_table_keeps_the_shape_of_a_plain_measure() {
    let w = world("fast");
    let id = w.add("Revenue", "SUM([sales].[amount])");
    let plain =
        w.q(json!({ "measures": [amount_of(&w)], "dimensions": [w.dim("stores", "region")] }));
    let named_r =
        w.q(json!({ "measures": [named(&id)], "dimensions": [w.dim("stores", "region")] }));
    assert_eq!(plain.rows, named_r.rows);
    assert!(named_r.parts.is_empty());
    // Two formulas of one table and a ratio of two totals of the same table also add up right.
    let per = w.add("Per sale", "SUM([sales].[amount]) / COUNT([sales].[id])");
    let r = w.q(json!({ "measures": [named(&per)], "dimensions": [w.dim("stores", "name")] }));
    same(&r, "Kopi A", 100.0);
    same(&r, "Kopi B", 100.0);
}

#[test]
fn top_n_and_other_is_a_ratio_of_the_rest_not_a_sum_of_ratios() {
    let w = world("top");
    let id = w.add("Achievement", ACHIEVEMENT);
    let t = w.tile(
        "bar",
        json!([named(&id)]),
        json!([w.dim("stores", "name")]),
        json!({ "top": { "n": 2, "other": true }, "sort": { "by": "value", "desc": true } }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    assert_eq!(r.other, Some(true));
    let keys: Vec<String> = r.rows.iter().map(|r| snapshot::js_key(&r[0])).collect();
    assert_eq!(keys, ["Kopi B", "Kopi C", OTHER]);
    // Kopi A and Kopi D together: (300 + 40) / (600 + 100). Not 0.5 + 0.4, and not their mean.
    near(&r.rows[2][1], 340.0 / 700.0);
}

#[test]
fn a_share_of_the_whole_needs_values_that_add_up() {
    let w = world("share");
    let ratio = w.add("Achievement", ACHIEVEMENT);
    let sum = w.add("Revenue", "SUM([sales].[amount])");
    let dims = json!([w.dim("stores", "name")]);

    let t = w.tile(
        "bar",
        json!([named(&sum)]),
        dims.clone(),
        json!({ "showAs": ["pct_total"] }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    let total: f64 = by(&r).iter().filter_map(|(_, v)| *v).sum();
    assert!((total - 1.0).abs() < 1e-9, "{total}");
    same(&r, "Kopi A", 300.0 / 700.0);

    // A ratio is shown as it is, and the words say why.
    let t = w.tile(
        "bar",
        json!([named(&ratio)]),
        dims.clone(),
        json!({ "showAs": ["pct_total"] }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    same(&r, "Kopi A", 0.5);
    assert!(r.columns[1].unit.is_none());
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("Achievement is not shown as % of total")),
        "{:?}",
        r.notes
    );
    // A running total is the same: a sum of ratios is not the ratio of the sums.
    let t = w.tile(
        "line",
        json!([named(&ratio)]),
        json!([w.month()]),
        json!({ "showAs": ["running"] }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    same(&r, "2026-01-01", 390.0 / 600.0);
    same(&r, "2026-02-01", 310.0 / 600.0);
    // The Sankey chart adds flows up, so it refuses a ratio with words.
    let t = w.tile(
        "sankey",
        json!([named(&ratio)]),
        json!([w.dim("stores", "name"), w.dim("sales", "product")]),
        json!({}),
    );
    let e = run_tile(&w.e, &t, &[]).unwrap_err();
    assert!(e.to_string().contains("adds the flows up"), "{e}");
}

#[test]
fn a_number_tile_compares_the_period_with_a_named_measure() {
    let w = world("kpi");
    let id = w.add("Achievement", ACHIEVEMENT);
    let t = w.tile("kpi", json!([named(&id)]), json!([w.month()]), json!({}));
    let r = run_tile(&w.e, &t, &[]).unwrap();
    let k = r.kpi.unwrap();
    // The data ends on 25 February: the period is not over. February so far against 1 to 25
    // January, with the targets of the month of each.
    assert!(k.partial);
    assert!((k.value.unwrap() - 310.0 / 600.0).abs() < 1e-9);
    assert!((k.prev.unwrap() - 390.0 / 600.0).abs() < 1e-9);
    assert!((k.delta.unwrap() - (310.0 / 600.0 - 0.65) / 0.65).abs() < 1e-9);
    assert_eq!(k.bars.len(), 2);
    // With no date it is the whole.
    let t = w.tile("kpi", json!([named(&id)]), json!([]), json!({}));
    let k = run_tile(&w.e, &t, &[]).unwrap().kpi.unwrap();
    assert!((k.value.unwrap() - 700.0 / 1200.0).abs() < 1e-9);
}

#[test]
fn the_same_period_one_step_earlier_lines_up_for_a_named_measure() {
    let w = world("compare");
    let id = w.add("Achievement", ACHIEVEMENT);
    let t = w.tile(
        "line",
        json!([named(&id)]),
        json!([w.month()]),
        json!({ "compare": "prev_period" }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    let c = r.compare.expect("a comparison series");
    // January's number sits under February.
    let feb = c.rows.iter().find(|r| r[0] == json!("2026-02-01")).unwrap();
    near(&feb[1], 0.65);
}

#[test]
fn a_table_with_totals_and_a_pivot_get_totals_from_the_engine() {
    let w = world("totals");
    let id = w.add("Achievement", ACHIEVEMENT);
    let t = w.tile(
        "table",
        json!([named(&id)]),
        json!([w.dim("stores", "region")]),
        json!({ "totals": true }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    near(&r.totals.as_ref().unwrap().grand[0], 700.0 / 1200.0);

    let t = w.tile(
        "pivot",
        json!([named(&id)]),
        json!([w.dim("stores", "region")]),
        json!({ "cols": w.month() }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    let totals = r.totals.unwrap();
    near(&totals.grand[0], 700.0 / 1200.0);
    // The total of a row is a ratio of totals: Java over both months is 600 / 1000.
    let java = totals
        .rows
        .unwrap()
        .into_iter()
        .find(|r| r[0] == json!("Java"))
        .unwrap();
    near(&java[1], 0.6);
    let jan = totals
        .cols
        .unwrap()
        .into_iter()
        .find(|r| r[0] == json!("2026-01-01"))
        .unwrap();
    near(&jan[1], 390.0 / 600.0);
}

#[test]
fn rows_behind_a_named_measure_are_the_rows_of_its_table_or_the_words_say_why() {
    let w = world("rows");
    let revenue = w.add("Revenue", "SUM([sales].[amount])");
    let ratio = w.add("Achievement", ACHIEVEMENT);
    let t = w.tile("kpi", json!([named(&revenue)]), json!([]), json!({}));
    let rows = tenun_core::filters::rows_behind(&w.e, &t, &[], None).unwrap();
    assert_eq!(rows.total, 8);
    let t = w.tile("kpi", json!([named(&ratio)]), json!([]), json!({}));
    let e = tenun_core::filters::rows_behind(&w.e, &t, &[], None).unwrap_err();
    let words = e.to_string();
    assert!(words.contains("more than one table"), "{words}");
    assert!(
        words.contains("sales") && words.contains("targets"),
        "{words}"
    );
}

#[test]
fn the_snapshot_of_a_dashboard_carries_the_engine_numbers_for_every_click() {
    let w = world("snapshot");
    let ratio = w.add("Achievement", ACHIEVEMENT);
    let tiles = json!([
        { "id": "a", "type": "bar", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
          "measures": [named(&ratio)], "dims": [w.dim("stores", "name")] },
        { "id": "b", "type": "bar", "title": "", "x": 6, "y": 0, "w": 6, "h": 4,
          "measures": [amount_of(&w)], "dims": [w.dim("sales", "product")] },
        { "id": "c", "type": "kpi", "title": "", "x": 0, "y": 4, "w": 6, "h": 3,
          "measures": [named(&ratio)], "dims": [] }
    ]);
    w.e.set_dashboards(vec![
        json!({ "id": "d1", "title": "Main", "refresh": 0, "tiles": tiles }),
    ]);
    let snap = snapshot::build(&w.e, "d1", &[]).unwrap();
    // The same numbers as the app.
    let live = run_tile(
        &w.e,
        &serde_json::from_value(tiles[0].clone()).unwrap(),
        &[],
    )
    .unwrap();
    assert_eq!(snap["base"]["a"]["rows"], json!(live.rows));
    // The viewer is told the name and whether the values add up, never the formula.
    let m = &snap["measures"][0];
    assert_eq!(m["name"], "Achievement");
    assert_eq!(m["additive"], false);
    assert!(m.get("formula").is_none());
    assert!(!snap.to_string().contains("targets].["));
    // A click on Latte in tile b filters tile a: the sales part only, and the numbers are the
    // engine's (250 of Kopi A's sales over its 600 of targets, and so on).
    let rows = snap["cross"]["b"]["Latte"]["a"]["rows"].as_array().unwrap();
    let of = |s: &str| {
        rows.iter().find(|r| r[0] == json!(s)).unwrap()[1]
            .as_f64()
            .unwrap()
    };
    assert!((of("Kopi A") - 250.0 / 600.0).abs() < 1e-9);
    assert!((of("Kopi B") - 220.0 / 400.0).abs() < 1e-9);
    assert!((of("Kopi C") - 0.6).abs() < 1e-9);
    assert!((of("Kopi D") - 0.4).abs() < 1e-9);
}

// ---- links ---------------------------------------------------------------------------------

#[test]
fn a_part_across_a_link_with_a_repeated_key_does_not_double_a_total() {
    let w = world("repeat");
    // Store 2 is listed twice after the link was made. The first row wins, as it does for any
    // query (V4-D50), and the link carries a warning.
    std::fs::write(
        w.d.join("stores.csv"),
        "id,name,region\n1,Kopi A,Java\n2,Kopi B,Java\n2,Kopi B again,Bali\n3,Kopi C,Bali\n4,Kopi D,\n",
    )
    .unwrap();
    let source =
        w.e.snapshot()
            .tables
            .iter()
            .find(|t| t.id == w.stores)
            .unwrap()
            .source_id
            .clone();
    w.e.load(&source);
    assert!(w.e.snapshot().errors.is_empty());
    let java = w.add(
        "Java sales",
        "SUMIF([sales].[amount], [stores].[region] = \"Java\")",
    );
    let r = w.q(json!({ "measures": [named(&java)] }));
    // Kopi A 300 and Kopi B 300. A join that kept both rows of store 2 would count 380.
    near(&r.rows[0][0], 600.0);
    let id = w.add("Achievement", ACHIEVEMENT);
    let r = w.q(json!({ "measures": [named(&id)], "dimensions": [w.dim("stores", "region")] }));
    same(&r, "Java", 600.0 / 1000.0);
    same(&r, "Bali", 60.0 / 100.0);
    // The plain total of the same question is the same.
    let total = w.q(json!({ "measures": [named(&id)] }));
    near(&total.rows[0][0], 700.0 / 1200.0);
    // The note of the link is on the tile, for a link the formula crosses too.
    let t = w.tile("kpi", json!([named(&java)]), json!([]), json!({}));
    let r = run_tile(&w.e, &t, &[]).unwrap();
    assert!(
        !r.notes.is_empty(),
        "the repeated key is told: {:?}",
        r.notes
    );
    let out = snapshot::run_tile_public(&w.e, &t, &[]).unwrap();
    assert!(
        out.notes.iter().all(|n| !n.contains("Kopi B")),
        "{:?}",
        out.notes
    );
}

// ---- renames ---------------------------------------------------------------------------

fn formula_of(w: &World, id: &str) -> String {
    w.e.measures()
        .into_iter()
        .find(|m| m.id == id)
        .unwrap()
        .formula
}

#[test]
fn a_column_rename_and_a_shown_name_rewrite_the_formula() {
    let w = world("rename");
    let id = w.add("Achievement", ACHIEVEMENT);
    let label = |name: &str| {
        let mut s = TableSetup::default();
        s.columns.insert(
            "amount".into(),
            tenun_core::workspace::ColumnSetup {
                label: Some(name.into()),
                ..Default::default()
            },
        );
        s
    };
    // A name shown is a new way to write the column. The formula can use it.
    w.e.set_table_setup(&w.sales, Some(label("Revenue")))
        .unwrap();
    let mut list = w.e.measures();
    list[0].formula = "SUM([sales].[Revenue]) / SUM([targets].[target])".into();
    w.e.set_measures(list, false).unwrap();
    let r = w.q(json!({ "measures": [named(&id)] }));
    near(&r.rows[0][0], 700.0 / 1200.0);
    // The name changes: the formula follows it, and the tile does not notice.
    w.e.set_table_setup(&w.sales, Some(label("Net sales")))
        .unwrap();
    assert_eq!(
        formula_of(&w, &id),
        "SUM([sales].[Net sales]) / SUM([targets].[target])"
    );
    let r = w.q(json!({ "measures": [named(&id)] }));
    near(&r.rows[0][0], 700.0 / 1200.0);
    // The name goes: the formula goes back to the real name.
    w.e.set_table_setup(&w.sales, None).unwrap();
    assert_eq!(formula_of(&w, &id), ACHIEVEMENT);
}

/// Two links from orders to places: where it came from, and where it went.
fn hubs(tag: &str) -> (Engine, String, String, Vec<String>) {
    let d = dir(tag);
    std::fs::write(
        d.join("orders.csv"),
        "id,amount,from_id,to_id\n1,10,1,2\n2,20,1,1\n3,40,2,2\n4,80,2,1\n",
    )
    .unwrap();
    std::fs::write(d.join("places.csv"), "id,kind\n1,hub\n2,shop\n").unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    for f in ["orders.csv", "places.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    let snap = e.snapshot();
    let id = |n: &str| snap.tables.iter().find(|t| t.name == n).unwrap().id.clone();
    let (orders, places) = (id("orders"), id("places"));
    let from = e
        .add_relationship(field(&orders, "from_id"), field(&places, "id"))
        .unwrap();
    let to = e
        .add_relationship(field(&orders, "to_id"), field(&places, "id"))
        .unwrap();
    e.rename_links(
        [
            (from.clone(), Some("Sent from".to_string())),
            (to.clone(), Some("Sent to".to_string())),
        ]
        .into(),
    )
    .unwrap();
    (e, orders, places, vec![from, to])
}

#[test]
fn a_link_rename_rewrites_the_formula_and_two_links_can_swap_names() {
    let (e, orders, _places, links) = hubs("swap");
    // Sent from a hub: orders 1 and 2 (10 + 20). Sent to a hub: orders 2 and 4 (20 + 80).
    let formula = "SUMIF([orders].[amount], [Sent from].[kind] = \"hub\") \
                   - SUMIF([orders].[amount], [Sent to].[kind] = \"hub\")";
    e.set_measures(
        vec![NamedMeasure {
            name: "Net".into(),
            formula: formula.into(),
            ..Default::default()
        }],
        false,
    )
    .unwrap();
    let id = e.measures()[0].id.clone();
    let q: SemanticQuery =
        serde_json::from_value(json!({ "measures": [{ "named": id }] })).unwrap();
    let net = |e: &Engine| e.query(&q).unwrap().rows[0][0].as_f64().unwrap();
    assert_eq!(net(&e), 30.0 - 100.0);

    // One link gets a new name.
    e.rename_link(&links[0], Some("Came from".into())).unwrap();
    assert!(e.measures()[0].formula.contains("[Came from].[kind]"));
    assert!(!e.measures()[0].formula.contains("[Sent from]"));
    assert_eq!(net(&e), 30.0 - 100.0);

    // The two names swap. Each reference follows its link, so the number does not move.
    e.rename_links(
        [
            (links[0].clone(), Some("Sent to".to_string())),
            (links[1].clone(), Some("Came from".to_string())),
        ]
        .into(),
    )
    .unwrap();
    let f = e.measures()[0].formula.clone();
    assert!(
        f.contains("[Sent to].[kind]") && f.contains("[Came from].[kind]"),
        "{f}"
    );
    assert_eq!(net(&e), 30.0 - 100.0);
    // `via` on a field is respected beside it: orders by the kind of place they came from.
    let by_from: SemanticQuery = serde_json::from_value(json!({
        "measures": [{ "named": id }, { "table": orders, "column": "amount", "agg": "sum", "via": links[0] }],
        "dimensions": [{ "table": _places, "column": "kind", "via": links[0] }]
    }))
    .unwrap();
    let r = e.query(&by_from).unwrap();
    // The first link is still the link of `from_id`, whatever it is called now: orders that came
    // from a hub are 1 and 2 (10 + 20), and from a shop 3 and 4 (40 + 80).
    let hub = r.rows.iter().find(|r| r[0] == json!("hub")).unwrap();
    assert_eq!(hub[2].as_f64(), Some(30.0));
    let shop = r.rows.iter().find(|r| r[0] == json!("shop")).unwrap();
    assert_eq!(shop[2].as_f64(), Some(120.0));
}

// ---- saving ----------------------------------------------------------------------------

#[test]
fn saving_gives_ids_checks_formulas_and_keeps_old_trouble_out_of_the_way() {
    let w = world("save");
    let a = w.add("Achievement", ACHIEVEMENT);
    let b = w.add("Revenue", "SUM([sales].[amount])");
    assert_eq!((a.as_str(), b.as_str()), ("m1", "m2"));

    // A formula that does not work is refused with its place, in UTF-16 units, and nothing is saved.
    let mut list = w.e.measures();
    list.push(measure("Bad", "SUM([sales].[amount]) / SUM([😀].[x])"));
    let err = w.e.set_measures(list, false).unwrap_err();
    let refused = err.downcast_ref::<MeasureRefused>().expect("a refusal");
    assert_eq!(refused.index, 2);
    let reply = refused.reply();
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["measure"], 2);
    let at = reply["error"]["at"].as_u64().unwrap() as usize;
    assert!(at > 0 && at <= refused.formula.encode_utf16().count());
    assert_eq!(w.e.measures().len(), 2);

    // A measure with no total in it, or no name, or the name of another, is refused too.
    for (name, formula, why) in [
        ("One", "1 + 1", "no total in it"),
        ("", "SUM([sales].[amount])", "name"),
        (
            "achievement",
            "SUM([sales].[amount])",
            "Another measure is called",
        ),
        ("Empty", " ", "Write a formula"),
    ] {
        let mut list = w.e.measures();
        list.push(measure(name, formula));
        let err = w.e.set_measures(list, false).unwrap_err();
        let r = err.downcast_ref::<MeasureRefused>().expect(name);
        assert!(r.error.message.contains(why), "{}", r.error.message);
    }

    // A measure that was saved and does not work now (its table went) does not stop another save.
    let mut list = w.e.measures();
    list[1].formula = "SUM([nowhere].[x])".into();
    assert!(
        w.e.set_measures(list.clone(), false).is_err(),
        "a changed formula is checked"
    );
    w.e.remove_source(
        w.e.snapshot()
            .tables
            .iter()
            .find(|t| t.id == w.targets)
            .unwrap()
            .source_id
            .as_str(),
    );
    let mut list = w.e.measures();
    list.push(measure("Another", "SUM([sales].[amount]) * 2"));
    w.e.set_measures(list, false).unwrap();
    // The tile that uses the measure that lost a table says why, in words.
    let q: SemanticQuery = serde_json::from_value(json!({ "measures": [named(&a)] })).unwrap();
    let e = w.e.query(&q).unwrap_err();
    assert!(e.to_string().contains("Achievement"), "{e}");
}

#[test]
fn a_measure_that_tiles_use_does_not_go_without_a_question() {
    let w = world("delete");
    let id = w.add("Achievement", ACHIEVEMENT);
    let spare = w.add("Spare", "SUM([sales].[amount])");
    let tiles = json!([
        { "id": "t1", "type": "bar", "title": "By store", "measures": [named(&id)], "dims": [w.dim("stores", "name")] },
        { "id": "t2", "type": "kpi", "title": "", "measures": [named(&id)], "dims": [] }
    ]);
    w.e.set_dashboards(vec![json!({ "id": "d1", "title": "Main", "tiles": tiles })]);
    // A measure no tile uses goes without a question.
    let keep: Vec<NamedMeasure> =
        w.e.measures()
            .into_iter()
            .filter(|m| m.id != spare)
            .collect();
    w.e.set_measures(keep, false).unwrap();
    // The other one lists its tiles and stays.
    let err = w.e.set_measures(vec![], false).unwrap_err();
    let used = err.downcast_ref::<MeasureInUse>().expect("tiles use it");
    assert_eq!(used.used.len(), 1);
    assert_eq!(used.used[0].measure, id);
    assert_eq!(used.used[0].tiles.len(), 2);
    assert_eq!(used.used[0].tiles[0].dashboard_title, "Main");
    assert_eq!(used.used[0].tiles[0].title, "By store");
    assert_eq!(w.e.measures().len(), 1);
    // After the user says yes, it goes. The tile says what happened.
    w.e.set_measures(vec![], true).unwrap();
    assert!(w.e.measures().is_empty());
    let t: Tile = serde_json::from_value(tiles[0].clone()).unwrap();
    let e = run_tile(&w.e, &t, &[]).unwrap_err();
    assert!(e.to_string().contains("measure that was deleted"), "{e}");
}

#[test]
fn the_limits_of_a_save_hold_when_a_file_is_opened() {
    let w = world("hostile");
    w.add("Good", "SUM([sales].[amount])");
    let path: &Path = &w.d.join("base.tenun");
    w.e.save(Some(path.to_path_buf())).unwrap();
    // A hostile file: a measure of 10,001 characters, and more measures than a workspace uses.
    let mut file: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let long = format!("SUM([sales].[amount]){}", " + 0".repeat(2_500));
    assert!(long.chars().count() > 10_000);
    let mut list = vec![json!({ "id": "m1", "name": "Long", "formula": long })];
    for i in 2..=203 {
        list.push(json!({ "id": format!("m{i}"), "name": format!("Fill {i}"), "formula": "SUM([sales].[amount])" }));
    }
    file["measures"] = json!(list);
    std::fs::write(path, serde_json::to_string(&file).unwrap()).unwrap();
    w.e.open(path).unwrap();
    assert_eq!(w.e.measures().len(), 203, "everything stays in the file");
    let info = w.e.snapshot().measure_info;
    assert!(
        info["m1"]
            .error
            .as_deref()
            .unwrap()
            .contains("10000 characters"),
        "{:?}",
        info["m1"]
    );
    assert!(
        info["m203"].error.as_deref().unwrap().contains("first 200"),
        "{:?}",
        info["m203"]
    );
    assert!(info["m2"].error.is_none());
    // The tile that uses the long one says why; the rest of the workspace works.
    let q: SemanticQuery = serde_json::from_value(json!({ "measures": [named("m1")] })).unwrap();
    let e = w.e.query(&q).unwrap_err();
    assert!(
        e.to_string().contains("Long") && e.to_string().contains("10000"),
        "{e}"
    );
    let q: SemanticQuery = serde_json::from_value(json!({ "measures": [named("m203")] })).unwrap();
    assert!(w.e.query(&q).unwrap_err().to_string().contains("first 200"));
    let q: SemanticQuery = serde_json::from_value(json!({ "measures": [named("m2")] })).unwrap();
    near(&w.e.query(&q).unwrap().rows[0][0], 700.0);
    // A save that adds one past the limit, or a long one, is refused.
    let mut list = w.e.measures();
    list.truncate(200);
    list.push(measure("One more", "SUM([sales].[amount])"));
    assert!(w.e.set_measures(list, false).is_err());
}

#[test]
fn a_table_cut_by_its_row_limit_says_how_many_rows_it_has_with_a_named_measure() {
    let mut sales = String::from("id,date,store_id,product,amount\n");
    for i in 1..=600 {
        sales.push_str(&format!("{i},2026-01-05,1,Latte,{i}\n"));
    }
    let w = world_of("rowlimit", &sales, STORES, TARGETS);
    let id = w.add("Revenue", "SUM([sales].[amount])");
    let t = w.tile(
        "table",
        json!([named(&id)]),
        json!([w.dim("sales", "id")]),
        json!({}),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    assert_eq!(r.rows.len(), 500);
    assert_eq!(r.total_rows, Some(600));
}

#[test]
fn a_file_with_no_measures_is_written_as_it_was() {
    let w = world("bytes");
    let path = w.d.join("a.tenun");
    w.e.save(Some(path.clone())).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains("measures") && !text.contains("mainDate"),
        "{text}"
    );
    w.add("Revenue", "SUM([sales].[amount])");
    w.e.save(Some(path.clone())).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"measures\""), "{text}");
}

#[test]
fn a_tile_filter_dashboard_filter_and_group_work_together_with_a_click() {
    let w = world("click");
    let id = w.add("Achievement", ACHIEVEMENT);
    let t = w.tile(
        "bar",
        json!([named(&id)]),
        json!([w.dim("stores", "name")]),
        json!({}),
    );
    // A click on the bar "Java" of another tile filters by the region of the store.
    let click = DashFilter {
        filter: serde_json::from_value(json!({
            "table": w.stores, "column": "region", "op": "in", "values": ["Java"]
        }))
        .unwrap(),
        from_tile: "other".into(),
    };
    let r = run_tile(&w.e, &t, &[click]).unwrap();
    assert_eq!(r.rows.len(), 2);
    same(&r, "Kopi A", 0.5);
    same(&r, "Kopi B", 0.75);
}

#[test]
fn the_snapshot_says_which_measures_add_up() {
    let w = world("additive");
    let ids: Vec<(String, bool)> = [
        ("Revenue", "SUM([sales].[amount])", true),
        (
            "Net",
            "SUM([sales].[amount]) - SUM([targets].[target])",
            true,
        ),
        ("Scaled", "SUM([sales].[amount]) * 1.11", true),
        ("Plus", "SUM([sales].[amount]) + 5", false),
        ("Ratio", ACHIEVEMENT, false),
        ("Mean", "AVERAGE([sales].[amount])", false),
        ("Distinct", "COUNTD([sales].[product])", false),
        ("Rounded", "ROUND(SUM([sales].[amount]), 0)", false),
        ("Rows", "COUNTROWS([sales])", true),
    ]
    .iter()
    .map(|(n, f, a)| (w.add(n, f), *a))
    .collect();
    let info = w.e.snapshot().measure_info;
    for (id, additive) in &ids {
        assert_eq!(
            info[id].additive,
            *additive,
            "{}",
            w.e.measures().iter().find(|m| &m.id == id).unwrap().name
        );
        assert!(info[id].error.is_none());
        assert_eq!(info[id].dtype, Some(tenun_core::catalog::DType::Number));
    }
    // Sales and targets are the two tables a ratio adds up.
    assert_eq!(info[&ids[4].0].tables, [w.sales.clone(), w.targets.clone()]);
}

#[test]
fn a_time_chart_fills_a_quiet_month_with_zero_for_a_sum_and_a_gap_for_a_ratio() {
    // Sales in January and April, targets in January and March. February has nothing at all.
    let sales =
        "id,date,store_id,product,amount\n1,2026-01-05,1,Latte,10\n2,2026-04-02,1,Latte,30\n";
    let targets = "store_id,month,target\n1,2026-01-01,20\n1,2026-03-01,40\n";
    let w = world_of("fill", sales, STORES, targets);
    let ratio = w.add("Achievement", ACHIEVEMENT);
    let sum = w.add("Revenue", "SUM([sales].[amount])");
    let t = w.tile(
        "line",
        json!([named(&sum), named(&ratio)]),
        json!([w.month()]),
        json!({}),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    let keys: Vec<String> = r.rows.iter().map(|r| snapshot::js_key(&r[0])).collect();
    assert_eq!(
        keys,
        ["2026-01-01", "2026-02-01", "2026-03-01", "2026-04-01"]
    );
    // February: nothing happened. A sum of nothing is 0 and a ratio of nothing is a gap.
    near(&r.rows[1][1], 0.0);
    assert!(r.rows[1][2].is_null());
    // March has a target and no sales: 0 of 40.
    near(&r.rows[2][1], 0.0);
    near(&r.rows[2][2], 0.0);
    // April has sales and no target: the ratio is a gap.
    near(&r.rows[3][1], 30.0);
    assert!(r.rows[3][2].is_null());
}

#[test]
fn a_table_with_a_trend_column_fills_quiet_periods_by_the_measure() {
    let w = world("spark");
    let sum = w.add("Revenue", "SUM([sales].[amount])");
    let t = w.tile(
        "table",
        json!([named(&sum)]),
        json!([w.dim("stores", "name")]),
        json!({ "spark": w.month() }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    let sparks = r.sparks.expect("a trend for each row");
    // Kopi C sold only in February: January is 0, not a gap.
    let line = &sparks["Kopi C"];
    assert_eq!(line.iter().flatten().count(), 12);
}

// ---- the routes ----------------------------------------------------------------------------

async fn served(tag: &str) -> (tenun::Running, String, String, String) {
    let running = tenun::start(tenun::Options {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        workspace: None,
        picker: None,
    })
    .await
    .unwrap();
    let d = dir(tag);
    for (f, text) in [
        ("sales.csv", SALES),
        ("stores.csv", STORES),
        ("targets.csv", TARGETS),
    ] {
        std::fs::write(d.join(f), text).unwrap();
    }
    let e = &running.state.engine;
    for f in ["sales.csv", "stores.csv", "targets.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    let snap = e.snapshot();
    let id = |name: &str| {
        snap.tables
            .iter()
            .find(|t| t.name == name)
            .unwrap()
            .id
            .clone()
    };
    let (sales, stores, targets) = (id("sales"), id("stores"), id("targets"));
    e.add_relationship(field(&sales, "store_id"), field(&stores, "id"))
        .unwrap();
    e.add_relationship(field(&targets, "store_id"), field(&stores, "id"))
        .unwrap();
    (running, sales, stores, targets)
}

/// Sends JSON with the method and returns the status and the JSON answer.
async fn send(running: &tenun::Running, method: &str, path: &str, body: Value) -> (u16, Value) {
    let url = format!("http://{}{path}", running.addr);
    let method = method.to_string();
    tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let req = match method.as_str() {
            "PUT" => agent.put(url),
            _ => agent.post(url),
        };
        let mut res = req
            .header("content-type", "application/json")
            .header("x-tenun", "1")
            .send(body.to_string())
            .unwrap();
        let status = res.status().as_u16();
        let text = res.body_mut().read_to_string().unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_route_saves_refuses_asks_and_the_tile_route_uses_the_measure() {
    let (s, sales, stores, _targets) = served("route").await;
    let ratio = json!({ "id": "", "name": "Achievement", "formula": ACHIEVEMENT,
                        "format": { "percent": true, "decimals": 1 } });
    // A good save answers the snapshot, with the id, the info and the format.
    let (status, v) = send(
        &s,
        "PUT",
        "/api/workspace/measures",
        json!({ "measures": [ratio] }),
    )
    .await;
    assert_eq!(status, 200);
    assert!(v.get("ok").is_none(), "{v}");
    assert_eq!(v["workspace"]["measures"][0]["id"], "m1");
    assert_eq!(v["workspace"]["measures"][0]["format"]["percent"], true);
    assert_eq!(v["measure_info"]["m1"]["additive"], false);
    assert_eq!(v["measure_info"]["m1"]["dtype"], "number");
    assert_eq!(
        v["measure_info"]["m1"]["tables"].as_array().unwrap().len(),
        2
    );

    // A formula that does not work is a normal answer with a place in UTF-16 units.
    let bad = json!({ "id": "", "name": "Bad", "formula": "\"😀\" & SUM([sales].[amout])" });
    let (status, v) = send(
        &s,
        "PUT",
        "/api/workspace/measures",
        json!({ "measures": [ratio, bad] }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(v["ok"], false);
    assert_eq!(v["measure"], 1);
    assert!(v["error"]["at"].as_u64().unwrap() >= 6, "{v}");

    // The tile route draws a tile that uses the measure: a bar by store.
    let tile = json!({ "id": "t", "type": "bar", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
        "measures": [{ "named": "m1", "table": "", "column": null, "agg": "sum" }],
        "dims": [{ "table": stores, "column": "name" }] });
    let (status, v) = send(
        &s,
        "POST",
        "/api/tile",
        json!({ "tile": tile, "filters": [] }),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let rows = v["rows"].as_array().unwrap();
    let kopi_a = rows.iter().find(|r| r[0] == "Kopi A").unwrap();
    assert!((kopi_a[1].as_f64().unwrap() - 0.5).abs() < 1e-9);

    // A tile that uses it makes the delete ask first.
    s.state.engine.set_dashboards(vec![json!({
        "id": "d1", "title": "Main", "tiles": [tile] })]);
    let (_, v) = send(
        &s,
        "PUT",
        "/api/workspace/measures",
        json!({ "measures": [] }),
    )
    .await;
    assert_eq!(v["ok"], false);
    assert_eq!(v["used"][0]["measure"], "m1");
    assert_eq!(v["used"][0]["tiles"][0]["dashboardTitle"], "Main");
    let (_, v) = send(
        &s,
        "PUT",
        "/api/workspace/measures",
        json!({ "measures": [], "confirm": true }),
    )
    .await;
    assert!(v.get("ok").is_none(), "{v}");
    assert!(v["workspace"].get("measures").is_none());

    // The rows behind a bar of a measure on one table.
    let rev = json!({ "id": "", "name": "Revenue", "formula": "SUM([sales].[amount])" });
    send(
        &s,
        "PUT",
        "/api/workspace/measures",
        json!({ "measures": [rev] }),
    )
    .await;
    let t2 = json!({ "id": "t2", "type": "kpi", "title": "", "x": 0, "y": 0, "w": 3, "h": 2,
        "measures": [{ "named": "m2", "table": "", "column": null, "agg": "sum" }], "dims": [] });
    // m1 was deleted while a tile named it, so the new measure is m2.
    assert_eq!(v_id(&s), "m2");
    let (status, v) = send(
        &s,
        "POST",
        "/api/rows",
        json!({ "tile": t2, "filters": [], "click": null }),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["total"], 8);
    let _ = sales;
}

fn v_id(s: &tenun::Running) -> String {
    s.state.engine.measures()[0].id.clone()
}

// ---- the review of build 03 ------------------------------------------------------------------
//
// Stores 1, 2, 3 and 5 (store 5 has a target and no sale), products with a launch date, sales, a
// monthly target for each store, tickets (their dates belong to no total) and transfers between stores.
// Sales 530 in all, targets 2600.

const R_STORES: &str =
    "id,name,region\n1,Kopi A,Java\n2,Kopi B,Java\n3,Kopi C,Bali\n5,Kopi E,Bali\n";
const R_PRODUCTS: &str = "id,name,category,launch\n1,Latte,Drink,2024-02-01\n2,Mocha,Drink,2025-03-01\n3,Mug,Gear,2024-06-01\n";
const R_SALES: &str = "id,date,store_id,product_id,amount\n1,2025-01-15,1,1,100\n2,2025-02-10,1,2,50\n\
    3,2025-03-05,2,1,200\n4,2025-03-25,2,3,30\n5,2025-04-12,3,2,80\n6,2025-05-20,1,1,70\n";
const R_TARGETS: &str = "store_id,month,target\n1,2025-01-01,300\n1,2025-02-01,300\n1,2025-03-01,300\n\
    1,2025-04-01,300\n1,2025-05-01,300\n2,2025-03-01,200\n2,2025-04-01,200\n3,2025-04-01,100\n\
    3,2025-05-01,100\n5,2025-02-01,500\n";
const R_TICKETS: &str = "store_id,created,closed,n\n1,2025-01-10,2025-01-20,3\n1,2025-03-10,2025-03-12,1\n\
    2,2025-02-02,2025-02-09,2\n";
const R_TRANSFERS: &str = "id,from_store,to_store,amount,day\n1,1,2,10,2025-01-05\n2,1,3,20,2025-02-05\n\
    3,2,1,40,2025-03-05\n4,3,2,80,2025-04-05\n";

struct R {
    w: World,
    products: String,
    tickets: String,
    transfers: String,
    sent_from: String,
}

fn review_world(tag: &str) -> R {
    review_world_with(tag, R_TARGETS)
}

fn review_world_with(tag: &str, targets: &str) -> R {
    let w = world_of(tag, R_SALES, R_STORES, targets);
    for (f, text) in [
        ("products.csv", R_PRODUCTS),
        ("tickets.csv", R_TICKETS),
        ("transfers.csv", R_TRANSFERS),
    ] {
        std::fs::write(w.d.join(f), text).unwrap();
        w.e.add_source(AddSource::File {
            path: w.d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    let snap = w.e.snapshot();
    let id = |n: &str| snap.tables.iter().find(|t| t.name == n).unwrap().id.clone();
    let (products, tickets, transfers) = (id("products"), id("tickets"), id("transfers"));
    w.e.add_relationship(field(&w.sales, "product_id"), field(&products, "id"))
        .unwrap();
    w.e.add_relationship(field(&tickets, "store_id"), field(&w.stores, "id"))
        .unwrap();
    let from =
        w.e.add_relationship(field(&transfers, "from_store"), field(&w.stores, "id"))
            .unwrap();
    let to =
        w.e.add_relationship(field(&transfers, "to_store"), field(&w.stores, "id"))
            .unwrap();
    w.e.rename_links(
        [
            (from.clone(), Some("Sent from".to_string())),
            (to, Some("Sent to".to_string())),
        ]
        .into(),
    )
    .unwrap();
    R {
        w,
        products,
        tickets,
        transfers,
        sent_from: from,
    }
}

impl R {
    fn notes(&self, r: &QueryResult) -> Vec<String> {
        r.parts.iter().map(|p| p.note.clone()).collect()
    }
}

#[test]
fn a_date_that_is_not_a_main_date_is_not_read_through_one() {
    let r = review_world("d98");
    let id = r.w.add("Achievement", ACHIEVEMENT);
    // The launch of a product is not a day of sale. The targets are not split by it: all 2600 stand
    // against each year, and the words say so (V4-D98).
    let launch_year = json!({ "table": r.products, "column": "launch", "grain": "year" });
    let by_year =
        r.w.q(json!({ "measures": [named(&id)], "dimensions": [launch_year] }));
    same(&by_year, "2024-01-01", 400.0 / 2600.0);
    same(&by_year, "2025-01-01", 130.0 / 2600.0);
    assert_eq!(r.notes(&by_year), ["targets is not split by launch."]);
    assert!(
        by_year.parts[0].repeats,
        "the table repeats across the groups"
    );

    // The same for a filter on it: only the sales are filtered.
    let launch_2024 = json!({ "table": r.products, "column": "launch", "op": "between",
                              "values": ["2024-01-01", "2024-12-31"] });
    let filtered =
        r.w.q(json!({ "measures": [named(&id)], "filters": [launch_2024] }));
    near(&filtered.rows[0][0], 400.0 / 2600.0);
    assert_eq!(r.notes(&filtered), ["targets is not filtered by launch."]);
    assert!(
        !filtered.parts[0].repeats,
        "a filter does not repeat a table across groups"
    );

    // A filter on a table no part reaches is applied to no part, and the tile ignores it.
    let created = json!({ "table": r.tickets, "column": "created", "op": "between",
                          "values": ["2025-01-01", "2025-02-28"] });
    let ignored =
        r.w.q(json!({ "measures": [named(&id)], "filters": [created] }));
    near(&ignored.rows[0][0], 530.0 / 2600.0);
    assert_eq!(ignored.ignored_filters, vec![0]);

    // A date the user has picked as the main date of a table is one, whatever the table is for.
    let setup = TableSetup {
        main_date: Some("launch".into()),
        ..Default::default()
    };
    r.w.e.set_table_setup(&r.products, Some(setup)).unwrap();
    let picked = r.w.q(json!({ "measures": [named(&id)], "dimensions": [
        { "table": r.products, "column": "launch", "grain": "year" }] }));
    assert!(picked.parts.is_empty(), "{:?}", picked.parts);
    same(&picked, "2025-01-01", 130.0 / 2600.0);
}

#[test]
fn a_table_of_things_with_one_date_is_not_split_by_it_when_it_holds_a_part() {
    // V4-D122. Sales point at stores; each store has the day it opened. Sales per square metre by the
    // month of the sale: the floor space is the same 150 in every month, not the space of the stores that
    // opened in that month (none did, so every month would be blank).
    let w = world_of(
        "d122",
        "date,store_id,amount\n2026-01-10,1,300\n2026-01-20,2,150\n2026-02-05,1,200\n2026-03-15,2,90\n",
        "id,name,sqm,opened\n1,A,100,2024-05-01\n2,B,50,2025-07-01\n",
        "store_id,month,target\n1,2026-01-01,1\n",
    );
    let id = w.add("Per sqm", "SUM([sales].[amount]) / SUM([stores].[sqm])");
    let r = w.q(json!({ "measures": [named(&id)], "dimensions": [w.month()] }));
    same(&r, "2026-01-01", 450.0 / 150.0);
    same(&r, "2026-02-01", 200.0 / 150.0);
    same(&r, "2026-03-01", 90.0 / 150.0);
    let notes: Vec<String> = r.parts.iter().map(|p| p.note.clone()).collect();
    assert_eq!(
        notes,
        [
            "stores is not split by date. Its date opened is not a main date: pick it in Data if it is one."
        ]
    );
    assert!(r.parts[0].repeats);
    // A window of sale days filters the sales only.
    let january = json!({ "table": w.sales, "column": "date", "op": "between",
                          "values": ["2026-01-01", "2026-01-31"] });
    let f = w.q(json!({ "measures": [named(&id)], "filters": [january] }));
    near(&f.rows[0][0], 450.0 / 150.0);
    let notes: Vec<String> = f.parts.iter().map(|p| p.note.clone()).collect();
    assert_eq!(
        notes,
        [
            "stores is not filtered by date. Its date opened is not a main date: pick it in Data if it is one."
        ]
    );
}

#[test]
fn a_date_of_a_table_with_no_link_reads_the_part_by_its_own_date_and_says_so() {
    // V4-D111 reads a table with no link as a table of events. When that table holds no part of the
    // question, it may be a table of things whose link is not drawn yet, so the tile says what it did.
    let d = dir("d122-unlinked");
    std::fs::write(
        d.join("sales.csv"),
        "date,product_id,amount\n2026-01-05,1,100\n2026-02-10,2,40\n2026-02-20,1,60\n",
    )
    .unwrap();
    std::fs::write(
        d.join("products.csv"),
        "id,launch\n1,2024-02-01\n2,2025-03-01\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    for f in ["sales.csv", "products.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    let snap = e.snapshot();
    let id = |name: &str| {
        snap.tables
            .iter()
            .find(|t| t.name == name)
            .unwrap()
            .id
            .clone()
    };
    let w = World {
        e,
        d,
        sales: id("sales"),
        stores: String::new(),
        targets: String::new(),
    };
    let products = id("products");
    let sales = w.add("Sales", "SUM([sales].[amount])");
    let by_launch = json!({ "measures": [named(&sales)],
        "dimensions": [{ "table": products, "column": "launch", "grain": "month" }] });
    let r = w.q(by_launch.clone());
    // Read by the month of the sale: January 100, February 40 + 60.
    same(&r, "2026-01-01", 100.0);
    same(&r, "2026-02-01", 100.0);
    let notes: Vec<String> = r.parts.iter().map(|p| p.note.clone()).collect();
    assert_eq!(
        notes,
        ["products has no link to sales, so sales is split by its own date, date."]
    );
    // With the link drawn, the launch month is the launch month: product 1 (Feb 2024) 160, product 2 40.
    w.e.add_relationship(field(&w.sales, "product_id"), field(&products, "id"))
        .unwrap();
    let r = w.q(by_launch);
    same(&r, "2024-02-01", 160.0);
    same(&r, "2025-03-01", 40.0);
    assert!(r.parts.is_empty(), "{:?}", r.parts);
}

#[test]
fn a_day_window_against_a_monthly_main_date_says_which_month_it_leaves_out_or_keeps() {
    let r = review_world("d98-window");
    let id = r.w.add("Achievement", ACHIEVEMENT);
    // 10 March to 20 May. A target counts when the window holds the first day of its month: April and
    // May count, March does not (its first day is before the window), and May counts whole.
    let window = json!({ "table": r.w.sales, "column": "date", "op": "between",
                         "values": ["2025-03-10", "2025-05-20"] });
    let res =
        r.w.q(json!({ "measures": [named(&id)], "filters": [window] }));
    near(&res.rows[0][0], 180.0 / 1000.0);
    assert_eq!(
        r.notes(&res),
        [
            "targets is read by month: March is left out because the window starts on 10 March.",
            "targets is read by month: May is counted whole because the window ends on 20 May."
        ]
    );
    // A window on the edges of its months says nothing.
    let whole = json!({ "table": r.w.sales, "column": "date", "op": "between",
                        "values": ["2025-03-01", "2025-05-31"] });
    let res =
        r.w.q(json!({ "measures": [named(&id)], "filters": [whole] }));
    assert!(res.parts.is_empty(), "{:?}", res.parts);
    // A window that crosses a year names the years. "Last 12 months" from 15 June 2025 starts on
    // 16 June 2024 and ends on 15 June 2025, both inside a month.
    r.w.e
        .set_today(Some(NaiveDate::from_ymd_opt(2025, 6, 15).unwrap()));
    let last = json!({ "table": r.w.sales, "column": "date", "op": "relative", "values": ["last_12_months"] });
    let res =
        r.w.q(json!({ "measures": [named(&id)], "filters": [last] }));
    assert_eq!(
        r.notes(&res),
        [
            "targets is read by month: June 2024 is left out because the window starts on 16 June 2024.",
            "targets is read by month: June 2025 is counted whole because the window ends on 15 June 2025."
        ]
    );
    // A group by day is finer than the months the targets are read by.
    let by_day = json!({ "table": r.w.sales, "column": "date", "grain": "day" });
    let res =
        r.w.q(json!({ "measures": [named(&id)], "dimensions": [by_day] }));
    assert_eq!(
        r.notes(&res),
        [
            "targets is read by month: only the first day of each month has a value, so the other days are blank."
        ]
    );
    // Targets with days that are not the first of a month are not read by month.
    let daily = "store_id,month,target\n1,2025-01-15,300\n1,2025-02-10,300\n";
    let r2 = review_world_with("d98-daily", daily);
    let id2 = r2.w.add("Achievement", ACHIEVEMENT);
    let window = json!({ "table": r2.w.sales, "column": "date", "op": "between",
                         "values": ["2025-03-10", "2025-05-20"] });
    let res =
        r2.w.q(json!({ "measures": [named(&id2)], "filters": [window] }));
    assert!(res.parts.is_empty(), "{:?}", res.parts);
}

#[test]
fn a_measure_that_repeats_a_table_across_the_groups_does_not_add_up() {
    let r = review_world("repeat-gate");
    let gap =
        r.w.add("Gap", "SUM([sales].[amount]) - SUM([targets].[target])");
    let category = json!({ "table": r.products, "column": "category" });
    let res =
        r.w.q(json!({ "measures": [named(&gap)], "dimensions": [category.clone()] }));
    // 530 of sales against 2600 of targets: the bars are 2100 and 2570 short, 4670 in all, and the whole is
    // 2070. The targets stand against each group, so the bars do not add up to the whole.
    same(&res, "Drink", 500.0 - 2600.0);
    same(&res, "Gear", 30.0 - 2600.0);
    let bars: f64 = by(&res).iter().filter_map(|(_, v)| *v).sum();
    let whole = r.w.q(json!({ "measures": [named(&gap)] })).rows[0][0]
        .as_f64()
        .unwrap();
    assert!((bars - whole).abs() > 1.0, "{bars} against {whole}");
    assert!(res.parts.iter().any(|p| p.repeats), "{:?}", res.parts);
    // A share of the whole would add the repeated target up, so it is not applied, with words.
    let t = r.w.tile(
        "bar",
        json!([named(&gap)]),
        json!([category]),
        json!({ "showAs": ["pct_total"] }),
    );
    let shown = run_tile(&r.w.e, &t, &[]).unwrap();
    same(&shown, "Drink", 500.0 - 2600.0);
    assert!(shown.columns[1].unit.is_none());
    assert!(
        shown
            .notes
            .iter()
            .any(|n| n.contains("Gap is not shown as % of total")
                && n.contains("not split by these groups")),
        "{:?}",
        shown.notes
    );
    // A sum of sales alone adds up: the share works.
    let rev = r.w.add("Revenue", "SUM([sales].[amount])");
    let t = r.w.tile(
        "bar",
        json!([named(&rev)]),
        json!([{ "table": r.products, "column": "category" }]),
        json!({ "showAs": ["pct_total"] }),
    );
    let ok = run_tile(&r.w.e, &t, &[]).unwrap();
    same(&ok, "Drink", 500.0 / 530.0);
}

#[test]
fn a_plain_measure_that_does_not_add_up_is_not_shown_as_a_share_either() {
    let w = world("plain-gate");
    let dims = json!([w.dim("stores", "name")]);
    let avg = json!([{ "table": w.sales, "column": "amount", "agg": "avg" }]);
    let t = w.tile("bar", avg, dims.clone(), json!({ "showAs": ["pct_total"] }));
    let r = run_tile(&w.e, &t, &[]).unwrap();
    // Kopi A: (100 + 50 + 150) / 3. Not a share, and the name keeps no suffix.
    same(&r, "Kopi A", 100.0);
    assert!(r.columns[1].unit.is_none());
    assert!(!r.columns[1].name.contains('%'), "{}", r.columns[1].name);
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("is not shown as % of total: its values do not add up")),
        "{:?}",
        r.notes
    );
    // A running total of a count of distinct values is not one either; a sum keeps it.
    let distinct = json!([{ "table": w.sales, "column": "product", "agg": "count_distinct" }]);
    let t = w.tile(
        "line",
        distinct,
        json!([w.month()]),
        json!({ "showAs": ["running"] }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    assert!(
        r.notes.iter().any(|n| n.contains("running total")),
        "{:?}",
        r.notes
    );
    let sum = json!([{ "table": w.sales, "column": "amount", "agg": "sum" }]);
    let t = w.tile(
        "line",
        sum,
        json!([w.month()]),
        json!({ "showAs": ["running"] }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    assert!(r.notes.is_empty(), "{:?}", r.notes);
    near(&r.rows[1][1], 700.0);
}

#[test]
fn a_part_that_reads_a_failed_column_is_blank_and_the_tile_says_so() {
    let w = world("failed-calc");
    // A column of sales computed from `amount`, and a measure that adds it up.
    let setup = TableSetup {
        calc: vec![tenun_core::workspace::CalcColumn {
            name: "Doubled".into(),
            formula: "[amount] * 2".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    w.e.set_table_setup(&w.sales, Some(setup)).unwrap();
    let id = w.add("Doubled sales", "SUM([sales].[Doubled])");
    let ratio = w.add(
        "Doubled against targets",
        "SUM([sales].[Doubled]) / SUM([targets].[target])",
    );
    near(&w.q(json!({ "measures": [named(&id)] })).rows[0][0], 1400.0);
    // The file loses `amount`: the column cannot be computed, and its values are blank.
    std::fs::write(
        w.d.join("sales.csv"),
        "id,date,store_id,product\n1,2026-01-05,1,Latte\n",
    )
    .unwrap();
    let source =
        w.e.snapshot()
            .tables
            .iter()
            .find(|t| t.id == w.sales)
            .unwrap()
            .source_id
            .clone();
    w.e.load(&source);
    let t = w.tile("kpi", json!([named(&id)]), json!([]), json!({}));
    let res = run_tile(&w.e, &t, &[]).unwrap();
    // Blank, not 0 (V4-D22's zero is for no rows).
    assert!(res.rows[0][0].is_null(), "{:?}", res.rows);
    assert!(
        res.notes
            .iter()
            .any(|n| n.contains("Doubled could not be computed")),
        "{:?}",
        res.notes
    );
    // What leaves the computer says the table and the column only.
    let public = snapshot::run_tile_public(&w.e, &t, &[]).unwrap();
    assert_eq!(public.notes, ["sales: Doubled could not be computed."]);
    // A formula with another part: the part that reads the failed column is blank, so the ratio is.
    let t = w.tile("kpi", json!([named(&ratio)]), json!([]), json!({}));
    let res = run_tile(&w.e, &t, &[]).unwrap();
    assert!(res.rows[0][0].is_null(), "{:?}", res.rows);
    // In a tile with a group and a plain sum of the failed column beside it, nothing turns to 0.
    let plain = json!({ "table": w.sales, "column": "Doubled", "agg": "sum" });
    let t = w.tile(
        "bar",
        json!([named(&ratio), plain]),
        json!([w.dim("stores", "name")]),
        json!({}),
    );
    let res = run_tile(&w.e, &t, &[]).unwrap();
    assert!(
        res.rows.iter().all(|r| r[2].is_null() && r[1].is_null()),
        "{:?}",
        res.rows
    );
}

#[test]
fn rows_behind_a_mark_of_a_measure_on_one_table_follow_the_click_through_its_main_date() {
    let r = review_world("rows-click");
    let targets = r.w.add("Targets", "SUM([targets].[target])");
    let t = r.w.tile(
        "bar",
        json!([named(&targets)]),
        json!([{ "table": r.w.sales, "column": "date", "grain": "month" }]),
        json!({}),
    );
    // The click on March of a chart by the date of the sales filters the sales by month; the rows are those of
    // the targets, read by their main date.
    let click: tenun_core::query::Filter = serde_json::from_value(json!({
        "table": r.w.sales, "column": "date", "grain": "month", "op": "in", "values": ["2025-03-01"] }))
    .unwrap();
    let rows = tenun_core::filters::rows_behind(&r.w.e, &t, &[], Some(click)).unwrap();
    assert_eq!(rows.total, 2, "the targets of store 1 and store 2 in March");
    // And with no click: every row of the table.
    let all = tenun_core::filters::rows_behind(&r.w.e, &t, &[], None).unwrap();
    assert_eq!(all.total, 10);
}

#[test]
fn what_leaves_the_computer_never_quotes_a_formula_or_a_text_typed_in_it() {
    let r = review_world("public-error");
    r.w.add("Fine", "SUM([sales].[amount])");
    // A file made by hand holds a measure whose message would quote its text.
    let path = r.w.d.join("secret.tenun");
    r.w.e.save(Some(path.clone())).unwrap();
    let mut file: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    file["measures"] = json!([
        { "id": "m1", "name": "Secret", "formula": "SUMIF([sales].[amount], [sales].[amount] = \"web secret\")" }
    ]);
    std::fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();
    r.w.e.open(&path).unwrap();
    let tile = json!({ "id": "t", "type": "kpi", "title": "", "x": 0, "y": 0, "w": 3, "h": 2,
                       "measures": [named("m1")], "dims": [] });
    r.w.e.set_dashboards(vec![
        json!({ "id": "d1", "title": "Main", "tiles": [tile.clone()] }),
    ]);
    let t: Tile = serde_json::from_value(tile).unwrap();
    // The app says why, with the words of the check.
    let app = run_tile(&r.w.e, &t, &[]).unwrap_err().to_string();
    assert!(app.contains("web secret"), "{app}");
    // Everything that leaves the computer says which measure and what happened.
    let public = snapshot::run_tile_public(&r.w.e, &t, &[])
        .unwrap_err()
        .to_string();
    assert_eq!(public, "The measure Secret cannot be worked out.");
    let built = snapshot::build(&r.w.e, "d1", &[]).unwrap_err().to_string();
    assert!(
        !built.contains("web secret") && built.contains("Secret"),
        "{built}"
    );
}

#[test]
fn a_refusal_counts_in_the_text_that_was_sent_and_says_which_field() {
    let w = world("save-places");
    // The live check reads the text as it is typed; so does the save.
    let raw = "   SUM([sales].[amoun])";
    let checked =
        w.e.check_formula(&tenun_core::formula::CheckRequest {
            formula: raw.into(),
            kind: tenun_core::formula::Kind::Measure,
            table: None,
        })
        .unwrap();
    let live = checked.error.unwrap();
    let err =
        w.e.set_measures(vec![measure("Bad", raw)], false)
            .unwrap_err();
    let refused = err.downcast_ref::<MeasureRefused>().unwrap();
    let reply = refused.reply();
    assert_eq!(reply["field"], "formula");
    assert_eq!(reply["error"]["at"], live.at as u64);
    assert_eq!(reply["error"]["len"], live.len as u64);
    // A name that cannot be used is about the name, not the formula.
    let err =
        w.e.set_measures(vec![measure("", "SUM([sales].[amount])")], false)
            .unwrap_err();
    assert_eq!(
        err.downcast_ref::<MeasureRefused>().unwrap().reply()["field"],
        "name"
    );
    // Too many measures, or a repeated id, are a normal refusal too, about the list.
    let many: Vec<NamedMeasure> = (0..201)
        .map(|i| NamedMeasure {
            id: format!("m{}", i + 1),
            name: format!("Measure {i}"),
            formula: "SUM([sales].[amount])".into(),
            ..Default::default()
        })
        .collect();
    let err = w.e.set_measures(many, false).unwrap_err();
    let reply = err.downcast_ref::<MeasureRefused>().unwrap().reply();
    assert_eq!(
        (
            reply["ok"].clone(),
            reply["field"].clone(),
            reply["measure"].clone()
        ),
        (json!(false), json!("list"), json!(200))
    );
    let twice = vec![
        NamedMeasure {
            id: "m1".into(),
            name: "One".into(),
            formula: "SUM([sales].[amount])".into(),
            ..Default::default()
        },
        NamedMeasure {
            id: "m1".into(),
            name: "Two".into(),
            formula: "SUM([sales].[amount])".into(),
            ..Default::default()
        },
    ];
    let err = w.e.set_measures(twice, false).unwrap_err();
    let reply = err.downcast_ref::<MeasureRefused>().unwrap().reply();
    assert_eq!(
        (reply["field"].clone(), reply["measure"].clone()),
        (json!("list"), json!(1))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_measure_sent_with_no_id_gets_one() {
    let (s, _sales, _stores, _targets) = served("route-noid").await;
    // The server gives the id: a measure of the body may leave it out.
    let (status, v) = send(
        &s,
        "PUT",
        "/api/workspace/measures",
        json!({ "measures": [{ "name": "Revenue", "formula": "SUM([sales].[amount])" }] }),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["workspace"]["measures"][0]["id"], "m1");
    // And a refusal is the same normal answer, with the field and the place in the text that was sent.
    let (status, v) = send(
        &s,
        "PUT",
        "/api/workspace/measures",
        json!({ "measures": [{ "name": "", "formula": "SUM([sales].[amount])" }] }),
    )
    .await;
    assert_eq!(
        (status, v["ok"].clone(), v["field"].clone()),
        (200, json!(false), json!("name"))
    );
}

#[test]
fn a_part_follows_a_group_that_names_a_link_by_its_own_way_to_the_same_table() {
    let r = review_world("via");
    let id = r.w.add(
        "Moved against targets",
        "SUM([transfers].[amount]) / SUM([targets].[target])",
    );
    // Transfers by the store that sent them. The targets have their own link to the stores, and are split
    // by it: 30 sent by Kopi A against its 1500 of targets, and so on (V4-D107).
    let res = r.w.q(json!({
        "measures": [named(&id)],
        "dimensions": [{ "table": r.w.stores, "column": "name", "via": r.sent_from }],
    }));
    assert!(res.parts.is_empty(), "{:?}", res.parts);
    same(&res, "Kopi A", 30.0 / 1500.0);
    same(&res, "Kopi B", 40.0 / 400.0);
    same(&res, "Kopi C", 80.0 / 200.0);
    // Kopi E has targets and sent nothing.
    same(&res, "Kopi E", 0.0);
    let _ = r.transfers;
}

#[test]
fn a_group_value_only_a_part_that_cannot_see_all_the_groups_has_appears_once() {
    let r = review_world("vanish");
    let id = r.w.add("Achievement", ACHIEVEMENT);
    let res = r.w.q(json!({
        "measures": [named(&id)],
        "dimensions": [
            { "table": r.w.stores, "column": "name" },
            { "table": r.products, "column": "category" }
        ],
    }));
    // The sales have rows for stores 1, 2 and 3 by category. Store 5 has a target and no sale: it is a row
    // of its own with the category blank, and its value is what the rules give (no sales is 0, over 500).
    let at = |s: &str, c: Option<&str>| -> Vec<f64> {
        res.rows
            .iter()
            .filter(|r| r[0] == json!(s) && r[1] == c.map_or(Value::Null, |c| json!(c)))
            .map(|r| r[2].as_f64().unwrap())
            .collect()
    };
    assert_eq!(at("Kopi E", None), [0.0]);
    assert!((at("Kopi A", Some("Drink"))[0] - 220.0 / 1500.0).abs() < 1e-9);
    assert!((at("Kopi B", Some("Drink"))[0] - 0.5).abs() < 1e-9);
    assert!((at("Kopi B", Some("Gear"))[0] - 30.0 / 400.0).abs() < 1e-9);
    assert!((at("Kopi C", Some("Drink"))[0] - 0.4).abs() < 1e-9);
    // A store that has sales has no extra blank row.
    for s in ["Kopi A", "Kopi B", "Kopi C"] {
        assert!(at(s, None).is_empty(), "{s}");
    }
    assert_eq!(res.rows.len(), 5, "{:?}", res.rows);
}

#[test]
fn a_plain_measure_beside_a_named_one_is_zero_for_a_group_it_has_no_rows_for() {
    // Sales in January and April, targets in January and March: March has no sale.
    let sales =
        "id,date,store_id,product,amount\n1,2026-01-05,1,Latte,10\n2,2026-04-02,1,Latte,30\n";
    let targets = "store_id,month,target\n1,2026-01-01,20\n1,2026-03-01,40\n";
    let w = world_of("plain-zero", sales, STORES, targets);
    let ratio = w.add("Achievement", ACHIEVEMENT);
    let count = json!({ "table": w.sales, "column": null, "agg": "count" });
    let sum = json!({ "table": w.sales, "column": "amount", "agg": "sum" });
    let avg = json!({ "table": w.sales, "column": "amount", "agg": "avg" });
    let r = w.q(json!({ "measures": [named(&ratio), count, sum, avg], "dimensions": [w.month()] }));
    let mar = r.rows.iter().find(|r| r[0] == json!("2026-03-01")).unwrap();
    // A count and a sum of no rows are 0, an average of no rows is blank.
    assert_eq!(mar[2], json!(0.0));
    assert_eq!(mar[3], json!(0.0));
    assert!(mar[4].is_null());
}

#[test]
fn a_change_of_main_date_alone_makes_the_tiles_ask_again() {
    let w = world("main-signal");
    let set = |c: Option<&str>| {
        w.e.set_table_setup(
            &w.targets,
            Some(TableSetup {
                main_date: c.map(String::from),
                ..Default::default()
            }),
        )
        .unwrap()
    };
    assert!(
        set(Some("month")),
        "a new main date is a change the tiles must read"
    );
    assert!(!set(Some("month")), "the same again is not");
    assert!(
        set(None),
        "and neither is going back to the default a non-change"
    );
}
