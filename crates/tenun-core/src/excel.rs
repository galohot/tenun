//! Reads spreadsheets with calamine and infers a type for every column.
//!
//! We do not use DuckDB's excel extension: it needs a download on first use,
//! reads only .xlsx, and guesses types from the first data row alone.

use calamine::{Data, Ods, Reader, Sheets, Xls, Xlsx, open_workbook, open_workbook_auto};
use chrono::{NaiveDateTime, Timelike};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read};
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
    /// What was detected and fixed: "Header on row 4", "Filled down Region".
    pub notes: Vec<String>,
    /// The row of the sheet (numbered as in Excel) that holds the column names.
    pub header_row: Option<u32>,
    /// Per column: the file had text and numbers (or dates) mixed in it.
    pub mixed: Vec<bool>,
}

/// Reads every sheet. `header_rows` holds the header row the user chose, by sheet name.
pub fn read_workbook(
    path: &Path,
    header_rows: &BTreeMap<String, u32>,
) -> anyhow::Result<Vec<Sheet>> {
    let mut wb = open_book(path)?;
    let mut out = Vec::new();
    for name in wb.sheet_names() {
        let range = match wb.worksheet_range(&name) {
            Ok(r) => r,
            Err(_) => continue, // chart sheets and the like
        };
        // The range starts at the first filled cell. Blank rows keep the numbering of the sheet.
        let top = range.start().map_or(0, |(r, _)| r as usize);
        let mut grid: Vec<Vec<Cell>> = vec![Vec::new(); top];
        grid.extend(
            range
                .rows()
                .map(|row| row.iter().map(to_cell).collect::<Vec<_>>()),
        );
        if let Some(sheet) = build_sheet_with(&name, grid, header_rows.get(&name).copied(), true) {
            out.push(sheet);
        }
    }
    Ok(out)
}

/// Opens a workbook by what the file holds, not only by its name: a `.xls` that is really a
/// `.xlsx` (a common export) opens, and a locked workbook or a web page with a spreadsheet name is
/// refused in words.
fn open_book(path: &Path) -> anyhow::Result<Sheets<BufReader<File>>> {
    let mut head = [0u8; 512];
    let n = File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .unwrap_or(0);
    let head = &head[..n];
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let zip = head.starts_with(b"PK\x03\x04");
    let ole = head.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]);
    let opened = if zip && matches!(ext.as_str(), "xls" | "xla") {
        open_workbook::<Xlsx<_>, _>(path)
            .map(Sheets::Xlsx)
            .map_err(calamine::Error::Xlsx)
            .or_else(|_| {
                open_workbook::<Ods<_>, _>(path)
                    .map(Sheets::Ods)
                    .map_err(calamine::Error::Ods)
            })
    } else if ole && ext != "xls" {
        open_workbook::<Xls<_>, _>(path)
            .map(Sheets::Xls)
            .map_err(calamine::Error::Xls)
    } else {
        open_workbook_auto(path)
    };
    opened.map_err(|e| {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if e.to_string().to_lowercase().contains("password") {
            anyhow::anyhow!(
                "{name} is locked with a password. Open it in Excel, save a copy without the password, and add the copy."
            )
        } else if let Some(kind) = text_kind(head) {
            anyhow::anyhow!(
                "{name} is {kind} saved with a spreadsheet name. Open it in Excel, save it as .xlsx, and add that file."
            )
        } else {
            anyhow::anyhow!("cannot open {}: {e}", path.display())
        }
    })
}

/// What a text file under a spreadsheet name is: some systems export a web page table, or Excel
/// 2003's XML, as `.xls`.
fn text_kind(head: &[u8]) -> Option<&'static str> {
    let text = String::from_utf8_lossy(head).to_lowercase();
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with("<!doctype") || t.starts_with("<html") || t.starts_with("<table") {
        Some("a web page")
    } else if t.starts_with("<?xml") {
        Some("an Excel 2003 XML file")
    } else {
        None
    }
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

/// Turns a raw grid into a typed table, cleaning the usual spreadsheet mess on the way (see
/// [`build_sheet_with`]).
pub fn build_sheet(name: &str, grid: Vec<Vec<Cell>>) -> Option<Sheet> {
    build_sheet_with(name, grid, None, true)
}

/// A row with a value in at least one cell.
fn has_value(r: &[Cell]) -> bool {
    r.iter().any(|c| *c != Cell::Null)
}

/// A whole number that reads as a year: a header like 2025 over a column of months.
fn is_year(c: &Cell) -> bool {
    match c {
        Cell::Int(i) => (1900..=2200).contains(i),
        Cell::Float(f) => f.fract() == 0.0 && (1900.0..=2200.0).contains(f),
        Cell::Text(t) => {
            t.len() == 4 && t.starts_with(['1', '2']) && t.bytes().all(|b| b.is_ascii_digit())
        }
        _ => false,
    }
}

/// The words that start a footer row in the first column.
fn is_total_word(t: &str) -> bool {
    let t = t
        .trim()
        .trim_end_matches([':', '.'])
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    matches!(
        t.as_str(),
        "total"
            | "jumlah"
            | "subtotal"
            | "sub total"
            | "sub-total"
            | "total keseluruhan"
            | "jumlah keseluruhan"
            | "total semua"
    ) || t.starts_with("grand total")
        || t.starts_with("jumlah total")
}

/// Picks the header row. It is the first row that has labels in at least half of the used
/// columns, unless it is a title block: a row with at most half the labels of one of the next
/// few filled rows. Without a candidate, the first row with any value (the v1 rule). The rows
/// below are never asked for typed data: a blank or a year-like amount in the first data row
/// must not move the header.
fn detect_header(grid: &[Vec<Cell>], first: usize) -> usize {
    header_candidate(grid, first)
        .or_else(|| most_named(grid, first))
        .unwrap_or(first)
}

/// No row names half of the columns (a header split over several rows, under notes): the row with
/// the most names, at least two and no plain number. The first such row wins a tie.
fn most_named(grid: &[Vec<Cell>], first: usize) -> Option<usize> {
    (first..grid.len())
        .filter(|&i| !has_number(&grid[i]))
        .map(|i| (i, label_count(&grid[i])))
        .filter(|&(_, n)| n >= 2)
        .max_by_key(|&(i, n)| (n, std::cmp::Reverse(i)))
        .map(|(i, _)| i)
}

/// The cells of a row that read as names: text, dates, and years.
fn label_count(r: &[Cell]) -> usize {
    r.iter()
        .filter(|c| matches!(c, Cell::Text(_) | Cell::Time(_)) || is_year(c))
        .count()
}

/// A number in a row that is not a year: a header holds names, and years at most, so such a row is
/// data even when most of its cells are text ("Enfants sans vie", 7, "0.41 %").
fn has_number(r: &[Cell]) -> bool {
    r.iter()
        .any(|c| matches!(c, Cell::Int(_) | Cell::Float(_)) && !is_year(c))
}

/// The header row of [`detect_header`], or `None` when no row has labels in at least half of the
/// used columns. A CSV file asks this of its first lines (see `csvread`).
pub(crate) fn header_candidate(grid: &[Vec<Cell>], first: usize) -> Option<usize> {
    let mut used = std::collections::BTreeSet::new();
    for r in &grid[first..] {
        for (i, c) in r.iter().enumerate() {
            if *c != Cell::Null {
                used.insert(i);
            }
        }
    }
    let need = used.len().div_ceil(2).max(1);
    let labels = |r: &[Cell]| label_count(r);
    let candidates: Vec<usize> = (first..grid.len())
        .filter(|&i| has_value(&grid[i]) && labels(&grid[i]) >= need && !has_number(&grid[i]))
        .collect();
    let is_title = |i: usize| {
        let mine = labels(&grid[i]);
        (i + 1..grid.len())
            .filter(|&j| has_value(&grid[j]))
            .take(4)
            .any(|j| labels(&grid[j]) >= mine * 2)
    };
    candidates
        .iter()
        .copied()
        .find(|&i| !is_title(i))
        .or_else(|| candidates.first().copied())
}

/// The row right above the header when it is a group heading ("2025" over "Jan", "Feb").
fn stacked_parent(grid: &[Vec<Cell>], h: usize) -> Option<usize> {
    let p = h.checked_sub(1)?;
    let (row, head) = (&grid[p], &grid[h]);
    let filled: Vec<usize> = (0..row.len()).filter(|&i| row[i] != Cell::Null).collect();
    if filled.is_empty() {
        return None;
    }
    let only_labels = filled
        .iter()
        .all(|&i| matches!(row[i], Cell::Text(_)) || is_year(&row[i]));
    // A group heading spans several columns, so it fills fewer cells than the header under it,
    // or it is made of years and quarters. Two full rows of text are a header and its data.
    let header_cells = head.iter().filter(|c| **c != Cell::Null).count();
    let grouped = (filled.len() >= 2 && filled.len() < header_cells)
        || filled
            .iter()
            .all(|&i| is_year(&row[i]) || is_quarter(&row[i]));
    let over_header = filled
        .iter()
        .any(|&i| head.get(i).is_some_and(|c| *c != Cell::Null));
    (only_labels && grouped && over_header).then_some(p)
}

fn is_quarter(c: &Cell) -> bool {
    matches!(c, Cell::Text(t) if {
        let t = t.trim().to_ascii_uppercase();
        t.len() == 2 && t.starts_with('Q') && matches!(t.as_bytes()[1], b'1'..=b'4')
    })
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Turns a raw grid into a typed table.
///
/// With `tidy` the sheet is cleaned: the header is found below title rows (or is the sheet row
/// `header_row`), two stacked header rows are joined, blank header cells get a name, footer rows
/// ("Total", a row of sums) are dropped, and a grouping column is filled down. What was done is
/// written to `notes`. Without `tidy` the header is the first row with any value (databases).
/// Columns with neither a header nor data are dropped.
pub fn build_sheet_with(
    name: &str,
    grid: Vec<Vec<Cell>>,
    header_row: Option<u32>,
    tidy: bool,
) -> Option<Sheet> {
    let first = grid.iter().position(|r| has_value(r))?;
    let mut notes = Vec::new();
    let forced = header_row
        .map(|n| n as usize)
        .filter(|&n| tidy && n >= 1 && n <= grid.len() && has_value(&grid[n - 1]))
        .map(|n| n - 1);
    let h = match forced {
        Some(h) => h,
        None if tidy => detect_header(&grid, first),
        None => first,
    };
    let parent = if tidy { stacked_parent(&grid, h) } else { None };
    let header = &grid[h];

    if tidy {
        let top = parent.unwrap_or(h);
        let skipped = grid[..top].iter().filter(|r| has_value(r)).count();
        if skipped > 0 {
            notes.push(format!(
                "Skipped {}",
                plural(skipped, "title row", "title rows")
            ));
        }
        if h > 0 {
            let by_you = if forced.is_some() {
                " (set by you)"
            } else {
                ""
            };
            notes.push(format!("Header on row {}{by_you}", h + 1));
        }
        if parent.is_some() {
            notes.push("Joined 2 header rows".to_string());
        }
    }

    // The rows below the header; each knows whether a blank row came just before it.
    let mut body: Vec<(&Vec<Cell>, bool)> = Vec::new();
    let mut after_blank = false;
    for r in &grid[h + 1..] {
        if has_value(r) {
            body.push((r, after_blank));
            after_blank = false;
        } else {
            after_blank = true;
        }
    }

    let width = header
        .len()
        .max(body.iter().map(|(r, _)| r.len()).max().unwrap_or(0));

    if tidy {
        let first_col = (0..width)
            .find(|&i| {
                header.get(i).is_some_and(|c| *c != Cell::Null)
                    || parent.is_some_and(|p| grid[p].get(i).is_some_and(|c| *c != Cell::Null))
                    || body
                        .iter()
                        .any(|(r, _)| r.get(i).is_some_and(|c| *c != Cell::Null))
            })
            .unwrap_or(0);
        let before = body.len();
        drop_footers(&mut body, first_col);
        if body.len() < before {
            notes.push(format!(
                "Dropped {}",
                plural(before - body.len(), "footer row", "footer rows")
            ));
        }
    }

    let mut keep = Vec::new();
    for i in 0..width {
        let has_header = header.get(i).is_some_and(|c| *c != Cell::Null)
            || parent.is_some_and(|p| grid[p].get(i).is_some_and(|c| *c != Cell::Null));
        let has_data = body
            .iter()
            .any(|(r, _)| r.get(i).is_some_and(|c| *c != Cell::Null));
        if has_header || has_data {
            keep.push(i);
        }
    }
    if keep.is_empty() {
        return None;
    }

    let (raw_names, named) = header_names(&grid, h, parent, &keep, tidy);
    if named > 0 {
        notes.push(format!(
            "Named {} from the column on the left",
            plural(named, "blank header", "blank headers")
        ));
    }
    let mut names: Vec<String> = Vec::new();
    for raw in raw_names {
        let mut n = raw.clone();
        let mut k = 2;
        while names.contains(&n) {
            n = format!("{raw}_{k}");
            k += 1;
        }
        names.push(n);
    }

    // Cells of the kept columns; a grouping column is filled down.
    let mut cells: Vec<Vec<Cell>> = body
        .iter()
        .map(|(r, _)| {
            keep.iter()
                .map(|&i| r.get(i).cloned().unwrap_or(Cell::Null))
                .collect()
        })
        .collect();
    if tidy && fill_down(&mut cells, &names[0]) {
        notes.push(format!("Filled down {}", names[0]));
    }

    let mixed: Vec<bool> = (0..keep.len())
        .map(|j| is_mixed(cells.iter().map(|r| &r[j])))
        .collect();
    let columns: Vec<(String, ColType)> = (0..keep.len())
        .map(|j| (names[j].clone(), infer(cells.iter().map(|r| &r[j]))))
        .collect();
    let rows = cells
        .iter()
        .map(|r| {
            r.iter()
                .zip(&columns)
                .map(|(c, (_, t))| coerce(c, *t))
                .collect()
        })
        .collect();

    Some(Sheet {
        name: name.to_string(),
        columns,
        rows,
        notes,
        header_row: tidy.then_some(h as u32 + 1),
        mixed,
    })
}

/// Removes rows that are not data: "Total" rows anywhere, the notes under the last one, and a
/// last row of column sums.
fn drop_footers(body: &mut Vec<(&Vec<Cell>, bool)>, first_col: usize) {
    let is_total = |r: &[Cell]| matches!(r.get(first_col), Some(Cell::Text(t)) if is_total_word(t));
    let filled = |r: &[Cell]| r.iter().filter(|c| **c != Cell::Null).count();
    let width = body.iter().map(|(r, _)| r.len()).max().unwrap_or(0);
    if let Some(last) = body.iter().rposition(|(r, _)| is_total(r)) {
        let mut at = 0;
        body.retain(|(r, _)| {
            let i = at;
            at += 1;
            !(is_total(r) || (i > last && filled(r) * 2 <= width))
        });
    }
    // A last row that only holds the sums of the rows above it.
    if body.len() >= 3 {
        let (last, after_blank) = body[body.len() - 1];
        let nums: Vec<(usize, f64)> = last
            .iter()
            .enumerate()
            .filter_map(|(i, c)| match c {
                Cell::Int(v) => Some((i, *v as f64)),
                Cell::Float(v) => Some((i, *v)),
                _ => None,
            })
            .collect();
        let only_numbers = nums.len() == filled(last);
        let sums_match = nums.iter().all(|&(i, v)| {
            let sum: f64 = body[..body.len() - 1]
                .iter()
                .map(|(r, _)| match r.get(i) {
                    Some(Cell::Int(x)) => *x as f64,
                    Some(Cell::Float(x)) => *x,
                    _ => 0.0,
                })
                .sum();
            (v - sum).abs() <= 1e-6 * sum.abs().max(1.0)
        });
        let some_sum = nums.iter().any(|&(_, v)| v != 0.0);
        if only_numbers && sums_match && some_sum && (after_blank || nums.len() >= 2) {
            body.pop();
        }
    }
}

/// The column names. A stacked parent joins its child ("2025 Jan"); a blank header between
/// filled ones takes its left neighbour's name and a number ("Q1 2"). Returns the count of
/// blanks that got a name.
fn header_names(
    grid: &[Vec<Cell>],
    h: usize,
    parent: Option<usize>,
    keep: &[usize],
    tidy: bool,
) -> (Vec<String>, usize) {
    let text = |c: Option<&Cell>| match c {
        Some(Cell::Null) | None => None,
        Some(Cell::Text(s)) => Some(s.clone()),
        Some(other) => Some(cell_text(other)),
    };
    let mut heads: Vec<Option<String>> = Vec::new();
    let mut carry: Option<String> = None;
    for &i in keep {
        let child = text(grid[h].get(i));
        let name = match parent {
            Some(p) => {
                let own = text(grid[p].get(i));
                if own.is_some() {
                    carry = own.clone();
                }
                if child.is_none() && own.is_none() {
                    carry = None;
                }
                let above = own.or_else(|| carry.clone().filter(|_| child.is_some()));
                match (above, child) {
                    (Some(a), Some(c)) if a != c => Some(format!("{a} {c}")),
                    (Some(a), None) => Some(a),
                    (_, c) => c,
                }
            }
            None => child,
        };
        heads.push(name);
    }
    let last_filled = heads.iter().rposition(|n| n.is_some());
    let mut out = Vec::new();
    let mut named = 0;
    let mut base: Option<String> = None;
    let mut k = 2;
    for (j, &i) in keep.iter().enumerate() {
        match &heads[j] {
            Some(n) => {
                base = Some(n.clone());
                k = 2;
                out.push(n.clone());
            }
            None => match &base {
                Some(b) if tidy && last_filled.is_some_and(|l| l > j) => {
                    out.push(format!("{b} {k}"));
                    k += 1;
                    named += 1;
                }
                _ => out.push(format!("column_{}", i + 1)),
            },
        }
    }
    (out, named)
}

/// Fills blank cells of the first column from the cell above, when the column is a grouping
/// column: text, at least half blank, with a value in the first row. The values must also look
/// like groups. Either one comes back in a later block, or every block spans 2 rows or more and
/// the header is not the name of a notes column. A column of unique notes is left alone.
fn fill_down(cells: &mut [Vec<Cell>], name: &str) -> bool {
    let col: Vec<&Cell> = cells.iter().map(|r| &r[0]).collect();
    let blanks = col.iter().filter(|c| ***c == Cell::Null).count();
    let all_text = col.iter().all(|c| matches!(c, Cell::Null | Cell::Text(_)));
    if cells.len() < 2 || !all_text || blanks * 2 < cells.len() || *col[0] == Cell::Null {
        return false;
    }
    let filled: Vec<String> = col
        .iter()
        .filter(|c| ***c != Cell::Null)
        .map(|c| cell_text(c))
        .collect();
    let distinct: std::collections::BTreeSet<&String> = filled.iter().collect();
    let repeats = distinct.len() < filled.len();
    let blocks_of_two = col
        .windows(2)
        .all(|w| *w[0] == Cell::Null || *w[1] == Cell::Null)
        && *col[col.len() - 1] == Cell::Null;
    if !repeats && !(blocks_of_two && !is_notes_name(name)) {
        return false;
    }
    let mut cur = cells[0][0].clone();
    for r in cells.iter_mut() {
        if r[0] == Cell::Null {
            r[0] = cur.clone();
        } else {
            cur = r[0].clone();
        }
    }
    true
}

/// Headers of columns that hold free notes, not group names.
fn is_notes_name(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    [
        "remark",
        "note",
        "comment",
        "memo",
        "catatan",
        "keterangan",
        "descri",
    ]
    .iter()
    .any(|w| n.contains(w))
}

/// Text and numbers (or dates, or yes/no) in one column.
fn is_mixed<'a>(cells: impl Iterator<Item = &'a Cell>) -> bool {
    let (mut texts, mut others) = (false, false);
    for c in cells {
        match c {
            Cell::Null => {}
            Cell::Text(_) => texts = true,
            _ => others = true,
        }
    }
    texts && others
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
    fn a_row_with_a_plain_number_is_never_the_header() {
        // A sheet without a header: a title, then label, count and share. The last rows have two
        // text cells, but their number makes them data.
        let grid = vec![
            vec![Cell::Null, t("Statistiques 2011")],
            vec![
                Cell::Null,
                t("Naissances"),
                Cell::Int(1017),
                Cell::Float(0.5979),
            ],
            vec![
                Cell::Null,
                t("Mariages"),
                Cell::Int(60),
                Cell::Float(0.0353),
            ],
            vec![Cell::Null, t("Enfants sans vie"), Cell::Int(7), t("0.41 %")],
            vec![
                Cell::Null,
                t("Changement de nom"),
                Cell::Int(6),
                t("0.35 %"),
            ],
        ];
        let s = build_sheet("s", grid).unwrap();
        assert_eq!(s.header_row, Some(1));
        assert_eq!(s.rows.len(), 4);
    }

    #[test]
    fn a_header_split_over_rows_under_notes_is_the_row_with_the_most_names() {
        let grid = vec![
            vec![t("Earthquake: population exposure")],
            vec![t("Analysis by a team, preliminary")],
            vec![
                Cell::Null,
                Cell::Null,
                Cell::Null,
                Cell::Null,
                t("POPULATION"),
                Cell::Null,
                t("TOTAL"),
            ],
            vec![
                Cell::Null,
                Cell::Null,
                Cell::Null,
                t("INTENSITY"),
                Cell::Int(3),
                Cell::Int(4),
            ],
            vec![Cell::Null, t("ADM0_NAME"), t("ADM1_NAME"), t("ADM2_NAME")],
            vec![
                Cell::Null,
                t("Indonesia"),
                t("Maluku"),
                t("Maluku Tengah"),
                Cell::Int(0),
                Cell::Int(0),
                Cell::Int(0),
            ],
            vec![
                Cell::Null,
                Cell::Null,
                Cell::Null,
                t("Tual"),
                Cell::Int(5),
                Cell::Int(1),
                Cell::Int(6),
            ],
        ];
        let s = build_sheet("s", grid).unwrap();
        assert_eq!(s.header_row, Some(5));
        assert_eq!(s.columns[0].0, "ADM0_NAME");
        assert_eq!(s.rows.len(), 2);
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

    fn n() -> Cell {
        Cell::Null
    }
    fn i(v: i64) -> Cell {
        Cell::Int(v)
    }
    fn names(s: &Sheet) -> Vec<&str> {
        s.columns.iter().map(|c| c.0.as_str()).collect()
    }

    #[test]
    fn a_title_block_two_header_rows_and_a_footer() {
        let grid = vec![
            vec![t("Laporan"), n(), n(), n()],
            vec![n(), n(), n(), n()],
            vec![t("Toko Senja"), t("Periode 2026"), n(), n()],
            vec![n(), n(), n(), n()],
            vec![n(), i(2025), n(), i(2026)],
            vec![t("Region"), t("Jan"), t("Feb"), t("Jan")],
            vec![t("North"), i(1), i(2), i(3)],
            vec![t("South"), i(4), i(5), i(6)],
            vec![n(), n(), n(), n()],
            vec![t("Jumlah"), i(5), i(7), i(9)],
        ];
        let s = build_sheet("S", grid).unwrap();
        assert_eq!(names(&s), ["Region", "2025 Jan", "2025 Feb", "2026 Jan"]);
        assert_eq!(s.rows.len(), 2);
        assert_eq!(s.header_row, Some(6));
        assert_eq!(
            s.notes,
            [
                "Skipped 2 title rows",
                "Header on row 6",
                "Joined 2 header rows",
                "Dropped 1 footer row"
            ]
        );
    }

    #[test]
    fn a_blank_header_between_filled_ones_takes_the_left_name() {
        let grid = vec![
            vec![t("Region"), t("Q1"), n(), n(), t("Q2"), n()],
            vec![t("A"), i(1), i(2), i(3), i(4), i(5)],
            vec![t("B"), i(6), i(7), i(8), i(9), i(10)],
        ];
        let s = build_sheet("S", grid).unwrap();
        // The last blank has no filled header after it, so it keeps the v1 name.
        assert_eq!(
            names(&s),
            ["Region", "Q1", "Q1 2", "Q1 3", "Q2", "column_6"]
        );
        assert_eq!(
            s.notes,
            ["Named 2 blank headers from the column on the left"]
        );
    }

    #[test]
    fn a_row_of_sums_goes_but_a_look_alike_row_stays() {
        let grid = vec![
            vec![t("Item"), t("A"), t("B")],
            vec![t("x"), i(1), i(2)],
            vec![t("y"), i(3), i(4)],
            vec![n(), n(), n()],
            vec![n(), i(4), i(6)],
        ];
        let s = build_sheet("S", grid).unwrap();
        assert_eq!(s.rows.len(), 2);
        assert_eq!(s.notes, ["Dropped 1 footer row"]);
        // Not the sums: it is data.
        let grid = vec![
            vec![t("Item"), t("A"), t("B")],
            vec![t("x"), i(1), i(2)],
            vec![t("y"), i(3), i(4)],
            vec![t("z"), i(4), i(7)],
        ];
        assert_eq!(build_sheet("S", grid).unwrap().rows.len(), 3);
    }

    #[test]
    fn total_rows_are_footers_only_by_their_words() {
        let grid = vec![
            vec![t("Item"), t("Amount")],
            vec![t("Total Care"), i(5)],
            vec![t("Subtotal"), i(5)],
            vec![t("Bolt"), i(7)],
            vec![t("Grand Total"), i(12)],
            vec![t("Source: x"), n()],
        ];
        let s = build_sheet("S", grid).unwrap();
        // Subtotal and Grand Total go, and so does the note under the last one.
        let items: Vec<&Cell> = s.rows.iter().map(|r| &r[0]).collect();
        assert_eq!(items, [&t("Total Care"), &t("Bolt")]);
    }

    #[test]
    fn a_grouping_column_is_filled_down_but_a_sparse_number_column_is_not() {
        let grid = vec![
            vec![t("Region"), t("Item"), t("Qty")],
            vec![t("North"), t("a"), i(1)],
            vec![n(), t("b"), i(2)],
            vec![n(), t("c"), i(3)],
            vec![t("South"), t("d"), i(4)],
            vec![n(), t("e"), i(5)],
        ];
        let s = build_sheet("S", grid.clone()).unwrap();
        assert_eq!(s.rows[2][0], t("North"));
        assert_eq!(s.rows[3][0], t("South"));
        assert_eq!(s.rows[4][0], t("South"));
        assert_eq!(s.notes, ["Filled down Region"]);
        // A value that comes back in a later block is a group too, even with a one-row block.
        let mut again = grid;
        again[5] = vec![t("North"), t("e"), i(5)];
        again.push(vec![n(), t("f"), i(6)]);
        again.push(vec![n(), t("g"), i(7)]);
        let s = build_sheet("S", again).unwrap();
        assert_eq!(s.rows[3][0], t("South"));
        assert_eq!(s.rows[5][0], t("North"));
        assert_eq!(s.rows[6][0], t("North"));
        let grid = vec![
            vec![t("Discount"), t("Item")],
            vec![i(5), t("a")],
            vec![n(), t("b")],
            vec![n(), t("c")],
        ];
        let s = build_sheet("S", grid).unwrap();
        assert_eq!(s.rows[1][0], Cell::Null);
        assert!(s.notes.is_empty());
    }

    #[test]
    fn groups_of_exactly_two_rows_are_filled_down() {
        let grid = vec![
            vec![t("Region"), t("Item"), t("Qty")],
            vec![t("North"), t("a"), i(1)],
            vec![n(), t("b"), i(2)],
            vec![t("South"), t("c"), i(3)],
            vec![n(), t("d"), i(4)],
        ];
        let s = build_sheet("S", grid).unwrap();
        assert_eq!(s.rows[1][0], t("North"));
        assert_eq!(s.rows[3][0], t("South"));
        assert_eq!(s.notes, ["Filled down Region"]);
        // A notes column in the same shape stays as it is.
        let notes = vec![
            vec![t("Remark"), t("Item")],
            vec![t("VIP"), t("a")],
            vec![n(), t("b")],
            vec![t("Late"), t("c")],
            vec![n(), t("d")],
        ];
        let s = build_sheet("S", notes).unwrap();
        assert_eq!(s.rows[1][0], Cell::Null);
    }

    #[test]
    fn unique_notes_in_a_mostly_blank_column_are_not_filled_down() {
        let grid = vec![
            vec![t("Remark"), t("Item")],
            vec![t("VIP"), t("a")],
            vec![n(), t("b")],
            vec![t("Late"), t("c")],
            vec![n(), t("d")],
            vec![n(), t("e")],
            vec![n(), t("f")],
        ];
        let s = build_sheet("Notes", grid).unwrap();
        assert_eq!(s.rows[1][0], Cell::Null);
        assert_eq!(s.rows[5][0], Cell::Null);
        assert!(s.notes.is_empty());
    }

    #[test]
    fn a_blank_or_year_like_first_data_row_does_not_move_the_header() {
        // Score is blank in the first data row.
        let grid = vec![
            vec![t("Name"), t("Email"), t("Score")],
            vec![t("Bob"), t("b@x.com"), n()],
            vec![t("Ann"), t("a@x.com"), i(7)],
            vec![t("Cy"), t("c@x.com"), i(9)],
        ];
        let s = build_sheet("S", grid).unwrap();
        assert_eq!(names(&s), ["Name", "Email", "Score"]);
        assert_eq!(s.rows.len(), 3);
        assert_eq!(s.header_row, Some(1));
        assert!(s.notes.is_empty());
        // Amounts that read as years.
        let grid = vec![
            vec![t("Item"), t("Amount")],
            vec![t("Rent"), i(2000)],
            vec![t("Food"), i(1500)],
            vec![t("Fuel"), i(1800)],
        ];
        let s = build_sheet("S", grid).unwrap();
        assert_eq!(names(&s), ["Item", "Amount"]);
        assert_eq!(s.rows.len(), 3);
        assert!(s.notes.is_empty());
    }

    #[test]
    fn years_can_head_number_columns_and_mixed_cells_are_flagged() {
        let grid = vec![
            vec![t("Region"), i(2025), i(2026), i(2027)],
            vec![t("North"), i(1), i(2), i(3)],
            vec![t("South"), i(4), t("n/a"), i(6)],
        ];
        let s = build_sheet("S", grid).unwrap();
        assert_eq!(names(&s), ["Region", "2025", "2026", "2027"]);
        assert!(s.notes.is_empty());
        assert_eq!(s.mixed, [false, false, true, false]);
    }

    #[test]
    fn the_header_row_can_be_chosen_and_databases_keep_the_first_row() {
        let grid = vec![
            vec![t("Date"), t("Store")],
            vec![t("2026-01-02"), t("A")],
            vec![t("2026-01-03"), t("B")],
        ];
        let s = build_sheet_with("S", grid.clone(), Some(2), true).unwrap();
        assert_eq!(names(&s), ["2026-01-02", "A"]);
        assert_eq!(s.rows.len(), 1);
        // A row number outside the sheet is ignored.
        let s = build_sheet_with("S", grid.clone(), Some(40), true).unwrap();
        assert_eq!(names(&s), ["Date", "Store"]);
        // No cleaning for a database: a "Total" row is data there.
        let mut db = grid;
        db.push(vec![t("Total"), t("x")]);
        let s = build_sheet_with("S", db, None, false).unwrap();
        assert_eq!(s.rows.len(), 3);
        assert!(s.notes.is_empty());
    }

    #[test]
    fn empty_sheet_is_skipped() {
        assert!(build_sheet("S", vec![vec![Cell::Null]]).is_none());
    }
}
