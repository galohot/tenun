//! A table pasted from the clipboard: TSV from Excel or Sheets, or CSV. It is read here, shown as
//! a small preview, and written out as a CSV file that becomes a normal source.

use serde::Serialize;

/// The most text a paste may hold: 20 MB.
pub const MAX_CHARS: usize = 20 * 1024 * 1024;

/// A pasted table, before the header choice is applied. Every row has the same width.
#[derive(Debug, Clone, PartialEq)]
pub struct Pasted {
    pub delimiter: Option<char>,
    pub rows: Vec<Vec<String>>,
}

/// What the paste sheet shows.
#[derive(Debug, Clone, Serialize)]
pub struct PastePreview {
    pub columns: Vec<String>,
    /// The first rows of data.
    pub rows: Vec<Vec<String>>,
    /// Rows of data, not counting the header.
    pub total: usize,
}

/// Reads tabular text. `None` when the text is one line of one value, which is not a table.
pub fn parse(text: &str) -> Option<Pasted> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.len() > MAX_CHARS {
        return None;
    }
    let delimiter = detect(text);
    let mut rows = read_rows(text, delimiter);
    while rows.last().is_some_and(|r| r.iter().all(|c| c.is_empty())) {
        rows.pop();
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 || (rows.len() < 2 && width < 2) {
        return None;
    }
    for r in &mut rows {
        r.resize(width, String::new());
    }
    Some(Pasted { delimiter, rows })
}

/// Tab if the first line has one; otherwise the comma or semicolon that occurs most outside quotes.
fn detect(text: &str) -> Option<char> {
    let line = text.lines().find(|l| !l.trim().is_empty())?;
    if line.contains('\t') {
        return Some('\t');
    }
    let (mut comma, mut semi, mut quoted) = (0, 0, false);
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => comma += 1,
            ';' if !quoted => semi += 1,
            _ => {}
        }
    }
    match (comma, semi) {
        (0, 0) => None,
        (c, s) if c >= s => Some(','),
        _ => Some(';'),
    }
}

fn read_rows(text: &str, delimiter: Option<char>) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut started = false; // the current field has had a character
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if !started => {
                quoted = true;
                started = true;
            }
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                started = false;
            }
            c if Some(c) == delimiter => {
                row.push(std::mem::take(&mut field));
                started = false;
            }
            c => {
                field.push(c);
                started = true;
            }
        }
    }
    if started || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Column names and data rows. With `header` false the names are "Column 1", "Column 2", …
/// A blank name becomes "Column N" and a repeated name gets a number ("Region 2").
pub fn split(p: &Pasted, header: bool) -> (Vec<String>, Vec<Vec<String>>) {
    let width = p.rows.first().map_or(0, Vec::len);
    let (raw, body): (Vec<String>, &[Vec<String>]) = if header && !p.rows.is_empty() {
        (p.rows[0].clone(), &p.rows[1..])
    } else {
        (Vec::new(), &p.rows[..])
    };
    let mut names: Vec<String> = Vec::new();
    for i in 0..width {
        let base = raw
            .get(i)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("Column {}", i + 1));
        let mut name = base.clone();
        let mut n = 2;
        while names.iter().any(|x| x.eq_ignore_ascii_case(&name)) {
            name = format!("{base} {n}");
            n += 1;
        }
        names.push(name);
    }
    (names, body.to_vec())
}

pub fn preview(p: &Pasted, header: bool, limit: usize) -> PastePreview {
    let (columns, body) = split(p, header);
    PastePreview {
        columns,
        total: body.len(),
        rows: body.into_iter().take(limit).collect(),
    }
}

/// The table as CSV text: a header line, then the rows, with quotes where a value needs them.
pub fn to_csv(names: &[String], rows: &[Vec<String>]) -> String {
    fn put(out: &mut String, v: &str) {
        if v.contains([',', '"', '\n', '\r']) || v.starts_with(' ') || v.ends_with(' ') {
            out.push('"');
            out.push_str(&v.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(v);
        }
    }
    let mut out = String::new();
    let mut line = |cells: &[String]| {
        for (i, c) in cells.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            put(&mut out, c);
        }
        out.push('\n');
    };
    line(names);
    for r in rows {
        line(r);
    }
    out
}

/// A file name for a pasted table: the characters a file system refuses are dropped.
pub fn file_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| {
            !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()
        })
        .collect();
    let s = s.trim().trim_matches('.').trim().to_string();
    if s.is_empty() {
        "Pasted table".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tsv_from_excel() {
        let p = parse("Region\tAmount\nNorth\t10\nSouth\t20\n").unwrap();
        assert_eq!(p.delimiter, Some('\t'));
        assert_eq!(p.rows.len(), 3);
        let (names, body) = split(&p, true);
        assert_eq!(names, ["Region", "Amount"]);
        assert_eq!(body[1], ["South", "20"]);
    }

    #[test]
    fn reads_csv_with_quotes_and_a_semicolon_variant() {
        let p = parse("a,b\n\"x, y\",\"he said \"\"hi\"\"\"\n").unwrap();
        assert_eq!(p.rows[1], ["x, y", "he said \"hi\""]);
        let p = parse("a;b\n1,5;2\n").unwrap();
        assert_eq!(p.delimiter, Some(';'));
        assert_eq!(p.rows[1], ["1,5", "2"]);
    }

    #[test]
    fn a_quoted_cell_may_hold_a_new_line() {
        let p = parse("a\tb\n\"line 1\nline 2\"\tz\n").unwrap();
        assert_eq!(p.rows.len(), 2);
        assert_eq!(p.rows[1][0], "line 1\nline 2");
    }

    #[test]
    fn one_plain_value_is_not_a_table() {
        assert!(parse("hello").is_none());
        assert!(parse("").is_none());
        // A list down one column is a table of one column.
        assert_eq!(parse("a\nb\nc\n").unwrap().rows.len(), 3);
    }

    #[test]
    fn header_choice_and_names() {
        let p = parse("x\tx\t\n1\t2\t3\n").unwrap();
        let (names, _) = split(&p, true);
        assert_eq!(names, ["x", "x 2", "Column 3"]);
        let (names, body) = split(&p, false);
        assert_eq!(names, ["Column 1", "Column 2", "Column 3"]);
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn csv_output_quotes_what_needs_it() {
        let csv = to_csv(
            &["a".into(), "b".into()],
            &[vec!["x,y".into(), "he said \"hi\"".into()]],
        );
        assert_eq!(csv, "a,b\n\"x,y\",\"he said \"\"hi\"\"\"\n");
    }
}
