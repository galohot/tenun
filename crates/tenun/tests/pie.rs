//! Pie and donut (build 07, V4-D7): at most six slices and "Other", five unless the tile says.
//!
//! Sales by category: A 80, B 70, C 60, D 50, E 40, F 30, G 20, H 10. In all 360.

use serde_json::{Value, json};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{Tile, run_tile};

const SALES: &str = "category,amount\nA,80\nB,70\nC,60\nD,50\nE,40\nF,30\nG,20\nH,10\n";

fn engine(tag: &str) -> (Engine, String) {
    let d = std::env::temp_dir().join(format!("tenun-pie-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("sales.csv"), SALES).unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.add_source(AddSource::File {
        path: d.join("sales.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let id = e.snapshot().tables[0].id.clone();
    (e, id)
}

fn tile(t: &str, kind: &str, top: Value) -> Tile {
    let mut v = json!({
        "id": "p", "type": kind,
        "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": t, "column": "category" }]
    });
    if !top.is_null() {
        v["top"] = top;
    }
    serde_json::from_value(v).unwrap()
}

fn slices(e: &Engine, t: &Tile) -> Vec<(String, f64)> {
    run_tile(e, t, &[])
        .unwrap()
        .rows
        .iter()
        .map(|r| {
            (
                r[0].as_str().unwrap().replace('\u{0}', ""),
                r[1].as_f64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn a_pie_keeps_five_slices_and_other_unless_the_tile_says() {
    let (e, t) = engine("five");
    let mut got = slices(&e, &tile(&t, "pie", Value::Null));
    got.sort_by(|a, b| b.1.total_cmp(&a.1));
    // A to E are the five biggest; Other is F + G + H = 60.
    assert_eq!(
        got,
        [
            ("A".into(), 80.0),
            ("B".into(), 70.0),
            ("C".into(), 60.0),
            ("other".into(), 60.0),
            ("D".into(), 50.0),
            ("E".into(), 40.0)
        ]
    );
    // Three slices and Other: D to H, 150.
    let three = slices(&e, &tile(&t, "pie", json!({ "n": 3 })));
    assert_eq!(three.len(), 4);
    assert!(three.contains(&("other".into(), 150.0)));
    // At most six, at least two, and always an Other.
    assert_eq!(slices(&e, &tile(&t, "pie", json!({ "n": 20 }))).len(), 7);
    assert_eq!(
        slices(&e, &tile(&t, "pie", json!({ "n": 1, "other": false }))).len(),
        3
    );
}

#[test]
fn a_donut_of_an_old_file_is_a_donut_with_the_same_slices() {
    let (e, t) = engine("donut");
    let pie = slices(&e, &tile(&t, "pie", Value::Null));
    let donut = slices(&e, &tile(&t, "donut", Value::Null));
    assert_eq!(pie, donut);
    // The share strip keeps every group when no top is set.
    assert_eq!(slices(&e, &tile(&t, "share", Value::Null)).len(), 8);
}
