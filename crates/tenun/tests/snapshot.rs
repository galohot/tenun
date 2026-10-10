//! The Rust snapshot engine must give exactly what the v1 TypeScript export
//! gave for the sample dashboard (fixture captured from the v1 app).

use serde_json::Value;
use std::path::Path;
use tenun_core::Engine;
use tenun_core::secrets::Secrets;

fn engine() -> Engine {
    let dir = std::env::temp_dir().join(format!("tenun-snap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sample");
    for f in ["coffee.xlsx", "stores.csv", "customers.sqlite"] {
        std::fs::copy(src.join(f), dir.join(f)).unwrap();
    }
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    e.replace(tenun::sample::workspace(), Some(dir.join("Coffee.tenun")));
    e
}

/// Compares JSON, treating numbers as f64 within a tiny tolerance, and
/// skipping fields that legitimately differ (timings).
fn same(a: &Value, b: &Value, path: &str) -> Result<(), String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            if (x - y).abs() <= 1e-9 * x.abs().max(1.0) {
                Ok(())
            } else {
                Err(format!("{path}: {x} != {y}"))
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                if k == "ms" {
                    continue;
                }
                let w = y
                    .get(k)
                    .ok_or(format!("{path}.{k}: missing in Rust output"))?;
                same(v, w, &format!("{path}.{k}"))?;
            }
            for k in y.keys() {
                if k != "ms" && !x.contains_key(k) {
                    return Err(format!("{path}.{k}: extra in Rust output"));
                }
            }
            Ok(())
        }
        (Value::Array(x), Value::Array(y)) if path.ends_with(".rows") => {
            // Rows that tie on the sort keys may come in any order: compare as sets.
            let key = |v: &Value| v.to_string();
            let (mut xs, mut ys) = (x.clone(), y.clone());
            xs.sort_by_key(key);
            ys.sort_by_key(key);
            same(
                &Value::Array(xs),
                &Value::Array(ys),
                &format!("{path}(sorted)"),
            )
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Err(format!("{path}: length {} != {}", x.len(), y.len()));
            }
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                same(v, w, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        _ if a == b => Ok(()),
        _ => Err(format!("{path}: {a} != {b}")),
    }
}

#[test]
fn rust_snapshot_matches_the_v1_export() {
    let fixture: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample-snapshot-v1.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let e = engine();
    let mut snap = tenun_core::snapshot::build(&e, "d1", &[]).unwrap();
    let mut fixture = fixture;
    // Table order follows load order, which differs between sessions.
    for v in [&mut snap, &mut fixture] {
        v["tables"]
            .as_array_mut()
            .unwrap()
            .sort_by_key(|t| t["id"].as_str().unwrap().to_string());
    }
    for key in [
        "title",
        "workspace",
        "tables",
        "tiles",
        "base",
        "cross",
        "filters",
    ] {
        if let Err(diff) = same(&fixture[key], &snap[key], key) {
            panic!("{diff}");
        }
    }
}

#[test]
fn number_tile_compares_the_same_days() {
    let e = engine();
    let snap = tenun_core::snapshot::build(&e, "d1", &[]).unwrap();
    let kpi = &snap["base"]["t1"]["kpi"];
    assert_eq!(kpi["period"], "Oct 1–14, 2026");
    assert_eq!(kpi["versus"], "Sep 1–14");
    assert_eq!(kpi["partial"], true);
    assert_eq!(snap["base"]["t5"]["partial"]["until"], "2026-10-14");
}

#[test]
fn html_export_is_self_contained() {
    let e = engine();
    let snap = tenun_core::snapshot::build(&e, "d1", &[]).unwrap();
    let html = tenun::export_html(&snap);
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("window.TENUN = {"));
    assert!(!html.contains("src=\"http"), "no external scripts");
    // The data cannot close the script element early.
    let data_start = html.find("window.TENUN = ").unwrap();
    let data_end = html[data_start..].find("</script>").unwrap();
    assert!(!html[data_start..data_start + data_end].contains("</"));
}
