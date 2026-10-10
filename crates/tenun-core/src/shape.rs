//! Shapes a tile's result after the main query: fills missing periods and keeps
//! the top N groups with one "Other" row for the rest. The engine computes these
//! numbers once, so the app, exported files, published dashboards and the
//! headless agent all agree. The UI only draws what comes back.
//!
//! Histograms live here too: their bin width and counts are computed in one place.

use crate::catalog::DType;
use crate::engine::{Bins, Engine, QueryResult};
use crate::query::{
    Agg, Dimension, Filter, FilterOp, Grain, Measure, OutColumn, Param, SemanticQuery, Sort,
};
use crate::snapshot::{SortBy, Tile, add_grain, iso, js_key, parse_day};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// The key of the "Other" group in the first column of a result.
pub const OTHER_KEY: &str = "\u{0}other";

/// A filled result never grows past this many rows.
const MAX_ROWS: usize = 5000;
/// "Top N" never keeps more than this many groups.
const MAX_TOP: u32 = 1000;
/// A histogram the engine sizes has at least this many bins, and at most `AUTO_MAX_BINS`.
const MIN_BINS: u32 = 5;
pub const AUTO_MAX_BINS: usize = 60;
/// A width typed by the user never makes more bins than this.
const MAX_BINS: u32 = 200;

/// Fills gaps, then applies top N. Called by `snapshot::run_tile`.
pub fn apply(
    engine: &Engine,
    tile: &Tile,
    q: &SemanticQuery,
    res: &mut QueryResult,
) -> anyhow::Result<()> {
    densify(engine, tile, q, res);
    top_n(engine, tile, q, res)
}

/// What the first group-by column is.
enum Dim0 {
    /// A date cut into periods (months, weeks…).
    Period(Grain),
    /// A date part: month of year, weekday or hour.
    Part(Grain),
    /// A date with no grain.
    Time,
    Plain,
}

fn classify(engine: &Engine, d: &Dimension) -> Dim0 {
    let temporal = engine
        .column_dtype(&d.table, &d.column)
        .is_some_and(|t| t.is_temporal());
    match d.grain {
        Some(g) if temporal && g.is_part() => Dim0::Part(g),
        Some(g) if temporal => Dim0::Period(g),
        _ if temporal => Dim0::Time,
        _ => Dim0::Plain,
    }
}

fn part_range(g: Grain) -> (i32, i32) {
    match g {
        Grain::MonthOfYear => (1, 12),
        Grain::Weekday => (1, 7),
        _ => (0, 23),
    }
}

/// Fills every missing period of a time chart.
///
/// A missing period is 0 for sums and counts (nothing happened) and a gap for
/// every other aggregate (an average of nothing is not zero). With a split, each
/// split value that exists in some period is filled in every period. Date parts
/// fill their whole range. A result that would pass 5,000 rows is left alone, and
/// so is a chart sorted by value, because its groups are not in time order.
pub fn densify(engine: &Engine, tile: &Tile, q: &SemanticQuery, res: &mut QueryResult) {
    if !matches!(tile.kind.as_str(), "bar" | "line" | "area" | "waterfall") {
        return;
    }
    let sort = tile.sort.as_ref();
    if sort.is_some_and(|s| s.by == SortBy::Value) {
        return;
    }
    let desc = sort.is_some_and(|s| s.by == SortBy::Label && s.desc == Some(true));
    let Some(d0) = q.dimensions.first() else {
        return;
    };
    let nd = q.dimensions.len();
    let width = nd + q.measures.len();
    if nd > 2 || res.rows.iter().any(|r| r.len() != width) {
        return;
    }

    // The periods, in time order.
    let mut periods: Vec<Value> = Vec::new();
    match classify(engine, d0) {
        Dim0::Period(g) => {
            let days: Option<Vec<_>> = res
                .rows
                .iter()
                .filter_map(|r| r[0].as_str())
                .map(parse_day)
                .collect();
            let Some(days) = days else { return };
            let (Some(&first), Some(&last)) = (days.iter().min(), days.iter().max()) else {
                return;
            };
            let mut at = first;
            while at <= last {
                if periods.len() >= MAX_ROWS {
                    return;
                }
                periods.push(Value::String(iso(at)));
                let next = add_grain(at, g, 1);
                if next <= at {
                    return;
                }
                at = next;
            }
        }
        Dim0::Part(g) => {
            let (lo, hi) = part_range(g);
            periods.extend((lo..=hi).map(|i| json!(f64::from(i))));
        }
        _ => return,
    }

    // The split values that exist anywhere, in the order they first appear.
    let mut splits: Vec<(String, Value)> = Vec::new();
    if nd == 2 {
        let mut seen = HashSet::new();
        for r in &res.rows {
            if seen.insert(js_key(&r[1])) {
                splits.push((js_key(&r[1]), r[1].clone()));
            }
        }
    } else {
        splits.push((String::new(), Value::Null));
    }
    if periods.len() * splits.len() > MAX_ROWS {
        return;
    }

    // A blank date stays its own group, after the periods.
    let (blank, dated): (Vec<_>, Vec<_>) = res.rows.drain(..).partition(|r| r[0].is_null());
    let mut by_period: HashMap<String, Vec<Vec<Value>>> = HashMap::new();
    for r in dated {
        by_period.entry(js_key(&r[0])).or_default().push(r);
    }
    let fill: Vec<Value> = q
        .measures
        .iter()
        .map(|m| match m.agg {
            Agg::Sum | Agg::Count | Agg::CountDistinct => json!(0.0),
            _ => Value::Null,
        })
        .collect();

    let mut out = Vec::new();
    if desc {
        periods.reverse();
    }
    for p in periods {
        let mut rows = by_period.remove(&js_key(&p)).unwrap_or_default();
        let have: HashSet<String> = if nd == 2 {
            rows.iter().map(|r| js_key(&r[1])).collect()
        } else {
            HashSet::new()
        };
        for (k, v) in &splits {
            if nd == 2 && have.contains(k) {
                continue;
            }
            if nd == 1 && !rows.is_empty() {
                continue;
            }
            let mut r = vec![p.clone()];
            if nd == 2 {
                r.push(v.clone());
            }
            r.extend(fill.iter().cloned());
            rows.push(r);
        }
        out.append(&mut rows);
    }
    out.extend(blank);
    res.rows = out;
}

/// Keeps the top `n` groups of the first column, ranked by the first value, and
/// adds the rest as one "Other" group (key [`OTHER_KEY`]), always last.
///
/// The top groups come from a second query grouped by the first column alone, so
/// the ranking is right for averages, minimums and maximums too. "Other" is a
/// third query over everything outside the top groups, so it is never a sum of
/// averages. A date in the first column ignores `top`.
pub fn top_n(
    engine: &Engine,
    tile: &Tile,
    q: &SemanticQuery,
    res: &mut QueryResult,
) -> anyhow::Result<()> {
    let Some(top) = &tile.top else {
        return Ok(());
    };
    if !matches!(
        tile.kind.as_str(),
        "bar" | "table" | "share" | "donut" | "pivot"
    ) {
        return Ok(());
    }
    let (Some(d0), Some(m0)) = (q.dimensions.first(), q.measures.first()) else {
        return Ok(());
    };
    if !matches!(classify(engine, d0), Dim0::Plain) {
        return Ok(());
    }
    let n = top.n.clamp(1, MAX_TOP) as usize;
    let mut seen = HashSet::new();
    let distinct = res
        .rows
        .iter()
        .filter(|r| seen.insert(js_key(&r[0])))
        .count();
    if distinct <= n {
        return Ok(());
    }

    let ranked = engine.query(&SemanticQuery {
        measures: vec![m0.clone()],
        dimensions: vec![d0.clone()],
        filters: q.filters.clone(),
        sort: vec![Sort { by: 1, desc: true }],
        limit: Some(n as u32),
    })?;
    let keys: Vec<Value> = ranked
        .rows
        .iter()
        .filter_map(|r| r.first().cloned())
        .collect();
    let kept: HashSet<String> = keys.iter().map(js_key).collect();
    res.rows.retain(|r| kept.contains(&js_key(&r[0])));

    if top.other == Some(false) {
        return Ok(());
    }
    let mut filters = q.filters.clone();
    filters.push(Filter {
        table: d0.table.clone(),
        column: d0.column.clone(),
        grain: d0.grain,
        op: FilterOp::NotIn,
        values: keys.clone(),
    });
    let rest = engine.query(&SemanticQuery {
        measures: q.measures.clone(),
        dimensions: q.dimensions[1..].to_vec(),
        filters,
        sort: vec![],
        limit: q.limit,
    })?;
    for r in rest.rows {
        let mut row = vec![Value::String(OTHER_KEY.into())];
        row.extend(r);
        res.rows.push(row);
    }
    res.other = Some(true);
    res.other_of = Some(keys);
    Ok(())
}

// ---- histogram ---------------------------------------------------------------

/// The nice numbers a bin width can be: 1, 2, 2.5 or 5 times a power of ten.
const NICE: [f64; 4] = [1.0, 2.0, 2.5, 5.0];

/// The smallest nice number that is at least `x`.
fn nice_up(x: f64) -> f64 {
    let base = 10f64.powf(x.log10().floor());
    for k in [0.0, 1.0] {
        let b = base * 10f64.powf(k);
        if let Some(n) = NICE.iter().map(|n| n * b).find(|n| *n >= x * (1.0 - 1e-9)) {
            return n;
        }
    }
    base * 10.0
}

/// The biggest nice number that is below `x`.
fn nice_down(x: f64) -> f64 {
    let base = 10f64.powf(x.log10().floor());
    [0.0, -1.0]
        .iter()
        .flat_map(|k| NICE.iter().rev().map(move |n| n * base * 10f64.powf(*k)))
        .find(|n| *n < x * (1.0 - 1e-9))
        .unwrap_or(x / 2.0)
}

/// How many bins of `width` cover `min` to `max`, with the first bin starting on a multiple of `width`.
fn bin_count(min: f64, max: f64, width: f64) -> u32 {
    let start = (min / width).floor() * width;
    (((max - start) / width).floor() + 1.0).clamp(1.0, f64::from(u32::MAX)) as u32
}

/// The width of a histogram bin: Freedman–Diaconis `2·IQR·n^(−1/3)`, rounded up to a nice
/// number, then moved until there are 5 to 60 bins. With no spread (IQR 0) it falls back to
/// Sturges' rule. All values equal gives width 1 and one bin.
pub fn bin_width(n: f64, min: f64, max: f64, q1: f64, q3: f64) -> f64 {
    let range = max - min;
    if range.is_nan() || range <= 0.0 || n < 1.0 {
        return 1.0;
    }
    let mut raw = 2.0 * (q3 - q1) * n.powf(-1.0 / 3.0);
    if raw.is_nan() || raw <= 0.0 {
        raw = range / (n.log2().ceil() + 1.0);
    }
    let mut w = nice_up(raw.clamp(range / AUTO_MAX_BINS as f64, range / f64::from(MIN_BINS)));
    for _ in 0..50 {
        if bin_count(min, max, w) >= MIN_BINS {
            break;
        }
        w = nice_down(w);
    }
    for _ in 0..50 {
        if bin_count(min, max, w) as usize <= AUTO_MAX_BINS {
            break;
        }
        w = nice_up(w * 1.001);
    }
    w
}

/// Rounds an edge to 10 decimals, so 0.1 × 3 reads as 0.3 and not 0.30000000000000004.
pub fn round_edge(v: f64) -> f64 {
    format!("{v:.10}").parse().unwrap_or(v)
}

/// Counts a number column into equal bins. Rows are `[bin_start, count]` for every bin
/// from the first to the last, empty ones included, so the bars touch. `res.bins` has the
/// start and the width. `tile.bins.width` replaces the automatic width.
pub fn histogram(engine: &Engine, tile: &Tile, filters: &[Filter]) -> anyhow::Result<QueryResult> {
    let m = tile
        .measures
        .first()
        .filter(|m| m.column.is_some())
        .ok_or_else(|| anyhow::anyhow!("Pick a number column for the histogram."))?;
    let column = m.column.clone().unwrap_or_default();
    if engine.column_dtype(&m.table, &column) != Some(DType::Number) {
        anyhow::bail!("{column} is not a number column. A histogram needs one.");
    }
    let q = SemanticQuery {
        measures: vec![Measure {
            table: m.table.clone(),
            column: Some(column.clone()),
            agg: Agg::Count,
        }],
        dimensions: vec![],
        filters: filters.to_vec(),
        sort: vec![],
        limit: None,
    };
    let start_time = std::time::Instant::now();
    let (stats, ignored) = engine.query_custom(&q, |sc| {
        let (x, _) = sc.expr(&m.table, &column, None)?;
        Ok((
            format!(
                "SELECT count({x}), min({x}), max({x}), quantile_cont({x}, 0.25), \
                 quantile_cont({x}, 0.75){}",
                sc.from_where
            ),
            sc.params.clone(),
        ))
    })?;
    let num = |i: usize| stats.first().and_then(|r| r.get(i)).and_then(Value::as_f64);
    let columns = vec![
        OutColumn {
            name: column.clone(),
            role: "dimension".into(),
            dtype: DType::Number,
        },
        OutColumn {
            name: "Rows".into(),
            role: "measure".into(),
            dtype: DType::Number,
        },
    ];
    let mut res = QueryResult {
        columns,
        rows: vec![],
        ignored_filters: ignored,
        ms: 0,
        kpi: None,
        partial: None,
        other: None,
        other_of: None,
        bins: None,
    };
    let (Some(n), Some(min), Some(max), Some(q1), Some(q3)) =
        (num(0), num(1), num(2), num(3), num(4))
    else {
        return Ok(res);
    };
    if n < 1.0 {
        return Ok(res);
    }
    let mut width = match tile.bins.as_ref().and_then(|b| b.width) {
        Some(w) if w.is_finite() && w > 0.0 => w,
        _ => bin_width(n, min, max, q1, q3),
    };
    // A typed width that would make too many bars is made wider.
    if bin_count(min, max, width) > MAX_BINS {
        width = nice_up((max - min) / f64::from(MAX_BINS));
    }
    let start = round_edge((min / width).floor() * width);
    let count = bin_count(min, max, width) as usize;

    let (found, _) = engine.query_custom(&q, |sc| {
        let (x, _) = sc.expr(&m.table, &column, None)?;
        let mut params = vec![Param::Double(start), Param::Double(width)];
        params.extend(sc.params.iter().cloned());
        Ok((
            format!(
                "SELECT b, count(*) FROM (SELECT CAST(floor(({x} - CAST(? AS DOUBLE)) / \
                 CAST(? AS DOUBLE)) AS BIGINT) AS b{}) WHERE b IS NOT NULL GROUP BY b ORDER BY b",
                sc.from_where
            ),
            params,
        ))
    })?;
    let mut counts = vec![0.0; count];
    for r in &found {
        if let (Some(b), Some(c)) = (r[0].as_i64(), r[1].as_f64())
            && let Some(slot) = usize::try_from(b).ok().and_then(|b| counts.get_mut(b))
        {
            *slot += c;
        }
    }
    res.rows = counts
        .iter()
        .enumerate()
        .map(|(i, c)| vec![json!(round_edge(start + i as f64 * width)), json!(c)])
        .collect();
    res.bins = Some(Bins { start, width });
    res.ms = start_time.elapsed().as_millis();
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_numbers() {
        assert_eq!(nice_up(0.9), 1.0);
        assert_eq!(nice_up(1.0), 1.0);
        assert_eq!(nice_up(1.1), 2.0);
        assert_eq!(nice_up(2.2), 2.5);
        assert_eq!(nice_up(3.0), 5.0);
        assert_eq!(nice_up(5.1), 10.0);
        assert_eq!(nice_up(0.03), 0.05);
        assert_eq!(nice_up(240.0), 250.0);
        assert_eq!(nice_down(10.0), 5.0);
        assert_eq!(nice_down(2.5), 2.0);
        assert_eq!(nice_down(1.0), 0.5);
    }

    #[test]
    fn width_follows_freedman_diaconis() {
        // n = 1000, IQR = 25: 2 · 25 · 1000^(−1/3) = 5, a nice number already.
        assert_eq!(bin_width(1000.0, 0.0, 250.0, 50.0, 75.0), 5.0);
        // IQR = 40 gives 8, which is not nice: it rounds up to 10.
        assert_eq!(bin_width(1000.0, 0.0, 400.0, 100.0, 140.0), 10.0);
    }

    #[test]
    fn width_keeps_5_to_60_bins() {
        // A huge spread with a tiny IQR would give thousands of bins.
        let w = bin_width(100_000.0, 0.0, 1_000_000.0, 100.0, 200.0);
        assert!(bin_count(0.0, 1_000_000.0, w) <= 60, "{w}");
        // A tiny range with a wide IQR would give one or two bins.
        let w = bin_width(8.0, 0.0, 100.0, 0.0, 100.0);
        assert!(bin_count(0.0, 100.0, w) >= 5, "{w}");
        // All values equal: one bin.
        assert_eq!(bin_width(10.0, 7.0, 7.0, 7.0, 7.0), 1.0);
        // No IQR but a range falls back to Sturges' rule.
        let w = bin_width(100.0, 0.0, 100.0, 5.0, 5.0);
        assert!((5..=60).contains(&bin_count(0.0, 100.0, w)), "{w}");
    }
}
