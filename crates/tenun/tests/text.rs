//! Live numbers in a text tile (build 10): a named measure's value, its change and its period, worked out as a number
//! tile would show them, with the dashboard's filters, and for every click of an exported file.
//!
//! Orders (day, amount, region): 3 Aug 10 Java, 20 Aug 30 Bali, 2 Sep 5 Java, 14 Sep 20 Bali, 30 Sep 15 Java.
//! Revenue by month: August 40, September 40 (no change). Java only: August 10, September 20 (+100 %). Bali only:
//! September up to its last row, the 14th, 20; 1 to 14 August, nothing.
//! The third quarter (July to September): 80, and no second quarter to compare with.

use serde_json::{Value, json};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, is_static_tile, live_numbers, run_tile};
use tenun_core::workspace::NamedMeasure;

const ORDERS: &str = "day,amount,region\n2026-08-03,10,Java\n2026-08-20,30,Bali\n2026-09-02,5,Java\n2026-09-14,20,Bali\n2026-09-30,15,Java\n";
const TEXT: &str = "Revenue {{measure:m1}} ({{measure:m1|compare}}) in {{measure:m1|period}}; gone {{measure:m9}}; \\{{measure:m1x}} is text.";

fn engine() -> (Engine, String) {
    let d = std::env::temp_dir().join(format!("tenun-text-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("orders.csv"), ORDERS).unwrap();
    let e = Engine::new(Secrets::new(d.join("config").join("secrets.json"))).unwrap();
    e.add_source(AddSource::File {
        path: d.join("orders.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let t = e.snapshot().tables[0].clone();
    e.set_measures(
        vec![NamedMeasure {
            name: "Revenue".into(),
            formula: format!("SUM([{}].[amount])", t.label),
            ..NamedMeasure::default()
        }],
        false,
    )
    .unwrap();
    assert_eq!(e.snapshot().workspace.measures[0].id, "m1");
    (e, t.id)
}

fn text(extra: Value) -> Value {
    let mut v = json!({ "id": "note", "type": "text", "title": "", "measures": [], "dims": [], "text": TEXT });
    for (k, x) in extra.as_object().unwrap() {
        v[k] = x.clone();
    }
    v
}

fn values(e: &Engine, tile: Value, filters: &[DashFilter]) -> Value {
    let t: Tile = serde_json::from_value(tile).unwrap();
    serde_json::to_value(run_tile(e, &t, filters).unwrap().values.unwrap()).unwrap()
}

fn java(t: &str) -> DashFilter {
    serde_json::from_value(json!({ "table": t, "column": "region", "op": "in", "values": ["Java"], "fromTile": "bar" })).unwrap()
}

#[test]
fn a_text_reads_the_value_change_and_period_of_a_measure() {
    let (e, t) = engine();
    // The ids a text names, once each; the escaped one and a malformed one are text.
    assert_eq!(live_numbers(TEXT), ["m1", "m9"]);
    assert!(live_numbers("{{measure:}} {{measure:a b}} {{measure:m1|total}}").is_empty());
    let v = values(&e, text(json!({})), &[]);
    assert_eq!(
        v,
        json!({ "m1": 40.0, "m1|compare": 0.0, "m1|period": "September 2026" })
    );
    // With the dashboard filtered to Java: September 20 against August 10.
    let v = values(&e, text(json!({})), &[java(&t)]);
    assert_eq!(v["m1"], json!(20.0));
    assert_eq!(v["m1|compare"], json!(1.0));
    // The dashboard's grain: a quarter, with nothing before it.
    let v = values(&e, text(json!({ "grain": "quarter" })), &[]);
    assert_eq!(v["m1"], json!(80.0));
    assert_eq!(v["m1|compare"], Value::Null);
    // A text with no live number has no result, as before.
    let plain: Tile =
        serde_json::from_value(text(json!({ "text": "Just words, and \\{{measure:m1}}." })))
            .unwrap();
    assert!(is_static_tile(&plain));
    let live: Tile = serde_json::from_value(text(json!({}))).unwrap();
    assert!(!is_static_tile(&live));
}

#[test]
fn an_exported_text_follows_a_click_like_any_tile() {
    let (e, t) = engine();
    e.set_dashboards(vec![json!({ "id": "d", "title": "Words", "tiles": [
        { "id": "bar", "type": "bar", "title": "", "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
          "dims": [{ "table": t, "column": "region" }] },
        text(json!({})),
        { "id": "plain", "type": "text", "title": "", "measures": [], "dims": [], "text": "No numbers." }
    ]})]);
    let snap = snapshot::build(&e, "d", &[]).unwrap();
    assert_eq!(snap["base"]["note"]["values"]["m1"], json!(40.0));
    // The file names the measure, so the viewer writes the number with its name and format; m9 is gone.
    assert_eq!(snap["measures"][0]["id"], json!("m1"));
    assert_eq!(snap["measures"][0]["name"], json!("Revenue"));
    assert_eq!(snap["measures"].as_array().unwrap().len(), 1);
    assert!(snap["base"].get("plain").is_none());
    // A click on Java in the exported file: the text reads 20.
    assert_eq!(
        snap["cross"]["bar"]["Java"]["note"]["values"]["m1"],
        json!(20.0)
    );
    assert_eq!(
        snap["cross"]["bar"]["Bali"]["note"]["values"]["m1"],
        json!(20.0)
    );
    // Bali's last row is 14 September, so September is "so far" and is compared with 1 to 14 August, when Bali sold
    // nothing: no change to show, as a number tile would say.
    assert_eq!(
        snap["cross"]["bar"]["Bali"]["note"]["values"]["m1|period"],
        json!("Sep 1–14, 2026")
    );
    assert_eq!(
        snap["cross"]["bar"]["Bali"]["note"]["values"]["m1|compare"],
        Value::Null
    );
}
