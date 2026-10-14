//! Parameters and what-if (v5 build 05, V5-D7): a number or a choice that a measure reads with `PARAM("…")`, that a
//! Top N can take, and that an export holds step by step (V5-D5).
//!
//! The data, by hand: stores A 100 · B 200 · C 300 · O'Brien 50 · D 10 → 660 in all. With a price rise of 5 %:
//! 693; of 10 %: 726. The value binds; the SQL text never holds it.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::params::Param;
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, Tile};
use tenun_core::workspace::{NamedMeasure, TableSetup, Workspace};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-param-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn data(d: &Path) -> (Engine, String) {
    std::fs::write(
        d.join("sales.csv"),
        "store,amount\nA,100\nB,200\nC,300\nO'Brien,50\nD,10\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.replace(
        Workspace {
            name: "Params".into(),
            ..Workspace::default()
        },
        Some(d.join("Params.tenun")),
    );
    let id = e
        .add_source(AddSource::File {
            path: d.join("sales.csv").to_string_lossy().into_owned(),
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

fn number(name: &str, min: f64, max: f64, step: f64, value: f64) -> Param {
    serde_json::from_value(json!({ "name": name, "kind": "number", "min": min, "max": max, "step": step, "value": value }))
        .unwrap()
}

fn choice(name: &str, choices: Value, value: Value) -> Param {
    serde_json::from_value(
        json!({ "name": name, "kind": "choice", "choices": choices, "value": value }),
    )
    .unwrap()
}

fn measure(id: &str, name: &str, formula: &str) -> NamedMeasure {
    NamedMeasure {
        id: id.into(),
        name: name.into(),
        formula: formula.into(),
        ..NamedMeasure::default()
    }
}

/// Whether a value is this number, give or take the last bit of a float (660 × 1.1 is 726.0000000000001).
fn near(v: &Value, want: f64) -> bool {
    v.as_f64().is_some_and(|x| (x - want).abs() < 1e-9)
}

/// The grand total of a named measure.
fn total(e: &Engine, id: &str) -> Value {
    let q: SemanticQuery =
        serde_json::from_value(json!({ "measures": [{ "named": id }], "dimensions": [] })).unwrap();
    e.query(&q).unwrap().rows[0][0].clone()
}

#[test]
fn a_price_rise_moves_the_revenue_by_hand() {
    let (e, t) = data(&dir("rise"));
    e.set_params(vec![number("Price rise", 0.0, 0.2, 0.01, 0.0)])
        .unwrap();
    e.set_measures(
        vec![measure(
            "",
            "Revenue if prices rise",
            &format!("SUM([{t}].[amount]) * (1 + PARAM(\"Price rise\"))"),
        )],
        false,
    )
    .unwrap();
    let m = e.measures()[0].id.clone();
    assert!(near(&total(&e, &m), 660.0), "{}", total(&e, &m));
    // A sum times a number parameter still adds up: % of total and running totals work on it.
    assert!(e.measure_info()[&m].additive);
    e.set_param_values(BTreeMap::from([("p1".to_string(), json!(0.05))]))
        .unwrap();
    assert!(near(&total(&e, &m), 693.0), "{}", total(&e, &m));
    e.set_param_values(BTreeMap::from([("p1".to_string(), json!(0.10))]))
        .unwrap();
    assert!(near(&total(&e, &m), 726.0), "{}", total(&e, &m));
    // A value outside the range is kept at its end; a value that stands in for one call does not stay.
    e.set_param_values(BTreeMap::from([("p1".to_string(), json!(0.5))]))
        .unwrap();
    assert_eq!(e.params()[0].value, json!(0.2));
    let at = tenun_core::params::with(&BTreeMap::from([("p1".to_string(), json!(0.0))]), || {
        total(&e, &m)
    });
    assert!(near(&at, 660.0), "{at}");
    assert!(near(&total(&e, &m), 792.0), "{}", total(&e, &m));
    // The value is saved with the workspace and comes back.
    e.save(None).unwrap();
    let back = Workspace::load(&e.workspace_path().unwrap()).unwrap();
    assert_eq!(back.params[0].value, json!(0.2));
    assert_eq!(back.params[0].id, "p1");
}

#[test]
fn a_top_n_takes_its_number_from_a_parameter() {
    let (e, t) = data(&dir("top"));
    e.set_params(vec![number("Top", 1.0, 5.0, 1.0, 2.0)])
        .unwrap();
    let tile: Tile = serde_json::from_value(json!({ "id": "b", "type": "bar",
        "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": t, "column": "store" }],
        "top": { "n": 4, "other": true, "param": "p1" } }))
    .unwrap();
    let res = snapshot::run_tile(&e, &tile, &[]).unwrap();
    // C 300, B 200, then Other 160.
    assert_eq!(res.rows.len(), 3);
    assert_eq!(res.rows[0][0], json!("C"));
    assert_eq!(res.params, ["p1"]);
    e.set_param_values(BTreeMap::from([("p1".to_string(), json!(3.0))]))
        .unwrap();
    assert_eq!(snapshot::run_tile(&e, &tile, &[]).unwrap().rows.len(), 4);
    // A parameter that is gone: the tile's own number.
    e.set_params(vec![]).unwrap();
    assert_eq!(snapshot::run_tile(&e, &tile, &[]).unwrap().rows.len(), 5);
}

#[test]
fn a_rename_rewrites_formulas_and_a_column_refuses_a_parameter() {
    let (e, t) = data(&dir("rename"));
    e.set_params(vec![number("Price rise", 0.0, 0.2, 0.01, 0.1)])
        .unwrap();
    e.set_measures(
        vec![measure(
            "",
            "R",
            &format!("SUM([{t}].[amount]) * (1 + param(\"price RISE\"))"),
        )],
        false,
    )
    .unwrap();
    let mut list = e.params();
    list[0].name = "Kenaikan harga".into();
    e.set_params(list).unwrap();
    assert_eq!(
        e.measures()[0].formula,
        format!("SUM([{t}].[amount]) * (1 + param(\"Kenaikan harga\"))")
    );
    assert!(near(&total(&e, &e.measures()[0].id), 726.0));
    // An unknown name offers the closest.
    let err = e
        .set_measures(
            vec![measure(
                "",
                "S",
                &format!("SUM([{t}].[amount]) * PARAM(\"Kenaikan harg\")"),
            )],
            false,
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("Did you mean Kenaikan harga?"), "{err}");
    // A calculated column is built once, at load.
    let setup: TableSetup = serde_json::from_value(json!({ "calc": [
        { "name": "up", "formula": "[amount] * PARAM(\"Kenaikan harga\")" }
    ] }))
    .unwrap();
    let err = e.set_table_setup(&t, Some(setup)).unwrap_err().to_string();
    assert!(err.contains("built once, at load"), "{err}");
}

#[test]
fn a_choice_binds_as_text_whatever_it_holds() {
    let (e, t) = data(&dir("choice"));
    let evil = "'); DROP TABLE sales; --";
    e.set_params(vec![choice(
        "Store",
        json!(["A", "O'Brien", evil]),
        json!("O'Brien"),
    )])
    .unwrap();
    e.set_measures(
        vec![measure(
            "",
            "Of the store",
            &format!("SUMIF([{t}].[amount], [{t}].[store] = PARAM(\"Store\"))"),
        )],
        false,
    )
    .unwrap();
    let m = e.measures()[0].id.clone();
    assert_eq!(
        total(&e, &m),
        json!(50.0),
        "a quote in a value matches the row that holds it"
    );
    e.set_param_values(BTreeMap::from([("p1".to_string(), json!(evil))]))
        .unwrap();
    let v = total(&e, &m);
    assert!(v.is_null() || v == json!(0.0), "{v}");
    // Nothing changed: the table is all there.
    let q: SemanticQuery = serde_json::from_value(
        json!({ "measures": [{ "table": t, "column": "amount", "agg": "sum" }], "dimensions": [] }),
    )
    .unwrap();
    assert_eq!(e.query(&q).unwrap().rows[0][0], json!(660.0));
    // A value that is not a choice does not take.
    e.set_param_values(BTreeMap::from([("p1".to_string(), json!("Z"))]))
        .unwrap();
    assert_eq!(e.params()[0].value, json!(evil));
}

#[test]
fn an_export_holds_each_step_up_to_twenty_one() {
    let (e, t) = data(&dir("export"));
    e.set_params(vec![
        number("Price rise", 0.0, 0.2, 0.01, 0.0),
        number("Fine", 0.0, 0.21, 0.01, 0.0),
    ])
    .unwrap();
    e.set_measures(
        vec![
            measure(
                "",
                "Rise",
                &format!("SUM([{t}].[amount]) * (1 + PARAM(\"Price rise\"))"),
            ),
            measure(
                "",
                "Fine",
                &format!("SUM([{t}].[amount]) * (1 + PARAM(\"Fine\"))"),
            ),
        ],
        false,
    )
    .unwrap();
    let ms = e.measures();
    e.set_dashboards(vec![json!({ "id": "d1", "title": "What if", "refresh": 0, "tiles": [
        { "id": "k1", "type": "kpi", "title": "", "x": 0, "y": 0, "w": 3, "h": 2, "measures": [{ "named": ms[0].id }], "dims": [] },
        { "id": "k2", "type": "kpi", "title": "", "x": 3, "y": 0, "w": 3, "h": 2, "measures": [{ "named": ms[1].id }], "dims": [] },
        { "id": "s1", "type": "param", "title": "", "x": 6, "y": 0, "w": 3, "h": 2, "measures": [], "dims": [], "param": { "id": "p1" } }
    ] })]);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    // 21 values, the saved one is the page itself: 20 steps of the one tile that reads it.
    let steps = p["steps"]["p1"].as_object().unwrap();
    assert_eq!(steps.len(), 20);
    assert!(near(&steps["0.05"]["k1"]["kpi"]["value"], 693.0));
    assert!(near(&steps["0.2"]["k1"]["kpi"]["value"], 792.0));
    assert!(
        steps["0.05"].get("k2").is_none(),
        "k2 does not read Price rise"
    );
    // 22 values: the file holds only the saved value.
    assert!(p["steps"].get("p2").is_none());
    assert_eq!(p["base"]["k2"]["params"], json!(["p2"]));
    // The parameters go with the file, without their descriptions.
    assert_eq!(p["params"].as_array().unwrap().len(), 2);
    // Size: each step is one number tile.
    let size = serde_json::to_string(&p["steps"]).unwrap().len();
    assert!(size < 40_000, "steps take {size} bytes");
}

#[test]
fn a_saved_view_holds_its_values() {
    let (e, t) = data(&dir("view"));
    e.set_params(vec![number("Price rise", 0.0, 0.2, 0.01, 0.0)])
        .unwrap();
    e.set_measures(
        vec![measure(
            "",
            "Rise",
            &format!("SUM([{t}].[amount]) * (1 + PARAM(\"Price rise\"))"),
        )],
        false,
    )
    .unwrap();
    let m = e.measures()[0].id.clone();
    e.set_dashboards(vec![json!({ "id": "d1", "title": "V", "refresh": 0,
        "views": [{ "id": "v1", "name": "Ten percent", "filters": [], "params": { "p1": 0.1 } }],
        "tiles": [
            { "id": "k1", "type": "kpi", "title": "", "x": 0, "y": 0, "w": 3, "h": 2, "measures": [{ "named": m }], "dims": [] },
            { "id": "b", "type": "bar", "title": "", "x": 3, "y": 0, "w": 6, "h": 4,
              "measures": [{ "table": t, "column": "amount", "agg": "sum" }], "dims": [{ "table": t, "column": "store" }] }
        ] })]);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    assert!(near(&p["base"]["k1"]["kpi"]["value"], 660.0));
    assert!(near(
        &p["views"][0]["payload"]["base"]["k1"]["kpi"]["value"],
        726.0
    ));
    assert!(
        p["views"][0]["payload"].get("steps").is_none(),
        "a view holds its own values only"
    );
    // A click worked out ahead in the view, on forks of the engine in other threads, reads the view's values too:
    // store A is 100, so 110 at 10 %.
    let a = &p["views"][0]["payload"]["cross"]["b"]["A"]["k1"]["kpi"]["value"];
    assert!(near(a, 110.0), "{a}");
    // The saved value did not move.
    assert_eq!(e.params()[0].value, json!(0.0));
}
