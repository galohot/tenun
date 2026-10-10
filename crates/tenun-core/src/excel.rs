//! Reads spreadsheets with calamine and infers a type for every column.
//!
//! We do not use DuckDB's excel extension: it needs a download on first use,
//! reads only .xlsx, and guesses types from the first data row alone.

use calamine::{Data, Reader, open_workbook_auto};
use chrono::{NaiveDateTime, Timelike};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColType {
    Int,
    Float,
    Bool,
    Date,
    Timestamp,
    Text,
}

impl ColType {
    pub fn duckdb(self) -> &'static str {
        match self {
            ColType::Int => "BIGINT",
            ColType::Float => "DOUBLE",
            ColType::Bool => "BOOLEAN",
            ColType::Date => "DATE",
            ColType::Timestamp => "TIMESTAMP",
            ColType::Text => "VARCHAR",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    Time(NaiveDateTime),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct Sheet {
    pub name: String,
    pub columns: Vec<(String, ColType)>,
    pub rows: Vec<Vec<Cell>>,
}

pub fn read_workbook(path: &Path) -> anyhow::Result<Vec<Sheet>> {
    let mut wb = open_workbook_auto(path)
        .map_err(|e| anyhow::anyhow!("cannot open {}: {e}", path.display()))?;
    let mut out = Vec::new();
    for name in wb.sheet_names() {
        let range = match wb.worksheet_range(&name) {
            Ok(r) => r,
            Err(_) => continue, // chart sheets and the like
        };
        let grid: Vec<Vec<Cell>> = range
            .rows()
            .map(|row| row.iter().map(to_cell).collect())
            .collect();
        if let Some(sheet) = build_sheet(&name, grid) {
            out.push(sheet);
        }
    }
    Ok(out)
}

fn to_cell(d: &Data) -> Cell {
    match d {
        Data::Int(i) => Cell::Int(*i),
        Data::Float(f) => Cell::Float(*f),
        Data::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                Cell::Null
            } else {
                Cell::Text(t.to_string())
            }
        }
        Data::Bool(b) => Cell::Bool(*b),
        Data::DateTime(dt) => match dt.as_datetime() {
            Some(t) => Cell::Time(t),
            None => Cell::Float(dt.as_f64()),
        },
        Data::DateTimeIso(s) => parse_iso(s)
            .map(Cell::Time)
            .unwrap_or(Cell::Text(s.clone())),
        Data::DurationIso(s) => Cell::Text(s.clone()),
        Data::Error(_) | Data::Empty => Cell::Null,
    }
}

fn parse_iso(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .ok()
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        })
}

/// Turns a raw grid into a typed table. The header is the first row with any
/// value; columns with neither a header nor data are dropped.
pub fn build_sheet(name: &str, grid: Vec<Vec<Cell>>) -> Option<Sheet> {
    let header_idx = grid
        .iter()
        .position(|r| r.iter().any(|c| *c != Cell::Null))?;
    let header = &grid[header_idx];
    let body: Vec<&Vec<Cell>> = grid[header_idx + 1..]
        .iter()
        .filter(|r| r.iter().any(|c| *c != Cell::Null))
        .collect();

    let width = header
        .len()
        .max(body.iter().map(|r| r.len()).max().unwrap_or(0));
    let mut keep = Vec::new();
    for i in 0..width {
        let has_header = header.get(i).is_some_and(|c| *c != Cell::Null);
        let has_data = body
            .iter()
            .any(|r| r.get(i).is_some_and(|c| *c != Cell::Null));
        if has_header || has_data {
            keep.push(i);
        }
    }
    if keep.is_empty() {
        return None;
    }

    let mut names: Vec<String> = Vec::new();
    for &i in &keep {
        let raw = match header.get(i) {
            Some(Cell::Text(s)) => s.clone(),
            Some(Cell::Null) | None => format!("column_{}", i + 1),
            Some(other) => cell_text(other),
        };
        let mut n = raw.clone();
        let mut k = 2;
        while names.contains(&n) {
            n = format!("{raw}_{k}");
            k += 1;
        }
        names.push(n);
    }

    let columns: Vec<(String, ColType)> = keep
        .iter()
        .zip(names)
        .map(|(&i, n)| {
            let t = infer(body.iter().map(|r| r.get(i).unwrap_or(&Cell::Null)));
            (n, t)
        })
        .collect();

    let rows = body
        .iter()
        .map(|r| {
            keep.iter()
                .zip(&columns)
                .map(|(&i, (_, t))| coerce(r.get(i).unwrap_or(&Cell::Null), *t))
                .collect()
        })
        .collect();

    Some(Sheet {
        name: name.to_string(),
        columns,
        rows,
    })
}

fn infer<'a>(cells: impl Iterator<Item = &'a Cell>) -> ColType {
    let (mut ints, mut floats, mut bools, mut times, mut texts, mut midnight) =
        (0, 0, 0, 0, 0, true);
    for c in cells {
        match c {
            Cell::Null => {}
            Cell::Int(_) => ints += 1,
            Cell::Float(f) => {
                if f.fract() == 0.0 && f.abs() < 9.0e15 {
                    ints += 1
                } else {
                    floats += 1
                }
            }
            Cell::Bool(_) => bools += 1,
            Cell::Time(t) => {
                times += 1;
                if t.hour() != 0 || t.minute() != 0 || t.second() != 0 {
                    midnight = false;
                }
            }
            Cell::Text(_) => texts += 1,
        }
    }
    let total = ints + floats + bools + times + texts;
    if total == 0 || texts > 0 {
        return ColType::Text;
    }
    if times == total {
        return if midnight {
            ColType::Date
        } else {
            ColType::Timestamp
        };
    }
    if bools == total {
        return ColType::Bool;
    }
    if ints == total {
        return ColType::Int;
    }
    if ints + floats == total {
        return ColType::Float;
    }
    ColType::Text
}

fn coerce(c: &Cell, t: ColType) -> Cell {
    match (c, t) {
        (Cell::Null, _) => Cell::Null,
        (Cell::Float(f), ColType::Int) => Cell::Int(*f as i64),
        (Cell::Int(i), ColType::Float) => Cell::Float(*i as f64),
        (_, ColType::Text) => Cell::Text(cell_text(c)),
        _ => c.clone(),
    }
}

fn cell_text(c: &Cell) -> String {
    match c {
        Cell::Null => String::new(),
        Cell::Int(i) => i.to_string(),
        Cell::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 9.0e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Cell::Bool(b) => b.to_string(),
        Cell::Time(t) => {
            if t.hour() == 0 && t.minute() == 0 && t.second() == 0 {
                t.format("%Y-%m-%d").to_string()
            } else {
                t.format("%Y-%m-%d %H:%M:%S").to_string()
            }
        }
        Cell::Text(s) => s.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn t(s: &str) -> Cell {
        Cell::Text(s.into())
    }
    fn d(y: i32, m: u32, day: u32) -> Cell {
        Cell::Time(
            NaiveDate::from_ymd_opt(y, m, day)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        )
    }

    #[test]
    fn header_after_blank_rows_and_types() {
        let grid = vec![
            vec![],
            vec![Cell::Null, Cell::Null],
            vec![t("Date"), t("Store"), t("Amount"), t("Qty"), t("Paid")],
            vec![
                d(2026, 1, 2),
                t("A"),
                Cell::Float(10.5),
                Cell::Float(2.0),
                Cell::Bool(true),
            ],
            vec![Cell::Null, Cell::Null, Cell::Null],
            vec![
                d(2026, 1, 3),
                t("B"),
                Cell::Int(7),
                Cell::Int(1),
                Cell::Bool(false),
            ],
        ];
        let s = build_sheet("Orders", grid).unwrap();
        let types: Vec<ColType> = s.columns.iter().map(|c| c.1).collect();
        assert_eq!(
            types,
            vec![
                ColType::Date,
                ColType::Text,
                ColType::Float,
                ColType::Int,
                ColType::Bool
            ]
        );
        assert_eq!(s.rows.len(), 2);
        assert_eq!(s.rows[0][3], Cell::Int(2));
        assert_eq!(s.rows[1][2], Cell::Float(7.0));
    }

    #[test]
    fn mixed_column_becomes_text_and_duplicate_headers_are_renamed() {
        let grid = vec![
            vec![t("id"), t("id"), Cell::Null],
            vec![Cell::Int(1), t("x"), Cell::Int(5)],
            vec![t("two"), t("y"), Cell::Null],
        ];
        let s = build_sheet("S", grid).unwrap();
        assert_eq!(s.columns[0], ("id".into(), ColType::Text));
        assert_eq!(s.columns[1].0, "id_2");
        assert_eq!(s.columns[2].0, "column_3");
        assert_eq!(s.rows[0][0], t("1"));
    }

    #[test]
    fn timestamps_with_time_of_day() {
        let ts = Cell::Time(
            NaiveDate::from_ymd_opt(2026, 3, 1)
                .unwrap()
                .and_hms_opt(9, 30, 0)
                .unwrap(),
        );
        let s = build_sheet("S", vec![vec![t("at")], vec![ts]]).unwrap();
        assert_eq!(s.columns[0].1, ColType::Timestamp);
    }

    #[test]
    fn empty_sheet_is_skipped() {
        assert!(build_sheet("S", vec![vec![Cell::Null]]).is_none());
    }
}
