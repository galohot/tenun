//! The engine side of the Advanced charts: small multiples (the charts, and the top N inside
//! each), indexed lines, a manual order of the groups, and box plots. They are computed here so
//! the app, exported files, published dashboards and the headless agent agree; the UI only draws.

use crate::catalog::DType;
use crate::engine::{Engine, QueryResult, Repeat};
use crate::query::{Dimension, Filter, FilterOp, OutColumn, SemanticQuery};
use crate::shape::{Dim0, Layout, OTHER_KEY, classify, cmp_key};
use crate::snapshot::{SortBy, Tile, TopN, js_key};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// A small-multiples tile draws at most this many charts. The rest are counted ("and 5 more").
pub const MAX_CHARTS: usize = 12;
/// A box plot draws at most this many groups, the biggest ones by row count.
pub const MAX_BOX_GROUPS: usize = 40;
/// A box plot draws at most this many outlier dots for each group.
pub const MAX_OUTLIERS: usize = 50;
/// A repeat field is read for up to this many values, to find the charts.
const REPEAT_READ: u32 = 5000;

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
            });
            filters.push(Filter {
                table: d1.table.clone(),
                column: d1.column.clone(),
                grain: d1.grain,
                op: FilterOp::In,
                values: vec![key.clone()],
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
            let (x, _) = sc.expr(&m.table, &column, None)?;
            let (g, gtype) = match group {
                Some(d) => sc.expr(&d.table, &d.column, d.grain)?,
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
    })
}
