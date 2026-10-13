//! Drill-down inside a chart (v5 build 01, V5-D3): a date goes year › quarter › month › day, a hierarchy goes down its
//! fields, "Next level" shows the level below for every value, the path filters the tile and nothing else, and an
//! export holds one level for its first 50 values (V5-D5).
//!
//! The data, by hand: orders A 10 (Nov 2025), A 5 (Jan 2026), B 7 (Feb 2026), C 4 (Apr 2 2026), A 1 (Apr 30 2026),
//! B 2 (May 2026). Stores A and B are in Jawa, C in Sumatera. So 2025 = 10 and 2026 = 19; Q1 2026 = 12, Q2 2026 = 7;
//! April 2026 = 5, May 2026 = 2; Jawa = 25 (A 16, B 9), Sumatera = 4.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::drill::{self, DrillState, Step};
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile};
use tenun_core::workspace::{FieldRef, Hierarchy, Workspace};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-drill-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Data {
    e: Engine,
    orders: String,
    stores: String,
}

fn data(d: &Path) -> Data {
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
            name: "Drill".into(),
            ..Workspace::default()
        },
        Some(d.join("Drill.tenun")),
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
    let orders = add("orders.csv");
    let stores = add("stores.csv");
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
    Data { e, orders, stores }
}

fn steps(values: &[Option<Value>]) -> DrillState {
    DrillState {
        path: values.iter().map(|v| Step { value: v.clone() }).collect(),
    }
}

fn by_date(d: &Data, grain: &str) -> Tile {
    serde_json::from_value(json!({
        "id": "t1", "type": "bar", "drill": {},
        "measures": [{ "table": d.orders, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": d.orders, "column": "date", "grain": grain }],
        "sort": { "by": "label" }
    }))
    .unwrap()
}

fn rows(d: &Data, t: &Tile, state: &DrillState, filters: &[DashFilter]) -> Vec<(Value, f64)> {
    drill::run(&d.e, t, filters, state)
        .unwrap()
        .rows
        .into_iter()
        .map(|r| (r[0].clone(), r[1].as_f64().unwrap()))
        .collect()
}

#[test]
fn a_date_drills_from_years_to_quarters_to_months_and_next_level_shows_every_value() {
    let d = data(&dir("dates"));
    let t = by_date(&d, "year");
    let top = drill::run(&d.e, &t, &[], &DrillState::default()).unwrap();
    let info = top.drill.clone().unwrap();
    assert_eq!(info.levels, ["Year", "Quarter", "Month", "Day"]);
    assert_eq!((info.at, info.can_down), (0, true));
    assert_eq!(
        rows(&d, &t, &DrillState::default(), &[]),
        [(json!("2025-01-01"), 10.0), (json!("2026-01-01"), 19.0)]
    );

    // 2026 by quarter: 2025 is left out.
    let y2026 = steps(&[Some(json!("2026-01-01"))]);
    assert_eq!(
        rows(&d, &t, &y2026, &[]),
        [(json!("2026-01-01"), 12.0), (json!("2026-04-01"), 7.0)]
    );
    // Q2 2026 by month.
    let q2 = steps(&[Some(json!("2026-01-01")), Some(json!("2026-04-01"))]);
    assert_eq!(
        rows(&d, &t, &q2, &[]),
        [(json!("2026-04-01"), 5.0), (json!("2026-05-01"), 2.0)]
    );
    let res = drill::run(&d.e, &t, &[], &q2).unwrap();
    assert_eq!(res.drill.unwrap().at, 2);

    // Next level from the top: every quarter of every year.
    let every = steps(&[None]);
    assert_eq!(
        rows(&d, &t, &every, &[]),
        [
            (json!("2025-10-01"), 10.0),
            (json!("2026-01-01"), 12.0),
            (json!("2026-04-01"), 7.0)
        ]
    );
    // A path longer than the chain stops at the deepest level.
    let deep = steps(&[None, None, None, None, None]);
    let res = drill::run(&d.e, &t, &[], &deep).unwrap();
    let info = res.drill.unwrap();
    assert_eq!((info.at, info.can_down, info.path.len()), (3, false, 3));
    // Every day from Nov 3, 2025 to May 10, 2026: a time axis has no gaps.
    assert_eq!(res.rows.len(), 189);
}

#[test]
fn a_tile_by_month_drills_to_days_only_and_a_tile_without_drill_is_as_it_was() {
    let d = data(&dir("month"));
    let t = by_date(&d, "month");
    let info = drill::run(&d.e, &t, &[], &DrillState::default())
        .unwrap()
        .drill
        .unwrap();
    assert_eq!(info.levels, ["Month", "Day"]);
    let mut plain = t.clone();
    plain.drill = None;
    let res = drill::run(&d.e, &plain, &[], &steps(&[Some(json!("2026-04-01"))])).unwrap();
    assert!(res.drill.is_none());
    assert_eq!(res.rows.len(), 7, "Nov 2025 to May 2026, the path ignored");
}

#[test]
fn a_hierarchy_drills_from_region_to_its_stores() {
    let d = data(&dir("hier"));
    d.e.set_hierarchies(vec![Hierarchy {
        id: String::new(),
        name: "Region › Store".into(),
        levels: vec![
            FieldRef {
                table: d.stores.clone(),
                column: "region".into(),
            },
            FieldRef {
                table: d.stores.clone(),
                column: "store".into(),
            },
        ],
    }])
    .unwrap();
    let h = d.e.hierarchies();
    assert_eq!(h[0].id, "h1");
    let t: Tile = serde_json::from_value(json!({
        "id": "t2", "type": "pie", "drill": { "hierarchy": "h1" },
        "measures": [{ "table": d.orders, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": d.stores, "column": "region" }],
        "sort": { "by": "label" }
    }))
    .unwrap();
    assert_eq!(
        rows(&d, &t, &DrillState::default(), &[]),
        [(json!("Jawa"), 25.0), (json!("Sumatera"), 4.0)]
    );
    let jawa = steps(&[Some(json!("Jawa"))]);
    assert_eq!(
        rows(&d, &t, &jawa, &[]),
        [(json!("A"), 16.0), (json!("B"), 9.0)]
    );
    let info = drill::run(&d.e, &t, &[], &jawa).unwrap().drill.unwrap();
    assert_eq!(info.levels, ["region", "store"]);
    assert!(!info.can_down);

    // A click on another tile still filters a drilled tile: only 2026 here, so A is 6.
    let click: Vec<DashFilter> = serde_json::from_value(json!([{
        "table": d.orders, "column": "date", "grain": "year", "op": "in", "values": ["2026-01-01"], "fromTile": "other"
    }]))
    .unwrap();
    assert_eq!(
        rows(&d, &t, &jawa, &click),
        [(json!("A"), 6.0), (json!("B"), 9.0)]
    );

    // The hierarchy is saved with the workspace and comes back.
    d.e.save(None).unwrap();
    let back = Workspace::load(&d.e.workspace_path().unwrap()).unwrap();
    assert_eq!(back.hierarchies, h);
}

#[test]
fn a_hierarchy_that_cannot_be_drilled_is_refused_in_words() {
    let dd = dir("refuse");
    let d = data(&dd);
    std::fs::write(dd.join("other.csv"), "kind,n\nx,1\n").unwrap();
    let id =
        d.e.add_source(AddSource::File {
            path: dd.join("other.csv").to_string_lossy().into_owned(),
        })
        .unwrap();
    let other =
        d.e.snapshot()
            .tables
            .iter()
            .find(|t| t.source_id == id)
            .unwrap()
            .id
            .clone();
    let h = |name: &str, levels: Vec<FieldRef>| Hierarchy {
        id: String::new(),
        name: name.into(),
        levels,
    };
    let region = FieldRef {
        table: d.stores.clone(),
        column: "region".into(),
    };
    let err =
        d.e.set_hierarchies(vec![h(
            "Kinds",
            vec![
                region.clone(),
                FieldRef {
                    table: other,
                    column: "kind".into(),
                },
            ],
        )])
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Kinds: kind does not reach region through the model. Link their tables first."
    );
    let err =
        d.e.set_hierarchies(vec![h("One", vec![region.clone()])])
            .unwrap_err();
    assert_eq!(
        err.to_string(),
        "One needs 2 to 6 fields, from the top down."
    );
    let err =
        d.e.set_hierarchies(vec![h(" ", vec![region.clone(), region.clone()])])
            .unwrap_err();
    assert_eq!(err.to_string(), "A hierarchy needs a name.");
    let err =
        d.e.set_hierarchies(vec![h(
            "Gone",
            vec![
                region,
                FieldRef {
                    table: d.stores.clone(),
                    column: "nope".into(),
                },
            ],
        )])
        .unwrap_err();
    assert_eq!(err.to_string(), "Gone: the field nope is not in the data.");
    assert!(
        d.e.hierarchies().is_empty(),
        "nothing is kept from a refused list"
    );
}

#[test]
fn an_export_holds_one_level_for_the_first_fifty_values_and_next_level() {
    let dd = dir("export");
    let d = data(&dd);
    // 60 groups of 2: the export holds 50 of them.
    let mut big = String::from("grp,sub,amount\n");
    for g in 0..60 {
        for s in 0..2 {
            big.push_str(&format!("G{g:02},S{s},{}\n", g * 10 + s));
        }
    }
    std::fs::write(dd.join("big.csv"), big).unwrap();
    let id =
        d.e.add_source(AddSource::File {
            path: dd.join("big.csv").to_string_lossy().into_owned(),
        })
        .unwrap();
    let bigt =
        d.e.snapshot()
            .tables
            .iter()
            .find(|t| t.source_id == id)
            .unwrap()
            .id
            .clone();
    d.e.set_hierarchies(vec![Hierarchy {
        id: String::new(),
        name: "Group".into(),
        levels: vec![
            FieldRef {
                table: bigt.clone(),
                column: "grp".into(),
            },
            FieldRef {
                table: bigt.clone(),
                column: "sub".into(),
            },
        ],
    }])
    .unwrap();
    d.e.set_dashboards(vec![json!({
        "id": "d1", "title": "Drill", "refresh": 0,
        "tiles": [
            { "id": "y", "type": "bar", "title": "", "x": 0, "y": 0, "w": 6, "h": 4, "drill": {},
              "measures": [{ "table": d.orders, "column": "amount", "agg": "sum" }],
              "dims": [{ "table": d.orders, "column": "date", "grain": "year" }] },
            { "id": "g", "type": "table", "title": "", "x": 6, "y": 0, "w": 6, "h": 4, "drill": { "hierarchy": "h1" },
              "measures": [{ "table": bigt, "column": "amount", "agg": "sum" }],
              "dims": [{ "table": bigt, "column": "grp" }], "sort": { "by": "label" } },
            { "id": "plain", "type": "bar", "title": "", "x": 0, "y": 4, "w": 6, "h": 4,
              "measures": [{ "table": d.orders, "column": "amount", "agg": "sum" }],
              "dims": [{ "table": d.stores, "column": "region" }] }
        ]
    })]);
    let p = snapshot::build(&d.e, "d1", &[]).unwrap();
    assert_eq!(p["base"]["y"]["drill"]["at"], json!(0));
    assert!(p["base"]["plain"].get("drill").is_none());
    let year = &p["down"]["y"];
    let quarters: Vec<(Value, f64)> = year["2026-01-01"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r[0].clone(), r[1].as_f64().unwrap()))
        .collect();
    let mut quarters = quarters;
    quarters.sort_by_key(|q| q.0.to_string());
    assert_eq!(
        quarters,
        [(json!("2026-01-01"), 12.0), (json!("2026-04-01"), 7.0)]
    );
    assert_eq!(year["2026-01-01"]["drill"]["at"], json!(1));
    assert_eq!(year["\u{0}all"]["rows"].as_array().unwrap().len(), 3);
    let groups = p["down"]["g"].as_object().unwrap();
    assert_eq!(
        groups.len(),
        snapshot::MAX_DRILL_VALUES + 1,
        "50 values and the next level"
    );
    // G00 is 0 + 1; its two subgroups.
    let g00: Vec<f64> = groups["G00"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r[1].as_f64().unwrap())
        .collect();
    assert_eq!(g00.iter().sum::<f64>(), 1.0);
    assert!(p["down"].get("plain").is_none());
    assert_eq!(p["hierarchies"][0]["id"], json!("h1"));
}

#[test]
fn a_hierarchy_read_through_a_named_link_keeps_the_link_at_every_level() {
    // Sold at A, shipped from C: 10. Sold at B, shipped from A: 5. Sold at C, shipped from C: 4.
    // Through "Shipped from": Jawa 5 (A), Sumatera 14 (C). Through the store it was sold at: Jawa 15, Sumatera 4.
    let dd = dir("via");
    std::fs::write(
        dd.join("orders.csv"),
        "store,from_store,amount\nA,C,10\nB,A,5\nC,C,4\n",
    )
    .unwrap();
    std::fs::write(
        dd.join("stores.csv"),
        "store,region\nA,Jawa\nB,Jawa\nC,Sumatera\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(dd.join("secrets.json"))).unwrap();
    let add = |f: &str| {
        let id = e
            .add_source(AddSource::File {
                path: dd.join(f).to_string_lossy().into_owned(),
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
    let link = |col: &str| {
        e.add_relationship(
            FieldRef {
                table: orders.clone(),
                column: col.into(),
            },
            FieldRef {
                table: stores.clone(),
                column: "store".into(),
            },
        )
        .unwrap()
    };
    let sold = link("store");
    let shipped = link("from_store");
    e.rename_link(&sold, Some("Sold at".into())).unwrap();
    e.rename_link(&shipped, Some("Shipped from".into()))
        .unwrap();
    e.set_hierarchies(vec![Hierarchy {
        id: String::new(),
        name: "Region › Store".into(),
        levels: vec![
            FieldRef {
                table: stores.clone(),
                column: "region".into(),
            },
            FieldRef {
                table: stores.clone(),
                column: "store".into(),
            },
        ],
    }])
    .unwrap();
    let t: Tile = serde_json::from_value(json!({
        "id": "t", "type": "bar", "drill": { "hierarchy": "h1" },
        "measures": [{ "table": orders, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": stores, "column": "region", "via": shipped }],
        "sort": { "by": "label" }
    }))
    .unwrap();
    let d = Data { e, orders, stores };
    assert_eq!(
        rows(&d, &t, &DrillState::default(), &[]),
        [(json!("Jawa"), 5.0), (json!("Sumatera"), 14.0)]
    );
    let sumatera = steps(&[Some(json!("Sumatera"))]);
    assert_eq!(rows(&d, &t, &sumatera, &[]), [(json!("C"), 14.0)]);
}
