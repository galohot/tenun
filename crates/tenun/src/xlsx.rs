//! An Excel file for one tile: the numbers it shows, with header row, number formats, real dates
//! and a bold total row. The engine ran the tile; this only writes the result.

use rust_xlsxwriter::{ExcelDateTime, Format, FormatBorder, Workbook};
use serde_json::Value;
use tenun_core::catalog::DType;
use tenun_core::engine::QueryResult;
use tenun_core::query::Unit;

/// The key the engine gives the "Other" group of a top N.
const OTHER_KEY: &str = "\u{0}other";

/// What one column holds, for the number format and the cell type.
struct Col {
    head: String,
    /// A group-by column that holds dates.
    date: Option<DateKind>,
    /// A number format string, when the column holds numbers.
    num: Option<String>,
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
                },
                Col {
                    head: head(headers, 1, "Value"),
                    date: None,
                    num: Some(num_format(
                        format_of(0).as_ref(),
                        None,
                        bars.iter().filter_map(|b| b.value),
                    )),
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
                cols.push(Col {
                    head: name,
                    date,
                    num,
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
            sheet.write_number_with_format(row, col, n.as_f64().unwrap_or(0.0), &f)?;
        }
        Value::String(s) if s == OTHER_KEY => {
            sheet.write_string_with_format(row, col, "Other", base)?;
        }
        Value::String(s) => {
            if let Some(kind) = c.date
                && let Some(d) = date_of(s, kind)
            {
                let pattern = match kind {
                    DateKind::Year => "yyyy",
                    DateKind::Month => "mmm yyyy",
                    DateKind::Day => "yyyy-mm-dd",
                    DateKind::Time => "yyyy-mm-dd hh:mm",
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

/// Thousands, decimals, a percent sign for shares and `percent` values, and a currency.
/// Excel shows every digit; "short" (1,2 jt) is for screens.
fn num_format(f: Option<&Value>, unit: Option<&str>, values: impl Iterator<Item = f64>) -> String {
    let decimals = f
        .and_then(|f| f["decimals"].as_u64())
        .map(|d| d.min(4) as usize);
    let currency = f
        .and_then(|f| f["currency"].as_str())
        .filter(|c| !c.is_empty());
    let percent =
        unit == Some("percent") || f.is_some_and(|f| f["percent"].as_bool() == Some(true));
    if unit == Some("rank") {
        return "\"#\"0".into();
    }
    let places = decimals.unwrap_or_else(|| {
        // No setting: whole numbers stay whole, anything else gets two decimals.
        let mut whole = true;
        for v in values {
            if (v - v.round()).abs() > 1e-9 {
                whole = false;
                break;
            }
        }
        if percent {
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
    if percent {
        return format!("0{digits}%");
    }
    let body = format!("#,##0{digits}");
    match currency {
        Some(code) => {
            let code: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            match code.as_str() {
                "IDR" => format!("\"Rp \"{body}"),
                "USD" => format!("\"$\"{body}"),
                "EUR" => format!("\"€\"{body}"),
                "SGD" => format!("\"S$\"{body}"),
                "MZN" => format!("{body}\" MT\""),
                other => format!("\"{other} \"{body}"),
            }
        }
        None => body,
    }
}
