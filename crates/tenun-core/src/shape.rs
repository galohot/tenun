//! Shapes a tile's result after the main query: fills missing periods and keeps
//! the top N groups with one "Other" row for the rest. The engine computes these
//! numbers once, so the app, exported files, published dashboards and the
//! headless agent all agree. The UI only draws what comes back.

use crate::engine::{Engine, QueryResult};
use crate::query::{Agg, Dimension, Filter, FilterOp, Grain, SemanticQuery, Sort};
use crate::snapshot::{SortBy, Tile, add_grain, iso, js_key, parse_day};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// The key of the "Other" group in the first column of a result.
pub const OTHER_KEY: &str = "\u{0}other";

/// A filled result never grows past this many rows.
const MAX_ROWS: usize = 5000;
/// "Top N" never keeps more than this many groups.
const MAX_TOP: u32 = 1000;

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
    if !matches!(tile.kind.as_str(), "bar" | "line" | "area") {
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
