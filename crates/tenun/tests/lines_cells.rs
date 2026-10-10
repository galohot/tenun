//! A table that hit its row limit says how many rows it has in all (build 07).

use serde_json::{Value, json};
use std::path::PathBuf;
use tenun_core::Engine;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{Tile, run_tile};
use tenun_core::workspace::{Source, SourceKind, Workspace};

// 620 orders in 3 regions; each order has its own id.
fn engine(tag: &str) -> Engine {
    let dir: PathBuf = std::env::temp_dir().join(format!("tenun-lc-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut csv = String::from("id,region,amount\n");
    for i in 1..=620 {
        csv.push_str(&format!("o{i:04},R{},{i}\n", i % 3));
    }
    std::fs::write(dir.join("orders.csv"), csv).unwrap();
    let ws = Workspace {
        name: "LC".into(),
        sources: vec![Source {
            id: "s1".into(),
            name: "orders.csv".into(),
            kind: SourceKind::Csv {
                path: "orders.csv".into(),
            },
        }],
        ..Workspace::default()
    };
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    e.replace(ws, Some(dir.join("T.tenun")));
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    e
}

fn table(extra: Value) -> Tile {
    let mut t = json!({
        "id": "t", "type": "table",
        "measures": [{ "table": "s1/orders", "column": "amount", "agg": "sum" }],
        "dims": [{ "table": "s1/orders", "column": "id" }],
    });
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        t[k] = v;
    }
    serde_json::from_value(t).unwrap()
}

#[test]
fn a_cut_table_counts_its_rows() {
    let e = engine("cut");
    let r = run_tile(&e, &table(json!({})), &[]).unwrap();
    assert_eq!(r.rows.len(), 500);
    assert_eq!(r.total_rows, Some(620));
    // Tile options the engine does not know do not change the result.
    let r = run_tile(
        &e,
        &table(json!({ "cond": [{ "kind": "bars" }], "widths": [120, 80], "labels": "all" })),
        &[],
    )
    .unwrap();
    assert_eq!(r.total_rows, Some(620));
}

#[test]
fn the_count_follows_the_filters_and_the_groups() {
    let e = engine("groups");
    // Three regions fit under the limit: nothing to count.
    let by_region = table(json!({ "dims": [{ "table": "s1/orders", "column": "region" }] }));
    let r = run_tile(&e, &by_region, &[]).unwrap();
    assert_eq!(r.rows.len(), 3);
    assert_eq!(r.total_rows, None);
    // Region and id: still one group for each order.
    let both = table(json!({ "dims": [
        { "table": "s1/orders", "column": "id" },
        { "table": "s1/orders", "column": "region" },
    ] }));
    assert_eq!(run_tile(&e, &both, &[]).unwrap().total_rows, Some(620));
    // A filter from another tile leaves 570 of the 620 orders (amount 51 and over).
    let f: tenun_core::snapshot::DashFilter = serde_json::from_value(json!({
        "table": "s1/orders", "column": "amount", "op": "gte", "values": [51], "fromTile": "other",
    }))
    .unwrap();
    let r = run_tile(&e, &table(json!({})), &[f]).unwrap();
    assert_eq!(r.total_rows, Some(570));
}

#[test]
fn a_short_table_has_no_count() {
    let e = engine("short");
    let r = run_tile(&e, &table(json!({ "top": { "n": 5 } })), &[]).unwrap();
    assert_eq!(r.total_rows, None);
}
