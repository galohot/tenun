//! Story pages (build 11): an exported story carries its layout and its order, so the viewer draws it as a column in
//! that order; a grid page's file is as it was.

use serde_json::json;
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot;

#[test]
fn an_exported_story_keeps_its_layout_and_order() {
    let d = std::env::temp_dir().join(format!("tenun-story-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("sales.csv"), "region,amount\nJava,10\nBali,5\n").unwrap();
    let e = Engine::new(Secrets::new(d.join("config").join("secrets.json"))).unwrap();
    e.add_source(AddSource::File {
        path: d.join("sales.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let t = e.snapshot().tables[0].id.clone();
    let tiles = json!([
        { "id": "chart", "type": "bar", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
          "measures": [{ "table": t, "column": "amount", "agg": "sum" }], "dims": [{ "table": t, "column": "region" }],
          "flow": { "width": "half", "height": "l" } },
        { "id": "words", "type": "text", "title": "", "x": 6, "y": 0, "w": 6, "h": 2, "measures": [], "dims": [], "text": "# Title" }
    ]);
    e.set_dashboards(vec![
        json!({ "id": "s", "title": "Story", "layout": "flow", "order": ["words", "chart"], "tiles": tiles }),
        json!({ "id": "g", "title": "Grid", "tiles": tiles }),
    ]);
    let story = snapshot::build(&e, "s", &[]).unwrap();
    assert_eq!(story["layout"], json!("flow"));
    assert_eq!(story["order"], json!(["words", "chart"]));
    // A tile keeps its width and height in the story, and its place on the grid.
    assert_eq!(
        story["tiles"][0]["flow"],
        json!({ "width": "half", "height": "l" })
    );
    assert_eq!(story["tiles"][0]["x"], json!(0));
    let grid = snapshot::build(&e, "g", &[]).unwrap();
    assert!(grid.get("layout").is_none());
    assert!(grid.get("order").is_none());
    assert_eq!(
        grid["base"]["chart"]["rows"].as_array().map(Vec::len),
        Some(2)
    );
}

#[test]
fn site_settings_are_kept_in_the_workspace_file() {
    let d = std::env::temp_dir().join(format!("tenun-site-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let e = Engine::new(Secrets::new(d.join("config").join("secrets.json"))).unwrap();
    let path = d.join("site.tenun");
    // Not a site: the file says nothing about it, as a v3 file did.
    e.save(Some(path.clone())).unwrap();
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("\"site\":")
    );
    let site = json!({ "on": true, "title": "Coffee", "nav": "side", "home": "d1",
        "groups": [{ "name": "Sales", "pages": ["d1"] }], "footer": "Made by the team" });
    e.set_site(site.clone()).unwrap();
    assert!(e.set_site(json!("not settings")).is_err());
    e.save(Some(path.clone())).unwrap();
    let back = Engine::new(Secrets::new(d.join("config2").join("secrets.json"))).unwrap();
    back.open(&path).unwrap();
    assert_eq!(back.site(), Some(site));
}
