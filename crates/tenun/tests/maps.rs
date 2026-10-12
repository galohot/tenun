//! The map tile (build 06): places read from names and codes, one row per place, the names that are not on
//! the map, points, and a click on a place.
//!
//! Sales (place, amount): Jawa Barat 100, Jabar 50, 32 20 (all three are ID-JB), DKI Jakarta 70, Jakarta 30
//! (ID-JK), Bandunk 5 (a typo), and one row with no place, 9. Every number below is worked out by hand.

use serde_json::{Value, json};
use tenun_core::Engine;
use tenun_core::engine::{AddSource, QueryResult};
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile};

const SALES: &str = "place,amount,lat,lon,kind\nJawa Barat,100,-6.9,107.6,shop\nJabar,50,-6.9,107.6,shop\n\
    32,20,-6.2,106.8,cafe\nDKI Jakarta,70,-6.2,106.8,cafe\nJakarta,30,-6.2,106.8,shop\nBandunk,5,95,107,shop\n,9,,,shop\n";
const REGENCIES: &str =
    "kab,amount\nKabupaten Bandung,10\n3204,5\nKota Bandung,7\nBandung,3\nKab. Bogor,2\n";
const COUNTRIES: &str =
    "country,visitors\nIndonesia,100\nIDN,20\nMozambique,30\nAmerika Serikat,40\nAtlantis,1\n";
const KENYA: &str = "county,amount\nNairobi,10\nKE-30,5\nMombasa,4\n";

struct W {
    e: Engine,
}

fn world(tag: &str) -> W {
    let d = std::env::temp_dir().join(format!("tenun-maps-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let e = Engine::new(Secrets::new(d.join("config").join("secrets.json"))).unwrap();
    for (f, text) in [
        ("sales.csv", SALES),
        ("regencies.csv", REGENCIES),
        ("countries.csv", COUNTRIES),
        ("kenya.csv", KENYA),
    ] {
        std::fs::write(d.join(f), text).unwrap();
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    W { e }
}

impl W {
    fn id(&self, name: &str) -> String {
        self.e
            .snapshot()
            .tables
            .iter()
            .find(|t| t.name == name)
            .unwrap()
            .id
            .clone()
    }

    fn map(&self, table: &str, place: &str, value: &str, agg: &str, map: Value) -> Tile {
        let t = self.id(table);
        serde_json::from_value(json!({
            "id": "m", "type": "map",
            "measures": [{ "table": t, "column": value, "agg": agg }],
            "dims": [{ "table": t, "column": place }],
            "map": map
        }))
        .unwrap()
    }
}

fn by_code(r: &QueryResult) -> Vec<(String, f64)> {
    r.rows
        .iter()
        .map(|row| {
            (
                row[0].as_str().unwrap().to_string(),
                row[1].as_f64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn a_province_written_three_ways_is_one_place_and_a_typo_is_listed() {
    let w = world("provinces");
    let t = w.map(
        "sales",
        "place",
        "amount",
        "sum",
        json!({ "level": "province", "country": "ID" }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    // Jawa Barat 100 + Jabar 50 + 32 20 = 170; DKI Jakarta 70 + Jakarta 30 = 100. Biggest first.
    assert_eq!(
        by_code(&r),
        [("ID-JB".into(), 170.0), ("ID-JK".into(), 100.0)]
    );
    let geo = r.geo.as_ref().unwrap();
    assert_eq!(geo.codes, [Some("ID-JB".into()), Some("ID-JK".into())]);
    assert_eq!(geo.names, ["Jawa Barat", "Daerah Khusus Ibukota Jakarta"]);
    let mut jb: Vec<&str> = geo.raw[0].iter().map(|v| v.as_str().unwrap()).collect();
    jb.sort();
    assert_eq!(jb, ["32", "Jabar", "Jawa Barat"]);
    // The typo is listed; the blank place is not a name to fix.
    assert_eq!(geo.unmatched_count, 1);
    assert_eq!(geo.unmatched[0].name, "Bandunk");
    assert_eq!(geo.unmatched[0].hint, None);
    // An average is worked out again over the three names, not added: (100 + 50 + 20) / 3.
    let avg = w.map(
        "sales",
        "place",
        "amount",
        "avg",
        json!({ "level": "province" }),
    );
    let r = run_tile(&w.e, &avg, &[]).unwrap();
    let jb = by_code(&r).into_iter().find(|(c, _)| c == "ID-JB").unwrap();
    assert!((jb.1 - 170.0 / 3.0).abs() < 1e-9, "{jb:?}");
}

#[test]
fn a_kabupaten_and_a_kota_are_two_places_and_a_bare_name_asks_which() {
    let w = world("regencies");
    let t = w.map(
        "regencies",
        "kab",
        "amount",
        "sum",
        json!({ "level": "regency", "country": "ID" }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    // Kabupaten Bandung 10 + 3204 5 = 15; Kota Bandung 7; Kab. Bogor 2.
    assert_eq!(
        by_code(&r),
        [
            ("32.04".into(), 15.0),
            ("32.73".into(), 7.0),
            ("32.01".into(), 2.0)
        ]
    );
    let geo = r.geo.unwrap();
    assert_eq!(geo.unmatched.len(), 1);
    assert_eq!(geo.unmatched[0].name, "Bandung");
    assert_eq!(
        geo.unmatched[0].hint.as_deref(),
        Some("Write Kabupaten Bandung or Kota Bandung.")
    );
}

#[test]
fn a_world_map_reads_iso_codes_and_english_and_indonesian_names() {
    let w = world("countries");
    let t = w.map(
        "countries",
        "country",
        "visitors",
        "sum",
        json!({ "level": "country" }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    assert_eq!(
        by_code(&r),
        [
            ("ID".into(), 120.0),
            ("US".into(), 40.0),
            ("MZ".into(), 30.0)
        ]
    );
    let geo = r.geo.unwrap();
    assert_eq!(geo.country, None);
    assert_eq!(geo.unmatched[0].name, "Atlantis");
}

#[test]
fn another_country_is_read_from_its_downloaded_file_or_asks_for_it() {
    let w = world("kenya");
    let t = w.map(
        "kenya",
        "county",
        "amount",
        "sum",
        json!({ "level": "province", "country": "KE" }),
    );
    // Not on this computer yet: the app downloads it, then asks again.
    let r = run_tile(&w.e, &t, &[]).unwrap();
    assert!(r.geo.as_ref().unwrap().download);
    assert!(r.rows.is_empty());
    // The file geoBoundaries would give.
    let dir = w.e.geo_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("KEN-ADM1.geojson"),
        r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","properties":{"shapeName":"Nairobi","shapeISO":"KE-30"},"geometry":null},
            {"type":"Feature","properties":{"shapeName":"Mombasa","shapeISO":"KE-28"},"geometry":null}]}"#,
    )
    .unwrap();
    let r = run_tile(&w.e, &t, &[]).unwrap();
    // Nairobi 10 + KE-30 5 = 15; Mombasa 4.
    assert_eq!(by_code(&r), [("KE-30".into(), 15.0), ("KE-28".into(), 4.0)]);
    let geo = r.geo.unwrap();
    assert!(!geo.download);
    assert_eq!(geo.names, ["Nairobi", "Mombasa"]);
}

#[test]
fn points_drop_what_is_not_on_the_earth_and_are_sized_by_the_value() {
    let w = world("points");
    let s = w.id("sales");
    let t: Tile = serde_json::from_value(json!({
        "id": "p", "type": "map",
        "measures": [{ "table": s, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": s, "column": "kind" }],
        "map": { "level": "country", "style": "points",
                 "lat": { "table": s, "column": "lat" }, "lon": { "table": s, "column": "lon" } }
    }))
    .unwrap();
    let r = run_tile(&w.e, &t, &[]).unwrap();
    // Bandung shops 150; Jakarta cafés 90; a Jakarta shop 30. Latitude 95 and the blank row are dropped.
    let mut got: Vec<(f64, f64, String, f64)> = r
        .rows
        .iter()
        .map(|x| {
            (
                x[0].as_f64().unwrap(),
                x[1].as_f64().unwrap(),
                x[2].as_str().unwrap().to_string(),
                x[3].as_f64().unwrap(),
            )
        })
        .collect();
    got.sort_by(|a, b| a.3.total_cmp(&b.3));
    assert_eq!(
        got,
        [
            (-6.2, 106.8, "shop".into(), 30.0),
            (-6.2, 106.8, "cafe".into(), 90.0),
            (-6.9, 107.6, "shop".into(), 150.0)
        ]
    );
    assert_eq!(r.geo.unwrap().dropped, 2);
}

#[test]
fn a_click_on_a_place_filters_by_every_name_of_it() {
    let w = world("click");
    let s = w.id("sales");
    let map = w.map(
        "sales",
        "place",
        "amount",
        "sum",
        json!({ "level": "province" }),
    );
    let kpi: Tile = serde_json::from_value(json!({
        "id": "k", "type": "kpi", "measures": [{ "table": s, "column": "amount", "agg": "sum" }], "dims": []
    }))
    .unwrap();
    w.e.set_dashboards(vec![json!({ "id": "d", "title": "Map", "tiles": [
        { "id": "m", "type": "map",
          "measures": [{ "table": s, "column": "amount", "agg": "sum" }],
          "dims": [{ "table": s, "column": "place" }],
          "map": { "level": "province" } },
        { "id": "k", "type": "kpi", "measures": [{ "table": s, "column": "amount", "agg": "sum" }], "dims": [] }
    ]})]);
    let snap = snapshot::build(&w.e, "d", &[]).unwrap();
    // A click on Jawa Barat: Jawa Barat, Jabar and 32: 170. On Jakarta: 100.
    let total = |code: &str| {
        snap["cross"]["m"][code]["k"]["rows"][0][0]
            .as_f64()
            .unwrap()
    };
    assert_eq!(total("ID-JB"), 170.0);
    assert_eq!(total("ID-JK"), 100.0);
    // The same filter in the app, as the tile sends it.
    let filter: DashFilter = serde_json::from_value(json!({
        "table": s, "column": "place", "op": "in", "values": ["Jawa Barat", "Jabar", "32"], "fromTile": "m"
    }))
    .unwrap();
    let r = run_tile(&w.e, &kpi, &[filter.clone()]).unwrap();
    assert_eq!(r.rows[0][0].as_f64(), Some(170.0));
    // The map itself is not filtered by its own click: both places stay.
    assert_eq!(run_tile(&w.e, &map, &[filter]).unwrap().rows.len(), 2);
}

#[test]
fn a_map_without_its_fields_says_what_it_needs() {
    let w = world("words");
    let s = w.id("sales");
    let empty: Tile = serde_json::from_value(json!({
        "id": "m", "type": "map", "measures": [{ "table": s, "column": "amount", "agg": "sum" }], "dims": [],
        "map": { "level": "province" }
    }))
    .unwrap();
    assert_eq!(
        run_tile(&w.e, &empty, &[]).unwrap_err().to_string(),
        "A map needs a field that names the places."
    );
    let points: Tile = serde_json::from_value(json!({
        "id": "m", "type": "map", "measures": [], "dims": [], "map": { "level": "country", "style": "points" }
    }))
    .unwrap();
    assert_eq!(
        run_tile(&w.e, &points, &[]).unwrap_err().to_string(),
        "A map of points needs a latitude and a longitude."
    );
}

#[test]
fn an_exported_file_carries_only_the_maps_its_tiles_draw() {
    let w = world("export");
    let s = w.id("sales");
    let r = w.id("regencies");
    let dash = |tiles: Value| json!({ "id": "d", "title": "Map", "tiles": tiles });
    // A kabupaten map: the kabupaten and, for their lines, the provinces. Not the world.
    w.e.set_dashboards(vec![dash(json!([
        { "id": "m", "type": "map", "measures": [{ "table": r, "column": "amount", "agg": "sum" }],
          "dims": [{ "table": r, "column": "kab" }], "map": { "level": "regency", "country": "ID" } }
    ]))]);
    let mut payload = snapshot::build(&w.e, "d", &[]).unwrap();
    tenun::add_maps(&mut payload, &w.e.geo_dir());
    let mut carried: Vec<&String> = payload["geo"].as_object().unwrap().keys().collect();
    carried.sort();
    assert_eq!(carried, ["ID-1", "ID-2"]);
    assert_eq!(payload["geo"]["ID-2"]["type"], "Topology");
    // No map, no shapes.
    w.e.set_dashboards(vec![dash(json!([
        { "id": "k", "type": "kpi", "measures": [{ "table": s, "column": "amount", "agg": "sum" }], "dims": [] }
    ]))]);
    let mut payload = snapshot::build(&w.e, "d", &[]).unwrap();
    tenun::add_maps(&mut payload, &w.e.geo_dir());
    assert!(payload.get("geo").is_none());
    // Another country's map comes from its downloaded file.
    let dir = w.e.geo_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("KEN-ADM1.geojson"),
        r#"{"type":"FeatureCollection","features":[]}"#,
    )
    .unwrap();
    let k = w.id("kenya");
    w.e.set_dashboards(vec![dash(json!([
        { "id": "m", "type": "map", "measures": [{ "table": k, "column": "amount", "agg": "sum" }],
          "dims": [{ "table": k, "column": "county" }], "map": { "level": "province", "country": "KE" } },
        { "id": "w", "type": "map", "measures": [{ "table": k, "column": "amount", "agg": "sum" }],
          "dims": [{ "table": k, "column": "county" }], "map": { "level": "country" } }
    ]))]);
    let mut payload = snapshot::build(&w.e, "d", &[]).unwrap();
    tenun::add_maps(&mut payload, &w.e.geo_dir());
    let mut carried: Vec<&String> = payload["geo"].as_object().unwrap().keys().collect();
    carried.sort();
    assert_eq!(carried, ["KE-province", "world"]);
}
