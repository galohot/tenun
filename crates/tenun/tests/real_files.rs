//! What the real-file test set found (v5 build 00a), each as a small file of the same shape: a quote
//! deep in a big CSV, title lines above a header of years, Windows and UTF-16 text, decimal commas,
//! a CSV without a header, an `.xlsx` saved as `.xls`, and a web page saved as `.xls`.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::catalog::{DType, Table};
use tenun_core::engine::AddSource;
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-real-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Adds one file to a fresh engine and returns the engine and its table.
fn load(d: &Path, name: &str, bytes: &[u8]) -> (Engine, Table) {
    let path = d.join(name);
    std::fs::write(&path, bytes).unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.add_source(AddSource::File {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    let snap = e.snapshot();
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    let t = snap.tables[0].clone();
    (e, t)
}

fn ask(e: &Engine, v: Value) -> Vec<Vec<Value>> {
    let q: SemanticQuery = serde_json::from_value(v).unwrap();
    e.query(&q).unwrap().rows
}

fn names(t: &Table) -> Vec<&str> {
    t.columns.iter().map(|c| c.name.as_str()).collect()
}

fn dtype(t: &Table, col: &str) -> DType {
    t.columns.iter().find(|c| c.name == col).unwrap().dtype
}

#[test]
fn a_quote_after_the_first_twenty_thousand_rows_does_not_break_queries() {
    let d = dir("quote");
    let mut text = String::from("region,code,year,share\n");
    for i in 0..25_000 {
        text.push_str(&format!("Place {i},P{i},{},1.5\n", 1990 + i % 30));
    }
    // DuckDB's own guess reads 20,480 rows, sees no quotes and reads this line as five fields.
    text.push_str("\"Middle East, North Africa\",WB,1990,34.5\n");
    let (e, t) = load(&d, "poverty.csv", text.as_bytes());
    assert_eq!(t.rows, Some(25_001));
    assert_eq!(dtype(&t, "year"), DType::Number);
    let rows = ask(
        &e,
        json!({ "measures": [{ "table": t.id, "column": "share", "agg": "sum" }],
                "dimensions": [{ "table": t.id, "column": "region" }],
                "filters": [{ "table": t.id, "column": "code", "op": "in", "values": ["WB"] }] }),
    );
    assert_eq!(
        rows,
        vec![vec![json!("Middle East, North Africa"), json!(34.5)]]
    );
}

#[test]
fn title_lines_above_a_header_of_years_are_skipped() {
    let d = dir("titles");
    let text = "\u{feff}\"Data Source\",\"World Development Indicators\",\r\n\r\n\"Last Updated Date\",\"2026-10-08\",\r\n\r\n\
                \"Country Name\",\"Country Code\",\"1960\",\"1961\",\r\n\
                \"Aruba\",\"ABW\",\"\",\"\",\r\n\"Indonesia\",\"IDN\",\"5.6\",\"5.9\",\r\n";
    let (e, t) = load(&d, "API_GDP.csv", text.as_bytes());
    assert_eq!(
        &names(&t)[..4],
        ["Country Name", "Country Code", "1960", "1961"]
    );
    assert_eq!(t.rows, Some(2));
    assert_eq!(dtype(&t, "1961"), DType::Number);
    assert!(
        t.notes
            .iter()
            .any(|n| n.contains("2 title lines") && n.contains("line 5")),
        "{:?}",
        t.notes
    );
    let total = ask(
        &e,
        json!({ "measures": [{ "table": t.id, "column": "1961", "agg": "sum" }] }),
    );
    assert_eq!(total, vec![vec![json!(5.9)]]);
}

#[test]
fn windows_text_is_read_as_windows_1252() {
    let d = dir("cp1252");
    // “Calves” and café in Windows-1252, as Excel on Windows saves a CSV.
    let (e, t) = load(
        &d,
        "slaughter.csv",
        b"kind,head\n\x93Calves\x94,141.8\ncaf\xe9,2\n",
    );
    assert!(
        t.notes.iter().any(|n| n.contains("Windows-1252")),
        "{:?}",
        t.notes
    );
    let rows = ask(
        &e,
        json!({ "measures": [{ "table": t.id, "column": "head", "agg": "sum" }],
                "dimensions": [{ "table": t.id, "column": "kind" }],
                "sort": [{ "by": 0 }] }),
    );
    assert_eq!(
        rows,
        vec![
            vec![json!("café"), json!(2.0)],
            vec![json!("“Calves”"), json!(141.8)]
        ]
    );
}

#[test]
fn utf16_text_with_a_byte_order_mark_is_read() {
    let d = dir("utf16");
    let mut bytes = vec![0xFF, 0xFE];
    for u in "kota\tjumlah\nBandung\t3\nBogor\t4\n".encode_utf16() {
        bytes.extend(u.to_le_bytes());
    }
    let (e, t) = load(&d, "export.csv", &bytes);
    assert_eq!(names(&t), ["kota", "jumlah"]);
    let total = ask(
        &e,
        json!({ "measures": [{ "table": t.id, "column": "jumlah", "agg": "sum" }] }),
    );
    assert_eq!(total, vec![vec![json!(7.0)]]);
}

#[test]
fn a_decimal_comma_reads_as_a_number_where_the_delimiter_is_a_semicolon() {
    let d = dir("comma");
    let (e, t) = load(
        &d,
        "anggaran.csv",
        b"wilayah;nilai;jumlah\nBandung;1.234,5;3\nBogor;22838,52;4\n",
    );
    assert_eq!(dtype(&t, "nilai"), DType::Number);
    let total = ask(
        &e,
        json!({ "measures": [{ "table": t.id, "column": "nilai", "agg": "sum" }] }),
    );
    assert_eq!(total, vec![vec![json!(24073.02)]]);
    // With a comma as the delimiter, nothing changes.
    let (_, t) = load(&dir("comma2"), "plain.csv", b"a,b\nx,1.5\n");
    assert!(t.notes.is_empty(), "{:?}", t.notes);
}

#[test]
fn a_csv_without_a_header_keeps_its_first_row() {
    let d = dir("noheader");
    let (_, t) = load(
        &d,
        "codes.csv",
        "A01Z;Produits de la culture;200901;999912\nA02Z;Produits sylvicoles;200901;999912\n"
            .as_bytes(),
    );
    assert_eq!(t.rows, Some(2));
}

#[test]
fn an_xlsx_saved_with_an_xls_name_opens() {
    let d = dir("xlsasxlsx");
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    ws.write(0, 0, "store").unwrap();
    ws.write(0, 1, "amount").unwrap();
    ws.write(1, 0, "A").unwrap();
    ws.write(1, 1, 10).unwrap();
    let (e, t) = load(&d, "report.xls", &wb.save_to_buffer().unwrap());
    let total = ask(
        &e,
        json!({ "measures": [{ "table": t.id, "column": "amount", "agg": "sum" }] }),
    );
    assert_eq!(total, vec![vec![json!(10.0)]]);
}

#[test]
fn a_web_page_saved_as_xls_is_refused_in_words() {
    let d = dir("htmlxls");
    let path = d.join("tabel.xls");
    std::fs::write(
        &path,
        "<html><body><table><tr><td>a</td></tr></table></body></html>",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    let err = match e.add_source(AddSource::File {
        path: path.to_string_lossy().into_owned(),
    }) {
        Ok(id) => e.snapshot().errors[&id].clone(),
        Err(err) => err.to_string(),
    };
    assert_eq!(
        err,
        "tabel.xls is a web page saved with a spreadsheet name. Open it in Excel, save it as .xlsx, and add that file."
    );
}
