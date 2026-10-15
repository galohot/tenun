//! An Excel file for one tile: the numbers it shows, with header row, number formats, real dates
//! and a bold total row. The engine ran the tile; this only writes the result.

use rust_xlsxwriter::{ExcelDateTime, Format, FormatBorder, Workbook};
use serde_json::Value;
use tenun_core::catalog::DType;
use tenun_core::engine::QueryResult;
use tenun_core::numcode;
use tenun_core::numtext::{self, Negative, NumFormat};
use tenun_core::query::Unit;
use tenun_core::workspace::Locale;

/// The key the engine gives the "Other" group of a top N.
const OTHER_KEY: &str = "\u{0}other";

/// What one column holds, for the number format and the cell type.
struct Col {
    head: String,
    /// A group-by column that holds dates.
    date: Option<DateKind>,
    /// A number format string, when the column holds numbers.
    num: Option<String>,
    /// A duration: what one unit of the value is in days, as Excel counts time.
    to_days: Option<f64>,
    /// The column's own date pattern, already in Excel's words.
    date_code: Option<String>,
}

#[derive(Clone, Copy)]
enum DateKind {
    Year,
    Month,
    Day,
    Time,
}

/// Turns a result into the bytes of an `.xlsx` file. `headers` are the column names the app
/// shows; without them the engine's own names are used.
pub fn build(
    tile: &Value,
    res: &QueryResult,
    headers: &[String],
    title: &str,
) -> anyhow::Result<Vec<u8>> {
    build_in(tile, res, headers, title, &|_, _| None)
}

/// [`build`] with the date pattern of each column (`pattern_of(table, column)`): a raw date or a day keeps its
/// column's pattern.
pub fn build_in(
    tile: &Value,
    res: &QueryResult,
    headers: &[String],
    title: &str,
    pattern_of: &dyn Fn(&str, &str) -> Option<String>,
) -> anyhow::Result<Vec<u8>> {
    let kind = tile["type"].as_str().unwrap_or("");
    let formats = tile["formats"].as_array().cloned().unwrap_or_default();
    let show_as = tile["showAs"].as_array().cloned().unwrap_or_default();
    let format_of = |j: usize| formats.get(j).filter(|f| f.is_object()).cloned();

    // A number tile has no rows of its own: its history is the table.
    let (columns, rows) = if kind == "kpi" {
        let bars = res.kpi.as_ref().map(|k| k.bars.as_slice()).unwrap_or(&[]);
        (
            vec![
                Col {
                    head: head(headers, 0, "Period"),
                    date: None,
                    num: None,
                    to_days: None,
                    date_code: None,
                },
                Col {
                    head: head(headers, 1, "Value"),
                    date: None,
                    num: Some(num_format(
                        format_of(0).as_ref(),
                        None,
                        bars.iter().filter_map(|b| b.value),
                    )),
                    to_days: to_days(format_of(0).as_ref()),
                    date_code: None,
                },
            ],
            bars.iter()
                .map(|b| {
                    vec![
                        Value::from(b.label.clone()),
                        b.value.map_or(Value::Null, Value::from),
                    ]
                })
                .collect::<Vec<_>>(),
        )
    } else {
        let mut measure = 0;
        let mut cols = Vec::new();
        for (i, c) in res.columns.iter().enumerate() {
            let name = head(headers, i, &c.name);
            if c.role == "measure" {
                let unit = c.unit.map(|u| match u {
                    Unit::Percent => "percent",
                    Unit::Rank => "rank",
                });
                let values = res
                    .rows
                    .iter()
                    .filter_map(|r| r.get(i).and_then(Value::as_f64));
                cols.push(Col {
                    head: name,
                    date: None,
                    num: Some(num_format(format_of(measure).as_ref(), unit, values)),
                    to_days: to_days(format_of(measure).as_ref()),
                    date_code: None,
                });
                measure += 1;
            } else {
                let grain = tile["dims"][i]["grain"].as_str();
                let date = match c.dtype {
                    DType::Date => Some(match grain {
                        Some("year") => DateKind::Year,
                        Some("month") => DateKind::Month,
                        _ => DateKind::Day,
                    }),
                    DType::DateTime => Some(DateKind::Time),
                    _ => None,
                };
                // A date part (month 1 to 12) is a plain number.
                let num = (c.dtype == DType::Number).then(|| "0".to_string());
                // A raw date or a day keeps its column's pattern; longer periods keep their names.
                let dim = &tile["dims"][i];
                let date_code = match (date, grain) {
                    (Some(DateKind::Day | DateKind::Time), None | Some("day")) => dim["table"]
                        .as_str()
                        .zip(dim["column"].as_str())
                        .and_then(|(t, c)| pattern_of(t, c))
                        .map(|p| excel_date(&p))
                        .filter(|p| !p.is_empty()),
                    _ => None,
                };
                cols.push(Col {
                    head: name,
                    date,
                    num,
                    to_days: None,
                    date_code,
                });
            }
        }
        // The same question one year earlier sits in one more column, matched by the first group.
        let mut rows = res.rows.clone();
        if let Some(cmp) = &res.compare {
            let key = |v: &Value| v.to_string();
            let by: std::collections::HashMap<String, Value> = cmp
                .rows
                .iter()
                .filter_map(|r| Some((key(r.first()?), r.last()?.clone())))
                .collect();
            for r in &mut rows {
                let v = by.get(&key(&r[0])).cloned().unwrap_or(Value::Null);
                r.push(v);
            }
            let first = cols
                .iter()
                .position(|c| c.num.is_some() && c.date.is_none());
            cols.push(Col {
                head: cmp.label.clone(),
                date: None,
                num: first.and_then(|i| cols[i].num.clone()),
                to_days: first.and_then(|i| cols[i].to_days),
                date_code: None,
            });
        }
        (cols, rows)
    };

    let mut wb = Workbook::new();
    let sheet = wb.add_worksheet();
    sheet.set_name(sheet_name(title))?;

    let bold = Format::new().set_bold();
    let head_fmt = Format::new()
        .set_bold()
        .set_border_bottom(FormatBorder::Thin);
    let text_fmt = Format::new();
    for (c, col) in columns.iter().enumerate() {
        let fmt = if col.num.is_some() && col.date.is_none() {
            head_fmt
                .clone()
                .set_align(rust_xlsxwriter::FormatAlign::Right)
        } else {
            head_fmt.clone()
        };
        sheet.write_string_with_format(0, c as u16, &col.head, &fmt)?;
    }
    sheet.set_freeze_panes(1, 0)?;

    for (r, row) in rows.iter().enumerate() {
        let r = (r + 1) as u32;
        for (c, col) in columns.iter().enumerate() {
            write_cell(
                sheet,
                r,
                c as u16,
                row.get(c).unwrap_or(&Value::Null),
                col,
                false,
                &text_fmt,
            )?;
        }
    }

    // A table with a total row, and a pivot unless it turned totals off. A value shown another way
    // has no total.
    let totals_on = match kind {
        "table" => tile["totals"].as_bool() == Some(true),
        "pivot" => tile["totals"].as_bool() != Some(false),
        _ => false,
    };
    if let (true, Some(t)) = (totals_on, &res.totals) {
        let r = (rows.len() + 1) as u32;
        let mut j = 0;
        for (c, col) in columns.iter().enumerate() {
            let is_measure = res.columns.get(c).is_some_and(|x| x.role == "measure");
            if c == 0 {
                sheet.write_string_with_format(r, 0, "Total", &bold)?;
            } else if is_measure {
                let way = show_as.get(j).and_then(Value::as_str).unwrap_or("value");
                if way == "value" {
                    let v = t.grand.get(j).cloned().unwrap_or(Value::Null);
                    write_cell(sheet, r, c as u16, &v, col, true, &bold)?;
                }
                j += 1;
            }
        }
    }

    sheet.autofit();
    // What the reader should know about the links the numbers crossed (V4-D26). It is written
    // after the columns are sized, so a long sentence does not widen the first column.
    let note_fmt = Format::new().set_italic();
    let first = (rows.len() + if totals_on { 3 } else { 2 }) as u32;
    for (r, note) in (first..).zip(&res.notes) {
        sheet.write_string_with_format(r, 0, note, &note_fmt)?;
    }
    Ok(wb.save_to_buffer()?)
}

fn head(headers: &[String], i: usize, fallback: &str) -> String {
    headers
        .get(i)
        .filter(|h| !h.is_empty())
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

/// Excel allows 31 characters and forbids `[ ] : * ? / \` in a sheet name.
fn sheet_name(title: &str) -> String {
    let clean: String = title
        .chars()
        .filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\') && !c.is_control())
        .take(31)
        .collect();
    let clean = clean.trim().trim_matches('\'').to_string();
    if clean.is_empty() {
        "Tenun".into()
    } else {
        clean
    }
}

fn write_cell(
    sheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    v: &Value,
    c: &Col,
    bold: bool,
    base: &Format,
) -> anyhow::Result<()> {
    let with = |f: Format| if bold { f.set_bold() } else { f };
    match v {
        Value::Null => {}
        Value::Bool(b) => {
            sheet.write_boolean_with_format(row, col, *b, base)?;
        }
        Value::Number(n) => {
            let f = with(Format::new().set_num_format(c.num.as_deref().unwrap_or("General")));
            // A duration is written in days, which is how Excel counts time.
            let v = n.as_f64().unwrap_or(0.0) * c.to_days.unwrap_or(1.0);
            sheet.write_number_with_format(row, col, v, &f)?;
        }
        Value::String(s) if s == OTHER_KEY => {
            sheet.write_string_with_format(row, col, "Other", base)?;
        }
        Value::String(s) => {
            if let Some(kind) = c.date
                && let Some(d) = date_of(s, kind)
            {
                let pattern = match (kind, &c.date_code) {
                    (_, Some(own)) => own.as_str(),
                    (DateKind::Year, _) => "yyyy",
                    (DateKind::Month, _) => "mmm yyyy",
                    (DateKind::Day, _) => "yyyy-mm-dd",
                    (DateKind::Time, _) => "yyyy-mm-dd hh:mm",
                };
                let f = with(Format::new().set_num_format(pattern));
                sheet.write_datetime_with_format(row, col, &d, &f)?;
            } else {
                sheet.write_string_with_format(row, col, s, base)?;
            }
        }
        other => {
            sheet.write_string_with_format(row, col, other.to_string(), base)?;
        }
    }
    Ok(())
}

/// "2026-10-01" or "2026-10-01 14:30:00" as an Excel date.
fn date_of(s: &str, kind: DateKind) -> Option<ExcelDateTime> {
    let day = s.get(..10)?;
    let mut it = day.split('-').map(|p| p.parse::<u32>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    let base = ExcelDateTime::from_ymd(
        u16::try_from(y).ok()?,
        u8::try_from(m).ok()?,
        u8::try_from(d).ok()?,
    )
    .ok()?;
    if matches!(kind, DateKind::Time)
        && let Some(t) = s.get(11..19)
    {
        let mut it = t.split(':').map(|p| p.parse::<u16>().ok());
        let (h, mi, sec) = (it.next()??, it.next()??, it.next()??);
        return base.and_hms(h, u8::try_from(mi).ok()?, f64::from(sec)).ok();
    }
    Some(base)
}

/// Text written as it is in a format code.
fn quote(s: &str) -> String {
    if s.is_empty() {
        String::new()
    } else if s.contains('"') {
        s.chars().map(|c| format!("\\{c}")).collect()
    } else {
        format!("\"{s}\"")
    }
}

/// What one unit of a duration value is in days, when the format is a duration.
fn to_days(f: Option<&Value>) -> Option<f64> {
    let f = NumFormat::from_value(f?);
    if f.code.as_deref().is_some_and(numcode::reads) {
        return None;
    }
    f.duration.map(|u| u.seconds() / 86_400.0)
}

/// The Excel code of a tile's format: thousands, decimals, a percent sign for shares and `percent` values, a
/// currency, a fixed unit, the negative style, the zero text, the affixes, a duration. A format `code` is in Excel's
/// language already and is used as it is. Excel shows every digit; "short" (1,2 jt) is for screens. The digit style
/// is not written: Excel shows the marks of the reader's own computer.
fn num_format(f: Option<&Value>, unit: Option<&str>, values: impl Iterator<Item = f64>) -> String {
    if unit == Some("rank") {
        return "\"#\"0".into();
    }
    let f = f.map(NumFormat::from_value).unwrap_or_default();
    if let Some(code) = f.code.as_deref().filter(|c| numcode::reads(c)) {
        return code.to_string();
    }
    if let Some(u) = f.duration {
        return if u.seconds() < 60.0 {
            "[h]:mm:ss"
        } else {
            "[h]:mm"
        }
        .into();
    }
    let percent = unit == Some("percent") || f.percent == Some(true);
    // Excel has no significant digits: `sig` is written as `sig - 1` decimals, which is exact between 1 and 10 and
    // close enough elsewhere.
    let places = f
        .sig
        .map(|s| s as usize - 1)
        .or(f.decimals.map(|d| d as usize))
        .unwrap_or_else(|| {
            // No setting: whole numbers stay whole, anything else gets two decimals.
            let mut whole = true;
            for v in values {
                if (v - v.round()).abs() > 1e-9 {
                    whole = false;
                    break;
                }
            }
            if percent || f.scale.is_some() {
                1
            } else if whole {
                0
            } else {
                2
            }
        });
    let digits = if places == 0 {
        String::new()
    } else {
        format!(".{}", "0".repeat(places))
    };
    let body = if percent {
        format!("0{digits}%")
    } else if let Some(p) = f.scale {
        // English unit words: the file does not know the workspace's language.
        let word = match p {
            3 => "K",
            6 => "M",
            9 => "B",
            _ => "T",
        };
        format!("#,##0{digits}{}\"{word}\"", ",".repeat(p as usize / 3))
    } else {
        format!("#,##0{digits}")
    };
    let (pre, post) = match f.currency.as_deref().filter(|c| !percent && !c.is_empty()) {
        None => (String::new(), String::new()),
        Some(code) => {
            let code: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            if f.iso == Some(true) {
                (quote(&format!("{code} ")), String::new())
            } else if code == "MZN" {
                // As earlier files wrote it.
                (String::new(), quote(" MT"))
            } else {
                let (a, b) = numtext::currency(&code, Locale::En, false);
                (quote(&a), quote(&b))
            }
        }
    };
    let pos = format!("{pre}{body}{post}");
    let affix = |s: Option<&str>, before: bool| {
        let s = s.filter(|s| !s.is_empty())?;
        let edge = if before {
            s.chars().last()
        } else {
            s.chars().next()
        };
        let gap = edge.is_some_and(char::is_alphanumeric);
        Some(quote(&match (gap, before) {
            (false, _) => s.to_string(),
            (true, true) => format!("{s} "),
            (true, false) => format!(" {s}"),
        }))
    };
    let prefix = affix(f.prefix.as_deref(), true);
    let suffix = affix(f.suffix.as_deref(), false);
    let plain = matches!(f.negative, None | Some(Negative::Minus));
    if plain && f.zero.is_none() && prefix.is_none() {
        return format!("{pos}{}", suffix.unwrap_or_default());
    }
    let (prefix, suffix) = (prefix.unwrap_or_default(), suffix.unwrap_or_default());
    let neg = match f.negative {
        Some(Negative::Parens) => format!("{prefix}({pos}){suffix}"),
        Some(Negative::Red) => format!("[Red]{prefix}-{pos}{suffix}"),
        Some(Negative::ParensRed) => format!("[Red]{prefix}({pos}){suffix}"),
        _ => format!("{prefix}-{pos}{suffix}"),
    };
    let pos = format!("{prefix}{pos}{suffix}");
    match &f.zero {
        Some(z) => format!("{pos};{neg};{}", quote(z)),
        None => format!("{pos};{neg}"),
    }
}

/// A date pattern of the app (`yyyy-MM-dd`, `d MMM yyyy`, `h:mm a`, `'Day' d`) in Excel's words. Month and day names
/// follow the reader's Excel.
fn excel_date(p: &str) -> String {
    enum Part {
        Run(char, usize),
        Text(String),
    }
    let cs: Vec<char> = p.chars().collect();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c == '\'' {
            if cs.get(i + 1) == Some(&'\'') {
                parts.push(Part::Text("'".into()));
                i += 2;
                continue;
            }
            let end = cs[i + 1..]
                .iter()
                .position(|d| *d == '\'')
                .map_or(cs.len(), |e| e + i + 1);
            parts.push(Part::Text(cs[i + 1..end].iter().collect()));
            i = end + 1;
        } else if c.is_ascii_alphabetic() {
            let n = cs[i..].iter().take_while(|d| **d == c).count();
            parts.push(Part::Run(c, n));
            i += n;
        } else {
            parts.push(Part::Text(c.to_string()));
            i += 1;
        }
    }
    let mut out = String::new();
    for part in parts {
        let word = match part {
            Part::Run(c, n) => match (c, n) {
                ('y', 2) => "yy".to_string(),
                ('y', _) => "yyyy".into(),
                ('M' | 'L', 1) => "m".into(),
                ('M' | 'L', 2) => "mm".into(),
                ('M' | 'L', 3) => "mmm".into(),
                ('M' | 'L', _) => "mmmm".into(),
                ('d', 1) => "d".into(),
                ('d', _) => "dd".into(),
                ('E', 4..) => "dddd".into(),
                ('E', _) => "ddd".into(),
                ('H' | 'h', 1) => "h".into(),
                ('H' | 'h', _) => "hh".into(),
                ('m', 1) => "m".into(),
                ('m', _) => "mm".into(),
                ('s', 1) => "s".into(),
                ('s', _) => "ss".into(),
                ('a', _) => "AM/PM".into(),
                _ => quote(&c.to_string().repeat(n)),
            },
            Part::Text(t) if t.chars().all(|c| " -/:,.()".contains(c)) => t,
            Part::Text(t) => quote(&t),
        };
        out.push_str(&word);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn code(f: Value) -> String {
        num_format(Some(&f), None, [1.0].into_iter())
    }

    #[test]
    fn formats_become_excel_codes() {
        // As before.
        assert_eq!(code(json!({})), "#,##0");
        assert_eq!(
            code(json!({"decimals": 2, "currency": "IDR"})),
            "\"Rp \"#,##0.00"
        );
        assert_eq!(code(json!({"currency": "MZN"})), "#,##0\" MT\"");
        assert_eq!(code(json!({"percent": true})), "0.0%");
        // The currency table, and the code instead of the symbol.
        assert_eq!(code(json!({"currency": "GBP"})), "\"£\"#,##0");
        assert_eq!(code(json!({"currency": "CHF"})), "\"CHF \"#,##0");
        assert_eq!(
            code(json!({"currency": "USD", "iso": true})),
            "\"USD \"#,##0"
        );
        // A fixed unit: one comma per thousand.
        assert_eq!(code(json!({"scale": 6})), "#,##0.0,,\"M\"");
        assert_eq!(code(json!({"scale": 3, "decimals": 0})), "#,##0,\"K\"");
        // Negative styles and the zero text.
        assert_eq!(code(json!({"negative": "parens"})), "#,##0;(#,##0)");
        assert_eq!(code(json!({"negative": "red"})), "#,##0;[Red]-#,##0");
        assert_eq!(
            code(json!({"negative": "parens_red", "currency": "USD"})),
            "\"$\"#,##0;[Red](\"$\"#,##0)"
        );
        assert_eq!(code(json!({"zero": "–"})), "#,##0;-#,##0;\"–\"");
        // Affixes, with a space next to a letter.
        assert_eq!(code(json!({"suffix": "pcs"})), "#,##0\" pcs\"");
        assert_eq!(code(json!({"suffix": "°C"})), "#,##0\"°C\"");
        assert_eq!(code(json!({"prefix": "~"})), "\"~\"#,##0;\"~\"-#,##0");
        // Significant digits as decimals; durations as time; a code as it is.
        assert_eq!(code(json!({"sig": 3})), "#,##0.00");
        assert_eq!(code(json!({"duration": "s"})), "[h]:mm:ss");
        assert_eq!(code(json!({"duration": "h"})), "[h]:mm");
        assert_eq!(to_days(Some(&json!({"duration": "h"}))), Some(1.0 / 24.0));
        assert_eq!(to_days(Some(&json!({"decimals": 1}))), None);
        assert_eq!(
            code(json!({"code": "#,##0;[Red](#,##0)", "currency": "USD"})),
            "#,##0;[Red](#,##0)"
        );
        assert_eq!(code(json!({"code": "\"open"})), "#,##0");
    }

    #[test]
    fn date_patterns_become_excel_codes() {
        assert_eq!(excel_date("yyyy-MM-dd"), "yyyy-mm-dd");
        assert_eq!(excel_date("d MMM yy"), "d mmm yy");
        assert_eq!(excel_date("EEEE, d MMMM yyyy"), "dddd, d mmmm yyyy");
        assert_eq!(excel_date("EEE HH:mm:ss"), "ddd hh:mm:ss");
        assert_eq!(excel_date("h:mm a"), "h:mm AM/PM");
        assert_eq!(excel_date("'Day' d"), "\"Day\" d");
        assert_eq!(excel_date("dd.MM.yyyy"), "dd.mm.yyyy");
    }
}
