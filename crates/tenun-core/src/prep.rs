//! Data preparation that never touches the user's file: column types, wide sheets turned into
//! rows, and the profile of a column. The changes live in a DuckDB view over the loaded table.

use crate::catalog::{DType, ident, lit};
use crate::engine::run;
use crate::query::Param;
use crate::workspace::{DateFormat, TableSetup, Transform};
use chrono::NaiveDate;
use duckdb::Connection;
use serde::Serialize;
use serde_json::Value;

// ---------------------------------------------------------------------------------------------
// Periods in column headers

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodKind {
    Day,
    Month,
    Quarter,
    Year,
}

/// A header that names a period. `date` is the first day of it, and is missing when the header
/// has no year ("Jan", "Q1").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Period {
    pub kind: PeriodKind,
    pub date: Option<NaiveDate>,
}

fn month_of(word: &str) -> Option<u32> {
    Some(match word {
        "jan" | "january" | "januari" => 1,
        "feb" | "february" | "februari" | "pebruari" => 2,
        "mar" | "march" | "maret" => 3,
        "apr" | "april" => 4,
        "may" | "mei" => 5,
        "jun" | "june" | "juni" => 6,
        "jul" | "july" | "juli" => 7,
        "aug" | "august" | "agu" | "agt" | "agus" | "agustus" => 8,
        "sep" | "sept" | "september" => 9,
        "oct" | "october" | "okt" | "oktober" => 10,
        "nov" | "november" | "nop" | "nopember" => 11,
        "dec" | "december" | "des" | "desember" => 12,
        _ => return None,
    })
}

#[derive(Debug, PartialEq)]
enum Tok<'a> {
    Word(&'a str),
    Num(u32, usize),
}

fn tokens(s: &str) -> Option<Vec<Tok<'_>>> {
    let class = |b: u8| match b {
        b'a'..=b'z' => 1,
        b'0'..=b'9' => 2,
        _ => 0,
    };
    let mut out = Vec::new();
    let mut run: Option<(usize, u8)> = None;
    for (i, b) in s.bytes().chain(std::iter::once(b' ')).enumerate() {
        let c = class(b);
        if let Some((start, kind)) = run
            && c != kind
        {
            let part = &s[start..i];
            out.push(if kind == 1 {
                Tok::Word(part)
            } else {
                Tok::Num(part.parse().ok()?, part.len())
            });
            run = None;
        }
        if c != 0 && run.is_none() {
            run = Some((i, c));
        }
    }
    Some(out)
}

/// A year from 4 digits (1900 to 2200) or 2 digits (20xx).
fn year_of(n: u32, len: usize) -> Option<i32> {
    match len {
        4 if (1900..=2200).contains(&n) => Some(n as i32),
        2 => Some(2000 + n as i32),
        _ => None,
    }
}

/// Reads a column header as a period: "Jan-26", "January 2026", "Okt 2026", "2026-01", "Q1 2026",
/// "2025 Jan", "2026". Returns None for anything else.
pub fn parse_period(header: &str) -> Option<Period> {
    use PeriodKind::*;
    let lower = header.trim().to_lowercase();
    let toks = tokens(&lower)?;
    let ymd = |y: i32, m: u32, d: u32, kind| {
        NaiveDate::from_ymd_opt(y, m, d).map(|d| Period {
            kind,
            date: Some(d),
        })
    };
    // A quarter with a year is a date; without one it only names the quarter.
    let quarter = |q: u32, year: Option<(u32, usize)>| match year {
        Some((y, l)) => ymd(year_of(y, l)?, (q.clamp(1, 4) - 1) * 3 + 1, 1, Quarter)
            .filter(|_| (1..=4).contains(&q)),
        None => (1..=4).contains(&q).then_some(Period {
            kind: Quarter,
            date: None,
        }),
    };
    let is_q = |w: &str| w == "q" || w == "tw";
    match toks.as_slice() {
        [Tok::Num(n, 4)] => year_of(*n, 4).and_then(|y| ymd(y, 1, 1, Year)),
        [Tok::Word(w)] => month_of(w).map(|_| Period {
            kind: Month,
            date: None,
        }),
        [Tok::Word(m), Tok::Num(y, l)] if month_of(m).is_some() => {
            ymd(year_of(*y, *l)?, month_of(m)?, 1, Month)
        }
        [Tok::Num(y, l), Tok::Word(m)] if month_of(m).is_some() => {
            ymd(year_of(*y, *l)?, month_of(m)?, 1, Month)
        }
        [Tok::Word(q), Tok::Num(n, 1)] if is_q(q) => quarter(*n, None),
        [Tok::Word(q), Tok::Num(n, 1), Tok::Num(y, l)] if is_q(q) => quarter(*n, Some((*y, *l))),
        [Tok::Num(y, l), Tok::Word(q), Tok::Num(n, 1)] if is_q(q) => quarter(*n, Some((*y, *l))),
        [Tok::Num(y, 4), Tok::Num(m, 1..=2)] => ymd(year_of(*y, 4)?, *m, 1, Month),
        [Tok::Num(m, 1..=2), Tok::Num(y, 4)] => ymd(year_of(*y, 4)?, *m, 1, Month),
        [Tok::Num(y, 4), Tok::Num(m, 1..=2), Tok::Num(d, 1..=2)] => {
            ymd(year_of(*y, 4)?, *m, *d, Day)
        }
        [Tok::Num(d, 1..=2), Tok::Word(m), Tok::Num(y, 4)] if month_of(m).is_some() => {
            ymd(year_of(*y, 4)?, month_of(m)?, *d, Day)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Unpivot

/// A wide sheet found in the columns of a table: months, quarters or years across.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UnpivotPlan {
    /// The columns that stay as they are.
    pub keep: Vec<String>,
    /// The columns that turn into rows.
    pub columns: Vec<String>,
    /// Suggested names for the two new columns.
    pub name: String,
    pub value: String,
    /// Every header has a year, so the new name column is read as a date.
    pub dates: bool,
}

/// Three or more number columns whose headers all read as periods make a wide sheet.
pub fn detect_unpivot(columns: &[(String, DType)]) -> Option<UnpivotPlan> {
    let mut wide = Vec::new();
    let mut kept = Vec::new();
    for (name, dtype) in columns {
        match parse_period(name) {
            Some(p) if *dtype == DType::Number => wide.push((name.clone(), p)),
            _ => kept.push(name.clone()),
        }
    }
    if wide.len() < 3 {
        return None;
    }
    let kind = |k: PeriodKind| wide.iter().all(|(_, p)| p.kind == k);
    let word = if kind(PeriodKind::Year) {
        "Year"
    } else if kind(PeriodKind::Quarter) {
        "Quarter"
    } else if kind(PeriodKind::Month) {
        "Month"
    } else {
        "Period"
    };
    let name = unique(word, &kept);
    let value = unique("Value", &kept);
    Some(UnpivotPlan {
        dates: wide.iter().all(|(_, p)| p.date.is_some()),
        columns: wide.into_iter().map(|(n, _)| n).collect(),
        keep: kept,
        name,
        value,
    })
}

/// `wanted`, or `wanted 2`, `wanted 3`… when a kept column already has the name.
fn unique(wanted: &str, taken: &[String]) -> String {
    let mut n = wanted.to_string();
    let mut k = 2;
    while taken.contains(&n) {
        n = format!("{wanted} {k}");
        k += 1;
    }
    n
}

// ---------------------------------------------------------------------------------------------
// The view

/// A column of the relation a view reads from.
#[derive(Debug, Clone, PartialEq)]
pub struct ColInfo {
    pub name: String,
    pub dtype: DType,
}

/// The SELECT that gives a table its prepared form.
#[derive(Debug)]
pub struct ViewPlan {
    pub sql: String,
    /// The columns the SELECT gives.
    pub columns: Vec<ColInfo>,
    /// For each retyped column, a query that counts the cells that could not be read.
    pub checks: Vec<(String, String)>,
}

const DMY: &[&str] = &[
    "%d/%m/%Y %H:%M:%S",
    "%d/%m/%Y %H:%M",
    "%d/%m/%Y",
    "%d-%m-%Y",
    "%d.%m.%Y",
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%d",
];
const MDY: &[&str] = &[
    "%m/%d/%Y %H:%M:%S",
    "%m/%d/%Y %H:%M",
    "%m/%d/%Y",
    "%m-%d-%Y",
    "%m.%d.%Y",
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%d",
];

/// The SQL that reads column `q` (already quoted) as `to`. Cells that cannot be read give NULL.
fn convert(q: &str, from: DType, to: DType, fmt: DateFormat) -> String {
    let text = format!("CAST({q} AS VARCHAR)");
    let stamp = match from {
        DType::Date => format!("CAST({q} AS TIMESTAMP)"),
        DType::DateTime => q.to_string(),
        _ => match fmt {
            DateFormat::Auto => format!("TRY_CAST({text} AS TIMESTAMP)"),
            DateFormat::Dmy | DateFormat::Mdy => {
                let list = if fmt == DateFormat::Dmy { DMY } else { MDY };
                let list: Vec<String> = list.iter().map(|f| lit(f)).collect();
                format!("try_strptime({text}, [{}])", list.join(", "))
            }
        },
    };
    match to {
        DType::Text => text,
        DType::Number => match from {
            DType::Number | DType::Bool => format!("TRY_CAST({q} AS DOUBLE)"),
            _ => format!("TRY_CAST({text} AS DOUBLE)"),
        },
        DType::Bool => format!(
            "CASE lower(trim({text})) WHEN 'true' THEN TRUE WHEN 'yes' THEN TRUE WHEN 'y' THEN TRUE \
             WHEN 'ya' THEN TRUE WHEN '1' THEN TRUE WHEN 'false' THEN FALSE WHEN 'no' THEN FALSE \
             WHEN 'n' THEN FALSE WHEN 'tidak' THEN FALSE WHEN '0' THEN FALSE END"
        ),
        DType::Date => format!("CAST({stamp} AS DATE)"),
        DType::DateTime => stamp,
    }
}

/// Builds the SELECT for a table: the transforms first, then the column types. `base` is anything
/// that can follow FROM. Returns None when the setup changes nothing.
pub fn plan_view(base: &str, base_cols: &[ColInfo], setup: &TableSetup) -> Option<ViewPlan> {
    let mut rel = base.to_string();
    let mut cols = base_cols.to_vec();
    let mut n = 0;
    for t in &setup.transforms {
        let Transform::Unpivot { keep, name, value } = t;
        let (name, value) = (name.trim(), value.trim());
        let stay: Vec<&ColInfo> = cols.iter().filter(|c| keep.contains(&c.name)).collect();
        let wide: Vec<&ColInfo> = cols.iter().filter(|c| !keep.contains(&c.name)).collect();
        let clash = stay.iter().any(|c| c.name == name || c.name == value);
        if wide.is_empty() || name.is_empty() || value.is_empty() || name == value || clash {
            continue; // a transform that cannot apply is left out
        }
        let numeric = wide.iter().all(|c| c.dtype == DType::Number);
        let stay_sql: Vec<String> = stay.iter().map(|c| ident(&c.name)).collect();
        let wide_sql: Vec<String> = wide
            .iter()
            .map(|c| {
                let q = ident(&c.name);
                if numeric {
                    q
                } else {
                    format!("CAST({q} AS VARCHAR) AS {q}")
                }
            })
            .collect();
        let wide_names: Vec<String> = wide.iter().map(|c| ident(&c.name)).collect();
        let periods: Option<Vec<NaiveDate>> = wide
            .iter()
            .map(|c| parse_period(&c.name).and_then(|p| p.date))
            .collect();
        let name_sql = match &periods {
            Some(dates) => {
                let arms: Vec<String> = wide
                    .iter()
                    .zip(dates)
                    .map(|(c, d)| {
                        format!("WHEN {} THEN DATE '{}'", lit(&c.name), d.format("%Y-%m-%d"))
                    })
                    .collect();
                format!("CASE \"__name\" {} END", arms.join(" "))
            }
            None => "\"__name\"".to_string(),
        };
        let mut select = stay_sql.clone();
        select.push(format!("{name_sql} AS {}", ident(name)));
        select.push(format!("\"__value\" AS {}", ident(value)));
        let inner: Vec<String> = stay_sql.iter().cloned().chain(wide_sql).collect();
        n += 1;
        rel = format!(
            "(SELECT {} FROM (SELECT {} FROM {rel}) UNPIVOT (\"__value\" FOR \"__name\" IN ({}))) AS \"__t{n}\"",
            select.join(", "),
            inner.join(", "),
            wide_names.join(", ")
        );
        let mut next: Vec<ColInfo> = stay.into_iter().cloned().collect();
        next.push(ColInfo {
            name: name.to_string(),
            dtype: if periods.is_some() {
                DType::Date
            } else {
                DType::Text
            },
        });
        next.push(ColInfo {
            name: value.to_string(),
            dtype: if numeric { DType::Number } else { DType::Text },
        });
        cols = next;
    }

    // Column types, by the name the column has now.
    let mut checks = Vec::new();
    let mut retyped = false;
    let exprs: Vec<String> = cols
        .iter()
        .map(|c| {
            let q = ident(&c.name);
            match setup
                .columns
                .get(&c.name)
                .and_then(|s| s.dtype.map(|t| (t, s)))
            {
                Some((to, s)) if to != c.dtype => {
                    retyped = true;
                    let e = convert(&q, c.dtype, to, s.date_format.unwrap_or(DateFormat::Auto));
                    checks.push((
                        c.name.clone(),
                        format!(
                            "SELECT count(*) FROM {rel} WHERE {q} IS NOT NULL AND ({e}) IS NULL"
                        ),
                    ));
                    format!("{e} AS {q}")
                }
                _ => q,
            }
        })
        .collect();
    if n == 0 && !retyped {
        return None;
    }
    for c in &mut cols {
        if let Some(to) = setup.columns.get(&c.name).and_then(|s| s.dtype) {
            c.dtype = to;
        }
    }
    Some(ViewPlan {
        sql: format!("SELECT {} FROM {rel}", exprs.join(", ")),
        columns: cols,
        checks,
    })
}

// ---------------------------------------------------------------------------------------------
// Column profile

#[derive(Debug, Serialize)]
pub struct TopValue {
    pub value: String,
    pub count: u64,
}

/// What a column holds, for the hover card in the Data view.
#[derive(Debug, Serialize)]
pub struct Profile {
    pub rows: u64,
    /// The share of blank cells, 0 to 1.
    pub blank: f64,
    pub distinct: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Value>,
    /// Text and yes/no columns: the 5 most common values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<Vec<TopValue>>,
    /// Number and date columns: how many values fall in each of 12 equal bins between min and max.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub histogram: Option<Vec<u64>>,
    /// The file had text and numbers mixed in this column.
    pub mixed: bool,
}

const BINS: usize = 12;

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

pub fn profile(
    conn: &Connection,
    sql_ref: &str,
    column: &str,
    dtype: DType,
    mixed: bool,
) -> anyhow::Result<Profile> {
    let q = ident(column);
    let counts = run(
        conn,
        &format!("SELECT count(*), count({q}), count(DISTINCT {q}) FROM {sql_ref}"),
        &[],
    )?;
    let get = |i: usize| counts.first().and_then(|r| num(r.get(i))).unwrap_or(0.0) as u64;
    let (rows, filled, distinct) = (get(0), get(1), get(2));
    let mut p = Profile {
        rows,
        blank: if rows == 0 {
            0.0
        } else {
            (rows - filled) as f64 / rows as f64
        },
        distinct,
        min: None,
        max: None,
        top: None,
        histogram: None,
        mixed,
    };
    match dtype {
        DType::Number | DType::Date | DType::DateTime if filled > 0 => {
            let (shown_min, shown_max, v) = match dtype {
                DType::Number => (
                    format!("CAST(min({q}) AS DOUBLE)"),
                    format!("CAST(max({q}) AS DOUBLE)"),
                    format!("CAST({q} AS DOUBLE)"),
                ),
                DType::Date => (
                    format!("strftime(min({q}), '%Y-%m-%d')"),
                    format!("strftime(max({q}), '%Y-%m-%d')"),
                    format!("epoch(CAST({q} AS TIMESTAMP))"),
                ),
                _ => (
                    format!("strftime(min({q}), '%Y-%m-%d %H:%M')"),
                    format!("strftime(max({q}), '%Y-%m-%d %H:%M')"),
                    format!("epoch(CAST({q} AS TIMESTAMP))"),
                ),
            };
            let r = run(
                conn,
                &format!("SELECT {shown_min}, {shown_max}, min({v}), max({v}) FROM {sql_ref}"),
                &[],
            )?;
            let row = r.into_iter().next().unwrap_or_default();
            p.min = row.first().cloned();
            p.max = row.get(1).cloned();
            if let (Some(lo), Some(hi)) = (num(row.get(2)), num(row.get(3))) {
                let width = (hi - lo) / BINS as f64;
                let mut hist = vec![0u64; if width > 0.0 { BINS } else { 1 }];
                if width > 0.0 {
                    let rows = run(
                        conn,
                        &format!(
                            "SELECT LEAST({}, CAST(floor(({v} - ?) / ?) AS INTEGER)) AS b, count(*) \
                             FROM {sql_ref} WHERE {q} IS NOT NULL GROUP BY 1",
                            BINS - 1
                        ),
                        &[Param::Double(lo), Param::Double(width)],
                    )?;
                    for r in rows {
                        if let (Some(b), Some(c)) = (num(r.first()), num(r.get(1))) {
                            let b = (b.max(0.0) as usize).min(BINS - 1);
                            hist[b] += c as u64;
                        }
                    }
                } else {
                    hist[0] = filled;
                }
                p.histogram = Some(hist);
            }
        }
        DType::Text | DType::Bool => {
            let rows = run(
                conn,
                &format!(
                    "SELECT CAST({q} AS VARCHAR), count(*) FROM {sql_ref} WHERE {q} IS NOT NULL \
                     GROUP BY 1 ORDER BY 2 DESC, 1 LIMIT 5"
                ),
                &[],
            )?;
            p.top = Some(
                rows.into_iter()
                    .filter_map(|r| {
                        Some(TopValue {
                            value: r.first()?.as_str()?.to_string(),
                            count: num(r.get(1))? as u64,
                        })
                    })
                    .collect(),
            );
        }
        _ => {}
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use PeriodKind::*;

    fn d(y: i32, m: u32, day: u32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(y, m, day)
    }

    #[test]
    fn headers_that_name_a_period() {
        let p = |s: &str| parse_period(s).map(|p| (p.kind, p.date));
        assert_eq!(p("Jan-26"), Some((Month, d(2026, 1, 1))));
        assert_eq!(p("Jan 2026"), Some((Month, d(2026, 1, 1))));
        assert_eq!(p("Januari 2026"), Some((Month, d(2026, 1, 1))));
        assert_eq!(p("Agu-26"), Some((Month, d(2026, 8, 1))));
        assert_eq!(p("Okt 2026"), Some((Month, d(2026, 10, 1))));
        assert_eq!(p("2026-01"), Some((Month, d(2026, 1, 1))));
        assert_eq!(p("2025 Jan"), Some((Month, d(2025, 1, 1))));
        assert_eq!(p("2026-03-05"), Some((Day, d(2026, 3, 5))));
        assert_eq!(p("2026"), Some((Year, d(2026, 1, 1))));
        assert_eq!(p("Q2 2026"), Some((Quarter, d(2026, 4, 1))));
        assert_eq!(p("2026 Q3"), Some((Quarter, d(2026, 7, 1))));
        assert_eq!(p("Q4-26"), Some((Quarter, d(2026, 10, 1))));
        // No year: a period, but not a date.
        assert_eq!(p("Mei"), Some((Month, None)));
        assert_eq!(p("Q1"), Some((Quarter, None)));
        // Not periods.
        for s in [
            "Region",
            "Total",
            "Amount",
            "Q5",
            "13",
            "Jan Smith",
            "2026 sales",
            "Mar-ket",
        ] {
            assert_eq!(p(s), None, "{s}");
        }
    }

    #[test]
    fn a_wide_sheet_needs_three_period_number_columns() {
        use DType::*;
        let cols = |v: &[(&str, DType)]| -> Vec<(String, DType)> {
            v.iter().map(|(n, t)| (n.to_string(), *t)).collect()
        };
        let plan = detect_unpivot(&cols(&[
            ("Region", Text),
            ("Jan-26", Number),
            ("Feb-26", Number),
            ("Mar-26", Number),
        ]))
        .unwrap();
        assert_eq!(plan.keep, vec!["Region"]);
        assert_eq!(plan.columns, vec!["Jan-26", "Feb-26", "Mar-26"]);
        assert_eq!(
            (plan.name.as_str(), plan.value.as_str()),
            ("Month", "Value")
        );
        assert!(plan.dates);
        // Two are not enough; text columns do not count.
        assert!(
            detect_unpivot(&cols(&[
                ("a", Text),
                ("Jan-26", Number),
                ("Feb-26", Number)
            ]))
            .is_none()
        );
        assert!(
            detect_unpivot(&cols(&[
                ("Jan", Text),
                ("Feb", Text),
                ("Mar", Text),
                ("Apr", Number)
            ]))
            .is_none()
        );
        // Months without a year work, but the new column stays text.
        let p =
            detect_unpivot(&cols(&[("Jan", Number), ("Feb", Number), ("Mar", Number)])).unwrap();
        assert!(!p.dates);
    }
}
