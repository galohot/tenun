//! Filter reach and every-page filters (v5 build 02, V5-D37): a click or a filter tile filters only the tiles its tile
//! reaches; an export does not compute the others; a filter tile marked "Apply to every page" filters every page of
//! the workspace, also offline in an exported site, where each value is held for the other pages.
//!
//! The data, by hand: orders A 10, A 5, B 7, C 4, A 1, B 2. A and B are in Jawa, C in Sumatera. So Jawa = 25,
//! Sumatera = 4, all = 29; A = 16, B = 9, C = 4.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile};
use tenun_core::workspace::{FieldRef, Workspace};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-reach-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The engine, the orders table and the stores table.
fn data(d: &Path) -> (Engine, String, String) {
    std::fs::write(
        d.join("orders.csv"),
        "store,amount\nA,10\nA,5\nB,7\nC,4\nA,1\nB,2\n",
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
            name: "Reach".into(),
            ..Workspace::default()
        },
        Some(d.join("Reach.tenun")),
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

/// Three tiles: a bar by region (`r`), a bar by store (`s`) and a number (`k`), plus `extra`.
fn tiles(orders: &str, stores: &str, extra: Vec<Value>) -> Vec<Value> {
    let amount = json!({ "table": orders, "column": "amount", "agg": "sum" });
    let mut out = vec![
        json!({ "id": "r", "type": "bar", "title": "", "x": 0, "y": 0, "w": 6, "h": 4, "measures": [amount],
                "dims": [{ "table": stores, "column": "region" }], "reach": ["s"], "sort": { "by": "label" } }),
        json!({ "id": "s", "type": "bar", "title": "", "x": 6, "y": 0, "w": 6, "h": 4, "measures": [amount],
                "dims": [{ "table": stores, "column": "store" }], "sort": { "by": "label" } }),
        json!({ "id": "k", "type": "kpi", "title": "", "x": 0, "y": 4, "w": 3, "h": 2, "measures": [amount], "dims": [] }),
    ];
    out.extend(extra);
    out
}

fn total(res: &Value) -> f64 {
    res["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_array().unwrap().last().unwrap().as_f64().unwrap())
        .sum()
}

fn tile(v: &Value) -> Tile {
    serde_json::from_value(v.clone()).unwrap()
}

#[test]
fn a_click_filters_only_the_tiles_its_tile_reaches() {
    let (e, orders, stores) = data(&dir("click"));
    let ts = tiles(&orders, &stores, vec![]);
    let jawa = |to: Option<Vec<&str>>| DashFilter {
        filter: serde_json::from_value(
            json!({ "table": stores, "column": "region", "op": "in", "values": ["Jawa"] }),
        )
        .unwrap(),
        from_tile: "r".into(),
        to: to.map(|v| v.into_iter().map(str::to_string).collect()),
    };
    let run = |id: usize, f: &DashFilter| {
        total(
            &serde_json::to_value(
                snapshot::run_tile(&e, &tile(&ts[id]), std::slice::from_ref(f)).unwrap(),
            )
            .unwrap(),
        )
    };
    // Reach ["s"]: the store bar is filtered, the number is not.
    assert_eq!(run(1, &jawa(Some(vec!["s"]))), 25.0);
    let k = snapshot::run_tile(&e, &tile(&ts[2]), &[jawa(Some(vec!["s"]))]).unwrap();
    assert_eq!(k.kpi.unwrap().value, Some(29.0));
    // Reach none: nothing is filtered. No reach: every tile is.
    assert_eq!(run(1, &jawa(Some(vec![]))), 29.0);
    assert_eq!(run(1, &jawa(None)), 25.0);
    let k = snapshot::run_tile(&e, &tile(&ts[2]), &[jawa(None)]).unwrap();
    assert_eq!(k.kpi.unwrap().value, Some(25.0));
}

#[test]
fn a_filter_tile_and_an_export_follow_the_reach() {
    let (e, orders, stores) = data(&dir("export"));
    // A filter tile on region, Jawa picked, that filters only the number.
    let filter = json!({ "id": "f", "type": "filter", "title": "", "x": 3, "y": 4, "w": 3, "h": 3, "measures": [],
        "dims": [{ "table": stores, "column": "region" }], "reach": ["k"],
        "filter": { "style": "list", "values": ["Jawa"] } });
    e.set_dashboards(vec![json!({ "id": "d1", "title": "One", "refresh": 0, "tiles": tiles(&orders, &stores, vec![filter]) })]);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    assert_eq!(
        p["base"]["k"]["kpi"]["value"],
        json!(25.0),
        "the number is kept to Jawa"
    );
    assert_eq!(total(&p["base"]["s"]), 29.0, "the store bar is not");
    // A click on the region bar (reach ["s"]) holds results for the store bar only.
    let jawa = &p["cross"]["r"]["Jawa"];
    assert_eq!(total(&jawa["s"]), 25.0);
    assert!(
        jawa.get("k").is_none(),
        "the number is not reached, so it is not computed"
    );
    // A click on the store bar (every tile) holds the number and the region bar.
    assert!(p["cross"]["s"]["A"].get("k").is_some() && p["cross"]["s"]["A"].get("r").is_some());
}

#[test]
fn a_filter_tile_for_every_page_filters_the_other_pages() {
    let (e, orders, stores) = data(&dir("every"));
    let filter = json!({ "id": "f", "type": "filter", "title": "", "x": 0, "y": 6, "w": 3, "h": 3, "measures": [],
        "dims": [{ "table": stores, "column": "region" }],
        "filter": { "style": "list", "values": ["Sumatera"], "everyPage": true } });
    e.set_dashboards(vec![
        json!({ "id": "d1", "title": "One", "refresh": 0, "tiles": [filter] }),
        json!({ "id": "d2", "title": "Two", "refresh": 0, "tiles": tiles(&orders, &stores, vec![]) }),
    ]);
    let p = snapshot::build(&e, "d2", &[]).unwrap();
    assert_eq!(total(&p["base"]["s"]), 4.0, "page two is kept to Sumatera");
    let every = p["every"].as_array().unwrap();
    assert_eq!(every.len(), 1);
    assert_eq!(every[0]["fromTile"], json!("\u{0}every:d1:f"));
    // A filter the caller sends for the same source takes its place (an exported site's other value).
    let jawa = DashFilter {
        filter: serde_json::from_value(
            json!({ "table": stores, "column": "region", "op": "in", "values": ["Jawa"] }),
        )
        .unwrap(),
        from_tile: "\u{0}every:d1:f".into(),
        to: None,
    };
    let p = snapshot::build(&e, "d2", &[jawa]).unwrap();
    assert_eq!(total(&p["base"]["s"]), 25.0);
    // A filter tile not marked for every page stays on its page.
    let mut ds = e.dashboards();
    ds[0]["tiles"][0]["filter"]["everyPage"] = json!(false);
    e.set_dashboards(ds);
    let p = snapshot::build(&e, "d2", &[]).unwrap();
    assert_eq!(total(&p["base"]["s"]), 29.0);
}

#[test]
fn an_exported_site_holds_the_other_pages_for_each_every_page_value() {
    let (e, orders, stores) = data(&dir("site"));
    let filter = json!({ "id": "f", "type": "filter", "title": "", "x": 0, "y": 0, "w": 3, "h": 3, "measures": [],
        "dims": [{ "table": stores, "column": "region" }],
        "filter": { "style": "list", "everyPage": true } });
    e.set_dashboards(vec![
        json!({ "id": "d1", "title": "One", "refresh": 0, "tiles": [filter] }),
        json!({ "id": "d2", "title": "Two", "refresh": 0, "tiles": tiles(&orders, &stores, vec![]) }),
    ]);
    e.set_site(json!({ "on": true, "title": "Reach", "nav": "top" }))
        .unwrap();
    let site = tenun_core::site::build_site(&e, |_, _| {}).unwrap();
    let every = &site["every"]["d1/f"];
    assert_eq!(
        (
            every["page"].clone(),
            every["tile"].clone(),
            every["more"].clone()
        ),
        (json!("d1"), json!("f"), json!(false))
    );
    assert_eq!(total(&every["values"]["Jawa"]["d2"]["s"]), 25.0);
    assert_eq!(total(&every["values"]["Sumatera"]["d2"]["s"]), 4.0);
    assert_eq!(
        every["values"]["Sumatera"]["d2"]["k"]["kpi"]["value"],
        json!(4.0)
    );
    assert!(
        every["values"]["Jawa"].get("d1").is_none(),
        "the filter's own page is not repeated"
    );
    // Nothing is selected, so the pages themselves are not filtered.
    assert_eq!(total(&site["pages"]["d2"]["base"]["s"]), 29.0);
}
