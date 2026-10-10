//! Tiles and snapshots: what each tile asks, how a number tile compares
//! periods fairly, how a time chart marks an unfinished last period, and the
//! complete snapshot of a dashboard (every tile, plus the results needed to
//! cross-filter offline). One implementation for live tiles, exported files,
//! published dashboards and the headless agent.

use crate::engine::{Engine, QueryResult};
use crate::query::{Agg, Dimension, Filter, FilterOp, Grain, Measure, SemanticQuery, Sort};
use crate::shape;
use crate::workspace::Locale;
use chrono::{Datelike, Duration, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

/// A dashboard tile, as the UI stores it. Unknown fields are kept by callers
/// that need them (the payload carries the original JSON).
#[derive(Debug, Clone, Deserialize)]
pub struct Tile {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub measures: Vec<Measure>,
    #[serde(default)]
    pub dims: Vec<Dimension>,
    #[serde(default)]
    pub sort: Option<TileSort>,
    #[serde(default)]
    pub top: Option<TopN>,
    #[serde(default)]
    pub bins: Option<TileBins>,
    /// Tables: add a total row. Pivots have totals unless this is `false`.
    #[serde(default)]
    pub totals: Option<bool>,
    /// Pivot: the field whose values become the columns.
    #[serde(default)]
    pub cols: Option<Dimension>,
    /// How each value is shown (share, running total, change, rank…), index-aligned with
    /// `measures`.
    #[serde(default, rename = "showAs")]
    pub show_as: Option<Vec<Option<shape::ShowAs>>>,
    /// A time chart also draws the same question one year or one period earlier.
    #[serde(default)]
    pub compare: Option<shape::CompareTo>,
}

/// Tiles that show something the user typed or picked, not a query: they have no result.
pub fn is_static(kind: &str) -> bool {
    matches!(kind, "text" | "image" | "filter")
}

/// Histogram options. No width means the engine picks one.
#[derive(Debug, Clone, Deserialize)]
pub struct TileBins {
    #[serde(default)]
    pub width: Option<f64>,
}

/// How a tile orders its groups. `manual` is left to the UI.
#[derive(Debug, Clone, Deserialize)]
pub struct TileSort {
    pub by: SortBy,
    /// Biggest first for `value`, A to Z for `label`.
    #[serde(default)]
    pub desc: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    Value,
    Label,
    Manual,
}

/// Keep the biggest `n` groups and, unless `other` is false, add the rest as one "Other".
#[derive(Debug, Clone, Deserialize)]
pub struct TopN {
    pub n: u32,
    #[serde(default)]
    pub other: Option<bool>,
}

/// A cross-filter from a click on a tile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashFilter {
    #[serde(flatten)]
    pub filter: Filter,
    #[serde(rename = "fromTile")]
    pub from_tile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KpiInfo {
    pub value: Option<f64>,
    pub period: String,
    pub versus: Option<String>,
    pub prev: Option<f64>,
    pub delta: Option<f64>,
    pub partial: bool,
    pub bars: Vec<Bar>,
    /// The share of the period that has data, 0 to 1: days up to the last day with data, over
    /// the days of the period. 1 for a finished period. Absent for "All data".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bar {
    pub label: String,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Partial {
    pub key: String,
    pub until: String,
}

const MAX_CROSS_VALUES: usize = 30;
/// A pivot reads up to this many rows, and its row and column totals up to the same.
pub(crate) const PIVOT_LIMIT: u32 = 5000;
/// A pivot draws at most this many columns. Its cells are read for these columns only.
pub(crate) const PIVOT_MAX_COLS: usize = 30;
const PIVOT_MAX_ROWS: usize = 3;
const PIVOT_MAX_MEASURES: usize = 4;
/// The key of a pivot column click in the offline cross-filter map of an export.
const COL_PREFIX: &str = "\u{1}col:";

fn own_excluded(tile: &Tile, filters: &[DashFilter]) -> Vec<Filter> {
    filters
        .iter()
        .filter(|f| f.from_tile != tile.id)
        .map(|f| f.filter.clone())
        .collect()
}

fn count_rows(table: &str) -> Measure {
    Measure {
        table: table.to_string(),
        column: None,
        agg: Agg::Count,
        per: None,
    }
}

/// The question a tile asks; `None` when it has nothing to ask yet.
pub fn tile_query(tile: &Tile, filters: &[DashFilter]) -> Option<SemanticQuery> {
    let kind = tile.kind.as_str();
    if is_static(kind) || (tile.measures.is_empty() && tile.dims.is_empty()) {
        return None;
    }
    let one = matches!(kind, "kpi" | "share" | "donut");
    // Bar, line and area take up to four values side by side. Then there is no split.
    let many = matches!(kind, "bar" | "line" | "area") && tile.measures.len() > 1;
    let dims: Vec<Dimension> = if one || kind == "waterfall" {
        tile.dims.iter().take(1).cloned().collect()
    } else if kind == "histogram" {
        vec![]
    } else if kind == "table" {
        tile.dims.clone()
    } else if kind == "pivot" {
        // Rows first, then the column field.
        tile.dims
            .iter()
            .take(PIVOT_MAX_ROWS)
            .chain(tile.cols.iter())
            .cloned()
            .collect()
    } else if many {
        tile.dims.iter().take(1).cloned().collect()
    } else {
        tile.dims.iter().take(2).cloned().collect()
    };
    // A chart with a group but no value counts rows of the grouped table.
    let measures = if !tile.measures.is_empty() {
        if kind == "table" {
            tile.measures.clone()
        } else if kind == "pivot" {
            tile.measures
                .iter()
                .take(PIVOT_MAX_MEASURES)
                .cloned()
                .collect()
        } else if many {
            tile.measures
                .iter()
                .take(MAX_CHART_MEASURES)
                .cloned()
                .collect()
        } else if kind == "scatter" {
            // Across and up: two values for each dot.
            tile.measures.iter().take(2).cloned().collect()
        } else {
            vec![tile.measures[0].clone()]
        }
    } else if kind == "table" {
        vec![]
    } else {
        vec![count_rows(&dims.first().unwrap_or(&tile.dims[0]).table)]
    };
    // Output columns are the dimensions, then the measures.
    let mut sort = vec![];
    // A pivot keeps the engine's order: its rows and columns are not one list to sort.
    if let Some(s) = &tile.sort
        && !dims.is_empty()
        && kind != "pivot"
    {
        match s.by {
            SortBy::Value if !measures.is_empty() => sort.push(Sort {
                by: dims.len(),
                desc: s.desc.unwrap_or(true),
            }),
            SortBy::Label => sort.push(Sort {
                by: 0,
                desc: s.desc.unwrap_or(false),
            }),
            _ => {}
        }
    }
    Some(SemanticQuery {
        measures,
        dimensions: dims,
        filters: own_excluded(tile, filters),
        sort,
        limit: Some(match kind {
            "table" => 500,
            "pivot" => PIVOT_LIMIT,
            "kpi" => 400,
            _ => 2000,
        }),
    })
}

const MAX_CHART_MEASURES: usize = 4;

/// Runs one tile, with number-tile and unfinished-period extras.
pub fn run_tile(
    engine: &Engine,
    tile: &Tile,
    filters: &[DashFilter],
) -> anyhow::Result<QueryResult> {
    if tile.kind == "kpi" {
        return run_kpi(engine, tile, filters);
    }
    if tile.kind == "histogram" {
        return shape::histogram(engine, tile, &own_excluded(tile, filters));
    }
    let mut q =
        tile_query(tile, filters).ok_or_else(|| anyhow::anyhow!("the tile has no fields yet"))?;
    let wide = q.filters.clone();
    if tile.kind == "pivot"
        && let (Some(c), Some(m)) = (&tile.cols, q.measures.first())
    {
        // With many column values the cells are read for the first ones only, in the order of
        // the column totals. Otherwise the row limit could cut the data in the middle of a column.
        let all = engine.query(&SemanticQuery {
            measures: vec![m.clone()],
            dimensions: vec![c.clone()],
            filters: wide.clone(),
            sort: vec![],
            limit: Some(PIVOT_LIMIT),
        })?;
        if all.rows.len() > PIVOT_MAX_COLS {
            q.filters.push(Filter {
                table: c.table.clone(),
                column: c.column.clone(),
                grain: c.grain,
                op: FilterOp::In,
                values: all
                    .rows
                    .iter()
                    .take(PIVOT_MAX_COLS)
                    .filter_map(|r| r.first().cloned())
                    .collect(),
            });
        }
    }
    let mut res = engine.query(&q)?;
    // Fill gaps and cut to the top N first, so the unfinished-period check below
    // still sees the real last period.
    shape::apply(engine, tile, &q, &wide, &mut res)?;
    let d = match tile.dims.first() {
        Some(d)
            if d.grain.is_some_and(|g| !g.is_part())
                && matches!(tile.kind.as_str(), "bar" | "line" | "area" | "waterfall") =>
        {
            d
        }
        _ => return Ok(res),
    };
    // A time chart also asks where the data ends, so an unfinished last
    // period is drawn as unfinished instead of as a drop.
    let latest = engine.query(&SemanticQuery {
        measures: vec![Measure {
            table: d.table.clone(),
            column: Some(d.column.clone()),
            agg: Agg::Max,
            per: None,
        }],
        dimensions: vec![],
        filters: q.filters.clone(),
        sort: vec![],
        limit: None,
    })?;
    let grain = d.grain.unwrap();
    let last = res
        .rows
        .iter()
        .filter_map(|r| r.first().and_then(|v| v.as_str()))
        .max()
        .map(str::to_string);
    if let (Some(max_raw), Some(last)) = (first_str(&latest), last)
        && grain != Grain::Day
        && let (Some(start), Some(max_day)) = (parse_day(&last), parse_day(&max_raw))
    {
        let end = add_grain(start, grain, 1) - Duration::days(1);
        if max_day < end {
            res.partial = Some(Partial {
                key: last,
                until: max_raw[..10].to_string(),
            });
        }
    }
    Ok(res)
}

fn first_str(r: &QueryResult) -> Option<String> {
    r.rows.first()?.first()?.as_str().map(str::to_string)
}

fn first_num(r: &QueryResult) -> Option<f64> {
    let i = r.columns.iter().position(|c| c.role == "measure")?;
    r.rows.first()?.get(i)?.as_f64()
}

fn run_kpi(engine: &Engine, tile: &Tile, filters: &[DashFilter]) -> anyhow::Result<QueryResult> {
    let dim = tile.dims.first();
    // A number tile with only a date counts rows, like a chart with no value.
    let m = match (tile.measures.first(), dim) {
        (Some(m), _) => m.clone(),
        (None, Some(d)) => count_rows(&d.table),
        (None, None) => anyhow::bail!("the tile has no fields yet"),
    };
    let fs = own_excluded(tile, filters);
    let loc = engine.locale();
    let ask = |dims: Vec<Dimension>, filters: Vec<Filter>, limit: Option<u32>| {
        engine.query(&SemanticQuery {
            measures: vec![m.clone()],
            dimensions: dims,
            filters,
            sort: vec![],
            limit,
        })
    };
    let total = ask(vec![], fs.clone(), None)?;
    let mut res = total.clone();
    let plain = |value, period: &str| KpiInfo {
        value,
        period: period.into(),
        versus: None,
        prev: None,
        delta: None,
        partial: false,
        bars: vec![],
        elapsed: None,
    };

    // A number over time needs a date grouped by a grain; anything else is a plain total.
    let Some((d, g)) = dim.and_then(|d| d.grain.filter(|g| !g.is_part()).map(|g| (d, g))) else {
        res.kpi = Some(plain(first_num(&total), "All data"));
        return Ok(res);
    };
    let series = ask(vec![d.clone()], fs.clone(), Some(5000))?;
    let latest = engine.query(&SemanticQuery {
        measures: vec![Measure {
            table: d.table.clone(),
            column: Some(d.column.clone()),
            agg: Agg::Max,
            per: None,
        }],
        dimensions: vec![],
        filters: fs.clone(),
        sort: vec![],
        limit: None,
    })?;
    let mi = series
        .columns
        .iter()
        .position(|c| c.role == "measure")
        .unwrap_or(1);
    let rows: Vec<(String, Option<f64>)> = series
        .rows
        .iter()
        .filter_map(|r| {
            r.first()
                .and_then(|k| k.as_str())
                .map(|k| (k.to_string(), r.get(mi).and_then(|v| v.as_f64())))
        })
        .collect();
    let Some((last_key, value)) = rows.last().cloned() else {
        res.kpi = Some(plain(None, "No data"));
        return Ok(res);
    };
    let start = parse_day(&last_key).ok_or_else(|| anyhow::anyhow!("bad period {last_key}"))?;
    let end = add_grain(start, g, 1) - Duration::days(1);
    let max_raw = first_str(&latest);
    let max_day = max_raw.as_deref().and_then(parse_day).unwrap_or(end);
    let partial = max_day < end && g != Grain::Day;
    let prev_start = add_grain(start, g, -1);
    // Days with data over days in the period. The data runs up to its last day, so the days
    // between are counted even when no row falls on them (a quiet Sunday is not a missing day).
    let elapsed = if partial {
        let days = (end - start).num_days() + 1;
        (((max_day - start).num_days() + 1) as f64 / days as f64).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let (prev, period, versus) = if partial {
        // Compare the same number of days: Oct 1–14 against Sep 1–14.
        let days = (max_day - start).num_days();
        let prev_last = start - Duration::days(1);
        let prev_end = (prev_start + Duration::days(days)).min(prev_last);
        let datetime = series.columns.first().map(|c| c.dtype)
            == Some(crate::catalog::DType::DateTime)
            || max_raw.as_deref().is_some_and(|r| r.contains('T'));
        let hi = format!(
            "{}{}",
            iso(prev_end),
            if datetime { "T23:59:59" } else { "" }
        );
        let window = Filter {
            table: d.table.clone(),
            column: d.column.clone(),
            grain: None,
            op: FilterOp::Between,
            values: vec![json!(iso(prev_start)), json!(hi)],
        };
        let mut wfs = fs.clone();
        wfs.push(window);
        let prev = first_num(&ask(vec![], wfs, None)?);
        (
            prev,
            span(start, max_day, true, loc),
            span(prev_start, prev_end, false, loc),
        )
    } else {
        let prev = rows
            .iter()
            .find(|(k, _)| k.get(..10) == Some(iso(prev_start).as_str()))
            .and_then(|(_, v)| *v);
        (
            prev,
            period_name(start, g, false, loc),
            period_name(prev_start, g, false, loc),
        )
    };
    let bars = rows[rows.len().saturating_sub(12)..]
        .iter()
        .map(|(k, v)| Bar {
            label: parse_day(k)
                .map(|d| period_name(d, g, false, loc))
                .unwrap_or_else(|| k.clone()),
            value: *v,
        })
        .collect();
    let delta = match (value, prev) {
        (Some(v), Some(p)) if p != 0.0 => Some((v - p) / p.abs()),
        _ => None,
    };
    res.kpi = Some(KpiInfo {
        value,
        period,
        versus: Some(versus),
        prev,
        delta,
        partial,
        bars,
        elapsed: Some(elapsed),
    });
    Ok(res)
}

// ---- dates -------------------------------------------------------------------

pub(crate) fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

pub(crate) fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn add_grain(d: NaiveDate, g: Grain, n: i32) -> NaiveDate {
    let months = |k: i32| {
        if k >= 0 {
            d.checked_add_months(Months::new(k as u32))
        } else {
            d.checked_sub_months(Months::new((-k) as u32))
        }
        .unwrap_or(d)
    };
    match g {
        Grain::Day => d + Duration::days(n as i64),
        Grain::Week => d + Duration::days(7 * n as i64),
        Grain::Month => months(n),
        Grain::Quarter => months(3 * n),
        Grain::Year => months(12 * n),
        // A date part has no next period.
        Grain::MonthOfYear | Grain::Weekday | Grain::Hour => d,
    }
}

const MONTHS: [[&str; 12]; 2] = [
    [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    [
        "Januari",
        "Februari",
        "Maret",
        "April",
        "Mei",
        "Juni",
        "Juli",
        "Agustus",
        "September",
        "Oktober",
        "November",
        "Desember",
    ],
];

const SHORT_MONTHS: [[&str; 12]; 2] = [
    [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    [
        "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des",
    ],
];

fn table(loc: Locale) -> usize {
    match loc {
        Locale::En => 0,
        Locale::Id => 1,
    }
}

fn short(m: u32, loc: Locale) -> &'static str {
    SHORT_MONTHS[table(loc)][(m - 1) as usize]
}

/// A day and month in the order the locale reads them: "Oct 14" or "14 Okt".
fn day_month(m: u32, day: u32, loc: Locale) -> String {
    match loc {
        Locale::En => format!("{} {day}", short(m, loc)),
        Locale::Id => format!("{day} {}", short(m, loc)),
    }
}

/// English: "September 2026", "Q3 2026", "2026", "Week of Sep 7, 2026", "Sep 7, 2026".
/// Indonesian: "September 2026", "Q3 2026", "2026", "Minggu 7 Sep 2026", "7 Sep 2026".
pub fn period_name(d: NaiveDate, g: Grain, short_form: bool, loc: Locale) -> String {
    let (y, m, day) = (d.year(), d.month(), d.day());
    let full_date = match loc {
        Locale::En => format!("{} {day}, {y}", short(m, loc)),
        Locale::Id => format!("{day} {} {y}", short(m, loc)),
    };
    match g {
        Grain::Year => y.to_string(),
        Grain::Quarter => format!("Q{} {y}", (m - 1) / 3 + 1),
        Grain::Month if short_form => short(m, loc).to_string(),
        Grain::Month => format!("{} {y}", MONTHS[table(loc)][(m - 1) as usize]),
        Grain::Week if short_form => day_month(m, day, loc),
        Grain::Week => match loc {
            Locale::En => format!("Week of {full_date}"),
            Locale::Id => format!("Minggu {full_date}"),
        },
        Grain::Day => full_date,
        Grain::MonthOfYear | Grain::Weekday | Grain::Hour => short(m, loc).to_string(),
    }
}

/// English "Oct 1–14" (same month) or "Sep 28 – Oct 4"; Indonesian "1–14 Okt" or "28 Sep – 4 Okt".
fn span(a: NaiveDate, b: NaiveDate, with_year: bool, loc: Locale) -> String {
    let same = a.month() == b.month();
    let sep = if same { "–" } else { " – " };
    let year = match (with_year, loc) {
        (false, _) => String::new(),
        (true, Locale::En) => format!(", {}", b.year()),
        (true, Locale::Id) => format!(" {}", b.year()),
    };
    match loc {
        Locale::En => {
            let left = day_month(a.month(), a.day(), loc);
            let right = if same {
                b.day().to_string()
            } else {
                day_month(b.month(), b.day(), loc)
            };
            format!("{left}{sep}{right}{year}")
        }
        Locale::Id => {
            let left = if same {
                a.day().to_string()
            } else {
                day_month(a.month(), a.day(), loc)
            };
            format!("{left}{sep}{}{year}", day_month(b.month(), b.day(), loc))
        }
    }
}

// ---- snapshots ---------------------------------------------------------------

/// The key a category value has in the UI (`String(v)` in JavaScript).
pub fn js_key(v: &Value) -> String {
    match v {
        Value::Null => "\u{0}null".into(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{}", f as i64),
            Some(f) => format!("{f}"),
            None => n.to_string(),
        },
        other => other.to_string(),
    }
}

/// Builds the complete snapshot of a dashboard with the given cross-filters.
pub fn build(engine: &Engine, dashboard_id: &str, filters: &[DashFilter]) -> anyhow::Result<Value> {
    let dash = engine
        .dashboard(dashboard_id)
        .ok_or_else(|| anyhow::anyhow!("no dashboard {dashboard_id}"))?;
    let raw_tiles: Vec<Value> = dash
        .get("tiles")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    let mut tiles = Vec::new();
    let mut raw_kept = Vec::new();
    for raw in raw_tiles {
        let t: Tile = serde_json::from_value(raw.clone())?;
        // A text or image tile has no result, but the export and the viewer draw it.
        if is_static(&t.kind) || tile_query(&t, filters).is_some() {
            tiles.push(t);
            raw_kept.push(raw);
        }
    }

    let mut base: BTreeMap<String, QueryResult> = BTreeMap::new();
    for t in tiles.iter().filter(|t| !is_static(&t.kind)) {
        base.insert(t.id.clone(), run_tile(engine, t, filters)?);
    }

    // Offline cross-filtering: each clickable tile, each of its values, every other tile.
    let mut cross: BTreeMap<String, BTreeMap<String, BTreeMap<String, QueryResult>>> =
        BTreeMap::new();
    for src in &tiles {
        if src.kind == "kpi" {
            continue;
        }
        let Some(res) = base.get(&src.id) else {
            continue;
        };
        // What a click on each mark filters by: a group of the first dimension, or for a
        // histogram the range of the bin.
        let mut groups: Vec<Vec<(Value, Filter)>> = vec![];
        let choices: Vec<(Value, Filter)> = if src.kind == "histogram" {
            let (Some(m), Some(bins)) = (
                src.measures.first().filter(|m| m.column.is_some()),
                res.bins,
            ) else {
                continue;
            };
            res.rows
                .iter()
                .filter(|r| r.get(1).and_then(Value::as_f64).is_some_and(|n| n > 0.0))
                .filter_map(|r| {
                    let lo = r.first()?.as_f64()?;
                    Some((
                        r[0].clone(),
                        Filter {
                            table: m.table.clone(),
                            column: m.column.clone()?,
                            grain: None,
                            op: FilterOp::Range,
                            values: vec![r[0].clone(), shape::round_edge(lo + bins.width).into()],
                        },
                    ))
                })
                .collect()
        } else {
            let Some(d) = src.dims.first() else { continue };
            let mut seen = HashSet::new();
            res.rows
                .iter()
                .filter_map(|r| r.first().cloned())
                .filter(|v| seen.insert(js_key(v)))
                .map(|v| {
                    // The "Other" bar stands for every group that is not in the top N.
                    let (op, values) = match (&res.other_of, js_key(&v) == shape::OTHER_KEY) {
                        (Some(keys), true) => (FilterOp::NotIn, keys.clone()),
                        _ => (FilterOp::In, vec![v.clone()]),
                    };
                    (
                        v,
                        Filter {
                            table: d.table.clone(),
                            column: d.column.clone(),
                            grain: d.grain,
                            op,
                            values,
                        },
                    )
                })
                .collect()
        };
        groups.push(choices);
        // A click on a pivot column header filters by the column field. Its key starts with
        // `COL_PREFIX`, so it can never collide with a value of the first row field.
        if let (true, Some(c)) = (src.kind == "pivot", &src.cols) {
            let at = src.dims.len().min(PIVOT_MAX_ROWS);
            let mut seen = HashSet::new();
            groups.push(
                res.rows
                    .iter()
                    .filter_map(|r| r.get(at).cloned())
                    .filter(|v| seen.insert(js_key(v)))
                    .map(|v| {
                        (
                            Value::String(format!("{COL_PREFIX}{}", js_key(&v))),
                            Filter {
                                table: c.table.clone(),
                                column: c.column.clone(),
                                grain: c.grain,
                                op: FilterOp::In,
                                values: vec![v],
                            },
                        )
                    })
                    .collect(),
            );
        }
        let cap = if src.kind == "histogram" {
            shape::AUTO_MAX_BINS
        } else {
            MAX_CROSS_VALUES
        };
        for choices in groups {
            if choices.is_empty() || choices.len() > cap {
                continue;
            }
            let per_value = cross.entry(src.id.clone()).or_default();
            for (v, filter) in choices {
                let mut fs: Vec<DashFilter> = filters
                    .iter()
                    .filter(|f| f.from_tile != src.id)
                    .cloned()
                    .collect();
                fs.push(DashFilter {
                    filter,
                    from_tile: src.id.clone(),
                });
                let mut per_tile = BTreeMap::new();
                for t in tiles
                    .iter()
                    .filter(|t| t.id != src.id && !is_static(&t.kind))
                {
                    per_tile.insert(t.id.clone(), run_tile(engine, t, &fs)?);
                }
                per_value.insert(js_key(&v), per_tile);
            }
        }
    }

    let used: HashSet<&str> = tiles
        .iter()
        .flat_map(|t| {
            t.measures
                .iter()
                .flat_map(|m| {
                    std::iter::once(m.table.as_str())
                        .chain(m.per.as_ref().map(|p| p.table.as_str()))
                })
                .chain(t.dims.iter().map(|d| d.table.as_str()))
        })
        .collect();
    let tables: Vec<Value> = engine
        .catalog()
        .tables
        .iter()
        .filter(|t| used.contains(t.id.as_str()))
        .map(|t| json!({ "id": t.id, "name": t.name, "label": t.label, "columns": t.columns }))
        .collect();
    Ok(json!({
        "version": 2,
        "title": dash.get("title").cloned().unwrap_or(json!("Dashboard")),
        "workspace": engine.workspace_name(),
        "locale": engine.locale(),
        "generated": chrono_now(),
        "tables": tables,
        "tiles": raw_kept,
        "base": base,
        "cross": cross,
        "filters": filters,
    }))
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    chrono::DateTime::from_timestamp(now.as_secs() as i64, now.subsec_nanos())
        .map(|t| t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: Locale = Locale::En;
    const ID: Locale = Locale::Id;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn names_and_spans() {
        assert_eq!(
            period_name(d("2026-09-01"), Grain::Month, false, EN),
            "September 2026"
        );
        assert_eq!(
            period_name(d("2026-07-01"), Grain::Quarter, false, EN),
            "Q3 2026"
        );
        assert_eq!(
            period_name(d("2026-09-07"), Grain::Week, false, EN),
            "Week of Sep 7, 2026"
        );
        assert_eq!(
            span(d("2026-10-01"), d("2026-10-14"), true, EN),
            "Oct 1–14, 2026"
        );
        assert_eq!(
            span(d("2026-09-28"), d("2026-10-04"), false, EN),
            "Sep 28 – Oct 4"
        );
    }

    #[test]
    fn indonesian_names_and_spans() {
        assert_eq!(
            period_name(d("2026-09-01"), Grain::Month, false, ID),
            "September 2026"
        );
        assert_eq!(
            period_name(d("2026-10-01"), Grain::Month, false, ID),
            "Oktober 2026"
        );
        assert_eq!(period_name(d("2026-08-01"), Grain::Month, true, ID), "Agu");
        assert_eq!(
            period_name(d("2026-07-01"), Grain::Quarter, false, ID),
            "Q3 2026"
        );
        assert_eq!(
            period_name(d("2026-09-07"), Grain::Week, false, ID),
            "Minggu 7 Sep 2026"
        );
        assert_eq!(
            period_name(d("2026-10-14"), Grain::Day, false, ID),
            "14 Okt 2026"
        );
        assert_eq!(
            span(d("2026-10-01"), d("2026-10-14"), true, ID),
            "1–14 Okt 2026"
        );
        assert_eq!(
            span(d("2026-09-28"), d("2026-10-04"), false, ID),
            "28 Sep – 4 Okt"
        );
        assert_eq!(
            span(d("2026-09-28"), d("2026-10-04"), true, ID),
            "28 Sep – 4 Okt 2026"
        );
    }

    #[test]
    fn grain_arithmetic() {
        assert_eq!(
            add_grain(d("2026-10-01"), Grain::Month, -1),
            d("2026-09-01")
        );
        assert_eq!(
            add_grain(d("2026-01-01"), Grain::Quarter, -1),
            d("2025-10-01")
        );
        assert_eq!(add_grain(d("2026-09-07"), Grain::Week, 1), d("2026-09-14"));
    }

    #[test]
    fn keys_match_javascript() {
        assert_eq!(js_key(&json!(1.0)), "1");
        assert_eq!(js_key(&json!(1.5)), "1.5");
        assert_eq!(js_key(&json!("a")), "a");
        assert_eq!(js_key(&Value::Null), "\u{0}null");
        assert_eq!(js_key(&json!(true)), "true");
    }
}
