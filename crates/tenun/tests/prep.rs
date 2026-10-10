//! A column override and an unpivot survive saving, reopening, the exported file and the snapshot.

use serde_json::{Value, json};
use std::path::Path;
use tenun_core::Engine;
use tenun_core::catalog::DType;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot;
use tenun_core::workspace::{ColumnSetup, Source, SourceKind, TableSetup, Transform, Workspace};

fn setup() -> TableSetup {
    TableSetup {
        columns: [
            (
                "Budget".to_string(),
                ColumnSetup {
                    label: Some("Budget (Rp)".into()),
                    description: Some("Per month".into()),
                    ..ColumnSetup::default()
                },
            ),
            (
                "Owner".to_string(),
                ColumnSetup {
                    hidden: Some(true),
                    ..ColumnSetup::default()
                },
            ),
        ]
        .into(),
        transforms: vec![Transform::Unpivot {
            keep: vec!["Department".into(), "Owner".into()],
            name: "Month".into(),
            value: "Budget".into(),
        }],
        ..TableSetup::default()
    }
}

#[test]
fn overrides_and_unpivot_survive_save_reopen_export_and_snapshot() {
    let dir = std::env::temp_dir().join(format!("tenun-prep-survive-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tenun-core/tests/fixtures/months_across.xlsx");
    std::fs::copy(fixture, dir.join("months_across.xlsx")).unwrap();

    let tile = json!({
        "id": "t1", "type": "bar", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
        "measures": [{ "table": "s1/Budget", "column": "Budget", "agg": "sum" }],
        "dims": [{ "table": "s1/Budget", "column": "Month", "grain": "month" }]
    });
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    e.replace(
        Workspace {
            name: "Prep".into(),
            sources: vec![Source {
                id: "s1".into(),
                name: "months_across.xlsx".into(),
                kind: SourceKind::Excel {
                    path: "months_across.xlsx".into(),
                },
            }],
            dashboards: vec![json!({ "id": "d1", "title": "T", "refresh": 0, "tiles": [tile] })],
            ..Workspace::default()
        },
        Some(dir.join("Prep.tenun")),
    );
    e.set_table_setup("s1/Budget", Some(setup())).unwrap();
    let path = e.save(None).unwrap();

    // The file holds the setup next to the sources, and no data.
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        saved["tables"]["s1/Budget"]["transforms"][0]["kind"],
        "unpivot"
    );
    assert_eq!(
        saved["tables"]["s1/Budget"]["columns"]["Budget"]["label"],
        "Budget (Rp)"
    );
    assert_eq!(
        saved["tables"]["s1/Budget"]["columns"]["Owner"]["hidden"],
        true
    );

    // A new engine opens it and sees the same table, the same labels and the same numbers.
    let e2 = Engine::new(Secrets::new(dir.join("secrets2.json"))).unwrap();
    e2.open(&path).unwrap();
    assert!(e2.snapshot().errors.is_empty());
    let t = e2.catalog().table("s1/Budget").cloned().unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Department", "Owner", "Month", "Budget"]);
    assert_eq!(t.column("Month").unwrap().dtype, DType::Date);
    assert_eq!(
        snapshot_json(&e2)["workspace"]["tables"]["s1/Budget"],
        saved["tables"]["s1/Budget"]
    );

    // The snapshot of the dashboard: the totals by month and the labels of the columns.
    let snap = snapshot::build(&e2, "d1", &[]).unwrap();
    let rows = &snap["base"]["t1"]["rows"];
    let got: Vec<(String, f64)> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r[0].as_str().unwrap().to_string(), r[1].as_f64().unwrap()))
        .collect();
    assert_eq!(
        got,
        [
            ("2026-01-01".to_string(), 160.0),
            ("2026-02-01".to_string(), 180.0),
            ("2026-03-01".to_string(), 130.0),
            ("2026-04-01".to_string(), 220.0),
        ]
    );
    let cols = snap["tables"][0]["columns"].as_array().unwrap();
    let col = |n: &str| cols.iter().find(|c| c["name"] == n).unwrap().clone();
    assert_eq!(col("Budget")["label"], "Budget (Rp)");
    assert_eq!(col("Budget")["description"], "Per month");
    assert_eq!(col("Owner")["hidden"], true);

    // The exported file carries the same labels, and the numbers are the unpivoted ones.
    let html = tenun::export_html(&snap);
    assert!(html.contains("Budget (Rp)"));
    assert!(html.contains("2026-04-01"));
}

fn snapshot_json(e: &Engine) -> Value {
    serde_json::to_value(e.snapshot()).unwrap()
}
