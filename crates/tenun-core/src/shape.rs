//! Shapes a tile's result after the main query: fills missing periods, keeps
//! the top N groups with one "Other" row for the rest, shows a value another way
//! (share, running total, change, rank…) and adds the series of "same period last
//! year". The engine computes these numbers once, so the app, exported files,
//! published dashboards and the headless agent all agree. The UI only draws what
//! comes back.
//!
//! Histograms live here too: their bin width and counts are computed in one place.

use crate::advanced;
use crate::catalog::DType;
use crate::engine::{Bins, Compare, Engine, QueryResult, Totals};
use crate::query::{
    Agg, Dimension, Filter, FilterOp, Grain, Measure, OutColumn, Param, PartNote, SemanticQuery,
    Sort, Unit,
};
use crate::snapshot::{SortBy, Tile, add_grain, iso, js_key, parse_day};
use crate::workspace::Locale;
use chrono::{Datelike, Duration, NaiveDate};
use serde::Deserialize;
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

/// Fills gaps, applies top N, adds totals, reads the comparison series and shows values another
/// way. Called by `snapshot::run_tile`.
///
/// `wide` holds the filters of the whole question. It differs from `q.filters` only for a pivot
/// with many columns, where the cell query reads just the columns on show. The ranking of a top N
/// and every total still cover all the data.
pub fn apply(
    engine: &Engine,
    tile: &Tile,
    q: &SemanticQuery,
    wide: &[Filter],
    res: &mut QueryResult,
) -> anyhow::Result<()> {
    densify(engine, tile, q, res);
    top_n(engine, tile, q, wide, res)?;
    advanced::manual_order(engine, tile, q, res);
    totals(engine, tile, q, wide, res)?;
    compare(engine, tile, q, res)?;
    show_as(tile, q, engine, res);
    advanced::indexed(engine, tile, q, res);
    total_rows(engine, tile, q, res)?;
    Ok(())
}

/// How several values (or the groups of a split) share a chart (`tile.layout`). `panels` is
/// drawn by the UI from the normal result; `indexed` is computed here ([`advanced::indexed`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Layout {
    #[serde(rename = "together")]
    Together,
    #[serde(rename = "panels")]
    Panels,
    #[serde(rename = "indexed")]
    Indexed,
    #[serde(other)]
    Unknown,
}

/// A table that hit its row limit says how many rows it has in all, so the footer can read
/// "Rows 1–500 of 12,345". One count over the same groups; nothing is read when the limit did not
/// cut the result, and a top N already decided the rows on show.
fn total_rows(
    engine: &Engine,
    tile: &Tile,
    q: &SemanticQuery,
    res: &mut QueryResult,
) -> anyhow::Result<()> {
    let cut = q.limit.is_some_and(|l| res.rows.len() >= l as usize);
    if tile.kind != "table" || q.dimensions.is_empty() || tile.top.is_some() || !cut {
        return Ok(());
    }
    // A named measure has no one table to count the groups from: ask the question again, cut at the
    // most any question returns.
    if q.measures.iter().any(|m| m.named.is_some()) {
        let all = engine.query(&SemanticQuery {
            measures: q.measures.clone(),
            dimensions: q.dimensions.clone(),
            filters: q.filters.clone(),
            sort: vec![],
            limit: Some(10_000),
        })?;
        res.total_rows = Some(all.rows.len() as u64).filter(|n| *n > res.rows.len() as u64);
        return Ok(());
    }
    let (found, _) = engine.query_custom(q, |sc| {
        let mut exprs = Vec::new();
        for d in &q.dimensions {
            exprs.push(
                sc.expr_via(&d.table, d.via.as_deref(), &d.column, d.grain)?
                    .0,
            );
        }
        let n = exprs.len();
        let select: Vec<String> = exprs
            .iter()
            .enumerate()
            .map(|(i, e)| format!("{e} AS d{i}"))
            .collect();
        let group: Vec<String> = (1..=n).map(|i| i.to_string()).collect();
        Ok((
            format!(
                "SELECT count(*) FROM (SELECT {}{} GROUP BY {})",
                select.join(", "),
                sc.from_where,
                group.join(", ")
            ),
            sc.params.clone(),
        ))
    })?;
    res.total_rows = found
        .first()
        .and_then(|r| r.first())
        .and_then(Value::as_u64)
        .filter(|n| *n > res.rows.len() as u64);
    Ok(())
}

/// What the first group-by column is.
pub(crate) enum Dim0 {
    /// A date cut into periods (months, weeks…).
    Period(Grain),
    /// A date part: month of year, weekday or hour.
    Part(Grain),
    /// A date with no grain.
    Time,
    Plain,
}

pub(crate) fn classify(engine: &Engine, d: &Dimension) -> Dim0 {
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
    if !matches!(
        tile.kind.as_str(),
        "bar" | "line" | "area" | "waterfall" | "calendar"
    ) {
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
    // A sum or a count of nothing is 0. A ratio of nothing is a gap, not zero (a named measure
    // is worked out over nothing, so `SUM(a) / SUM(b)` is a gap and `SUM(a)` is 0).
    let fill: Vec<Value> = engine.empty_values(&q.measures);

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
    wide: &[Filter],
    res: &mut QueryResult,
) -> anyhow::Result<()> {
    let Some(top) = &tile.top else {
        return Ok(());
    };
    if !matches!(
        tile.kind.as_str(),
        "bar" | "table" | "share" | "donut" | "pivot" | "heatmap" | "bullet"
    ) {
        return Ok(());
    }
    if res.repeat.is_some() {
        return advanced::top_n_each(engine, tile, q, top, res);
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
        filters: wide.to_vec(),
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
        via: d0.via.clone(),
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

// ---- totals ------------------------------------------------------------------

/// Adds the totals of a table or pivot. They are separate queries with the same measures and
/// filters, so a ratio or an average is computed over the rows and never summed from cells.
///
/// A table has `grand` when `tile.totals` is true. A pivot has `grand`, `rows` (grouped by the row
/// fields) and `cols` (grouped by the column field) unless `tile.totals` is false. With a top N,
/// `rows` follows the rows shown: the kept groups, and one "Other" entry for the rest. `grand` and
/// `cols` always cover all the data, as the KPI does.
pub fn totals(
    engine: &Engine,
    tile: &Tile,
    q: &SemanticQuery,
    wide: &[Filter],
    res: &mut QueryResult,
) -> anyhow::Result<()> {
    let wanted = match tile.kind.as_str() {
        "table" => tile.totals == Some(true),
        // A heat map keeps its totals for the order of its rows and columns: the biggest first.
        "pivot" | "heatmap" => tile.totals != Some(false),
        _ => false,
    };
    if !wanted || q.measures.is_empty() {
        return Ok(());
    }
    let ask = |dims: Vec<Dimension>, filters: Vec<Filter>, limit: Option<u32>| {
        engine.query(&SemanticQuery {
            measures: q.measures.clone(),
            dimensions: dims,
            filters,
            sort: vec![],
            limit,
        })
    };
    let Some(grand) = ask(vec![], wide.to_vec(), None)?.rows.into_iter().next() else {
        return Ok(());
    };
    let mut out = Totals {
        grand,
        rows: None,
        cols: None,
        groups: None,
    };
    let pivotish = matches!(tile.kind.as_str(), "pivot" | "heatmap");
    let col = tile.cols.as_ref().filter(|_| q.dimensions.len() > 1);
    // An expandable pivot with no column field still needs the subtotal of each group.
    let groups_only = tile.kind == "pivot" && tile.expand == Some(true) && col.is_none();
    if pivotish && (col.is_some() || groups_only) {
        let row_dims = if col.is_some() {
            q.dimensions[..q.dimensions.len() - 1].to_vec()
        } else {
            q.dimensions.clone()
        };
        // The totals of the groups follow the rows on show: with a top N, the kept first groups,
        // then one "Other" entry for the rest.
        let kept: HashSet<String> = res
            .rows
            .iter()
            .filter(|r| js_key(&r[0]) != OTHER_KEY)
            .map(|r| js_key(&r[0]))
            .collect();
        let follow = |dims: Vec<Dimension>| -> anyhow::Result<Vec<Vec<Value>>> {
            let mut rows = ask(
                dims.clone(),
                wide.to_vec(),
                Some(crate::snapshot::PIVOT_LIMIT),
            )?
            .rows;
            if tile.top.is_some() {
                rows.retain(|r| kept.contains(&js_key(&r[0])));
                if let Some(keys) = &res.other_of {
                    let mut filters = wide.to_vec();
                    filters.push(Filter {
                        table: row_dims[0].table.clone(),
                        column: row_dims[0].column.clone(),
                        grain: row_dims[0].grain,
                        op: FilterOp::NotIn,
                        values: keys.clone(),
                        via: row_dims[0].via.clone(),
                    });
                    for r in ask(
                        dims[1..].to_vec(),
                        filters,
                        Some(crate::snapshot::PIVOT_LIMIT),
                    )?
                    .rows
                    {
                        let mut row = vec![Value::String(OTHER_KEY.into())];
                        row.extend(r);
                        rows.push(row);
                    }
                }
            }
            Ok(rows)
        };
        let mut rows = if col.is_some() {
            follow(row_dims.clone())?
        } else {
            vec![]
        };
        let mut groups = None;
        // An expandable pivot also gets a subtotal for each group: every prefix of the row fields,
        // alone (its total, in `rows`) and over each column (its cells, in `groups`).
        if tile.expand == Some(true) && tile.kind == "pivot" && row_dims.len() > 1 {
            let mut cells = Vec::new();
            for p in 1..row_dims.len() {
                rows.extend(follow(row_dims[..p].to_vec())?);
                if let Some(c) = col {
                    let mut with_col = row_dims[..p].to_vec();
                    with_col.push(c.clone());
                    cells.extend(follow(with_col)?);
                }
            }
            groups = col.map(|_| cells);
        }
        out.rows = Some(rows);
        out.groups = groups;
        if let Some(c) = col {
            out.cols = Some(
                ask(
                    vec![c.clone()],
                    wide.to_vec(),
                    Some(crate::snapshot::PIVOT_LIMIT),
                )?
                .rows,
            );
        }
    }
    res.totals = Some(out);
    Ok(())
}

// ---- show value as -----------------------------------------------------------

/// How a value is shown next to the others (`tile.showAs`). A value this version does not know
/// reads as `value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ShowAs {
    #[serde(rename = "value")]
    Value,
    #[serde(rename = "pct_total")]
    PctTotal,
    #[serde(rename = "pct_group")]
    PctGroup,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "moving_3")]
    Moving3,
    #[serde(rename = "moving_7")]
    Moving7,
    #[serde(rename = "moving_12")]
    Moving12,
    #[serde(rename = "change")]
    Change,
    #[serde(rename = "pct_change")]
    PctChange,
    #[serde(rename = "rank")]
    Rank,
    #[serde(rename = "ytd")]
    Ytd,
    #[serde(other)]
    Unknown,
}

impl ShowAs {
    /// The words that follow the name of the column: "amount (% of total)".
    fn suffix(self) -> &'static str {
        match self {
            ShowAs::PctTotal => "% of total",
            ShowAs::PctGroup => "% of group",
            ShowAs::Running => "running total",
            ShowAs::Moving3 => "3-period average",
            ShowAs::Moving7 => "7-period average",
            ShowAs::Moving12 => "12-period average",
            ShowAs::Change => "change",
            ShowAs::PctChange => "% change",
            ShowAs::Rank => "rank",
            ShowAs::Ytd => "year to date",
            ShowAs::Value | ShowAs::Unknown => "",
        }
    }

    fn unit(self) -> Option<Unit> {
        match self {
            ShowAs::PctTotal | ShowAs::PctGroup | ShowAs::PctChange => Some(Unit::Percent),
            ShowAs::Rank => Some(Unit::Rank),
            _ => None,
        }
    }

    /// The ways that read the points in time order.
    fn needs_order(self) -> bool {
        matches!(
            self,
            ShowAs::Running
                | ShowAs::Moving3
                | ShowAs::Moving7
                | ShowAs::Moving12
                | ShowAs::Change
                | ShowAs::PctChange
                | ShowAs::Ytd
        )
    }

    /// The window of a moving average.
    fn window(self) -> Option<usize> {
        match self {
            ShowAs::Moving3 => Some(3),
            ShowAs::Moving7 => Some(7),
            ShowAs::Moving12 => Some(12),
            _ => None,
        }
    }
}

/// The year of a period start, for year to date.
fn year_of(v: &Value) -> Option<i32> {
    parse_day(v.as_str()?).map(|d| d.year())
}

/// Shows each value the way `tile.showAs` asks. It runs on the rows the tile shows (after the gaps
/// are filled and the top N is cut, "Other" included). Rows keep the order they are in. A way that
/// reads in order (running, moving, change, year to date) goes through the points by the first
/// group-by column ascending, whatever the sort of the tile, and writes each result back to its
/// own row. With a split, each group of the other columns is its own sequence. A null stays null. The column name gains its words ("amount
/// (% of total)") and a share or a rank gets a `unit`. Number tiles and charts that do not read in
/// order (scatter, waterfall, share) are left alone.
pub fn show_as(tile: &Tile, q: &SemanticQuery, engine: &Engine, res: &mut QueryResult) {
    let Some(list) = &tile.show_as else { return };
    if !matches!(
        tile.kind.as_str(),
        "bar" | "line" | "area" | "table" | "pivot"
    ) {
        return;
    }
    let (Some(d0), nd) = (q.dimensions.first(), q.dimensions.len()) else {
        return;
    };
    let first = classify(engine, d0);
    let dated = matches!(first, Dim0::Period(_) | Dim0::Time);
    let ordered = dated || matches!(first, Dim0::Part(_));
    for j in 0..q.measures.len() {
        let Some(how) = list.get(j).copied().flatten() else {
            continue;
        };
        if matches!(how, ShowAs::Value | ShowAs::Unknown) {
            continue;
        }
        // A share of the whole, a running total and a year to date add the values up. A named
        // measure whose values do not add up (a ratio, an average) is shown as it is, with words
        // (V4-D85d).
        if let Some(why) = refuses_to_add(engine, &q.measures[j], how, repeats_at(res, j)) {
            res.parts.push(PartNote {
                measure: j,
                note: why,
                repeats: false,
            });
            continue;
        }
        // Year to date needs a date to read the year from. The other ways that read in order
        // need a date or a date part first, or the order is only the order of the rows.
        if (how == ShowAs::Ytd && !dated) || (how.needs_order() && !ordered) {
            continue;
        }
        let col = nd + j;
        if res.rows.iter().any(|r| r.len() <= col) || res.columns.len() <= col {
            continue;
        }
        let how = if how == ShowAs::PctGroup && (nd < 2 || res.repeat.is_some()) {
            ShowAs::PctTotal
        } else {
            how
        };
        // In small multiples the second column is the chart: a share is a share of its own chart.
        shape_column(&mut res.rows, nd, col, how, res.repeat.is_some());
        let c = &mut res.columns[col];
        c.name = format!("{} ({})", c.name, how.suffix());
        c.unit = how.unit();
    }
}

/// Replaces column `col` of `rows` with the value shown as `how`. The first `nd` columns are the
/// groups. Used for the result and for the comparison series.
fn shape_column(rows: &mut [Vec<Value>], nd: usize, col: usize, how: ShowAs, per_chart: bool) {
    let vals: Vec<Option<f64>> = rows.iter().map(|r| r[col].as_f64()).collect();
    let out = match how {
        ShowAs::PctTotal if per_chart => share_of(&vals, &group_ids(rows, 1..2)),
        ShowAs::PctTotal => share_of(&vals, &vec![0; vals.len()]),
        ShowAs::PctGroup => {
            // One group for each value of the first column, across the split.
            let ids = group_ids(rows, 0..1);
            share_of(&vals, &ids)
        }
        _ => {
            // A sequence for each combination of the split columns, in the order rows come.
            let ids = group_ids(rows, 1..nd);
            let mut out = vec![None; vals.len()];
            let groups = ids.iter().copied().max().map_or(0, |m| m + 1);
            for g in 0..groups {
                let mut at: Vec<usize> = (0..vals.len()).filter(|&i| ids[i] == g).collect();
                if how.needs_order() {
                    // Time order, not the order the tile shows (a stable sort keeps ties).
                    at.sort_by(|&a, &b| cmp_key(&rows[a][0], &rows[b][0]));
                }
                let v: Vec<Option<f64>> = at.iter().map(|&i| vals[i]).collect();
                let years: Vec<Option<i32>> = at.iter().map(|&i| year_of(&rows[i][0])).collect();
                for (k, x) in sequence(how, &v, &years).into_iter().enumerate() {
                    out[at[k]] = x;
                }
            }
            out
        }
    };
    for (r, v) in rows.iter_mut().zip(out) {
        r[col] = match v {
            Some(x) if x.is_finite() => json!(x),
            _ => Value::Null,
        };
    }
}

/// The ways of showing a value that add values up (V4-D85d).
fn adds_up(how: ShowAs) -> bool {
    matches!(
        how,
        ShowAs::PctTotal | ShowAs::PctGroup | ShowAs::Running | ShowAs::Ytd
    )
}

/// Why a value cannot be shown this way, or `None` when it can (V4-D85d, V4-D99). A share of the whole, a
/// running total and a year to date add values up. A value whose groups do not add up to the whole is shown
/// as it is, with words: an average, a middle value, a distinct count, a least or a most, a ratio, a named
/// formula that is not a sum, and any measure that repeats a table across the groups of the tile (`repeats`).
fn refuses_to_add(engine: &Engine, m: &Measure, how: ShowAs, repeats: bool) -> Option<String> {
    if !adds_up(how) {
        return None;
    }
    let f = engine.measure_facts(m).ok()?;
    let why = if !f.additive {
        "its values do not add up"
    } else if repeats {
        "a table it reads is not split by these groups, so its values do not add up"
    } else {
        return None;
    };
    Some(format!(
        "{} is not shown as {}: {why}. The value is shown instead.",
        f.name,
        how.suffix()
    ))
}

/// A table of the value has the same number for every group of the tile (V4-D99a).
fn repeats_at(res: &QueryResult, j: usize) -> bool {
    res.parts.iter().any(|p| p.measure == j && p.repeats)
}

/// Orders the first-column values of a time chart: dates (ISO text) and date parts (numbers).
pub(crate) fn cmp_key(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        _ => a.as_str().cmp(&b.as_str()),
    }
}

/// A number for each row that says which group of the given columns it belongs to.
fn group_ids(rows: &[Vec<Value>], cols: std::ops::Range<usize>) -> Vec<usize> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    rows.iter()
        .map(|r| {
            let key: Vec<String> = cols.clone().map(|c| js_key(&r[c])).collect();
            let next = seen.len();
            *seen.entry(key.join("\u{1}")).or_insert(next)
        })
        .collect()
}

/// Each value over the sum of its group. A group that sums to zero has no shares.
fn share_of(vals: &[Option<f64>], ids: &[usize]) -> Vec<Option<f64>> {
    let mut sum: HashMap<usize, f64> = HashMap::new();
    for (v, g) in vals.iter().zip(ids) {
        if let Some(v) = v {
            *sum.entry(*g).or_default() += v;
        }
    }
    vals.iter()
        .zip(ids)
        .map(|(v, g)| match (v, sum.get(g)) {
            (Some(v), Some(t)) if *t != 0.0 => Some(v / t),
            _ => None,
        })
        .collect()
}

/// The shows that read a sequence of points in order. `years` is the year of each point, for
/// year to date.
fn sequence(how: ShowAs, v: &[Option<f64>], years: &[Option<i32>]) -> Vec<Option<f64>> {
    let mut out = Vec::with_capacity(v.len());
    match how {
        ShowAs::Running => {
            let mut acc = 0.0;
            for x in v {
                out.push(x.map(|x| {
                    acc += x;
                    acc
                }));
            }
        }
        ShowAs::Ytd => {
            let (mut acc, mut year) = (0.0, None);
            for (x, y) in v.iter().zip(years) {
                out.push(match (x, y) {
                    (Some(x), Some(y)) => {
                        if year != Some(*y) {
                            year = Some(*y);
                            acc = 0.0;
                        }
                        acc += x;
                        Some(acc)
                    }
                    _ => None,
                });
            }
        }
        ShowAs::Moving3 | ShowAs::Moving7 | ShowAs::Moving12 => {
            // The mean of the last k values that exist. Fewer than k so far is a gap, so a part
            // of a window never reads as a full one.
            let k = how.window().unwrap_or(3);
            let mut seen: Vec<f64> = Vec::new();
            for x in v {
                out.push(match x {
                    Some(x) => {
                        seen.push(*x);
                        (seen.len() >= k)
                            .then(|| seen[seen.len() - k..].iter().sum::<f64>() / k as f64)
                    }
                    None => None,
                });
            }
        }
        ShowAs::Change | ShowAs::PctChange => {
            for (i, x) in v.iter().enumerate() {
                let prev = i.checked_sub(1).and_then(|p| v[p]);
                out.push(match (x, prev) {
                    (Some(x), Some(p)) if how == ShowAs::Change => Some(x - p),
                    (Some(x), Some(p)) if p != 0.0 => Some((x - p) / p.abs()),
                    _ => None,
                });
            }
        }
        ShowAs::Rank => {
            // 1 is the biggest. Equal values share a rank and the next rank is not skipped.
            let mut distinct: Vec<f64> = v.iter().flatten().copied().collect();
            distinct.sort_by(|a, b| b.total_cmp(a));
            distinct.dedup();
            for x in v {
                out.push(x.and_then(|x| {
                    distinct
                        .iter()
                        .position(|d| *d == x)
                        .map(|p| (p + 1) as f64)
                }));
            }
        }
        _ => out.extend(v.iter().copied()),
    }
    out
}

// ---- same period last year ---------------------------------------------------

/// What a time chart is compared with (`tile.compare`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum CompareTo {
    #[serde(rename = "prev_year")]
    PrevYear,
    #[serde(rename = "prev_period")]
    PrevPeriod,
    #[serde(other)]
    Unknown,
}

/// A period start moved one step: `dir` is -1 for back and 1 for forward. A year is a calendar
/// year (the same date), except for weeks, where it is 52 weeks so a Monday meets a Monday.
fn shift(d: NaiveDate, g: Grain, to: CompareTo, dir: i32) -> NaiveDate {
    match (to, g) {
        (CompareTo::PrevPeriod, _) => add_grain(d, g, dir),
        (_, Grain::Week) => d + Duration::days(364 * i64::from(dir)),
        _ => add_grain(d, Grain::Year, dir),
    }
}

/// A date text moved back one step, keeping the time of day if it has one.
fn shift_text(s: &str, g: Grain, to: CompareTo) -> Option<String> {
    let d = parse_day(s)?;
    Some(format!("{}{}", iso(shift(d, g, to, -1)), s.get(10..)?))
}

/// The label of the comparison line, in the language of the workspace.
fn compare_label(to: CompareTo, loc: Locale) -> &'static str {
    match (to, loc) {
        (CompareTo::PrevYear, Locale::En) => "Same period last year",
        (CompareTo::PrevYear, Locale::Id) => "Periode yang sama tahun lalu",
        (_, Locale::En) => "Previous period",
        (_, Locale::Id) => "Periode sebelumnya",
    }
}

/// Reads the same question one year (or one period) earlier, for a bar, line or area with a date
/// grouped by a grain and one value. The date filters move back by the same step, then the keys
/// move forward, so each earlier point sits under the period it is compared with. A key that has
/// no partner after the move (29 February) is dropped. The series is filled and shown the same
/// way as the result, and keeps only the periods the result has.
pub fn compare(
    engine: &Engine,
    tile: &Tile,
    q: &SemanticQuery,
    res: &mut QueryResult,
) -> anyhow::Result<()> {
    let Some(to) = tile.compare.filter(|c| *c != CompareTo::Unknown) else {
        return Ok(());
    };
    // An indexed chart and a small-multiples chart have no comparison series.
    if tile.layout == Some(Layout::Indexed) || res.repeat.is_some() {
        return Ok(());
    }
    if !matches!(tile.kind.as_str(), "bar" | "line" | "area")
        || q.measures.len() != 1
        || q.dimensions.len() != 1
    {
        return Ok(());
    }
    let d0 = &q.dimensions[0];
    let Dim0::Period(g) = classify(engine, d0) else {
        return Ok(());
    };
    let today = engine.today();
    let filters = q
        .filters
        .iter()
        .map(|f| {
            // A relative preset is a range of days for today; it moves back like any other.
            let mut f = crate::query::resolve_relative(f, today);
            if f.table == d0.table && f.column == d0.column {
                f.values = f
                    .values
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .and_then(|s| shift_text(s, g, to))
                            .map_or_else(|| v.clone(), Value::String)
                    })
                    .collect();
            }
            f
        })
        .collect();
    let mut earlier = engine.query(&SemanticQuery {
        measures: q.measures.clone(),
        dimensions: q.dimensions.clone(),
        filters,
        sort: vec![],
        limit: q.limit,
    })?;
    // Move the keys forward. A key that does not come back to itself has no partner.
    earlier.rows.retain_mut(|r| {
        let Some(d) = r[0].as_str().and_then(parse_day) else {
            return false;
        };
        let at = shift(d, g, to, 1);
        if shift(at, g, to, -1) != d {
            return false;
        }
        r[0] = Value::String(iso(at));
        true
    });
    densify(engine, tile, q, &mut earlier);
    if let Some(how) = tile
        .show_as
        .as_ref()
        .and_then(|l| l.first().copied().flatten())
        .filter(|h| !matches!(h, ShowAs::Value | ShowAs::Unknown))
        .filter(|h| refuses_to_add(engine, &q.measures[0], *h, repeats_at(res, 0)).is_none())
        .map(|h| {
            if h == ShowAs::PctGroup {
                ShowAs::PctTotal
            } else {
                h
            }
        })
    {
        shape_column(&mut earlier.rows, 1, 1, how, false);
    }
    let mine: HashSet<String> = res.rows.iter().map(|r| js_key(&r[0])).collect();
    earlier.rows.retain(|r| mine.contains(&js_key(&r[0])));
    res.compare = Some(Compare {
        label: compare_label(to, engine.locale()).into(),
        rows: earlier.rows,
    });
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
            per: None,
            via: m.via.clone(),
            named: None,
        }],
        dimensions: vec![],
        filters: filters.to_vec(),
        sort: vec![],
        limit: None,
    };
    let start_time = std::time::Instant::now();
    let (stats, ignored) = engine.query_custom(&q, |sc| {
        let (x, _) = sc.expr_via(&m.table, m.via.as_deref(), &column, None)?;
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
            unit: None,
        },
        OutColumn {
            name: "Rows".into(),
            role: "measure".into(),
            dtype: DType::Number,
            unit: None,
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
        totals: None,
        compare: None,
        total_rows: None,
        repeat: None,
        real: None,
        outliers: None,
        sparks: None,
        pair: None,
        notes: Vec::new(),
        public_notes: Vec::new(),
        parts: Vec::new(),
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
        let (x, _) = sc.expr_via(&m.table, m.via.as_deref(), &column, None)?;
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

    #[test]
    fn rank_is_dense() {
        let v = [Some(10.0), Some(20.0), Some(20.0), None, Some(5.0)];
        let r = sequence(ShowAs::Rank, &v, &[None; 5]);
        assert_eq!(r, vec![Some(2.0), Some(1.0), Some(1.0), None, Some(3.0)]);
    }

    #[test]
    fn shares_of_a_zero_group_are_gaps() {
        let r = share_of(&[Some(0.0), Some(0.0), Some(3.0)], &[0, 0, 1]);
        assert_eq!(r, vec![None, None, Some(1.0)]);
    }
}
