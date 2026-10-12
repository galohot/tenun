//! Filters and drill: filter tiles, the filters a dashboard saves, one grain for every time
//! tile, the values a filter tile offers, and the rows behind a mark.
//!
//! Everything a filter tile selects becomes an ordinary `DashFilter` (`from_tile` = the tile),
//! so the live dashboard, the export and a published dashboard all filter the same way.

use crate::catalog::DType;
use crate::engine::{Bins, Engine, QueryResult};
use crate::query::{
    Agg, Dimension, Filter, FilterOp, Grain, Measure, OutColumn, Param, RELATIVE_PRESETS,
    SemanticQuery, Sort,
};
use crate::snapshot::{DashFilter, Tile, parse_day, tile_query};
use chrono::Duration;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A list or dropdown filter shows at most this many values.
pub const MAX_OPTIONS: u32 = 200;
/// The slider of a range filter sits on a histogram of this many bins.
pub const RANGE_BINS: usize = 24;
/// "Show rows" returns at most this many rows.
pub const MAX_ROWS: u32 = 500;

/// What a filter tile is and what it has selected (`tile.filter`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TileFilter {
    /// `list`, `dropdown`, `range` or `dates`.
    #[serde(default)]
    pub style: String,
    /// `list` and `dropdown`: the values kept. `null` is the blank value. Empty keeps everything.
    #[serde(default)]
    pub values: Option<Vec<Value>>,
    /// `range`: the lowest and highest number kept. `dates` (custom): the first and last day kept.
    /// Either end may be `null`.
    #[serde(default)]
    pub range: Option<Vec<Value>>,
    /// `dates`: one of the relative presets. It wins over `range`.
    #[serde(default)]
    pub preset: Option<String>,
}

/// The filter a filter tile stands for, or `None` while it keeps everything.
pub fn tile_filter(engine: &Engine, tile: &Tile) -> Option<DashFilter> {
    if tile.kind != "filter" {
        return None;
    }
    let f = tile.filter.as_ref()?;
    let d = tile.dims.first()?;
    let make = |op, values| DashFilter {
        filter: Filter {
            table: d.table.clone(),
            column: d.column.clone(),
            grain: None,
            op,
            values,
            via: d.via.clone(),
        },
        from_tile: tile.id.clone(),
    };
    match f.style.as_str() {
        "range" => {
            let [lo, hi] = ends(f.range.as_deref()?);
            match (lo.is_null(), hi.is_null()) {
                (true, true) => None,
                (false, true) => Some(make(FilterOp::Gte, vec![lo])),
                (true, false) => Some(make(FilterOp::Lte, vec![hi])),
                (false, false) => Some(make(FilterOp::Between, vec![lo, hi])),
            }
        }
        "dates" => {
            if let Some(p) = f.preset.as_deref().filter(|p| RELATIVE_PRESETS.contains(p)) {
                return Some(make(FilterOp::Relative, vec![json!(p)]));
            }
            // A custom span of whole days: the last day is kept in full, so the range ends at
            // the start of the day after it (a date-time late on that day is still inside).
            let [lo, hi] = ends(f.range.as_deref()?);
            let day = |v: &Value| v.as_str().and_then(parse_day);
            let after = |v: &Value| {
                day(v).map(|d| json!((d + Duration::days(1)).format("%Y-%m-%d").to_string()))
            };
            match (day(&lo).map(|_| lo.clone()), after(&hi)) {
                (Some(lo), Some(hi)) => Some(make(FilterOp::Range, vec![lo, hi])),
                (Some(lo), None) => Some(make(FilterOp::Gte, vec![lo])),
                (None, Some(hi)) => Some(make(FilterOp::Range, vec![json!("0001-01-01"), hi])),
                (None, None) => None,
            }
        }
        _ => {
            let values = f.values.as_ref().filter(|v| !v.is_empty())?;
            Some(make(
                FilterOp::In,
                with_empty_text(engine, &d.table, &d.column, values.clone()),
            ))
        }
    }
}

fn ends(r: &[Value]) -> [Value; 2] {
    [
        r.first().cloned().unwrap_or(Value::Null),
        r.get(1).cloned().unwrap_or(Value::Null),
    ]
}

/// A text column keeps empty text beside `NULL`, so "(blank)" means both.
pub fn with_empty_text(
    engine: &Engine,
    table: &str,
    column: &str,
    mut v: Vec<Value>,
) -> Vec<Value> {
    if v.contains(&Value::Null)
        && engine.column_dtype(table, column) == Some(DType::Text)
        && !v.contains(&json!(""))
    {
        v.push(json!(""));
    }
    v
}

/// The filters a dashboard saves ("Keep only", "Exclude"), as written in it.
pub fn saved_filters(dash: &Value) -> Vec<Value> {
    dash.get("filters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The filters of a dashboard: the ones it saves (`filters`), then one for each filter tile.
pub fn dashboard_filters(engine: &Engine, dash: &Value, tiles: &[Tile]) -> Vec<DashFilter> {
    let mut out: Vec<DashFilter> = dash
        .get("filters")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|f| serde_json::from_value(f.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    out.extend(tiles.iter().filter_map(|t| tile_filter(engine, t)));
    out
}

/// The grain a dashboard asks for, if it asks for one that is a period.
pub fn dashboard_grain(dash: &Value) -> Option<Grain> {
    serde_json::from_value::<Grain>(dash.get("grain")?.clone())
        .ok()
        .filter(|g| !g.is_part())
}

/// Gives a tile the dashboard's grain when its first group is a date cut into periods.
pub fn apply_grain(tile: &mut Value, grain: Grain) {
    // A text tile's live numbers cover a period of this grain (build 10).
    if tile.get("type").and_then(Value::as_str) == Some("text") && !grain.is_part() {
        tile["grain"] = serde_json::to_value(grain).unwrap_or(Value::Null);
        return;
    }
    let first = tile
        .get_mut("dims")
        .and_then(Value::as_array_mut)
        .and_then(|d| d.first_mut());
    if let Some(d) = first
        && let Some(own) = d
            .get("grain")
            .and_then(|g| serde_json::from_value::<Grain>(g.clone()).ok())
        && !own.is_part()
    {
        d["grain"] = serde_json::to_value(grain).unwrap_or(Value::Null);
    }
}

// ---- what a filter tile offers -------------------------------------------------

fn col(name: &str, role: &str, dtype: DType) -> OutColumn {
    OutColumn {
        name: name.into(),
        role: role.into(),
        dtype,
        unit: None,
    }
}

/// The values behind a filter tile. `filters` are the other filters of the dashboard (not its own).
/// - `list` and `dropdown`: rows `[value, count]`, the biggest first, at most 200. Blank text and
///   `NULL` are one row with the value `null`. The other filters narrow the counts.
/// - `range`: 24 bins `[bin_start, count]` over the lowest to the highest number, in `bins`.
/// - `dates`: one row `[first day, last day]`.
///
/// A range and a span of dates read the whole field, whatever the other filters say, so the ends
/// of a slider or a date box do not move while the user picks elsewhere.
pub fn options(engine: &Engine, tile: &Tile, filters: &[Filter]) -> anyhow::Result<QueryResult> {
    let Some(d) = tile.dims.first() else {
        return Ok(QueryResult::plain(vec![], vec![]));
    };
    let style = tile.filter.as_ref().map_or("list", |f| f.style.as_str());
    let filters = if matches!(style, "range" | "dates") {
        &[]
    } else {
        filters
    };
    let dtype = engine
        .column_dtype(&d.table, &d.column)
        .ok_or_else(|| anyhow::anyhow!("The column {} is not in the table.", d.column))?;
    let plain = Dimension {
        table: d.table.clone(),
        column: d.column.clone(),
        grain: None,
        via: d.via.clone(),
    };
    match style {
        "range" => range_bins(engine, &plain, dtype, filters),
        "dates" => {
            if !dtype.is_temporal() {
                anyhow::bail!("{} is not a date column.", d.column);
            }
            let ask = |agg| Measure {
                table: d.table.clone(),
                column: Some(d.column.clone()),
                agg,
                per: None,
                via: d.via.clone(),
                named: None,
            };
            let mut res = engine.query(&SemanticQuery {
                measures: vec![ask(Agg::Min), ask(Agg::Max)],
                dimensions: vec![],
                filters: filters.to_vec(),
                sort: vec![],
                limit: None,
            })?;
            res.columns[0].name = "first".into();
            res.columns[1].name = "last".into();
            Ok(res)
        }
        _ => {
            let mut res = engine.query(&SemanticQuery {
                measures: vec![Measure {
                    table: d.table.clone(),
                    column: None,
                    agg: Agg::Count,
                    per: None,
                    via: d.via.clone(),
                    named: None,
                }],
                dimensions: vec![plain],
                filters: filters.to_vec(),
                sort: vec![Sort { by: 1, desc: true }],
                limit: Some(MAX_OPTIONS),
            })?;
            // Empty text reads as blank, like NULL.
            if dtype == DType::Text {
                let mut blank = 0.0;
                res.rows.retain(|r| {
                    let is_blank = r[0].is_null() || r[0] == json!("");
                    if is_blank {
                        blank += r[1].as_f64().unwrap_or(0.0);
                    }
                    !is_blank
                });
                if blank > 0.0 {
                    let at = res
                        .rows
                        .iter()
                        .position(|r| r[1].as_f64().unwrap_or(0.0) < blank)
                        .unwrap_or(res.rows.len());
                    res.rows.insert(at, vec![Value::Null, json!(blank)]);
                }
            }
            Ok(res)
        }
    }
}

/// A tiny histogram for the slider of a range filter: `RANGE_BINS` equal bins from the lowest
/// to the highest number, so the ends of the slider are the real ends of the data.
fn range_bins(
    engine: &Engine,
    d: &Dimension,
    dtype: DType,
    filters: &[Filter],
) -> anyhow::Result<QueryResult> {
    if dtype != DType::Number {
        anyhow::bail!(
            "{} is not a number column. A range filter needs one.",
            d.column
        );
    }
    let q = SemanticQuery {
        measures: vec![Measure {
            table: d.table.clone(),
            column: Some(d.column.clone()),
            agg: Agg::Count,
            per: None,
            via: d.via.clone(),
            named: None,
        }],
        dimensions: vec![],
        filters: filters.to_vec(),
        sort: vec![],
        limit: None,
    };
    let (stats, ignored) = engine.query_custom(&q, |sc| {
        let (x, _) = sc.expr_via(&d.table, d.via.as_deref(), &d.column, None)?;
        Ok((
            format!("SELECT count({x}), min({x}), max({x}){}", sc.from_where),
            sc.params.clone(),
        ))
    })?;
    let num = |i: usize| stats.first().and_then(|r| r.get(i)).and_then(Value::as_f64);
    let columns = vec![
        col(&d.column, "dimension", DType::Number),
        col("Rows", "measure", DType::Number),
    ];
    let (Some(n), Some(min), Some(max)) = (num(0), num(1), num(2)) else {
        return Ok(QueryResult::plain(columns, vec![]));
    };
    if n < 1.0 {
        return Ok(QueryResult::plain(columns, vec![]));
    }
    let width = if max > min {
        (max - min) / RANGE_BINS as f64
    } else {
        1.0
    };
    let (found, _) = engine.query_custom(&q, |sc| {
        let (x, _) = sc.expr_via(&d.table, d.via.as_deref(), &d.column, None)?;
        let mut params = vec![Param::Double(min), Param::Double(width)];
        params.extend(sc.params.iter().cloned());
        Ok((
            format!(
                "SELECT b, count(*) FROM (SELECT LEAST(CAST(floor(({x} - CAST(? AS DOUBLE)) / \
                 CAST(? AS DOUBLE)) AS BIGINT), {}) AS b{}) WHERE b IS NOT NULL GROUP BY b",
                RANGE_BINS - 1,
                sc.from_where
            ),
            params,
        ))
    })?;
    let mut counts = [0.0; RANGE_BINS];
    for r in &found {
        if let (Some(b), Some(c)) = (r[0].as_i64(), r[1].as_f64())
            && let Some(slot) = usize::try_from(b).ok().and_then(|b| counts.get_mut(b))
        {
            *slot += c;
        }
    }
    let mut res = QueryResult::plain(
        columns,
        counts
            .iter()
            .enumerate()
            .map(|(i, c)| {
                vec![
                    json!(crate::shape::round_edge(min + i as f64 * width)),
                    json!(c),
                ]
            })
            .collect(),
    );
    res.ignored_filters = ignored;
    res.bins = Some(Bins { start: min, width });
    Ok(res)
}

// ---- the rows behind a mark ----------------------------------------------------

/// One column of "Show rows".
#[derive(Debug, Clone, Serialize)]
pub struct RowColumn {
    /// The heading. A column of a related table reads "Table · column".
    pub label: String,
    pub dtype: DType,
}

/// The raw rows behind a tile or one of its marks.
#[derive(Debug, Clone, Serialize)]
pub struct RowsBehind {
    pub columns: Vec<RowColumn>,
    pub rows: Vec<Vec<Value>>,
    /// How many rows match. `rows` holds the first 500 of them.
    pub total: u64,
}

/// Up to 500 raw rows of the tile's base table that fall under its filters and the clicked mark.
/// `click` is the filter a click on the mark makes (the same one the cross-filter uses). The
/// columns are all the base table's, then the columns the tile uses from related
/// tables, which are joined the way the tile joins them.
pub fn rows_behind(
    engine: &Engine,
    tile: &Tile,
    filters: &[DashFilter],
    click: Option<Filter>,
) -> anyhow::Result<RowsBehind> {
    let mut q = tile_query(tile, filters)
        .ok_or_else(|| anyhow::anyhow!("This tile has no rows to show yet."))?;
    q.sort = vec![];
    q.limit = None;
    if let Some(c) = click {
        q.filters.push(c);
    }
    // A named measure adds up the rows of the tables its formula reads. With one table the rows
    // behind it are that table's; with more, there is no one list of rows, and the words say so.
    if let Some(m) = q.measures.iter().find(|m| m.named.is_some()) {
        let f = engine.measure_facts(m)?;
        let [home] = f.homes.as_slice() else {
            let catalog = engine.catalog();
            let labels: Vec<String> = f
                .homes
                .iter()
                .map(|t| catalog.table(t).map_or(t.clone(), |t| t.label.clone()))
                .collect();
            anyhow::bail!(
                "{} adds up rows of more than one table ({}), so there is no one list of rows behind it.",
                f.name,
                labels.join(", ")
            );
        };
        let home = home.clone();
        q.measures.retain(|m| m.named.is_none());
        q.measures.insert(0, crate::snapshot::count_rows(&home));
        // The groups and filters this table cannot reach are read the way the measure reads them: a date
        // that is a main date through the main date of this table, the rest left out. A click on a month
        // of a chart of sales by month, for a measure that adds up targets, is a click on its month
        // (V4-D98).
        q = engine.narrow_for_rows(&q, &home)?;
    }
    let base = q
        .measures
        .first()
        .map(|m| m.table.clone())
        .or_else(|| q.dimensions.first().map(|d| d.table.clone()))
        .ok_or_else(|| anyhow::anyhow!("This tile has no rows to show yet."))?;
    let catalog = engine.catalog();
    let table = |id: &str| {
        catalog
            .table(id)
            .ok_or_else(|| anyhow::anyhow!("The table {id} no longer exists."))
    };
    // Every base column, then each column a related table gives to the tile. A table read
    // through a named link is a field of its own (V4-D28).
    type Field = (String, Option<String>, String);
    let mut fields: Vec<Field> = table(&base)?
        .columns
        .iter()
        .map(|c| (base.clone(), None, c.name.clone()))
        .collect();
    let mut used: Vec<(&str, Option<&str>, &str)> = vec![];
    for d in &q.dimensions {
        used.push((&d.table, d.via.as_deref(), &d.column));
    }
    for m in &q.measures {
        if let Some(c) = &m.column {
            used.push((&m.table, m.via.as_deref(), c));
        }
        if let Some(p) = &m.per
            && let Some(c) = &p.column
        {
            used.push((&p.table, p.via.as_deref(), c));
        }
    }
    for (t, via, c) in used {
        let known = catalog.table(t).is_some_and(|x| x.column(c).is_some());
        if t != base
            && known
            && !fields
                .iter()
                .any(|(ft, fv, fc)| ft == t && fv.as_deref() == via && fc == c)
        {
            fields.push((t.to_string(), via.map(str::to_string), c.to_string()));
        }
    }
    let mut columns = Vec::new();
    for (t, via, c) in &fields {
        let tab = table(t)?;
        let dtype = tab.column(c).map_or(DType::Text, |x| x.dtype);
        columns.push(RowColumn {
            label: if *t == base {
                c.clone()
            } else {
                let from = via
                    .as_deref()
                    .and_then(|v| engine.link_label(v))
                    .unwrap_or_else(|| tab.label.clone());
                format!("{from} · {c}")
            },
            dtype,
        });
    }
    let (counted, _) = engine.query_custom(&q, |sc| {
        Ok((
            format!("SELECT count(*){}", sc.from_where),
            sc.params.clone(),
        ))
    })?;
    let total = counted
        .first()
        .and_then(|r| r.first())
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let (rows, _) = engine.query_custom(&q, |sc| {
        let mut exprs = Vec::new();
        for (i, (t, via, c)) in fields.iter().enumerate() {
            let (e, _) = sc.expr_via(t, via.as_deref(), c, None)?;
            exprs.push(format!(
                "{e} AS {}",
                crate::catalog::ident(&format!("c{i}"))
            ));
        }
        Ok((
            format!(
                "SELECT {}{} LIMIT {MAX_ROWS}",
                exprs.join(", "),
                sc.from_where
            ),
            sc.params.clone(),
        ))
    })?;
    Ok(RowsBehind {
        columns,
        rows,
        total,
    })
}
