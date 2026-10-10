//! Data prep on three small messy workbooks (made by tests/fixtures/make_prep_fixtures.py): a
//! title block with a footer, merged year and month headers, and months across the top. The
//! expected numbers are worked out by hand from the rows in that script.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::catalog::{DType, Table};
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::workspace::{
    ColumnSetup, DateFormat, Source, SourceKind, TableSetup, Transform, Workspace,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// An engine over a temp folder holding the given files (copied from the fixtures, or written).
fn engine(tag: &str, files: &[(&str, &str)]) -> (Engine, PathBuf) {
    let dir = std::env::temp_dir().join(format!("tenun-prep-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut sources = Vec::new();
    for (i, (file, text)) in files.iter().enumerate() {
        if text.is_empty() {
            std::fs::copy(fixture(file), dir.join(file)).unwrap();
        } else {
            std::fs::write(dir.join(file), text).unwrap();
        }
        let path = file.to_string();
        sources.push(Source {
            id: format!("s{}", i + 1),
            name: path.clone(),
            kind: if file.ends_with(".csv") {
                SourceKind::Csv { path }
            } else {
                SourceKind::Excel { path }
            },
        });
    }
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    let ws = Workspace {
        name: "Prep".into(),
        sources,
        ..Workspace::default()
    };
    e.replace(ws, Some(dir.join("T.tenun")));
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    (e, dir)
}

fn table(e: &Engine, id: &str) -> Table {
    e.catalog()
        .table(id)
        .cloned()
        .unwrap_or_else(|| panic!("no table {id}"))
}

fn names(t: &Table) -> Vec<&str> {
    t.columns.iter().map(|c| c.name.as_str()).collect()
}

fn ask(e: &Engine, measures: Value, dims: Value) -> Vec<Vec<Value>> {
    let q: SemanticQuery =
        serde_json::from_value(json!({ "measures": measures, "dimensions": dims })).unwrap();
    let mut rows = e.query(&q).unwrap().rows;
    rows.sort_by_key(|r| r[0].to_string());
    rows
}

fn sum(t: &str, c: &str) -> Value {
    json!([{ "table": t, "column": c, "agg": "sum" }])
}

#[test]
fn a_title_block_and_a_footer_are_left_out() {
    let (e, _) = engine("title", &[("title_footer.xlsx", "")]);
    let t = table(&e, "s1/Sales");
    assert_eq!(names(&t), ["Date", "Region", "Product", "Amount", "Qty"]);
    let types: Vec<DType> = t.columns.iter().map(|c| c.dtype).collect();
    assert_eq!(
        types,
        [
            DType::Date,
            DType::Text,
            DType::Text,
            DType::Number,
            DType::Number
        ]
    );
    assert_eq!(t.rows, Some(5));
    assert_eq!(t.header_row, Some(4));
    assert_eq!(
        t.notes,
        [
            "Skipped 2 title rows",
            "Header on row 4",
            "Dropped 2 footer rows"
        ]
    );
    // The total row did not become data: 100 + 50 + 200 + 30 + 70.
    let rows = ask(&e, sum("s1/Sales", "Amount"), json!([]));
    assert_eq!(rows[0][0].as_f64(), Some(450.0));
}

#[test]
fn merged_year_and_month_headers_join_and_the_region_fills_down() {
    let (e, _) = engine("merged", &[("merged_headers.xlsx", "")]);
    let t = table(&e, "s1/Revenue");
    assert_eq!(
        names(&t),
        [
            "Region", "Channel", "2025 Jan", "2025 Feb", "2026 Jan", "2026 Feb"
        ]
    );
    assert_eq!(t.rows, Some(6));
    assert_eq!(t.header_row, Some(2));
    assert!(
        t.notes.contains(&"Joined 2 header rows".to_string()),
        "{:?}",
        t.notes
    );
    assert!(
        t.notes.contains(&"Filled down Region".to_string()),
        "{:?}",
        t.notes
    );
    // Every row has a region. North: 10 + 11 + 12 in 2025 Jan; South: 5 + 15 + 25.
    let rows = ask(
        &e,
        sum("s1/Revenue", "2025 Jan"),
        json!([{ "table": "s1/Revenue", "column": "Region" }]),
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0], "North");
    assert_eq!(rows[0][1].as_f64(), Some(33.0));
    assert_eq!(rows[1][0], "South");
    assert_eq!(rows[1][1].as_f64(), Some(45.0));
}

fn months_setup() -> TableSetup {
    TableSetup {
        transforms: vec![Transform::Unpivot {
            keep: vec!["Department".into(), "Owner".into()],
            name: "Month".into(),
            value: "Budget".into(),
        }],
        ..TableSetup::default()
    }
}

#[test]
fn months_across_turn_into_rows_and_back() {
    let (e, _) = engine("months", &[("months_across.xlsx", "")]);
    let t = table(&e, "s1/Budget");
    assert_eq!(
        names(&t),
        [
            "Department",
            "Owner",
            "Jan-26",
            "Feb-26",
            "Mar-26",
            "Apr-26"
        ]
    );
    // Mar-26 has one blank cell of three.
    let mar = t.column("Mar-26").unwrap();
    assert!((mar.blank.unwrap() - 1.0 / 3.0).abs() < 1e-9);

    let plan = e.detect_unpivot("s1/Budget").expect("a wide sheet");
    assert_eq!(plan.keep, ["Department", "Owner"]);
    assert_eq!(plan.columns, ["Jan-26", "Feb-26", "Mar-26", "Apr-26"]);
    assert!(plan.dates);

    // The preview shows the first rows turned; the table itself has not changed yet.
    let pre = e
        .unpivot_preview(
            "s1/Budget",
            plan.keep.clone(),
            "Month".into(),
            "Budget".into(),
            5,
        )
        .unwrap();
    let cols: Vec<&str> = pre.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cols, ["Department", "Owner", "Month", "Budget"]);
    assert_eq!(pre.rows.len(), 5);
    assert_eq!(pre.rows[0][2], "2026-01-01");
    assert_eq!(table(&e, "s1/Budget").columns.len(), 6);

    assert!(
        e.set_table_setup("s1/Budget", Some(months_setup()))
            .unwrap()
    );
    let t = table(&e, "s1/Budget");
    assert_eq!(names(&t), ["Department", "Owner", "Month", "Budget"]);
    assert_eq!(t.column("Month").unwrap().dtype, DType::Date);
    // 3 departments x 4 months. The blank cell stays as a blank row.
    assert_eq!(t.rows, Some(12));
    assert!(e.detect_unpivot("s1/Budget").is_none());

    // By month: Jan 160, Feb 180, Mar 130 (the blank is not a zero row), Apr 220.
    let rows = ask(
        &e,
        sum("s1/Budget", "Budget"),
        json!([{ "table": "s1/Budget", "column": "Month", "grain": "month" }]),
    );
    let got: Vec<(String, f64)> = rows
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

    // Undo: the same menu clears it and the wide table is back.
    assert!(e.set_table_setup("s1/Budget", None).unwrap());
    assert_eq!(table(&e, "s1/Budget").columns.len(), 6);
    assert!(e.snapshot().workspace.tables.is_empty());
}

#[test]
fn the_header_row_can_be_chosen() {
    let (e, _) = engine("header", &[("title_footer.xlsx", "")]);
    let setup = TableSetup {
        header_row: Some(5),
        ..TableSetup::default()
    };
    e.set_table_setup("s1/Sales", Some(setup)).unwrap();
    let t = table(&e, "s1/Sales");
    // The first data row now names the columns; the header and the title rows are skipped.
    assert_eq!(names(&t), ["2026-01-02", "North", "Arabica", "100", "2"]);
    assert_eq!(t.header_row, Some(5));
    assert!(
        t.notes
            .contains(&"Header on row 5 (set by you)".to_string()),
        "{:?}",
        t.notes
    );
    // Back to detection.
    e.set_table_setup("s1/Sales", None).unwrap();
    assert_eq!(table(&e, "s1/Sales").header_row, Some(4));
}

const DATES: &str = "day,qty\n05/03/2026,1\n13/03/2026,2\nsoon,3\n,4\n";

#[test]
fn a_column_read_as_a_date_counts_what_it_could_not_read() {
    let (e, _) = engine("dates", &[("d.csv", DATES)]);
    assert_eq!(table(&e, "s1/d").column("day").unwrap().dtype, DType::Text);
    let set = |fmt| TableSetup {
        columns: [(
            "day".to_string(),
            ColumnSetup {
                dtype: Some(DType::Date),
                date_format: Some(fmt),
                ..ColumnSetup::default()
            },
        )]
        .into(),
        ..TableSetup::default()
    };
    // dd/mm/yyyy: 5 March and 13 March. "soon" cannot be read. The blank stays blank.
    e.set_table_setup("s1/d", Some(set(DateFormat::Dmy)))
        .unwrap();
    let t = table(&e, "s1/d");
    let day = t.column("day").unwrap();
    assert_eq!(day.dtype, DType::Date);
    assert_eq!(day.failed, Some(1));
    let p = e.preview("s1/d", 10).unwrap();
    let days: Vec<&Value> = p.rows.iter().map(|r| &r[0]).collect();
    assert_eq!(
        days,
        [
            &json!("2026-03-05"),
            &json!("2026-03-13"),
            &Value::Null,
            &Value::Null
        ]
    );
    // mm/dd/yyyy: 13/03 is not a month, so two cells fail and 05/03 is 3 May.
    e.set_table_setup("s1/d", Some(set(DateFormat::Mdy)))
        .unwrap();
    assert_eq!(table(&e, "s1/d").column("day").unwrap().failed, Some(2));
    let p = e.preview("s1/d", 10).unwrap();
    assert_eq!(p.rows[0][0], "2026-05-03");
    // Text to number: "soon" and "13/03/2026" are not numbers.
    let num = TableSetup {
        columns: [(
            "day".to_string(),
            ColumnSetup {
                dtype: Some(DType::Number),
                ..ColumnSetup::default()
            },
        )]
        .into(),
        ..TableSetup::default()
    };
    e.set_table_setup("s1/d", Some(num)).unwrap();
    assert_eq!(table(&e, "s1/d").column("day").unwrap().failed, Some(3));
}

#[test]
fn labels_hide_and_describe_without_loading_again() {
    let (e, dir) = engine("labels", &[("title_footer.xlsx", "")]);
    let setup = TableSetup {
        columns: [
            (
                "Amount".to_string(),
                ColumnSetup {
                    label: Some("Sales (Rp)".into()),
                    description: Some("Before tax".into()),
                    ..ColumnSetup::default()
                },
            ),
            (
                "Qty".to_string(),
                ColumnSetup {
                    hidden: Some(true),
                    ..ColumnSetup::default()
                },
            ),
        ]
        .into(),
        ..TableSetup::default()
    };
    assert!(!e.set_table_setup("s1/Sales", Some(setup.clone())).unwrap());
    let t = table(&e, "s1/Sales");
    let amount = t.column("Amount").unwrap();
    assert_eq!(amount.label.as_deref(), Some("Sales (Rp)"));
    assert_eq!(amount.description.as_deref(), Some("Before tax"));
    assert!(t.column("Qty").unwrap().hidden);
    // Queries keep the real names, and a hidden column still works.
    assert_eq!(
        ask(&e, sum("s1/Sales", "Qty"), json!([]))[0][0].as_f64(),
        Some(10.0)
    );
    // The setup is in the file, and a reopened workspace has the same labels.
    e.save(None).unwrap();
    let (e2, _) = (Engine::new(Secrets::new(dir.join("s2.json"))).unwrap(), ());
    e2.open(&dir.join("T.tenun")).unwrap();
    assert_eq!(e2.snapshot().workspace.tables["s1/Sales"], setup);
    let t = table(&e2, "s1/Sales");
    assert_eq!(
        t.column("Amount").unwrap().label.as_deref(),
        Some("Sales (Rp)")
    );
    assert!(t.column("Qty").unwrap().hidden);
    // A label that says nothing is not kept.
    let mut empty = setup.clone();
    empty.columns.get_mut("Amount").unwrap().label = Some("  ".into());
    e.set_table_setup("s1/Sales", Some(empty)).unwrap();
    assert_eq!(table(&e, "s1/Sales").column("Amount").unwrap().label, None);
}

#[test]
fn a_column_profile() {
    let (e, _) = engine(
        "profile",
        &[("title_footer.xlsx", ""), ("months_across.xlsx", "")],
    );
    // Numbers: min, max and a 12-bin histogram that adds up.
    let p = e.profile("s1/Sales", "Amount").unwrap();
    assert_eq!((p.rows, p.distinct), (5, 5));
    assert_eq!(p.blank, 0.0);
    assert_eq!(p.min, Some(json!(30.0)));
    assert_eq!(p.max, Some(json!(200.0)));
    let h = p.histogram.unwrap();
    assert_eq!(h.len(), 12);
    assert_eq!(h.iter().sum::<u64>(), 5);
    // The bins are 14.17 wide from 30: 30 is in the first, 50 in the second, 200 in the last.
    assert_eq!((h[0], h[1], h[11]), (1, 1, 1));
    // Text: the most common values first.
    let p = e.profile("s1/Sales", "Region").unwrap();
    let top = p.top.unwrap();
    assert_eq!((top[0].value.as_str(), top[0].count), ("North", 3));
    assert_eq!((top[1].value.as_str(), top[1].count), ("South", 2));
    // Dates: min and max are written as dates.
    let p = e.profile("s1/Sales", "Date").unwrap();
    assert_eq!(p.min, Some(json!("2026-01-02")));
    assert_eq!(p.max, Some(json!("2026-01-09")));
    // Blanks.
    let p = e.profile("s2/Budget", "Mar-26").unwrap();
    assert!((p.blank - 1.0 / 3.0).abs() < 1e-9);
    assert!(e.profile("s1/Sales", "Nope").is_err());
}

#[test]
fn whole_and_decimal_month_columns_turn_into_rows_together() {
    let csv = "dept,Jan-26,Feb-26,Mar-26\nA,1,2.5,3\nB,4,5,6.25\n";
    let (e, _) = engine("mixednum", &[("m.csv", csv)]);
    assert!(e.detect_unpivot("s1/m").is_some());
    e.set_table_setup("s1/m", Some(months_setup_for("dept")))
        .unwrap();
    let t = table(&e, "s1/m");
    assert_eq!(names(&t), ["dept", "Month", "Budget"]);
    assert_eq!(t.rows, Some(6));
    let rows = ask(&e, sum("s1/m", "Budget"), json!([]));
    assert_eq!(rows[0][0].as_f64(), Some(21.75));
    // Re-typing the new columns works too: the value as text, then back.
    let setup = TableSetup {
        columns: [(
            "Budget".to_string(),
            ColumnSetup {
                dtype: Some(DType::Text),
                ..ColumnSetup::default()
            },
        )]
        .into(),
        ..months_setup_for("dept")
    };
    e.set_table_setup("s1/m", Some(setup)).unwrap();
    assert_eq!(
        table(&e, "s1/m").column("Budget").unwrap().dtype,
        DType::Text
    );
}

fn months_setup_for(keep: &str) -> TableSetup {
    TableSetup {
        transforms: vec![Transform::Unpivot {
            keep: vec![keep.into()],
            name: "Month".into(),
            value: "Budget".into(),
        }],
        ..TableSetup::default()
    }
}

#[test]
fn a_change_that_cannot_apply_leaves_the_table_as_it_is() {
    let (e, _) = engine("bad", &[("title_footer.xlsx", "")]);
    // Every column is kept, so there is nothing to turn into rows: the table stays as it is.
    let setup = TableSetup {
        transforms: vec![Transform::Unpivot {
            keep: ["Date", "Region", "Product", "Amount", "Qty"]
                .map(String::from)
                .into(),
            name: "Month".into(),
            value: "Value".into(),
        }],
        ..TableSetup::default()
    };
    e.set_table_setup("s1/Sales", Some(setup)).unwrap();
    assert_eq!(table(&e, "s1/Sales").columns.len(), 5);
    assert_eq!(table(&e, "s1/Sales").rows, Some(5));
    // The two new names must differ.
    let same = TableSetup {
        transforms: vec![Transform::Unpivot {
            keep: vec![],
            name: "x".into(),
            value: "x".into(),
        }],
        ..TableSetup::default()
    };
    assert!(e.set_table_setup("s1/Sales", Some(same)).is_err());
}
