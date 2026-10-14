//! Unusual values (v5 build 09, V5-D11): a value far from the 6 periods around it, in robust spreads, is marked.
//!
//! The data: the v5 test data's revenue by month, Oct 2023 to Aug 2026 (35 finished months, with its 3 planted
//! months: Jul 2024 ×2, Feb 2025 ×0.35, Apr 2026 ×1.9), then September 2026 with a row on the 28th only (a month not
//! over, and far below the others).

use serde_json::json;
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, Tile};
use tenun_core::workspace::Workspace;

const REVENUE: [f64; 35] = [
    317985000.0,
    347400000.0,
    360630000.0,
    354030000.0,
    330780000.0,
    297025000.0,
    262235000.0,
    237110000.0,
    229010000.0,
    482935000.0,
    272200000.0,
    313860000.0,
    356250000.0,
    388870000.0,
    403095000.0,
    395470000.0,
    129070000.0,
    330780000.0,
    292025000.0,
    263545000.0,
    254465000.0,
    267930000.0,
    301800000.0,
    347710000.0,
    394350000.0,
    429935000.0,
    445155000.0,
    436760000.0,
    407075000.0,
    364740000.0,
    610925000.0,
    290030000.0,
    279840000.0,
    294790000.0,
    331480000.0,
];

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-unusual-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn lab(d: &Path) -> (Engine, String) {
    let mut csv = String::from("date,amount\n");
    for (t, v) in REVENUE.iter().enumerate() {
        let m = 9 + t as u32;
        csv.push_str(&format!("{}-{:02}-15,{v}\n", 2023 + m / 12, m % 12 + 1));
    }
    csv.push_str("2026-09-28,1000\n");
    std::fs::write(d.join("sales.csv"), csv).unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.replace(
        Workspace {
            name: "Unusual".into(),
            ..Workspace::default()
        },
        Some(d.join("Unusual.tenun")),
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

fn marked(e: &Engine, t: &str, kind: &str, level: &str) -> Vec<String> {
    let tile: Tile = serde_json::from_value(json!({ "id": "u", "type": kind,
        "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": t, "column": "date", "grain": "month" }],
        "unusual": { "level": level } }))
    .unwrap();
    let res = snapshot::run_tile(e, &tile, &[]).unwrap();
    res.unusual
        .unwrap()
        .iter()
        .map(|u| res.rows[u.at][0].as_str().unwrap()[..7].to_string())
        .collect()
}

#[test]
fn the_three_planted_months_are_found_and_no_other() {
    let (e, t) = lab(&dir("planted"));
    let medium = marked(&e, &t, "line", "medium");
    assert_eq!(medium, ["2024-07", "2025-02", "2026-04"]);
    // The words' numbers: July 2024 is about 4 spreads above the median of the 6 months around it.
    let tile: Tile = serde_json::from_value(json!({ "id": "u", "type": "bar",
        "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": t, "column": "date", "grain": "month" }],
        "unusual": {} }))
    .unwrap();
    let res = snapshot::run_tile(&e, &tile, &[]).unwrap();
    let july = &res.unusual.unwrap()[0];
    assert_eq!(july.around, 6);
    assert!((4.0..4.6).contains(&july.spreads), "{}", july.spreads);
    assert!(july.median > 0.0);
    // Low marks fewer, high at least as many; the unfinished September (1,000 so far) is never marked.
    let low = marked(&e, &t, "area", "low");
    let high = marked(&e, &t, "line", "high");
    assert!(low.len() <= medium.len() && medium.len() <= high.len());
    assert!(low.iter().all(|m| medium.contains(m)) && medium.iter().all(|m| high.contains(m)));
    assert!(!high.contains(&"2026-09".to_string()));
}
