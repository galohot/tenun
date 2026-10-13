//! Saved views (v5 build 03): a dashboard keeps named views; an export holds each one in full (its clicks, the
//! selections of its filter tiles, its grain, its drill paths), and says which one it opens on.
//!
//! The data, by hand: orders A 10 (2025), A 5, B 7, C 4, A 1, B 2 (2026). A and B are in Jawa, C in Sumatera.
//! Jawa = 25, Sumatera = 4, all = 29.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot;
use tenun_core::workspace::{FieldRef, Workspace};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-views-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn data(d: &Path) -> (Engine, String, String) {
    std::fs::write(
        d.join("orders.csv"),
        "date,store,amount\n2025-11-03,A,10\n2026-01-15,A,5\n2026-02-20,B,7\n2026-04-02,C,4\n2026-04-30,A,1\n2026-05-10,B,2\n",
    )
    .unwrap();
    std::fs::write(
        d.join("stores.csv"),
        "store,region\nA,Jawa\nB,Jawa\nC,Sumatera\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.replace(
        Workspace {
            name: "Views".into(),
            ..Workspace::default()
        },
        Some(d.join("Views.tenun")),
    );
    let add = |f: &str| {
        let id = e
            .add_source(AddSource::File {
                path: d.join(f).to_string_lossy().into_owned(),
            })
            .unwrap();
        e.snapshot()
            .tables
            .iter()
            .find(|t| t.source_id == id)
            .unwrap()
            .id
            .clone()
    };
    let (orders, stores) = (add("orders.csv"), add("stores.csv"));
    e.add_relationship(
        FieldRef {
            table: orders.clone(),
            column: "store".into(),
        },
        FieldRef {
            table: stores.clone(),
            column: "store".into(),
        },
    )
    .unwrap();
    (e, orders, stores)
}

fn total(res: &Value) -> f64 {
    res["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_array().unwrap().last().unwrap().as_f64().unwrap())
        .sum()
}

#[test]
fn an_export_holds_each_view_in_full_and_opens_on_the_default() {
    let (e, orders, stores) = data(&dir("export"));
    let amount = json!({ "table": orders, "column": "amount", "agg": "sum" });
    let tiles = json!([
        { "id": "f", "type": "filter", "title": "", "x": 0, "y": 0, "w": 3, "h": 3, "measures": [],
          "dims": [{ "table": stores, "column": "region" }], "filter": { "style": "list" } },
        { "id": "s", "type": "bar", "title": "", "x": 3, "y": 0, "w": 6, "h": 4, "measures": [amount],
          "dims": [{ "table": stores, "column": "store" }], "sort": { "by": "label" } },
        { "id": "y", "type": "bar", "title": "", "x": 0, "y": 4, "w": 6, "h": 4, "measures": [amount], "drill": {},
          "dims": [{ "table": orders, "column": "date", "grain": "year" }], "sort": { "by": "label" } }
    ]);
    let views = json!([
        { "id": "v1", "name": "Jawa", "filters": [], "selections": { "f": { "values": ["Jawa"] } }, "grain": null, "drill": {} },
        { "id": "v2", "name": "Store A, 2026 by quarter",
          "filters": [{ "table": stores, "column": "store", "op": "in", "values": ["A"], "fromTile": "s" }],
          "selections": {}, "grain": null, "drill": { "y": [{ "value": "2026-01-01" }] } }
    ]);
    e.set_dashboards(vec![json!({ "id": "d1", "title": "Views", "refresh": 0, "tiles": tiles, "views": views, "defaultView": "v1" })]);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    // The page as saved: nothing selected.
    assert_eq!(total(&p["base"]["s"]), 29.0);
    assert_eq!(p["defaultView"], json!("v1"));
    let v = p["views"].as_array().unwrap();
    assert_eq!(
        v.iter().map(|x| x["id"].clone()).collect::<Vec<_>>(),
        [json!("v1"), json!("v2")]
    );
    assert_eq!(v[0]["name"], json!("Jawa"));
    // Jawa: the filter tile's selection, and the clicks offline still work inside the view.
    let jawa = &v[0]["payload"];
    assert_eq!(total(&jawa["base"]["s"]), 25.0);
    assert_eq!(
        total(&jawa["cross"]["s"]["A"]["y"]),
        16.0,
        "a click on A inside the Jawa view"
    );
    assert_eq!(jawa["tiles"][0]["filter"]["values"], json!(["Jawa"]));
    // Store A, 2026 by quarter: the click and the drill path.
    let a = &v[1]["payload"];
    assert_eq!(a["filters"][0]["values"], json!(["A"]));
    let quarters: Vec<(Value, f64)> = a["base"]["y"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r[0].clone(), r[1].as_f64().unwrap()))
        .collect();
    assert_eq!(
        quarters,
        [(json!("2026-01-01"), 5.0), (json!("2026-04-01"), 1.0)]
    );
    assert_eq!(a["base"]["y"]["drill"]["at"], json!(1));
    // Offline, the view drills on from where it is: Q2 2026 opens its months (A: April 1).
    let q2 = &a["down"]["y"]["2026-04-01"];
    assert_eq!(q2["drill"]["at"], json!(2));
    assert_eq!(q2["rows"], json!([["2026-04-01", 1.0]]));
    assert!(
        a.get("views").is_none(),
        "a view does not hold the views again"
    );
}

#[test]
fn a_dashboard_without_views_exports_as_before() {
    let (e, orders, stores) = data(&dir("none"));
    let amount = json!({ "table": orders, "column": "amount", "agg": "sum" });
    e.set_dashboards(vec![json!({ "id": "d1", "title": "No views", "refresh": 0, "tiles": [
        { "id": "s", "type": "bar", "title": "", "x": 0, "y": 0, "w": 6, "h": 4, "measures": [amount],
          "dims": [{ "table": stores, "column": "store" }] }
    ] })]);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    assert!(p.get("views").is_none() && p.get("defaultView").is_none());
}
