//! The engine side of the Advanced charts: small multiples (the charts, and the top N inside
//! each), indexed lines, a manual order of the groups, and box plots. They are computed here so
//! the app, exported files, published dashboards and the headless agent agree; the UI only draws.

use crate::catalog::DType;
use crate::engine::{Engine, Pair, QueryResult, Repeat};
use crate::query::{
    Agg, Dimension, Filter, FilterOp, Grain, Measure, OutColumn, SemanticQuery, Sort, Unit,
};
use crate::shape::{Dim0, Layout, OTHER_KEY, classify, cmp_key};
use crate::snapshot::{Partial, SortBy, Tile, TopN, add_grain, count_rows, iso, js_key, parse_day};
use chrono::{Datelike, Duration, NaiveDate};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

/// A small-multiples tile draws at most this many charts. The rest are counted ("and 5 more").
pub const MAX_CHARTS: usize = 12;
/// A box plot draws at most this many groups, the biggest ones by row count.
pub const MAX_BOX_GROUPS: usize = 40;
/// A box plot draws at most this many outlier dots for each group.
pub const MAX_OUTLIERS: usize = 50;
/// A repeat field is read for up to this many values, to find the charts.
const REPEAT_READ: u32 = 5000;
/// A Sankey chart has two or three stages.
pub const SANKEY_STAGES: usize = 3;
/// A stage of a Sankey chart shows at most this many nodes, "Other" included.
pub const MAX_NODES: usize = 25;
/// Links are read for up to this many pairs of nodes between two stages.
const SANKEY_LINKS: u32 = 5000;
/// A Gantt chart draws at most this many rows.
pub const MAX_GANTT_ROWS: u32 = 100;
/// A compare chart draws at most this many rows, the ones with the biggest gap.
pub const MAX_COMPARE_ROWS: usize = 40;
/// A trend column draws this many periods.
pub const SPARK_POINTS: usize = 12;
/// A cohort grid shows the latest cohorts, this many at most.
pub const MAX_COHORTS: usize = 36;
/// A cohort grid follows each cohort for this many periods after its first.
pub const MAX_COHORT_OFFSET: i64 = 24;

/// Does this tile ask for small multiples? Only a bar, line or area with a group has them.
pub fn repeats(tile: &Tile) -> bool {
    tile.repeat.is_some()
        && !tile.dims.is_empty()
        && matches!(tile.kind.as_str(), "bar" | "line" | "area")
}

/// Cuts the question of a small-multiples tile to the charts that are drawn, and says which they
/// are. The repeat values are ranked by the first value (biggest first). A date reads in time order
/// instead and keeps the latest ones. A blank repeat value has no chart.
pub fn limit_repeat(
    engine: &Engine,
    tile: &Tile,
    q: &mut SemanticQuery,
    wide: &[Filter],
) -> anyhow::Result<Option<Repeat>> {
    if !repeats(tile) {
        return Ok(None);
    }
    let (Some(r), Some(m0)) = (tile.repeat.as_ref(), q.measures.first()) else {
        return Ok(None);
    };
    let all = engine.query(&SemanticQuery {
        measures: vec![m0.clone()],
        dimensions: vec![r.clone()],
        filters: wide.to_vec(),
        sort: vec![],
        limit: Some(REPEAT_READ),
    })?;
    let mut keys: Vec<(Value, Option<f64>)> = all
        .rows
        .iter()
        .filter(|row| !row[0].is_null())
        .map(|row| (row[0].clone(), row.get(1).and_then(Value::as_f64)))
        .collect();
    let temporal = matches!(
        classify(engine, r),
        Dim0::Period(_) | Dim0::Part(_) | Dim0::Time
    );
    let total = keys.len();
    if temporal {
        keys.sort_by(|a, b| cmp_key(&a.0, &b.0));
        let skip = total.saturating_sub(MAX_CHARTS);
        keys.drain(..skip);
    } else {
        // Biggest first; a gap (no value) last; equal values in the order of their names.
        keys.sort_by(|a, b| {
            b.1.unwrap_or(f64::NEG_INFINITY)
                .total_cmp(&a.1.unwrap_or(f64::NEG_INFINITY))
                .then_with(|| js_key(&a.0).cmp(&js_key(&b.0)))
        });
        keys.truncate(MAX_CHARTS);
    }
    let keys: Vec<Value> = keys.into_iter().map(|(k, _)| k).collect();
    if total > keys.len() {
        q.filters.push(Filter {
            table: r.table.clone(),
            column: r.column.clone(),
            grain: r.grain,
            op: FilterOp::In,
            values: keys.clone(),
            via: r.via.clone(),
        });
    }
    Ok(Some(Repeat {
        more: (total - keys.len()) as u32,
        keys,
    }))
}

/// Top N for small multiples: it applies inside each chart. Each chart keeps its biggest `n`
/// groups (ranked by the first value) and, unless `other` is false, adds the rest as its own
/// "Other" group, read with a query for that chart alone so it is never a sum of averages. There is
/// no `otherOf`: a click on "Other" cannot name the groups of one chart.
pub fn top_n_each(
    engine: &Engine,
    _tile: &Tile,
    q: &SemanticQuery,
    top: &TopN,
    res: &mut QueryResult,
) -> anyhow::Result<()> {
    let (Some(d0), Some(d1), Some(rep)) = (
        q.dimensions.first(),
        q.dimensions.get(1),
        res.repeat.as_ref(),
    ) else {
        return Ok(());
    };
    if !matches!(classify(engine, d0), Dim0::Plain) {
        return Ok(());
    }
    let n = top.n.clamp(1, 1000) as usize;
    let nd = q.dimensions.len();
    let mut made_other = false;
    let mut extra: Vec<Vec<Value>> = Vec::new();
    for key in rep.keys.clone() {
        let kk = js_key(&key);
        let mut mine: Vec<usize> = (0..res.rows.len())
            .filter(|&i| js_key(&res.rows[i][1]) == kk)
            .collect();
        if mine.len() <= n {
            continue;
        }
        mine.sort_by(|&a, &b| {
            let v = |i: usize| res.rows[i][nd].as_f64().unwrap_or(f64::NEG_INFINITY);
            v(b).total_cmp(&v(a))
        });
        let drop: HashSet<usize> = mine[n..].iter().copied().collect();
        let kept: Vec<Value> = mine[..n].iter().map(|&i| res.rows[i][0].clone()).collect();
        if top.other != Some(false) {
            let mut filters = q.filters.clone();
            filters.push(Filter {
                table: d0.table.clone(),
                column: d0.column.clone(),
                grain: d0.grain,
                op: FilterOp::NotIn,
                values: kept,
                via: d0.via.clone(),
            });
            filters.push(Filter {
                table: d1.table.clone(),
                column: d1.column.clone(),
                grain: d1.grain,
                op: FilterOp::In,
                values: vec![key.clone()],
                via: d1.via.clone(),
            });
            let rest = engine.query(&SemanticQuery {
                measures: q.measures.clone(),
                dimensions: vec![d1.clone()],
                filters,
                sort: vec![],
                limit: Some(1),
            })?;
            for r in rest.rows {
                let mut row = vec![Value::String(OTHER_KEY.into())];
                row.extend(r);
                extra.push(row);
                made_other = true;
            }
        }
        let mut i = 0;
        res.rows.retain(|_| {
            let keep = !drop.contains(&i);
            i += 1;
            keep
        });
    }
    res.rows.extend(extra);
    if made_other {
        res.other = Some(true);
    }
    Ok(())
}

/// Puts the groups in the order the user dragged them to (`sort.by = "manual"`, `sort.order`).
/// A group that is not in the list goes after those that are, in the order it already had, and
/// "Other" stays last. The order is by the first column, so a split or a chart of small multiples
/// keeps it inside every group. A date has an order of its own and ignores it.
pub fn manual_order(engine: &Engine, tile: &Tile, q: &SemanticQuery, res: &mut QueryResult) {
    let Some(sort) = tile.sort.as_ref().filter(|s| s.by == SortBy::Manual) else {
        return;
    };
    let Some(order) = sort.order.as_ref().filter(|o| !o.is_empty()) else {
        return;
    };
    if matches!(
        tile.kind.as_str(),
        "pivot" | "heatmap" | "kpi" | "scatter" | "histogram" | "box"
    ) {
        return;
    }
    let Some(d0) = q.dimensions.first() else {
        return;
    };
    if !matches!(classify(engine, d0), Dim0::Plain) {
        return;
    }
    let rank: HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, k)| (k.as_str(), i))
        .collect();
    res.rows.sort_by_key(|r| {
        let k = js_key(&r[0]);
        if k == OTHER_KEY {
            return usize::MAX;
        }
        rank.get(k.as_str()).copied().unwrap_or(usize::MAX - 1)
    });
}

/// Indexed lines: every series is divided by its first value and multiplied by 100, so lines of
/// different units start together and the chart compares growth. The series are the values of a
/// tile with several, or the groups of a split on one value. They read in time order when the
/// first field is a date. The base is the first value that is not a gap and not zero (a quiet
/// period that was filled with 0 cannot be a base); points before it are gaps, and so is a whole
/// series that never has a base. `res.real` keeps the rows as they were, for the tooltip.
pub fn indexed(engine: &Engine, tile: &Tile, q: &SemanticQuery, res: &mut QueryResult) {
    if tile.layout != Some(Layout::Indexed)
        || !matches!(tile.kind.as_str(), "bar" | "line" | "area")
        || res.repeat.is_some()
    {
        return;
    }
    let nd = q.dimensions.len();
    let nm = q.measures.len();
    if nd == 0 || nd > 2 || (nd == 2 && nm != 1) || res.rows.iter().any(|r| r.len() != nd + nm) {
        return;
    }
    let ordered = matches!(
        classify(engine, &q.dimensions[0]),
        Dim0::Period(_) | Dim0::Part(_) | Dim0::Time
    );
    let real = res.rows.clone();
    // The series: for each, the rows that belong to it and the column that holds its value.
    let mut series: Vec<(usize, Vec<usize>)> = Vec::new();
    if nd == 1 {
        for j in 0..nm {
            series.push((nd + j, (0..res.rows.len()).collect()));
        }
    } else {
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (i, r) in res.rows.iter().enumerate() {
            let g = *seen.entry(js_key(&r[1])).or_insert_with(|| {
                groups.push(vec![]);
                groups.len() - 1
            });
            groups[g].push(i);
        }
        series.extend(groups.into_iter().map(|g| (nd, g)));
    }
    for (col, mut at) in series {
        if ordered {
            at.sort_by(|&a, &b| cmp_key(&real[a][0], &real[b][0]));
        }
        let start = at.iter().position(|&i| {
            real[i][col]
                .as_f64()
                .is_some_and(|v| v != 0.0 && v.is_finite())
        });
        for (pos, &i) in at.iter().enumerate() {
            res.rows[i][col] = match (start, real[i][col].as_f64()) {
                (Some(s), Some(v)) if pos >= s => {
                    let base = real[at[s]][col].as_f64().unwrap_or(1.0);
                    json!(v / base * 100.0)
                }
                _ => Value::Null,
            };
        }
    }
    for c in res.columns.iter_mut().skip(nd) {
        c.unit = None;
    }
    res.real = Some(real);
}

// ---- box plot ----------------------------------------------------------------------------

/// Names of the box plot's columns after the group: the five numbers, the whiskers and the count.
pub const BOX_COLUMNS: [&str; 8] = [
    "Minimum",
    "Q1",
    "Median",
    "Q3",
    "Maximum",
    "Lower whisker",
    "Upper whisker",
    "Count",
];

/// Box plot data. For each group of the first field (or all rows when there is none) it reads the
/// minimum, the first quartile, the median, the third quartile and the maximum of one number
/// column, with DuckDB's `quantile_cont` (so the quartiles agree with the histogram and the
/// "median" and "p90" values). The whiskers reach the lowest and highest value within 1.5 × the
/// inter-quartile range of the box; values beyond them are outliers, at most 50 for each group
/// (the ones furthest from the median). Rows are `[group, min, q1, median, q3, max, lower whisker,
/// upper whisker, count]`; `res.outliers` holds `[group, value]`.
pub fn boxplot(engine: &Engine, tile: &Tile, filters: &[Filter]) -> anyhow::Result<QueryResult> {
    let m = tile
        .measures
        .first()
        .filter(|m| m.column.is_some())
        .ok_or_else(|| anyhow::anyhow!("Pick a number column for the box plot."))?;
    let column = m.column.clone().unwrap_or_default();
    if engine.column_dtype(&m.table, &column) != Some(DType::Number) {
        anyhow::bail!("{column} is not a number column. A box plot needs one.");
    }
    let group: Option<&Dimension> = tile.dims.first();
    let q = SemanticQuery {
        measures: vec![crate::query::Measure {
            table: m.table.clone(),
            column: Some(column.clone()),
            agg: crate::query::Agg::Count,
            per: None,
            via: m.via.clone(),
            named: None,
        }],
        dimensions: group.cloned().into_iter().collect(),
        filters: filters.to_vec(),
        sort: vec![],
        limit: None,
    };
    let start = std::time::Instant::now();
    // The shared start of both queries: the values of each group, and the numbers of its box.
    let prelude =
        |sc: &crate::query::Scope<'_>| -> Result<(String, DType), crate::query::QueryError> {
            let (x, _) = sc.expr_via(&m.table, m.via.as_deref(), &column, None)?;
            let (g, gtype) = match group {
                Some(d) => sc.expr_via(&d.table, d.via.as_deref(), &d.column, d.grain)?,
                None => ("CAST('All' AS VARCHAR)".to_string(), DType::Text),
            };
            Ok((
                format!(
                    "WITH base AS (SELECT {g} AS g, {x} AS x{}), \
                 vals AS (SELECT g, x FROM base WHERE x IS NOT NULL), \
                 st AS (SELECT g, CAST(count(*) AS DOUBLE) AS n, min(x) AS mn, \
                 quantile_cont(x, 0.25) AS q1, quantile_cont(x, 0.5) AS med, \
                 quantile_cont(x, 0.75) AS q3, max(x) AS mx FROM vals GROUP BY g)",
                    sc.from_where
                ),
                gtype,
            ))
        };
    let mut gtype = DType::Text;
    let (stats, ignored) = engine.query_custom(&q, |sc| {
        let (with, t) = prelude(sc)?;
        gtype = t;
        Ok((
            format!(
                "{with} SELECT st.g, st.mn, st.q1, st.med, st.q3, st.mx, \
                 min(CASE WHEN v.x >= st.q1 - 1.5 * (st.q3 - st.q1) THEN v.x END), \
                 max(CASE WHEN v.x <= st.q3 + 1.5 * (st.q3 - st.q1) THEN v.x END), st.n \
                 FROM st JOIN vals v ON v.g IS NOT DISTINCT FROM st.g \
                 GROUP BY st.g, st.mn, st.q1, st.med, st.q3, st.mx, st.n ORDER BY st.g"
            ),
            sc.params.clone(),
        ))
    })?;
    let (dots, _) = engine.query_custom(&q, |sc| {
        let (with, _) = prelude(sc)?;
        Ok((
            format!(
                "{with} SELECT g, x FROM (SELECT v.g AS g, v.x AS x, row_number() OVER \
                 (PARTITION BY v.g ORDER BY abs(v.x - st.med) DESC, v.x) AS rn \
                 FROM vals v JOIN st ON v.g IS NOT DISTINCT FROM st.g \
                 WHERE v.x < st.q1 - 1.5 * (st.q3 - st.q1) OR v.x > st.q3 + 1.5 * (st.q3 - st.q1)) \
                 WHERE rn <= {MAX_OUTLIERS} ORDER BY g, x"
            ),
            sc.params.clone(),
        ))
    })?;

    let mut rows = stats;
    // Too many groups: keep the ones with the most rows.
    if rows.len() > MAX_BOX_GROUPS {
        rows.sort_by(|a, b| {
            let n = |r: &Vec<Value>| r[8].as_f64().unwrap_or(0.0);
            n(b).total_cmp(&n(a))
        });
        rows.truncate(MAX_BOX_GROUPS);
    }
    // The order of the groups: A to Z by default, by the median, or the order the user dragged to.
    match tile.sort.as_ref().map(|s| (s.by, s.desc)) {
        Some((SortBy::Value, desc)) => rows.sort_by(|a, b| {
            let o = a[3]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&b[3].as_f64().unwrap_or(0.0));
            if desc.unwrap_or(true) { o.reverse() } else { o }
        }),
        Some((SortBy::Manual, _)) => {
            let rank: HashMap<&str, usize> = tile
                .sort
                .as_ref()
                .and_then(|s| s.order.as_ref())
                .map(|o| o.iter().enumerate().map(|(i, k)| (k.as_str(), i)).collect())
                .unwrap_or_default();
            rows.sort_by_key(|r| {
                rank.get(js_key(&r[0]).as_str())
                    .copied()
                    .unwrap_or(usize::MAX)
            });
        }
        other => {
            rows.sort_by(|a, b| cmp_key(&a[0], &b[0]));
            if other.is_some_and(|(_, d)| d == Some(true)) {
                rows.reverse();
            }
        }
    }
    let kept: HashSet<String> = rows.iter().map(|r| js_key(&r[0])).collect();
    // Rows: group, then min, q1, median, q3, max, lower whisker, upper whisker, count.
    let out_rows: Vec<Vec<Value>> = rows.into_iter().map(|r| r[..9].to_vec()).collect();
    let outliers: Vec<Vec<Value>> = dots
        .into_iter()
        .filter(|r| kept.contains(&js_key(&r[0])))
        .collect();

    let mut columns = vec![OutColumn {
        name: group.map_or_else(|| "All".to_string(), |d| d.column.clone()),
        role: "dimension".into(),
        dtype: gtype,
        unit: None,
    }];
    columns.extend(BOX_COLUMNS.iter().map(|name| OutColumn {
        name: (*name).to_string(),
        role: "measure".into(),
        dtype: DType::Number,
        unit: None,
    }));
    Ok(QueryResult {
        columns,
        rows: out_rows,
        ignored_filters: ignored,
        ms: start.elapsed().as_millis(),
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
        outliers: Some(outliers),
        sparks: None,
        pair: None,
        notes: Vec::new(),
        public_notes: Vec::new(),
        parts: Vec::new(),
        geo: None,
    })
}

// ---- compare: dumbbell and slope --------------------------------------------------------

/// A compare chart with one value reads two periods of its second field. They are the periods
/// the user picked (`tile.periods`), or the latest two. The question is cut to them. Returns the
/// two periods in the order of the two value columns; `None` when the chart compares two values.
pub fn compare_periods(
    engine: &Engine,
    tile: &Tile,
    q: &mut SemanticQuery,
    wide: &[Filter],
) -> anyhow::Result<Option<Vec<Value>>> {
    if tile.measures.len() >= 2 || q.dimensions.len() < 2 {
        return Ok(None);
    }
    let d = q.dimensions[1].clone();
    let chosen: Vec<Value> = match &tile.periods {
        Some(p) if p.len() == 2 => p.clone(),
        _ => {
            let all = engine.query(&SemanticQuery {
                measures: q.measures.clone(),
                dimensions: vec![d.clone()],
                filters: wide.to_vec(),
                sort: vec![],
                limit: Some(REPEAT_READ),
            })?;
            let mut keys: Vec<Value> = all
                .rows
                .iter()
                .filter(|r| !r[0].is_null())
                .map(|r| r[0].clone())
                .collect();
            keys.sort_by(cmp_key);
            // The latest period counts only when its data is finished: a month that has just
            // begun would make every line fall. Two finished periods are better than that; with
            // fewer than three periods the unfinished one stays (and is marked, see `compare_partial`).
            if keys.len() > 2 && unfinished(engine, &d, wide, keys.last()).is_some() {
                keys.pop();
            }
            let from = keys.len().saturating_sub(2);
            keys.split_off(from)
        }
    };
    q.filters.push(Filter {
        table: d.table,
        column: d.column,
        grain: d.grain,
        op: FilterOp::In,
        values: chosen.clone(),
        via: d.via,
    });
    Ok(Some(chosen))
}

/// The last day of data when `key` (the start of a period of `field`) is a period that the data
/// does not reach the end of; `None` for a finished period, a day grain or a key that is no date.
fn unfinished(
    engine: &Engine,
    field: &Dimension,
    filters: &[Filter],
    key: Option<&Value>,
) -> Option<NaiveDate> {
    let grain = field.grain.filter(|g| *g != Grain::Day)?;
    let start = key?.as_str().and_then(parse_day)?;
    let max_day = latest_day(engine, field, filters)?;
    (max_day < add_grain(start, grain, 1) - Duration::days(1)).then_some(max_day)
}

/// The mark for a slope chart whose second period is unfinished (the periods are picked by the
/// user, or there are fewer than three). The same `Partial` the time charts carry.
pub fn compare_partial(
    engine: &Engine,
    q: &SemanticQuery,
    wide: &[Filter],
    pair: Option<&Vec<Value>>,
) -> Option<Partial> {
    let d = q.dimensions.get(1)?;
    let key = pair?.last()?;
    let until = unfinished(engine, d, wide, Some(key))?;
    Some(Partial {
        key: key.as_str()?.to_string(),
        until: iso(until),
    })
}

/// Puts the rows of a compare chart in the shape `[group, first, second]`: the rows already have it
/// for two values, and one value over two periods is turned into it. Then it orders the rows by the
/// size of the gap, biggest first (or as the tile sorts them), and keeps the first
/// [`MAX_COMPARE_ROWS`]; `total_rows` says how many there were.
pub fn compare_pairs(
    tile: &Tile,
    q: &SemanticQuery,
    res: &mut QueryResult,
    periods: Option<Vec<Value>>,
) {
    if let Some(p) = periods {
        if q.dimensions.len() != 2 || res.rows.iter().any(|r| r.len() != 3) {
            return;
        }
        let mut at: HashMap<String, usize> = HashMap::new();
        let mut out: Vec<Vec<Value>> = Vec::new();
        for r in &res.rows {
            let i = *at.entry(js_key(&r[0])).or_insert_with(|| {
                out.push(vec![r[0].clone(), Value::Null, Value::Null]);
                out.len() - 1
            });
            if let Some(slot) = p.iter().position(|x| js_key(x) == js_key(&r[1])) {
                out[i][1 + slot] = r[2].clone();
            }
        }
        let value = res.columns[2].clone();
        let name = |v: Option<&Value>| v.map(js_key).unwrap_or_default();
        res.columns = vec![
            res.columns[0].clone(),
            OutColumn {
                name: name(p.first()),
                ..value.clone()
            },
            OutColumn {
                name: name(p.get(1)),
                ..value
            },
        ];
        res.rows = out;
        res.pair = Some(Pair { periods: Some(p) });
    }
    if res.rows.iter().any(|r| r.len() != 3) {
        return;
    }
    let num = |v: &Value| v.as_f64().filter(|x| x.is_finite());
    let gap = |r: &Vec<Value>| match (num(&r[1]), num(&r[2])) {
        (Some(a), Some(b)) => Some((b - a).abs()),
        _ => None,
    };
    match tile.sort.as_ref().map(|s| (s.by, s.desc)) {
        Some((SortBy::Label, desc)) => {
            res.rows.sort_by(|a, b| cmp_key(&a[0], &b[0]));
            if desc == Some(true) {
                res.rows.reverse();
            }
        }
        Some((SortBy::Value, desc)) => {
            res.rows.sort_by(|a, b| {
                let o = num(&b[1])
                    .unwrap_or(f64::NEG_INFINITY)
                    .total_cmp(&num(&a[1]).unwrap_or(f64::NEG_INFINITY));
                if desc == Some(false) { o.reverse() } else { o }
            });
        }
        // The default: the biggest gap first, rows with a missing value last.
        _ => res.rows.sort_by(|a, b| {
            gap(b)
                .unwrap_or(-1.0)
                .total_cmp(&gap(a).unwrap_or(-1.0))
                .then_with(|| cmp_key(&a[0], &b[0]))
        }),
    }
    if res.rows.len() > MAX_COMPARE_ROWS {
        res.total_rows = Some(res.rows.len() as u64);
        res.rows.truncate(MAX_COMPARE_ROWS);
    }
}

// ---- calendar ----------------------------------------------------------------------------

/// The first day of the month `n` months before the month of `d`.
fn month_start(d: NaiveDate, back: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap_or(d);
    first
        .checked_sub_months(chrono::Months::new(back))
        .unwrap_or(first)
}

/// A calendar shows the last 12 months that have data: the month of the latest day, and the 11
/// before it. The days come back in time order. A day with no date has no place on a calendar.
pub fn calendar_window(res: &mut QueryResult) {
    let latest = res
        .rows
        .iter()
        .filter_map(|r| r.first().and_then(Value::as_str).and_then(parse_day))
        .max();
    let Some(latest) = latest else {
        res.rows.clear();
        return;
    };
    let start = iso(month_start(latest, 11));
    res.rows.retain(|r| {
        r.first()
            .and_then(Value::as_str)
            .is_some_and(|d| d.get(..10).is_some_and(|d| d >= start.as_str()))
    });
    res.rows.sort_by(|a, b| cmp_key(&a[0], &b[0]));
}

// ---- Sankey ------------------------------------------------------------------------------

/// Sankey data. For each pair of neighbouring stages it reads the value of every pair of nodes,
/// and gives links as rows `[stage, from, to, value]`: `stage` is the index of the stage the link
/// starts at. Each stage keeps its biggest nodes (24, so with "Other" there are 25 at most) and the
/// rest fold into one "Other" node (key `OTHER_KEY`); links that meet there add up. A link with no
/// value, or a value of zero or less, has nothing to draw and is left out.
pub fn sankey(engine: &Engine, tile: &Tile, filters: &[Filter]) -> anyhow::Result<QueryResult> {
    let stages: Vec<&Dimension> = tile.dims.iter().take(SANKEY_STAGES).collect();
    if stages.len() < 2 {
        anyhow::bail!("A Sankey chart needs two or three fields, one for each stage.");
    }
    let m = tile
        .measures
        .first()
        .cloned()
        .unwrap_or_else(|| count_rows(&stages[0].table));
    // Nodes and "Other" add the link values up, so the values must add up (V4-D85d).
    if m.named.is_some() && !engine.measure_facts(&m)?.additive {
        anyhow::bail!(
            "A Sankey chart adds the flows up. Pick a measure that is a sum or a count, not a ratio or an average."
        );
    }
    let start = std::time::Instant::now();
    let mut links: Vec<(usize, Value, Value, f64)> = Vec::new();
    let mut ignored = vec![];
    let mut value_col = None;
    for i in 0..stages.len() - 1 {
        let r = engine.query(&SemanticQuery {
            measures: vec![m.clone()],
            dimensions: vec![stages[i].clone(), stages[i + 1].clone()],
            filters: filters.to_vec(),
            sort: vec![Sort { by: 2, desc: true }],
            limit: Some(SANKEY_LINKS),
        })?;
        // A field behind a link table puts a row in two nodes of one stage, so what passes through a
        // node would be counted again (V4-D112).
        if r.parts.iter().any(|p| p.overlap) {
            anyhow::bail!(
                "A Sankey chart follows each row from one stage to the next, and a field behind a link table puts a row in more than one place. Pick fields that are not behind a link table."
            );
        }
        if i == 0 {
            ignored = r.ignored_filters.clone();
            value_col = r.columns.get(2).cloned();
        }
        for row in r.rows {
            if let Some(v) = row
                .get(2)
                .and_then(Value::as_f64)
                .filter(|v| *v > 0.0 && v.is_finite())
            {
                links.push((i, row[0].clone(), row[1].clone(), v));
            }
        }
    }
    // What passes through each node: what leaves a first-stage node, what arrives at the others.
    let mut totals: Vec<HashMap<String, f64>> = vec![HashMap::new(); stages.len()];
    for (i, from, to, v) in &links {
        if *i == 0 {
            *totals[0].entry(js_key(from)).or_default() += v;
        }
        *totals[i + 1].entry(js_key(to)).or_default() += v;
    }
    // The nodes that stay, for each stage. The rest are "Other".
    let kept: Vec<HashSet<String>> = totals
        .iter()
        .map(|t| {
            let mut ranked: Vec<(&String, &f64)> = t.iter().collect();
            ranked.sort_by(|a, b| b.1.total_cmp(a.1).then_with(|| a.0.cmp(b.0)));
            if ranked.len() > MAX_NODES {
                ranked.truncate(MAX_NODES - 1);
            }
            ranked.into_iter().map(|(k, _)| k.clone()).collect()
        })
        .collect();
    let mut folded = false;
    let mut name = |stage: usize, v: &Value| -> Value {
        if kept[stage].contains(&js_key(v)) {
            v.clone()
        } else {
            folded = true;
            Value::String(OTHER_KEY.into())
        }
    };
    let mut merged: BTreeMap<(usize, String, String), (Value, Value, f64)> = BTreeMap::new();
    for (i, from, to, v) in &links {
        let (a, b) = (name(*i, from), name(i + 1, to));
        let e = merged
            .entry((*i, js_key(&a), js_key(&b)))
            .or_insert((a, b, 0.0));
        e.2 += v;
    }
    let mut rows: Vec<(usize, Value, Value, f64)> = merged
        .into_iter()
        .map(|((i, ..), (a, b, v))| (i, a, b, v))
        .collect();
    rows.sort_by(|x, y| {
        x.0.cmp(&y.0)
            .then_with(|| y.3.total_cmp(&x.3))
            .then_with(|| js_key(&x.1).cmp(&js_key(&y.1)))
            .then_with(|| js_key(&x.2).cmp(&js_key(&y.2)))
    });
    let dim = |name: &str, dtype| OutColumn {
        name: name.into(),
        role: "dimension".into(),
        dtype,
        unit: None,
    };
    let mut res = QueryResult::new(
        vec![
            dim("Stage", DType::Number),
            dim("From", DType::Text),
            dim("To", DType::Text),
            value_col.unwrap_or(OutColumn {
                name: "Value".into(),
                role: "measure".into(),
                dtype: DType::Number,
                unit: None,
            }),
        ],
        rows.into_iter()
            .map(|(i, a, b, v)| vec![json!(i), a, b, json!(v)])
            .collect(),
    );
    res.ignored_filters = ignored;
    res.other = folded.then_some(true);
    res.ms = start.elapsed().as_millis();
    Ok(res)
}

// ---- cohort grid -------------------------------------------------------------------------

/// How many periods of size `g` lie between two period starts: 0 for the same period.
pub fn cohort_offset(from: NaiveDate, to: NaiveDate, g: Grain) -> i64 {
    let months =
        i64::from(to.year() - from.year()) * 12 + i64::from(to.month()) - i64::from(from.month());
    match g {
        Grain::Year => i64::from(to.year() - from.year()),
        Grain::Quarter => months.div_euclid(3),
        Grain::Month => months,
        Grain::Week => (to - from).num_days().div_euclid(7),
        _ => (to - from).num_days(),
    }
}

/// Cohort data. The entity is the column of `measures[0]` (for example `customer_id`), the event
/// date is the field of `dims[0]` at a grain. An entity belongs to the cohort of its first period
/// with an event. Rows are `[cohort, periods since the cohort started, value, size]`: `value` is the
/// share of the cohort that has an event in that period (0 to 1), or, when `measures[1]` is set,
/// the sum of that column over the events of the cohort in that period. `size` is the number of
/// entities in the cohort. A cohort has a row for each period from its start to the last period
/// with data, also when nobody was active (0), and none after that. A period the data does not
/// reach the end of is left out, and a cohort that starts in it. The latest 36 cohorts are kept,
/// and 24 periods after each.
pub fn cohort(engine: &Engine, tile: &Tile, filters: &[Filter]) -> anyhow::Result<QueryResult> {
    let ent = tile
        .measures
        .first()
        .filter(|m| m.column.is_some())
        .ok_or_else(|| {
            anyhow::anyhow!("Pick the column that names who or what to follow, like customer_id.")
        })?;
    let ent_col = ent.column.clone().unwrap_or_default();
    let d = tile
        .dims
        .first()
        .ok_or_else(|| anyhow::anyhow!("Pick the date of the events, like the order date."))?;
    let Dim0::Period(grain) = classify(engine, d) else {
        anyhow::bail!(
            "The cohort date must be a date grouped by day, week, month, quarter or year."
        );
    };
    let amount = tile.measures.get(1);
    let mut measures = vec![Measure {
        table: ent.table.clone(),
        column: Some(ent_col.clone()),
        agg: Agg::Count,
        per: None,
        via: ent.via.clone(),
        named: None,
    }];
    measures.extend(amount.cloned());
    let q = SemanticQuery {
        measures,
        dimensions: vec![d.clone()],
        filters: filters.to_vec(),
        sort: vec![],
        limit: None,
    };
    let start = std::time::Instant::now();
    let (found, ignored) = engine.query_custom(&q, |sc| {
        let (e, _) = sc.expr_via(&ent.table, ent.via.as_deref(), &ent_col, None)?;
        let (p, _) = sc.expr_via(&d.table, d.via.as_deref(), &d.column, d.grain)?;
        let x = match amount.and_then(|a| a.column.as_deref().map(|c| (a, c))) {
            Some((a, c)) => format!(
                "CAST({} AS DOUBLE)",
                sc.expr_via(&a.table, a.via.as_deref(), c, None)?.0
            ),
            None => "1".to_string(),
        };
        Ok((
            format!(
                "WITH ev AS (SELECT {e} AS ent, {p} AS per, {x} AS x{}), \
                 fp AS (SELECT ent, min(per) AS c FROM ev \
                 WHERE ent IS NOT NULL AND per IS NOT NULL GROUP BY ent) \
                 SELECT fp.c, ev.per, CAST(count(DISTINCT ev.ent) AS DOUBLE), \
                 CAST(sum(ev.x) AS DOUBLE) FROM ev JOIN fp ON ev.ent = fp.ent \
                 WHERE ev.per IS NOT NULL GROUP BY fp.c, ev.per",
                sc.from_where
            ),
            sc.params.clone(),
        ))
    })?;
    let mut cells: Vec<(NaiveDate, NaiveDate, f64, f64)> = Vec::new();
    for r in &found {
        let day = |i: usize| r.get(i).and_then(Value::as_str).and_then(parse_day);
        if let (Some(c), Some(p)) = (day(0), day(1)) {
            let num = |i: usize| r.get(i).and_then(Value::as_f64).unwrap_or(0.0);
            cells.push((c, p, num(2), num(3)));
        }
    }
    // The last period counts only when the data reaches its last day (the rule of `sparks`): a month
    // that has just begun would draw as a drop. Each cohort then ends one period earlier, and a cohort
    // that starts in that period is left out.
    let mut last = cells.iter().map(|c| c.1).max();
    if let (Some(l), Some(max_day)) = (last, latest_day(engine, d, filters))
        && grain != Grain::Day
        && max_day < add_grain(l, grain, 1) - Duration::days(1)
    {
        last = Some(add_grain(l, grain, -1));
    }
    if let Some(l) = last {
        cells.retain(|c| c.0 <= l && c.1 <= l);
    }
    let mut cohorts: Vec<NaiveDate> = cells.iter().map(|c| c.0).collect();
    cohorts.sort();
    cohorts.dedup();
    let from = cohorts.len().saturating_sub(MAX_COHORTS);
    let cohorts = &cohorts[from..];
    let by: HashMap<(NaiveDate, i64), (f64, f64)> = cells
        .iter()
        .map(|(c, p, n, s)| ((*c, cohort_offset(*c, *p, grain)), (*n, *s)))
        .collect();
    let sum_mode = amount.is_some();
    let mut rows = Vec::new();
    if let Some(last) = last {
        for c in cohorts {
            let size = by.get(&(*c, 0)).map_or(0.0, |v| v.0);
            let reach = cohort_offset(*c, last, grain).min(MAX_COHORT_OFFSET);
            for k in 0..=reach {
                let (n, s) = by.get(&(*c, k)).copied().unwrap_or((0.0, 0.0));
                let value = if sum_mode {
                    s
                } else if size > 0.0 {
                    n / size
                } else {
                    0.0
                };
                rows.push(vec![json!(iso(*c)), json!(k), json!(value), json!(size)]);
            }
        }
    }
    let col = |name: &str, role: &str, dtype: DType, unit: Option<Unit>| OutColumn {
        name: name.into(),
        role: role.into(),
        dtype,
        unit,
    };
    let mut res = QueryResult::new(
        vec![
            col("Cohort", "dimension", DType::Date, None),
            col("Periods since the start", "dimension", DType::Number, None),
            col(
                if sum_mode { "Total" } else { "Active" },
                "measure",
                DType::Number,
                (!sum_mode).then_some(Unit::Percent),
            ),
            col("Size", "measure", DType::Number, None),
        ],
        rows,
    );
    res.ignored_filters = ignored;
    res.ms = start.elapsed().as_millis();
    Ok(res)
}

// ---- trend column ------------------------------------------------------------------------

/// The key of a table row in `res.sparks`: the values of its groups, joined by a unit separator.
pub fn spark_key(groups: &[Value]) -> String {
    groups.iter().map(js_key).collect::<Vec<_>>().join("\u{1f}")
}

/// The first day of the period of grain `g` that holds `d`. Weeks start on Monday.
pub fn period_start(d: NaiveDate, g: Grain) -> NaiveDate {
    match g {
        Grain::Year => NaiveDate::from_ymd_opt(d.year(), 1, 1),
        Grain::Quarter => NaiveDate::from_ymd_opt(d.year(), d.month0() / 3 * 3 + 1, 1),
        Grain::Month => NaiveDate::from_ymd_opt(d.year(), d.month(), 1),
        Grain::Week => Some(d - Duration::days(i64::from(d.weekday().num_days_from_monday()))),
        _ => Some(d),
    }
    .unwrap_or(d)
}

/// The latest day of a date field under the filters, or None when there is no date.
fn latest_day(engine: &Engine, field: &Dimension, filters: &[Filter]) -> Option<NaiveDate> {
    let latest = engine
        .query(&SemanticQuery {
            measures: vec![Measure {
                table: field.table.clone(),
                column: Some(field.column.clone()),
                agg: Agg::Max,
                per: None,
                via: field.via.clone(),
                named: None,
            }],
            dimensions: vec![],
            filters: filters.to_vec(),
            sort: vec![],
            limit: None,
        })
        .ok()?;
    latest
        .rows
        .first()
        .and_then(|r| r.first())
        .and_then(Value::as_str)
        .and_then(parse_day)
}

/// A table with a trend column (`tile.spark`) gets, for each of its rows, the first value over the
/// last 12 periods of the date field that are over: the period of the latest date counts only when
/// the data reaches its last day, so a month that has just begun does not draw as a drop. Periods with no
/// rows are 0 for sums and counts and gaps for every other kind of value, the way a time chart
/// fills them. The rows of the table are the ones asked for; the "Other" row of a top N has none.
pub fn sparks(
    engine: &Engine,
    tile: &Tile,
    q: &SemanticQuery,
    res: &mut QueryResult,
) -> anyhow::Result<()> {
    let (Some(sd), Some(m0)) = (tile.spark.as_ref(), q.measures.first()) else {
        return Ok(());
    };
    if tile.kind != "table" || q.dimensions.is_empty() || res.rows.is_empty() {
        return Ok(());
    }
    let grain = sd.grain.filter(|g| !g.is_part()).unwrap_or(Grain::Month);
    let nd = q.dimensions.len();
    let Some(max_day) = latest_day(engine, sd, &q.filters) else {
        return Ok(());
    };
    // The period of the latest date is left out while it is not over (the same rule as a time chart).
    let mut last = period_start(max_day, grain);
    if grain != Grain::Day && max_day < add_grain(last, grain, 1) - Duration::days(1) {
        last = add_grain(last, grain, -1);
    }
    let first = add_grain(last, grain, 1 - SPARK_POINTS as i32);
    let periods: Vec<String> = (0..SPARK_POINTS as i32)
        .map(|i| iso(add_grain(first, grain, i)))
        .collect();
    // Only the rows on show are read: the first group of the table, in the values shown.
    let shown: Vec<Value> = {
        let mut seen = HashSet::new();
        res.rows
            .iter()
            .map(|r| r[0].clone())
            .filter(|v| js_key(v) != OTHER_KEY && seen.insert(js_key(v)))
            .collect()
    };
    let mut filters = q.filters.clone();
    filters.push(Filter {
        table: q.dimensions[0].table.clone(),
        column: q.dimensions[0].column.clone(),
        grain: q.dimensions[0].grain,
        op: FilterOp::In,
        values: shown,
        via: q.dimensions[0].via.clone(),
    });
    filters.push(Filter {
        table: sd.table.clone(),
        column: sd.column.clone(),
        grain: None,
        op: FilterOp::Gte,
        values: vec![json!(iso(first))],
        via: sd.via.clone(),
    });
    let mut dims = q.dimensions.clone();
    dims.push(Dimension {
        grain: Some(grain),
        ..sd.clone()
    });
    let found = engine.query(&SemanticQuery {
        measures: vec![m0.clone()],
        dimensions: dims,
        filters,
        sort: vec![],
        limit: Some(10_000),
    })?;
    // Nothing happened is 0 for a sum or a count and a gap for the rest.
    let empty = engine
        .empty_values(std::slice::from_ref(m0))
        .first()
        .and_then(Value::as_f64);
    let mut lines: BTreeMap<String, Vec<Option<f64>>> = BTreeMap::new();
    for r in &res.rows {
        if js_key(&r[0]) != OTHER_KEY {
            lines.insert(spark_key(&r[..nd]), vec![empty; SPARK_POINTS]);
        }
    }
    for r in &found.rows {
        let key = spark_key(&r[..nd]);
        let at = r
            .get(nd)
            .and_then(Value::as_str)
            .and_then(|p| periods.iter().position(|x| p.get(..10) == Some(x.as_str())));
        if let (Some(line), Some(i)) = (lines.get_mut(&key), at) {
            line[i] = r.get(nd + 1).and_then(Value::as_f64);
        }
    }
    res.sparks = Some(lines);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn cohort_offsets_count_whole_periods() {
        let m = |a, b| cohort_offset(d(a), d(b), Grain::Month);
        assert_eq!(m("2026-01-01", "2026-01-01"), 0);
        assert_eq!(m("2026-01-01", "2026-03-01"), 2);
        // Across a year end.
        assert_eq!(m("2025-11-01", "2026-02-01"), 3);
        assert_eq!(
            cohort_offset(d("2025-10-01"), d("2026-04-01"), Grain::Quarter),
            2
        );
        assert_eq!(
            cohort_offset(d("2024-01-01"), d("2026-01-01"), Grain::Year),
            2
        );
        assert_eq!(
            cohort_offset(d("2026-01-05"), d("2026-01-26"), Grain::Week),
            3
        );
        assert_eq!(
            cohort_offset(d("2026-01-05"), d("2026-01-08"), Grain::Day),
            3
        );
    }

    #[test]
    fn period_starts() {
        assert_eq!(period_start(d("2026-05-17"), Grain::Month), d("2026-05-01"));
        assert_eq!(
            period_start(d("2026-05-17"), Grain::Quarter),
            d("2026-04-01")
        );
        assert_eq!(period_start(d("2026-05-17"), Grain::Year), d("2026-01-01"));
        // 17 May 2026 is a Sunday: its week began on Monday 11 May.
        assert_eq!(period_start(d("2026-05-17"), Grain::Week), d("2026-05-11"));
    }

    #[test]
    fn spark_keys_join_the_groups() {
        assert_eq!(spark_key(&[json!("A")]), "A");
        assert_eq!(spark_key(&[json!("A"), json!(2)]), "A\u{1f}2");
    }

    #[test]
    fn the_calendar_keeps_twelve_months() {
        let mut res = QueryResult::new(vec![], vec![]);
        res.rows = ["2026-03-20", "2025-04-02", "2025-03-31", "2025-06-15"]
            .iter()
            .map(|d| vec![json!(d), json!(1)])
            .collect();
        calendar_window(&mut res);
        let days: Vec<&str> = res.rows.iter().map(|r| r[0].as_str().unwrap()).collect();
        assert_eq!(days, ["2025-04-02", "2025-06-15", "2026-03-20"]);
    }
}
