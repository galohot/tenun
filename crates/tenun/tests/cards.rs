//! KPI cards (build 08, V4-D10): card styles are kept in the workspace file, an exported file carries every card with
//! its style merged in (so a viewer needs no styles), and a card's other values are worked out for the same period as
//! its value.
//!
//! Orders (day, amount, cups): 3 Aug 10 1, 20 Aug 30 2, 2 Sep 5 1, 14 Sep 20 4, 30 Sep 15 2.
//! August: amount 40, cups 3, 2 rows. September: amount 40, cups 7, 3 rows. In all: amount 80, cups 10.
//! Without the 30 September row, September is not over: 1 to 14 September is amount 25, cups 5, 2 rows, and 1 to 14
//! August (what it is compared with) is amount 10.

use serde_json::{Value, json};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, Tile, run_tile};

const ORDERS: &str = "day,amount,cups\n2026-08-03,10,1\n2026-08-20,30,2\n2026-09-02,5,1\n2026-09-14,20,4\n2026-09-30,15,2\n";

fn engine(tag: &str, text: &str) -> (Engine, String, std::path::PathBuf) {
    let d = std::env::temp_dir().join(format!("tenun-cards-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("orders.csv"), text).unwrap();
    let e = Engine::new(Secrets::new(d.join("config").join("secrets.json"))).unwrap();
    e.add_source(AddSource::File {
        path: d.join("orders.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let id = e.snapshot().tables[0].id.clone();
    (e, id, d)
}

fn kpi(t: &str, by_month: bool, card: Value) -> Value {
    let mut v = json!({
        "id": "k", "type": "kpi", "title": "Revenue",
        "measures": [
            { "table": t, "column": "amount", "agg": "sum" },
            { "table": t, "column": "cups", "agg": "sum" },
            { "table": t, "column": null, "agg": "count" }
        ],
        "dims": if by_month { json!([{ "table": t, "column": "day", "grain": "month" }]) } else { json!([]) }
    });
    if !card.is_null() {
        v["card"] = card;
    }
    v
}

fn info(e: &Engine, tile: Value) -> Value {
    let t: Tile = serde_json::from_value(tile).unwrap();
    serde_json::to_value(run_tile(e, &t, &[]).unwrap().kpi.unwrap()).unwrap()
}

#[test]
fn the_other_values_of_a_card_cover_the_same_period_as_its_value() {
    let (e, t, _) = engine("extras", ORDERS);
    // September is over: the value is September's amount, the extras its cups and rows.
    let k = info(&e, kpi(&t, true, Value::Null));
    assert_eq!(k["value"], json!(40.0));
    assert_eq!(k["period"], json!("September 2026"));
    assert_eq!(k["extras"], json!([7.0, 3.0]));
    // No date: every row.
    let all = info(&e, kpi(&t, false, Value::Null));
    assert_eq!(all["value"], json!(80.0));
    assert_eq!(all["extras"], json!([10.0, 5.0]));
    // A tile with one value has no extras, and says nothing about them (a v3 file's export is unchanged).
    let mut one = kpi(&t, true, Value::Null);
    one["measures"].as_array_mut().unwrap().truncate(1);
    assert!(info(&e, one).get("extras").is_none());

    // September is not over: 1 to 14 September, for the value and the extras alike.
    let (e, t, _) = engine("partial", &ORDERS.replace("2026-09-30,15,2\n", ""));
    let k = info(&e, kpi(&t, true, Value::Null));
    assert_eq!(k["partial"], json!(true));
    assert_eq!(k["value"], json!(25.0));
    assert_eq!(k["prev"], json!(10.0));
    assert_eq!(k["extras"], json!([5.0, 2.0]));
}

#[test]
fn an_exported_card_carries_its_style_and_its_own_fields_over_it() {
    let (e, t, _) = engine("export", ORDERS);
    let style = json!({ "id": "cs-q", "name": "Quarterly", "card": {
        "parts": [{ "kind": "value" }, { "kind": "title" }], "align": "center", "size": "l", "ground": "accent",
        "rules": [{ "op": "<", "value": 50, "tone": "bad" }]
    }});
    e.set_card_styles(vec![style.clone()]).unwrap();
    let mut styled = kpi(&t, true, json!({ "style": "cs-q", "size": "s" }));
    styled["id"] = json!("styled");
    let mut gone = kpi(&t, true, json!({ "style": "cs-gone", "ground": "tint" }));
    gone["id"] = json!("gone");
    let mut own = kpi(
        &t,
        true,
        json!({ "parts": [{ "kind": "value" }], "ground": "tint" }),
    );
    own["id"] = json!("own");
    let mut old = kpi(&t, true, Value::Null);
    old["id"] = json!("old");
    e.set_dashboards(vec![
        json!({ "id": "d", "title": "Cards", "tiles": [styled, gone, own, old] }),
    ]);
    let payload = snapshot::build(&e, "d", &[]).unwrap();
    let card = |id: &str| {
        payload["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == json!(id))
            .unwrap()
            .get("card")
            .cloned()
    };
    // The style's fields, the card's own size over them, and no style to look up.
    let mut want = style["card"].clone();
    want["size"] = json!("s");
    assert_eq!(card("styled"), Some(want));
    // A style that is gone: the card's own fields, nothing else.
    assert_eq!(card("gone"), Some(json!({ "ground": "tint" })));
    assert_eq!(
        card("own"),
        Some(json!({ "parts": [{ "kind": "value" }], "ground": "tint" }))
    );
    // A number tile with no card is exported as it always was.
    assert_eq!(card("old"), None);
    // The dashboard itself is not changed by the export.
    let saved = e.dashboard("d").unwrap();
    assert_eq!(
        saved["tiles"][0]["card"],
        json!({ "style": "cs-q", "size": "s" })
    );
}

#[test]
fn card_styles_are_kept_in_the_workspace_file() {
    let (e, _, d) = engine("file", ORDERS);
    let path = d.join("cards.tenun");
    // No styles: the file says nothing about them, as a v3 file did.
    e.save(Some(path.clone())).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("cardStyles"));
    let styles = vec![
        json!({ "id": "cs-a", "name": "  Big   blue ", "card": { "size": "l", "ground": "accent" } }),
        json!({ "id": "cs-b", "name": "Small", "card": { "size": "s" } }),
    ];
    e.set_card_styles(styles).unwrap();
    e.save(Some(path.clone())).unwrap();
    let back = Engine::new(Secrets::new(d.join("config2").join("secrets.json"))).unwrap();
    back.open(&path).unwrap();
    let got = back.card_styles();
    assert_eq!(got.len(), 2);
    // A name is trimmed and its spaces folded, as a measure's is.
    assert_eq!(got[0]["name"], json!("Big blue"));
    assert_eq!(got[1]["card"], json!({ "size": "s" }));
    // A style without an id or a name, or two with one id, is refused with words.
    let no_name = e.set_card_styles(vec![json!({ "id": "cs-x", "name": " ", "card": {} })]);
    assert_eq!(
        no_name.unwrap_err().to_string(),
        "A card style needs a name."
    );
    let twice = e.set_card_styles(vec![
        json!({ "id": "cs-x", "name": "A", "card": {} }),
        json!({ "id": "cs-x", "name": "B", "card": {} }),
    ]);
    assert_eq!(
        twice.unwrap_err().to_string(),
        "Two card styles have the id cs-x."
    );
    // A refused list changes nothing.
    assert_eq!(e.card_styles().len(), 2);
}
