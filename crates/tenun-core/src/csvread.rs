//! Reads the shape of a CSV file once, when it loads: the delimiter, the quotes, the header line,
//! the encoding and the type of every column, from the whole file. The table's view then reads the
//! file with those settings fixed.
//!
//! DuckDB's own guess looks at the first 20,480 rows on every query. A quoted value or a word
//! further down then breaks every query of a big file, and title lines above the header ("Data
//! Source", "Last Updated Date") make the header a row of data. Real files do both.

use crate::catalog::lit;
use crate::excel::{Cell, header_candidate};
use duckdb::Connection;
use std::io::Read;
use std::path::Path;

/// How a CSV file is read: the `SELECT` of its view, and what was found on the way.
#[derive(Debug, Clone)]
pub struct CsvPlan {
    pub select: String,
    pub notes: Vec<String>,
}

/// What DuckDB's sniffer found, with the options Tenun set.
struct Sniffed {
    delim: String,
    quote: String,
    escape: String,
    header: bool,
    skip: u32,
    columns: Vec<(String, String)>,
    date_format: String,
    timestamp_format: String,
    /// Numbers are written `1.234,5`: a decimal comma, and (when `thousands`) dots between thousands.
    decimal_comma: bool,
    thousands_dot: bool,
}

/// The lines read for the header: enough for any title block, small enough to be instant.
const HEAD_BYTES: usize = 256 * 1024;
const HEAD_LINES: usize = 60;

/// Plans how to read `path`. A file that is not UTF-8 is read from a UTF-8 copy in `copies`
/// (Windows-1252, the encoding Excel on Windows saves CSV in); UTF-16 is read as it is. Title lines
/// above a header that Tenun finds itself (the rule of Excel sheets) are skipped.
pub fn plan(db: &Connection, path: &Path, copies: &Path) -> anyhow::Result<CsvPlan> {
    let mut notes = Vec::new();
    let (read, encoding) = match encoding_of(path)? {
        Encoding::Utf8 => (path.to_path_buf(), ""),
        Encoding::Utf8MixedLines => {
            notes.push(
                "Read with one kind of line end: the file mixes Windows and Unix line ends"
                    .to_string(),
            );
            (utf8_copy(path, copies)?, "")
        }
        Encoding::Utf16 => {
            notes.push("Read as UTF-16 text".to_string());
            (path.to_path_buf(), "utf-16")
        }
        Encoding::Other => {
            notes.push("Read as Windows text (Windows-1252): the file is not UTF-8".to_string());
            (utf8_copy(path, copies)?, "")
        }
    };
    let file = lit(&read.to_string_lossy());
    let mut first = sniff(db, &file, encoding, None, Numbers::Point)?;
    let delim = first.delim.as_bytes().first().copied().unwrap_or(b',');
    let quote = first.quote.as_bytes().first().copied();
    let head = if encoding.is_empty() {
        read_head(&read)?
    } else {
        Vec::new()
    };
    let (grid, lines) = records(&head, delim, quote, HEAD_LINES);
    // `1.234,5`: only where the comma is not the delimiter (Indonesian and European exports).
    let numbers = number_style(&grid, delim);
    if numbers != Numbers::Point {
        notes.push("Read numbers written with a decimal comma (1.234,5)".to_string());
        first = sniff(db, &file, encoding, None, numbers)?;
    }
    let start = grid
        .iter()
        .position(|r| r.iter().any(|c| *c != Cell::Null))
        .unwrap_or(0);
    let sniffed = match header_candidate(&grid, start) {
        // A header Tenun found: DuckDB is told it is the header, because a header of years
        // ("1960", "1961") reads to it like a row of numbers.
        Some(h) if h > 0 => {
            let skip = lines[h];
            let titles = grid[..h]
                .iter()
                .filter(|r| r.iter().any(|c| *c != Cell::Null))
                .count();
            notes.push(format!(
                "Skipped {} above the header (line {})",
                if titles == 1 {
                    "1 title line".to_string()
                } else {
                    format!("{titles} title lines")
                },
                skip + 1
            ));
            sniff(db, &file, encoding, Some(skip), numbers)?
        }
        Some(_) if !first.header => sniff(db, &file, encoding, Some(0), numbers)?,
        _ => first,
    };
    Ok(CsvPlan {
        select: sniffed.select(&file, encoding),
        notes,
    })
}

enum Encoding {
    Utf8,
    /// UTF-8 with lines that end in `\r\n` and lines that end in `\n` alone. DuckDB reads one kind
    /// per file, so such a file is read from a copy (an edit in Notepad of an Excel export does this).
    Utf8MixedLines,
    Utf16,
    Other,
}

/// UTF-16 by its byte order mark; UTF-8 when every byte of the file is valid UTF-8 (checked in
/// blocks, so a big file is never held in memory). The same pass sees the kinds of line end.
fn encoding_of(path: &Path) -> anyhow::Result<Encoding> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    let mut carry = 0;
    let mut first = true;
    let (mut crlf, mut lf, mut prev) = (false, false, 0u8);
    loop {
        let n = f.read(&mut buf[carry..])?;
        let end = carry + n;
        for &b in &buf[carry..end] {
            if b == b'\n' {
                if prev == b'\r' {
                    crlf = true;
                } else {
                    lf = true;
                }
            }
            prev = b;
        }
        if first {
            first = false;
            if buf[..end].starts_with(&[0xFF, 0xFE]) || buf[..end].starts_with(&[0xFE, 0xFF]) {
                return Ok(Encoding::Utf16);
            }
        }
        if n == 0 {
            return Ok(if carry != 0 {
                Encoding::Other
            } else if crlf && lf {
                Encoding::Utf8MixedLines
            } else {
                Encoding::Utf8
            });
        }
        match std::str::from_utf8(&buf[..end]) {
            Ok(_) => carry = 0,
            // A character cut by the block: keep its first bytes for the next block.
            Err(e) if e.error_len().is_none() => {
                let valid = e.valid_up_to();
                buf.copy_within(valid..end, 0);
                carry = end - valid;
            }
            Err(_) => return Ok(Encoding::Other),
        }
    }
}

/// A UTF-8 copy of a file that is not UTF-8 or mixes line ends: its valid UTF-8 stays as it is,
/// other bytes are read as Windows-1252, and every `\r\n` becomes `\n`. The copy is named after the
/// file's path, so a reload replaces it.
fn utf8_copy(path: &Path, copies: &Path) -> anyhow::Result<std::path::PathBuf> {
    let bytes = std::fs::read(path)?;
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 8);
    for chunk in bytes.utf8_chunks() {
        out.push_str(chunk.valid());
        out.extend(chunk.invalid().iter().map(|&b| windows_1252(b)));
    }
    let out = out.replace("\r\n", "\n");
    std::fs::create_dir_all(copies)?;
    let key = path.to_string_lossy();
    // FNV-1a: a short, stable name for the path.
    let hash = key.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    let copy = copies.join(format!("{hash:016x}.csv"));
    std::fs::write(&copy, out)?;
    Ok(copy)
}

/// A byte of Windows-1252 as a character. 0x80–0x9F hold the curly quotes, dashes and the euro sign;
/// the five unused bytes keep their own code point, as Windows does.
fn windows_1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// How the numbers of a file are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Numbers {
    /// `1234.5` (also `1,234.5` in quotes, which stays text as before).
    Point,
    /// `1234,5`.
    Comma,
    /// `1.234,5`.
    CommaAndDots,
}

/// A decimal comma in the cells that did not read as numbers, in a file whose delimiter is not a
/// comma. Dots between groups of three digits are then thousands.
fn number_style(grid: &[Vec<Cell>], delim: u8) -> Numbers {
    if delim == b',' {
        return Numbers::Point;
    }
    let texts = || {
        grid.iter().skip(1).flatten().filter_map(|c| match c {
            Cell::Text(t) => Some(t.as_str()),
            _ => None,
        })
    };
    if !texts().any(is_decimal_comma) {
        return Numbers::Point;
    }
    if texts().any(|t| is_decimal_comma(t) && t.contains('.')) {
        Numbers::CommaAndDots
    } else {
        Numbers::Comma
    }
}

/// `-1.234,56` or `22838,52`: digits (with dots between groups of three), a comma, digits.
fn is_decimal_comma(t: &str) -> bool {
    let t = t.strip_prefix('-').unwrap_or(t);
    let Some((int, frac)) = t.split_once(',') else {
        return false;
    };
    let groups: Vec<&str> = int.split('.').collect();
    let int_ok = !int.is_empty()
        && groups
            .iter()
            .all(|g| !g.is_empty() && g.bytes().all(|b| b.is_ascii_digit()))
        && groups.iter().skip(1).all(|g| g.len() == 3)
        && groups[0].len() <= if groups.len() > 1 { 3 } else { usize::MAX };
    int_ok && !frac.is_empty() && frac.bytes().all(|b| b.is_ascii_digit())
}

/// Runs `sniff_csv` over the whole file with double quotes, the usual CSV quote. `skip` set: the
/// header is that line, whatever the sniffer thinks.
fn sniff(
    db: &Connection,
    file: &str,
    encoding: &str,
    skip: Option<u32>,
    numbers: Numbers,
) -> anyhow::Result<Sniffed> {
    let mut opts = String::from("quote = '\"', sample_size = -1, null_padding = true");
    opts.push_str(numbers_opts(numbers));
    if !encoding.is_empty() {
        opts.push_str(&format!(", encoding = {}", lit(encoding)));
    }
    if let Some(s) = skip {
        opts.push_str(&format!(", skip = {s}, header = true"));
    }
    // One row for each column, in order (`to_json` would need an extension that is not bundled).
    let sql = format!(
        "SELECT Delimiter, Quote, Escape, SkipRows, HasHeader, coalesce(DateFormat, ''), \
         coalesce(TimestampFormat, ''), c.name, c.type \
         FROM (SELECT *, unnest(Columns) AS c, generate_subscripts(Columns, 1) AS i \
         FROM sniff_csv({file}, {opts})) ORDER BY i"
    );
    let mut stmt = db.prepare(&sql).map_err(not_csv)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, u32>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        })
        .map_err(not_csv)?;
    let rows: Vec<_> = rows.collect::<Result<_, _>>().map_err(not_csv)?;
    let Some(first) = rows.first().cloned() else {
        anyhow::bail!("the file has no columns");
    };
    let empty = |s: String| if s == "(empty)" { String::new() } else { s };
    Ok(Sniffed {
        delim: first.0,
        quote: empty(first.1),
        escape: empty(first.2),
        skip: first.3,
        header: first.4,
        columns: rows.into_iter().map(|r| (r.7, r.8)).collect(),
        date_format: empty(first.5),
        timestamp_format: empty(first.6),
        decimal_comma: numbers != Numbers::Point,
        thousands_dot: numbers == Numbers::CommaAndDots,
    })
}

/// DuckDB's sniffer error is a page of options to try; the person needs one sentence.
fn not_csv(e: duckdb::Error) -> anyhow::Error {
    if e.to_string().contains("Error when sniffing file") {
        anyhow::anyhow!(
            "Tenun could not find the rows and columns of this CSV file. Open it in Excel and save it as CSV again."
        )
    } else {
        e.into()
    }
}

fn numbers_opts(numbers: Numbers) -> &'static str {
    match numbers {
        Numbers::Point => "",
        Numbers::Comma => ", decimal_separator = ','",
        Numbers::CommaAndDots => ", decimal_separator = ',', thousands = '.'",
    }
}

impl Sniffed {
    /// The `SELECT` that reads the file with every option fixed, so no query guesses again.
    fn select(&self, file: &str, encoding: &str) -> String {
        let columns = self
            .columns
            .iter()
            .map(|(n, t)| format!("{}: {}", lit(n), lit(t)))
            .collect::<Vec<_>>()
            .join(", ");
        // No escape seen in the file: a quote is escaped by doubling it, as in RFC 4180.
        let escape = if self.escape.is_empty() {
            &self.quote
        } else {
            &self.escape
        };
        let mut opts = format!(
            "auto_detect = false, delim = {}, quote = {}, escape = {}, header = {}, skip = {}, \
             null_padding = true, columns = {{{columns}}}",
            lit(&self.delim),
            lit(&self.quote),
            lit(escape),
            self.header,
            self.skip
        );
        if !self.date_format.is_empty() {
            opts.push_str(&format!(", dateformat = {}", lit(&self.date_format)));
        }
        if !self.timestamp_format.is_empty() {
            opts.push_str(&format!(
                ", timestampformat = {}",
                lit(&self.timestamp_format)
            ));
        }
        if !encoding.is_empty() {
            opts.push_str(&format!(", encoding = {}", lit(encoding)));
        }
        opts.push_str(numbers_opts(
            match (self.decimal_comma, self.thousands_dot) {
                (false, _) => Numbers::Point,
                (true, false) => Numbers::Comma,
                (true, true) => Numbers::CommaAndDots,
            },
        ));
        format!("SELECT * FROM read_csv({file}, {opts})")
    }
}

fn read_head(path: &Path) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(HEAD_BYTES);
    std::fs::File::open(path)?
        .take(HEAD_BYTES as u64)
        .read_to_end(&mut buf)?;
    Ok(buf)
}

/// The first `max` records of a CSV text, as cells (numbers read as numbers, so a row of data does
/// not look like a header), with the line each record starts on. A blank line is an empty record,
/// so the lines count as the file does. Quotes may hold delimiters and line breaks.
fn records(bytes: &[u8], delim: u8, quote: Option<u8>, max: usize) -> (Vec<Vec<Cell>>, Vec<u32>) {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let (mut grid, mut starts) = (Vec::new(), Vec::new());
    let (mut row, mut field) = (Vec::new(), Vec::new());
    let (mut quoted, mut line, mut start) = (false, 0u32, 0u32);
    let mut i = 0;
    while i < bytes.len() && grid.len() < max {
        let b = bytes[i];
        if quoted {
            if Some(b) == quote {
                if bytes.get(i + 1) == Some(&b) {
                    field.push(b);
                    i += 1;
                } else {
                    quoted = false;
                }
            } else {
                if b == b'\n' {
                    line += 1;
                }
                field.push(b);
            }
        } else if Some(b) == quote && field.is_empty() {
            quoted = true;
        } else if b == delim {
            row.push(cell(&field));
            field.clear();
        } else if b == b'\n' || b == b'\r' {
            if b == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            row.push(cell(&field));
            field.clear();
            grid.push(std::mem::take(&mut row));
            starts.push(start);
            line += 1;
            start = line;
        } else {
            field.push(b);
        }
        i += 1;
    }
    // A last record without a line break, unless the head was cut in the middle of the file.
    if (!field.is_empty() || !row.is_empty()) && bytes.len() < HEAD_BYTES && grid.len() < max {
        row.push(cell(&field));
        grid.push(row);
        starts.push(start);
    }
    (grid, starts)
}

fn cell(raw: &[u8]) -> Cell {
    let s = String::from_utf8_lossy(raw);
    let t = s.trim();
    if t.is_empty() {
        Cell::Null
    } else if let Ok(i) = t.parse::<i64>() {
        Cell::Int(i)
    } else if let Ok(f) = t.parse::<f64>()
        && f.is_finite()
    {
        Cell::Float(f)
    } else {
        Cell::Text(t.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_keep_line_numbers_through_blank_lines_and_quoted_breaks() {
        let text = b"\"Data Source\",\"WDI\",\r\n\r\n\"Last\",\"2026\",\r\n\r\n\"a\",\"b \"\"x\"\"\nc\",1\r\n";
        let (grid, lines) = records(text, b',', Some(b'"'), 10);
        assert_eq!(lines, [0, 1, 2, 3, 4]);
        assert_eq!(grid[4][1], Cell::Text("b \"x\"\nc".into()));
        assert_eq!(grid[4][2], Cell::Int(1));
        assert!(grid[1].iter().all(|c| *c == Cell::Null));
    }

    #[test]
    fn windows_1252_bytes_become_their_characters_and_utf8_stays() {
        let d = std::env::temp_dir().join(format!("tenun-csv1252-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("a.csv");
        // “Calves” in Windows-1252, then é in UTF-8, then é in Latin-1.
        std::fs::write(&f, b"name\n\x93Calves\x94 caf\xC3\xA9 caf\xE9 \x80\n").unwrap();
        assert!(matches!(encoding_of(&f).unwrap(), Encoding::Other));
        let copy = utf8_copy(&f, &d.join("copies")).unwrap();
        assert_eq!(
            std::fs::read_to_string(copy).unwrap(),
            "name\n“Calves” café café €\n"
        );
        std::fs::write(&f, "name\ncafé\n").unwrap();
        assert!(matches!(encoding_of(&f).unwrap(), Encoding::Utf8));
        std::fs::write(&f, [0xFF, 0xFE, b'a', 0]).unwrap();
        assert!(matches!(encoding_of(&f).unwrap(), Encoding::Utf16));
    }

    #[test]
    fn a_file_that_mixes_line_ends_is_read_from_a_copy_with_one_kind() {
        let d = std::env::temp_dir().join(format!("tenun-csvlines-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("m.csv");
        std::fs::write(&f, "region,amount\nA,1\nB,2\r\n").unwrap();
        assert!(matches!(encoding_of(&f).unwrap(), Encoding::Utf8MixedLines));
        let db = Connection::open_in_memory().unwrap();
        let plan = plan(&db, &f, &d.join("copies")).unwrap();
        let n: i64 = db
            .query_row(
                &format!("SELECT sum(amount) FROM ({})", plan.select),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 3);
        std::fs::write(&f, "region,amount\r\nA,1\r\n").unwrap();
        assert!(matches!(encoding_of(&f).unwrap(), Encoding::Utf8));
    }

    #[test]
    fn a_character_cut_by_a_block_is_still_utf8() {
        let d = std::env::temp_dir().join(format!("tenun-csvcut-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("b.csv");
        let mut text = vec![b'a'; (1 << 20) - 1];
        text.extend("é".as_bytes());
        std::fs::write(&f, text).unwrap();
        assert!(matches!(encoding_of(&f).unwrap(), Encoding::Utf8));
    }

    #[test]
    fn a_decimal_comma_is_found_only_where_the_comma_is_not_the_delimiter() {
        assert!(is_decimal_comma("22838,52"));
        assert!(is_decimal_comma("-1.234,5"));
        assert!(is_decimal_comma("12.345.678,0"));
        assert!(!is_decimal_comma("1.23,4")); // not groups of three
        assert!(!is_decimal_comma("1234.5"));
        assert!(!is_decimal_comma("a,5"));
        assert!(!is_decimal_comma(",5"));
        let row = |cells: &[&str]| cells.iter().map(|c| cell(c.as_bytes())).collect::<Vec<_>>();
        let grid = vec![row(&["a", "b"]), row(&["x", "1.234,5"])];
        assert_eq!(number_style(&grid, b';'), Numbers::CommaAndDots);
        assert_eq!(number_style(&grid, b','), Numbers::Point);
        let grid = vec![row(&["a", "b"]), row(&["x", "34,5"])];
        assert_eq!(number_style(&grid, b';'), Numbers::Comma);
    }

    #[test]
    fn a_cut_head_does_not_make_a_half_record() {
        let mut text = vec![b'a'; HEAD_BYTES];
        text[1] = b',';
        text[2] = b'\n';
        let (grid, _) = records(&text, b',', Some(b'"'), 10);
        assert_eq!(grid.len(), 1);
    }
}
