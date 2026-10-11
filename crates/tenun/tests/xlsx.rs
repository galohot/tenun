//! The Excel file of a tile: header row, numbers, real dates and a bold total row, read back
//! with calamine.

use calamine::{Data, DataType, Reader, Xlsx, open_workbook_from_rs};
use serde_json::{Value, json};
use std::io::Cursor;
use tenun::xlsx;
use tenun_core::Engine;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{Tile, run_tile};
use tenun_core::workspace::{Source, SourceKind, Workspace};

const SALES: &str = "date,cat,amount
2026-09-05,Coffee,100
2026-09-20,Tea,50
2026-10-02,Coffee,200
2026-10-03,Coffee,40.5
";

fn engine(tag: &str) -> Engine {
    let dir = std::env::temp_dir().join(format!("tenun-xl-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sales.csv"), SALES).unwrap();
    let ws = Workspace {
        name: "XL".into(),
        sources: vec![Source {
            id: "s1".into(),
            name: "sales.csv".into(),
            kind: SourceKind::Csv {
                path: "sales.csv".into(),
            },
        }],
        ..Workspace::default()
    };
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    e.replace(ws, Some(dir.join("X.tenun")));
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    e
}

fn file(e: &Engine, tile: &Value, headers: &[&str]) -> Vec<u8> {
    let t: Tile = serde_json::from_value(tile.clone()).unwrap();
    let res = run_tile(e, &t, &[]).unwrap();
    let headers: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    xlsx::build(tile, &res, &headers, "Sales: by [category]").unwrap()
}

fn read(bytes: Vec<u8>) -> Vec<Vec<Data>> {
    let mut wb: Xlsx<_> = open_workbook_from_rs(Cursor::new(bytes)).unwrap();
    let name = wb.sheet_names()[0].clone();
    let range = wb.worksheet_range(&name).unwrap();
    range.rows().map(|r| r.to_vec()).collect()
}

#[test]
fn table_has_header_numbers_and_a_total_row() {
    let e = engine("table");
    let tile = json!({
        "id": "t", "type": "table", "totals": true,
        "measures": [{ "table": "s1/sales", "column": "amount", "agg": "sum" }],
        "dims": [{ "table": "s1/sales", "column": "cat" }],
        "formats": [{ "decimals": 1, "currency": "IDR" }],
    });
    let rows = read(file(&e, &tile, &["Category", "Amount"]));
    assert_eq!(rows[0][0], Data::String("Category".into()));
    assert_eq!(rows[0][1], Data::String("Amount".into()));
    // Coffee 340.5 and Tea 50, then the total.
    let body: Vec<(String, f64)> = rows[1..3]
        .iter()
        .map(|r| (r[0].to_string(), r[1].as_f64().unwrap()))
        .collect();
    assert!(body.contains(&("Coffee".to_string(), 340.5)));
    assert!(body.contains(&("Tea".to_string(), 50.0)));
    assert_eq!(rows[3][0], Data::String("Total".into()));
    assert_eq!(rows[3][1].as_f64(), Some(390.5));
    assert_eq!(rows.len(), 4);
}

#[test]
fn dates_are_dates_and_there_is_no_total_row_without_totals() {
    let e = engine("dates");
    let tile = json!({
        "id": "t", "type": "line",
        "measures": [{ "table": "s1/sales", "column": "amount", "agg": "sum" }],
        "dims": [{ "table": "s1/sales", "column": "date", "grain": "month" }],
    });
    let rows = read(file(&e, &tile, &[]));
    assert!(matches!(rows[1][0], Data::DateTime(_)), "{:?}", rows[1][0]);
    assert_eq!(
        rows[1][0]
            .as_datetime()
            .unwrap()
            .format("%Y-%m")
            .to_string(),
        "2026-09"
    );
    assert_eq!(rows.len(), 3);
}

#[test]
fn the_note_about_a_link_is_written_under_the_numbers() {
    let e = engine("note");
    let tile = json!({
        "id": "t", "type": "table",
        "measures": [{ "table": "s1/sales", "column": "amount", "agg": "sum" }],
        "dims": [{ "table": "s1/sales", "column": "cat" }],
    });
    let t: Tile = serde_json::from_value(tile.clone()).unwrap();
    let mut res = run_tile(&e, &t, &[]).unwrap();
    res.notes = vec!["stores.csv has 2 rows with id 7. Tenun used the first.".into()];
    let rows = read(xlsx::build(&tile, &res, &[], "Sales").unwrap());
    // The header, two groups, a blank row, then the sentence.
    assert_eq!(rows.len(), 5, "{rows:?}");
    assert_eq!(
        rows[4][0].as_string().as_deref(),
        Some("stores.csv has 2 rows with id 7. Tenun used the first.")
    );
}
