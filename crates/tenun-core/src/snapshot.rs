//! Tiles and snapshots: what each tile asks, how a number tile compares
//! periods fairly, how a time chart marks an unfinished last period, and the
//! complete snapshot of a dashboard (every tile, plus the results needed to
//! cross-filter offline). One implementation for live tiles, exported files,
//! published dashboards and the headless agent.

use crate::engine::{Engine, QueryResult};
use crate::query::{Agg, Dimension, Filter, FilterOp, Grain, Measure, SemanticQuery};
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
    }
}

/// The question a tile asks; `None` when it has nothing to ask yet.
pub fn tile_query(tile: &Tile, filters: &[DashFilter]) -> Option<SemanticQuery> {
    if tile.measures.is_empty() && tile.dims.is_empty() {
        return None;
    }
    let kind = tile.kind.as_str();
    let one = matches!(kind, "kpi" | "share" | "donut");
    let dims: Vec<Dimension> = if one {
        tile.dims.iter().take(1).cloned().collect()
    } else if kind == "table" {
        tile.dims.clone()
    } else {
        tile.dims.iter().take(2).cloned().collect()
    };
    // A chart with a group but no value counts rows of the grouped table.
    let measures = if !tile.measures.is_empty() {
        if kind == "table" {
            tile.measures.clone()
        } else {
            vec![tile.measures[0].clone()]
        }
    } else if kind == "table" {
        vec![]
    } else {
        vec![count_rows(&tile.dims[0].table)]
    };
    Some(SemanticQuery {
        measures,
        dimensions: dims,
        filters: own_excluded(tile, filters),
        sort: vec![],
        limit: Some(match kind {
            "table" => 500,
            "kpi" => 400,
            _ => 2000,
        }),
    })
}

/// Runs one tile, with number-tile and unfinished-period extras.
pub fn run_tile(
    engine: &Engine,
    tile: &Tile,
    filters: &[DashFilter],
) -> anyhow::Result<QueryResult> {
    if tile.kind == "kpi" {
        return run_kpi(engine, tile, filters);
    }
    let q =
        tile_query(tile, filters).ok_or_else(|| anyhow::anyhow!("the tile has no fields yet"))?;
    let mut res = engine.query(&q)?;
    let d = match tile.dims.first() {
        Some(d) if d.grain.is_some() && matches!(tile.kind.as_str(), "bar" | "line" | "area") => d,
        _ => return Ok(res),
    };
    // A time chart also asks where the data ends, so an unfinished last
    // period is drawn as unfinished instead of as a drop.
    let latest = engine.query(&SemanticQuery {
        measures: vec![Measure {
            table: d.table.clone(),
            column: Some(d.column.clone()),
            agg: Agg::Max,
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
    };

    // A number over time needs a date grouped by a grain; anything else is a plain total.
    let Some((d, g)) = dim.and_then(|d| d.grain.map(|g| (d, g))) else {
        res.kpi = Some(plain(first_num(&total), "All data"));
        return Ok(res);
    };
    let series = ask(vec![d.clone()], fs.clone(), Some(5000))?;
    let latest = engine.query(&SemanticQuery {
        measures: vec![Measure {
            table: d.table.clone(),
            column: Some(d.column.clone()),
            agg: Agg::Max,
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
            span(start, max_day, true),
            span(prev_start, prev_end, false),
        )
    } else {
        let prev = rows
            .iter()
            .find(|(k, _)| k.get(..10) == Some(iso(prev_start).as_str()))
            .and_then(|(_, v)| *v);
        (
            prev,
            period_name(start, g, false),
            period_name(prev_start, g, false),
        )
    };
    let bars = rows[rows.len().saturating_sub(12)..]
        .iter()
        .map(|(k, v)| Bar {
            label: parse_day(k)
                .map(|d| period_name(d, g, false))
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
    });
    Ok(res)
}

// ---- dates -------------------------------------------------------------------

fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

fn iso(d: NaiveDate) -> String {
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
    }
}

const MONTHS: [&str; 12] = [
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
];

fn short(m: u32) -> &'static str {
    &MONTHS[(m - 1) as usize][..3]
}

/// "September 2026", "Q3 2026", "2026", "Week of Sep 7, 2026", "Sep 7, 2026".
pub fn period_name(d: NaiveDate, g: Grain, short_form: bool) -> String {
    let (y, m, day) = (d.year(), d.month(), d.day());
    match g {
        Grain::Year => y.to_string(),
        Grain::Quarter => format!("Q{} {y}", (m - 1) / 3 + 1),
        Grain::Month if short_form => short(m).to_string(),
        Grain::Month => format!("{} {y}", MONTHS[(m - 1) as usize]),
        Grain::Week if short_form => format!("{} {day}", short(m)),
        Grain::Week => format!("Week of {} {day}, {y}", short(m)),
        Grain::Day => format!("{} {day}, {y}", short(m)),
    }
}

/// "Oct 1–14" (same month) or "Sep 28 – Oct 4".
fn span(a: NaiveDate, b: NaiveDate, with_year: bool) -> String {
    let left = format!("{} {}", short(a.month()), a.day());
    let same = a.month() == b.month();
    let right = if same {
        b.day().to_string()
    } else {
        format!("{} {}", short(b.month()), b.day())
    };
    let year = if with_year {
        format!(", {}", b.year())
    } else {
        String::new()
    };
    format!("{left}{}{right}{year}", if same { "–" } else { " – " })
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
        if tile_query(&t, filters).is_some() {
            tiles.push(t);
            raw_kept.push(raw);
        }
    }

    let mut base: BTreeMap<String, QueryResult> = BTreeMap::new();
    for t in &tiles {
        base.insert(t.id.clone(), run_tile(engine, t, filters)?);
    }

    // Offline cross-filtering: each clickable tile, each of its values, every other tile.
    let mut cross: BTreeMap<String, BTreeMap<String, BTreeMap<String, QueryResult>>> =
        BTreeMap::new();
    for src in &tiles {
        let Some(d) = src.dims.first() else { continue };
        if src.kind == "kpi" {
            continue;
        }
        let Some(res) = base.get(&src.id) else {
            continue;
        };
        let mut seen = HashSet::new();
        let values: Vec<Value> = res
            .rows
            .iter()
            .filter_map(|r| r.first().cloned())
            .filter(|v| seen.insert(js_key(v)))
            .collect();
        if values.is_empty() || values.len() > MAX_CROSS_VALUES {
            continue;
        }
        let per_value = cross.entry(src.id.clone()).or_default();
        for v in values {
            let mut fs: Vec<DashFilter> = filters
                .iter()
                .filter(|f| f.from_tile != src.id)
                .cloned()
                .collect();
            fs.push(DashFilter {
                filter: Filter {
                    table: d.table.clone(),
                    column: d.column.clone(),
                    grain: d.grain,
                    op: FilterOp::In,
                    values: vec![v.clone()],
                },
                from_tile: src.id.clone(),
            });
            let mut per_tile = BTreeMap::new();
            for t in tiles.iter().filter(|t| t.id != src.id) {
                per_tile.insert(t.id.clone(), run_tile(engine, t, &fs)?);
            }
            per_value.insert(js_key(&v), per_tile);
        }
    }

    let used: HashSet<&str> = tiles
        .iter()
        .flat_map(|t| {
            t.measures
                .iter()
                .map(|m| m.table.as_str())
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

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn names_and_spans() {
        assert_eq!(
            period_name(d("2026-09-01"), Grain::Month, false),
            "September 2026"
        );
        assert_eq!(
            period_name(d("2026-07-01"), Grain::Quarter, false),
            "Q3 2026"
        );
        assert_eq!(
            period_name(d("2026-09-07"), Grain::Week, false),
            "Week of Sep 7, 2026"
        );
        assert_eq!(
            span(d("2026-10-01"), d("2026-10-14"), true),
            "Oct 1–14, 2026"
        );
        assert_eq!(
            span(d("2026-09-28"), d("2026-10-04"), false),
            "Sep 28 – Oct 4"
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
