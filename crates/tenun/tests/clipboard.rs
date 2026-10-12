//! Copy and paste (build 09, V4-D11): a named measure that comes with pasted tiles is checked and saved like one made
//! here. Its formula was renamed to this workspace's table first (`clipboard.ts rewriteFormula`); one that still names
//! a table this workspace does not have is refused in words, and the measures stay as they were.
//!
//! Orders: amount 10, 30, 5, 20, 15 (80 in all), cups 1, 2, 1, 4, 2 (10 in all). Cups per order: 10 / 5 = 2.

use serde_json::json;
use tenun_core::Engine;
use tenun_core::engine::{AddSource, MeasureRefused};
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{Tile, run_tile};
use tenun_core::workspace::NamedMeasure;

const ORDERS: &str = "day,amount,cups\n2026-08-03,10,1\n2026-08-20,30,2\n2026-09-02,5,1\n2026-09-14,20,4\n2026-09-30,15,2\n";

fn measure(id: &str, name: &str, formula: &str) -> NamedMeasure {
    NamedMeasure {
        id: id.into(),
        name: name.into(),
        formula: formula.into(),
        ..NamedMeasure::default()
    }
}

#[test]
fn a_pasted_measure_is_checked_and_saved_and_a_wrong_one_changes_nothing() {
    let d = std::env::temp_dir().join(format!("tenun-paste-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("orders.csv"), ORDERS).unwrap();
    let e = Engine::new(Secrets::new(d.join("config").join("secrets.json"))).unwrap();
    e.add_source(AddSource::File {
        path: d.join("orders.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let table = e.snapshot().tables[0].clone();
    let label = table.label.clone();
    // One measure is here already; the pasted one comes with no id, as the app sends it.
    e.set_measures(
        vec![measure("", "Revenue", &format!("SUM([{label}].[amount])"))],
        false,
    )
    .unwrap();
    let here = e.snapshot().workspace.measures.clone();
    let pasted = measure(
        "",
        "Cups per order",
        &format!("SUM([{label}].[cups]) / COUNT([{label}].[amount])"),
    );
    e.set_measures([here.clone(), vec![pasted]].concat(), false)
        .unwrap();
    let saved = e.snapshot().workspace.measures.clone();
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0], here[0]);
    assert_eq!(saved[1].name, "Cups per order");
    assert!(!saved[1].id.is_empty() && saved[1].id != saved[0].id);
    // A tile pointed at the new id reads it.
    let t: Tile = serde_json::from_value(json!({
        "id": "k", "type": "kpi",
        "measures": [{ "table": table.id, "column": null, "agg": "count", "named": saved[1].id }],
        "dims": []
    }))
    .unwrap();
    let k = serde_json::to_value(run_tile(&e, &t, &[]).unwrap().kpi.unwrap()).unwrap();
    assert_eq!(k["value"], json!(2.0));

    // The formula still names the other workspace's table: refused, by name, and nothing changes.
    let wrong = measure("", "Margin", "SUM([Sales].[amount])");
    let err = e
        .set_measures([saved.clone(), vec![wrong]].concat(), false)
        .unwrap_err();
    let refused = err.downcast_ref::<MeasureRefused>().expect("a refusal");
    assert_eq!(refused.index, 2);
    assert_eq!(refused.name, "Margin");
    assert!(
        refused.error.message.contains("Sales"),
        "{}",
        refused.error.message
    );
    assert_eq!(e.snapshot().workspace.measures, saved);
}
