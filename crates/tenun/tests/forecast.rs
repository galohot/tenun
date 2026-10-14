//! Forecast (v5 build 08, V5-D10): a line over time forecasts its next periods from its own rows, leaves the
//! unfinished period out of the fit, refuses in words with too few periods, and travels with an export.
//!
//! The data: 36 finished months from Oct 2023 of a level of 1,000 with a yearly wave of ± 200, one row on the 15th of
//! each month, then October 2026 with one row on the 5th (300, a month that has just begun).

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, Tile};
use tenun_core::workspace::Workspace;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-forecast-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn wave(t: usize) -> f64 {
    1000.0 + 200.0 * (2.0 * std::f64::consts::PI * t as f64 / 12.0).sin()
}

fn month(t: usize) -> (i32, u32) {
    let m = 9 + t as u32; // October 2023 is month 9 counted from January 2023, from 0
    (2023 + (m / 12) as i32, m % 12 + 1)
}

fn lab(d: &Path, months: usize) -> (Engine, String) {
    let mut csv = String::from("date,amount\n");
    for t in 0..months {
        let (y, m) = month(t);
        csv.push_str(&format!("{y}-{m:02}-15,{:.4}\n", wave(t)));
    }
    let (y, m) = month(months);
    csv.push_str(&format!("{y}-{m:02}-05,300\n"));
    std::fs::write(d.join("sales.csv"), csv).unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.replace(
        Workspace {
            name: "Forecast".into(),
            ..Workspace::default()
        },
        Some(d.join("Forecast.tenun")),
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

fn line(t: &str) -> Tile {
    serde_json::from_value(json!({ "id": "l", "type": "line",
        "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": t, "column": "date", "grain": "month" }],
        "forecast": { "periods": 3 } }))
    .unwrap()
}

#[test]
fn a_season_goes_on_and_the_unfinished_month_is_left_out() {
    let (e, t) = lab(&dir("season"), 36);
    let res = snapshot::run_tile(&e, &line(&t), &[]).unwrap();
    assert_eq!(
        res.partial.as_ref().map(|p| p.key.as_str()),
        Some("2026-10-01")
    );
    let f = res.forecast.expect("a forecast");
    assert!(f.note.is_none(), "{:?}", f.note);
    // October (the unfinished month, estimated) and the three months after it.
    let keys: Vec<&str> = f.rows.iter().map(|r| r[0].as_str().unwrap()).collect();
    assert_eq!(
        keys,
        ["2026-10-01", "2026-11-01", "2026-12-01", "2027-01-01"]
    );
    for (h, r) in f.rows.iter().enumerate() {
        let (v, lo, hi) = (
            r[1].as_f64().unwrap(),
            r[2].as_f64().unwrap(),
            r[3].as_f64().unwrap(),
        );
        let want = wave(36 + h);
        // Within 1 % of the true wave: the half month (300) did not pull the estimate down.
        assert!(
            (v - want).abs() / want < 0.01,
            "step {h}: {v} against {want}"
        );
        assert!(lo <= v && v <= hi);
    }
    assert!(
        f.method.starts_with("Exponential smoothing"),
        "{}",
        f.method
    );
    // The same rows give the same forecast.
    let again = snapshot::run_tile(&e, &line(&t), &[])
        .unwrap()
        .forecast
        .unwrap();
    assert_eq!(again.rows, f.rows);
}

#[test]
fn too_few_months_are_said_in_words_and_an_export_carries_the_forecast() {
    let (e, t) = lab(&dir("short"), 14);
    let f = snapshot::run_tile(&e, &line(&t), &[])
        .unwrap()
        .forecast
        .unwrap();
    assert!(f.rows.is_empty());
    assert_eq!(
        f.note.as_deref(),
        Some("A forecast needs at least 26 months of data; this chart has 14.")
    );
    let (e, t) = lab(&dir("export"), 36);
    let mut tile = serde_json::to_value(
        json!({ "id": "l", "type": "line", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
        "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": t, "column": "date", "grain": "month" }],
        "forecast": { "periods": 3 } }),
    )
    .unwrap();
    tile["forecast"]["band"] = Value::Bool(true);
    e.set_dashboards(vec![
        json!({ "id": "d1", "title": "F", "refresh": 0, "tiles": [tile] }),
    ]);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    assert_eq!(
        p["base"]["l"]["forecast"]["rows"].as_array().unwrap().len(),
        4
    );
}
