//! An exported site (build 13): every page, hidden ones too; each drill-through opens its page kept to the clicked
//! value, for the first 50 values; and a workspace that is not a site is refused in words.
//!
//! Sales: 60 stores, s01 … s60; store sNN sold NN. A bar of sales by store (biggest first: s60, s59, …) opens the page
//! "store" on a click; that page has a number tile of sales.

use serde_json::json;
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::site::{DRILL, DRILL_MAX, build_site};

fn engine() -> (Engine, String) {
    let d = std::env::temp_dir().join(format!("tenun-siteexp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let mut csv = String::from("store,sales\n");
    for n in 1..=60 {
        csv.push_str(&format!("s{n:02},{n}\n"));
    }
    std::fs::write(d.join("sales.csv"), csv).unwrap();
    let e = Engine::new(Secrets::new(d.join("config").join("secrets.json"))).unwrap();
    e.add_source(AddSource::File {
        path: d.join("sales.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let t = e.snapshot().tables[0].id.clone();
    (e, t)
}

#[test]
fn a_site_holds_every_page_and_its_drill_throughs() {
    let (e, t) = engine();
    let sum = json!({ "table": t, "column": "sales", "agg": "sum" });
    e.set_dashboards(vec![
        json!({ "id": "home", "title": "Overview", "tiles": [
            { "id": "bar", "type": "bar", "title": "", "measures": [sum], "dims": [{ "table": t, "column": "store" }],
              "action": { "page": "store" } },
            { "id": "self", "type": "bar", "title": "", "measures": [sum], "dims": [{ "table": t, "column": "store" }],
              "action": { "page": "home" } }
        ]}),
        json!({ "id": "store", "title": "Store", "hidden": true, "tiles": [
            { "id": "k", "type": "kpi", "title": "", "measures": [sum], "dims": [] }
        ]}),
    ]);
    // Not a site yet: refused, in words.
    let err = build_site(&e, |_, _| {}).unwrap_err();
    assert_eq!(
        err.to_string(),
        "This workspace is not a site. Switch it on under Site… first."
    );
    e.set_site(json!({ "on": true, "title": "Shops", "nav": "top", "home": "home" }))
        .unwrap();
    let mut heard = Vec::new();
    let site = build_site(&e, |done, total| heard.push((done, total))).unwrap();
    assert_eq!(heard, [(1, 2), (2, 2)]);
    assert_eq!(site["kind"], json!("site"));
    assert_eq!(site["title"], json!("Shops"));
    assert_eq!(site["order"], json!(["home", "store"]));
    assert_eq!(site["hidden"], json!(["store"]));
    assert_eq!(
        site["titles"],
        json!({ "home": "Overview", "store": "Store" })
    );
    // The hidden page is in the file: a click reaches it.
    assert_eq!(
        site["pages"]["store"]["base"]["k"]["kpi"]["value"],
        json!(1830.0)
    );
    // The drill-through: 60 values, the first 50 opened, and the file says there are more.
    let d = &site["drill"]["home/bar"];
    assert_eq!(d["page"], json!("store"));
    assert_eq!(d["more"], json!(true));
    let values = d["values"].as_object().unwrap();
    assert_eq!(values.len(), DRILL_MAX);
    // The biggest stores come first: s60 … s11. A click on s42 opens the store page kept to s42.
    assert!(
        values.contains_key("s60") && values.contains_key("s11") && !values.contains_key("s10")
    );
    let s42 = &values["s42"];
    assert_eq!(s42["base"]["k"]["kpi"]["value"], json!(42.0));
    assert_eq!(s42["filters"][0]["fromTile"], json!(DRILL));
    assert_eq!(s42["filters"][0]["values"], json!(["s42"]));
    // The opened page names the table of its filter, for the chip.
    assert!(
        s42["tables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["id"] == json!(t))
    );
    // A page a drill-through opens works out no clicks of its own: the file stays small.
    assert_eq!(s42["cross"], json!({}));
    // An action to its own page is not a drill-through.
    assert!(site["drill"].get("home/self").is_none());
}
