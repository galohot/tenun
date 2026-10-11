//! The semantic query and its compiler.
//!
//! A dashboard tile asks for measures grouped by dimensions, with filters. The
//! compiler picks a base table (the table of the measures), walks relationships
//! from the many side to the one side to reach every other table it needs, and
//! emits one SQL statement. Identifiers are quoted; every value is a parameter.

use crate::catalog::{Catalog, DType, Table, ident};
use crate::links::{Edge, Links, PathError, Paths, join_on};
use chrono::{Datelike, Duration, Months, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SemanticQuery {
    #[serde(default)]
    pub measures: Vec<Measure>,
    #[serde(default)]
    pub dimensions: Vec<Dimension>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub sort: Vec<Sort>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measure {
    pub table: String,
    /// `None` with `count` means "number of rows".
    #[serde(default)]
    pub column: Option<String>,
    pub agg: Agg,
    /// Makes a ratio: `agg(column)` divided by `agg(per.column)`, in the same SELECT.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per: Option<Per>,
    /// The link (a relationship id) the path to `table` must cross, when two links lead to it
    /// (V4-D28). Not set: the default path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

/// The bottom of a ratio measure. It may come from any table the query can reach.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Per {
    pub table: String,
    #[serde(default)]
    pub column: Option<String>,
    pub agg: Agg,
    /// The link the path to `table` must cross (see [`Measure::via`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Agg {
    Sum,
    Avg,
    Min,
    Max,
    Count,
    CountDistinct,
    /// The middle value. Numbers only.
    Median,
    /// The 90th percentile. Numbers only.
    P90,
    /// Sample standard deviation. Numbers only. The UI offers it in Advanced.
    Stdev,
    /// Sample variance. Numbers only. The UI offers it in Advanced.
    Variance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dimension {
    pub table: String,
    pub column: String,
    #[serde(default)]
    pub grain: Option<Grain>,
    /// The link the path to `table` must cross (see [`Measure::via`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Grain {
    #[serde(rename = "year")]
    Year,
    #[serde(rename = "quarter")]
    Quarter,
    #[serde(rename = "month")]
    Month,
    #[serde(rename = "week")]
    Week,
    #[serde(rename = "day")]
    Day,
    /// Date parts: the month (1–12), the weekday (1–7, Monday = 1) or the hour (0–23),
    /// whatever the year. They come back as numbers.
    #[serde(rename = "month_of_year")]
    MonthOfYear,
    #[serde(rename = "weekday")]
    Weekday,
    #[serde(rename = "hour")]
    Hour,
}

impl Grain {
    /// A date part, not a period. Parts are numbers and read in ascending order.
    pub fn is_part(self) -> bool {
        matches!(self, Grain::MonthOfYear | Grain::Weekday | Grain::Hour)
    }

    /// The SQL for this grain applied to a date or time expression.
    fn sql(self, r: &str) -> String {
        let part = |f: &str| format!("CAST({f}({r}) AS DOUBLE)");
        let trunc = |u: &str| format!("strftime(date_trunc('{u}', {r}), '%Y-%m-%d')");
        match self {
            Grain::Year => trunc("year"),
            Grain::Quarter => trunc("quarter"),
            Grain::Month => trunc("month"),
            Grain::Week => trunc("week"),
            Grain::Day => trunc("day"),
            Grain::MonthOfYear => part("month"),
            Grain::Weekday => part("isodow"),
            Grain::Hour => part("hour"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub table: String,
    pub column: String,
    #[serde(default)]
    pub grain: Option<Grain>,
    pub op: FilterOp,
    #[serde(default)]
    pub values: Vec<serde_json::Value>,
    /// The link the path to `table` must cross (see [`Measure::via`]). A click on a mark carries
    /// the `via` of the field it came from, so it filters by that role only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    In,
    NotIn,
    Between,
    Gte,
    Lte,
    /// `[lo, hi]`, with lo ≤ x < hi.
    Range,
    /// The text contains the value, whatever the case. `%` and `_` in the value are plain characters.
    Contains,
    /// The text starts with the value, whatever the case.
    StartsWith,
    /// No value: empty, or `NULL`.
    Blank,
    /// The opposite of `Blank`.
    NotBlank,
    /// `[preset]` for a date column: `last_7_days`, `last_30_days`, `this_month`, `last_month`,
    /// `this_quarter`, `last_quarter`, `ytd`, `last_12_months`. The engine resolves it against
    /// today's date into a half-open range (see `relative_range`).
    Relative,
}

/// The presets a `Relative` filter understands.
pub const RELATIVE_PRESETS: [&str; 8] = [
    "last_7_days",
    "last_30_days",
    "this_month",
    "last_month",
    "this_quarter",
    "last_quarter",
    "ytd",
    "last_12_months",
];

/// The date this computer shows right now.
pub fn local_today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// A relative preset as a half-open range of days: `lo ≤ day < hi`. The last day counted is
/// today, so "last 7 days" is today and the six days before. "This month", "this quarter" and
/// "year to date" run from their start to today. A date-time column keeps its whole last day,
/// because the range ends at the start of the next one.
pub fn relative_range(preset: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let tomorrow = today + Duration::days(1);
    let month = today.with_day(1)?;
    let quarter = month.with_month0(today.month0() / 3 * 3)?;
    Some(match preset {
        "last_7_days" => (today - Duration::days(6), tomorrow),
        "last_30_days" => (today - Duration::days(29), tomorrow),
        "this_month" => (month, tomorrow),
        "last_month" => (month.checked_sub_months(Months::new(1))?, month),
        "this_quarter" => (quarter, tomorrow),
        "last_quarter" => (quarter.checked_sub_months(Months::new(3))?, quarter),
        "ytd" => (today.with_month(1)?.with_day(1)?, tomorrow),
        "last_12_months" => (tomorrow.checked_sub_months(Months::new(12))?, tomorrow),
        _ => return None,
    })
}

/// A `Relative` filter written as the `Range` it stands for today. Other filters are unchanged.
pub fn resolve_relative(f: &Filter, today: NaiveDate) -> Filter {
    let range = (f.op == FilterOp::Relative)
        .then(|| f.values.first().and_then(|v| v.as_str()))
        .flatten()
        .and_then(|p| relative_range(p, today));
    match range {
        Some((lo, hi)) => Filter {
            op: FilterOp::Range,
            values: vec![
                serde_json::json!(lo.format("%Y-%m-%d").to_string()),
                serde_json::json!(hi.format("%Y-%m-%d").to_string()),
            ],
            ..f.clone()
        },
        None => f.clone(),
    }
}

/// The value of a `LIKE` pattern with `\`, `%` and `_` made plain characters.
fn like_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sort {
    /// Index into the output columns (dimensions first, then measures).
    pub by: usize,
    #[serde(default)]
    pub desc: bool,
}

/// A bound value. Kept independent of DuckDB so the compiler is testable alone.
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Null,
    Text(String),
    Double(f64),
    Bool(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutColumn {
    pub name: String,
    pub role: String,
    pub dtype: DType,
    /// Set when a value is shown another way (`tile.showAs`): the number is a share or a rank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<Unit>,
}

/// What a shaped value counts. A `percent` is a fraction: 0.25 reads as 25 %.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Percent,
    Rank,
}

#[derive(Debug, Clone)]
pub struct Compiled {
    pub sql: String,
    pub params: Vec<Param>,
    pub columns: Vec<OutColumn>,
    /// Indexes of filters that could not reach the base table and were skipped.
    pub ignored_filters: Vec<usize>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum QueryError {
    #[error("Pick at least one value or group.")]
    Empty,
    #[error("The table {0} no longer exists.")]
    UnknownTable(String),
    #[error("The column {1} is not in {0}.")]
    UnknownColumn(String, String),
    #[error("{1} is not related to {0}. Draw a line between them in Model.")]
    Unreachable(String, String),
    #[error("A field of this tile uses a link that was removed. Pick the field again.")]
    LinkGone,
    #[error(
        "{1} cannot be used with {0} now: the link between them repeats its values on both sides. Open Model to see which file to fix."
    )]
    LinkUnusable(String, String),
    #[error("{0} cannot be {1}.")]
    BadAgg(String, &'static str),
    #[error("A filter value does not match the column type.")]
    BadValue,
    #[error("A date filter needs a date column. {0} is not one.")]
    NotADate(String),
}

const DEFAULT_LIMIT: u32 = 1000;
const MAX_LIMIT: u32 = 10_000;

/// The FROM, JOIN and WHERE part of a semantic query, with its parameters.
/// `compile()` builds on it, and custom SELECTs (bins, quantiles) reuse it so
/// they get the same joins and filters. Values stay parameters and identifiers
/// stay quoted. Only the tables the query itself names are joined, so a custom
/// SELECT may use the query's own columns and nothing else.
pub struct Scope<'a> {
    /// ` FROM <base> AS t0 LEFT JOIN … [WHERE …]`, with a leading space.
    pub from_where: String,
    /// One value for each `?` in `from_where`, in order.
    pub params: Vec<Param>,
    /// Indexes of filters that could not reach the base table and were skipped.
    pub ignored_filters: Vec<usize>,
    catalog: &'a Catalog,
    joins: Joins<'a>,
}

impl Scope<'_> {
    /// The SQL expression and type of a column at a grain, for use in a custom SELECT.
    pub fn expr(
        &self,
        table: &str,
        column: &str,
        grain: Option<Grain>,
    ) -> Result<(String, DType), QueryError> {
        self.expr_via(table, None, column, grain)
    }

    /// Like `expr`, for a field that crosses a named link (V4-D28).
    pub fn expr_via(
        &self,
        table: &str,
        via: Option<&str>,
        column: &str,
        grain: Option<Grain>,
    ) -> Result<(String, DType), QueryError> {
        dim_expr(self.catalog, &self.joins, table, via, column, grain)
    }

    /// The SQL expression, type and name of a measure.
    pub fn measure(&self, m: &Measure) -> Result<(String, DType, String), QueryError> {
        measure_expr(self.catalog, &self.joins, m)
    }
}

pub fn scope<'a>(
    q: &SemanticQuery,
    catalog: &'a Catalog,
    links: impl Into<Links<'a>>,
) -> Result<Scope<'a>, QueryError> {
    scope_on(q, catalog, links, local_today())
}

/// The joins of a query: which tables it reads and the links they cross.
fn plan<'a>(
    q: &SemanticQuery,
    catalog: &'a Catalog,
    links: Links<'a>,
) -> Result<(Joins<'a>, Vec<usize>), QueryError> {
    let base_id = q
        .measures
        .first()
        .map(|m| m.table.clone())
        .or_else(|| q.dimensions.first().map(|d| d.table.clone()))
        .ok_or(QueryError::Empty)?;
    lookup(catalog, &base_id)?;

    let mut joins = Joins::new(&base_id, links);

    // Dimensions and measures must all be reachable.
    let fields = q
        .dimensions
        .iter()
        .map(|d| (&d.table, d.via.as_deref()))
        .chain(q.measures.iter().map(|m| (&m.table, m.via.as_deref())))
        .chain(
            q.measures
                .iter()
                .filter_map(|m| m.per.as_ref().map(|p| (&p.table, p.via.as_deref()))),
        );
    for (t, via) in fields {
        lookup(catalog, t)?;
        joins.add(t, via).map_err(|e| match e {
            PathError::Gone => QueryError::LinkGone,
            PathError::Unusable => {
                QueryError::LinkUnusable(label(catalog, &base_id), label(catalog, t))
            }
            PathError::No => unreachable_error(catalog, links, &base_id, t),
        })?;
    }
    // Filters that cannot reach the base table are skipped and reported.
    let mut ignored = Vec::new();
    for (i, f) in q.filters.iter().enumerate() {
        if catalog.table(&f.table).is_none() || joins.add(&f.table, f.via.as_deref()).is_err() {
            ignored.push(i);
        }
    }
    Ok((joins, ignored))
}

/// "X is not related to Y", or when a link that would relate them repeats its values on both
/// sides, why that link is not used.
fn unreachable_error(catalog: &Catalog, links: Links<'_>, base: &str, table: &str) -> QueryError {
    let all = Links::new(links.rels, None);
    if crate::links::reach(base, all).contains_key(table) {
        QueryError::LinkUnusable(label(catalog, base), label(catalog, table))
    } else {
        QueryError::Unreachable(label(catalog, base), label(catalog, table))
    }
}

/// The links a query crosses that carry a warning: the ones whose far side repeats keys, or that
/// are read the other way round. A filter that cannot reach the base table crosses nothing.
pub fn crossed_dirty<'a>(
    q: &SemanticQuery,
    catalog: &'a Catalog,
    links: impl Into<Links<'a>>,
) -> Vec<String> {
    let Ok((joins, _)) = plan(q, catalog, links.into()) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for step in &joins.list {
        if step.edge.warn && !out.contains(&step.edge.rel) {
            out.push(step.edge.rel.clone());
        }
    }
    out
}

/// Like `scope`, with the day that relative date filters count from.
pub fn scope_on<'a>(
    q: &SemanticQuery,
    catalog: &'a Catalog,
    links: impl Into<Links<'a>>,
    today: NaiveDate,
) -> Result<Scope<'a>, QueryError> {
    let (joins, ignored) = plan(q, catalog, links.into())?;
    let base = lookup(catalog, &joins.base)?;

    let mut params = Vec::new();
    let mut wheres = Vec::new();
    for (i, f) in q.filters.iter().enumerate() {
        if ignored.contains(&i) {
            continue;
        }
        let (expr, dtype) = dim_expr(
            catalog,
            &joins,
            &f.table,
            f.via.as_deref(),
            &f.column,
            f.grain,
        )?;
        wheres.push(filter_sql(&expr, dtype, f, today, &mut params)?);
    }

    let mut sql = format!(" FROM {} AS t0", base.sql_ref);
    for j in &joins.list {
        let to = lookup(catalog, &j.edge.to_table)?;
        let from_t = lookup(catalog, &j.edge.from_table)?;
        let fc = col(from_t, &j.edge.from_col)?;
        let tc = col(to, &j.edge.to_col)?;
        let (l, r) = (
            format!("t{}.{}", j.from, ident(&j.edge.from_col)),
            format!("t{}.{}", j.alias, ident(&j.edge.to_col)),
        );
        let as_text = fc.dtype != tc.dtype;
        let on = join_on(&l, &r, fc.dtype, tc.dtype);
        sql.push_str(&format!(
            " LEFT JOIN {} AS t{} ON {on}",
            j.edge.target_sql(to, as_text),
            j.alias
        ));
    }
    if !wheres.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&wheres.join(" AND "));
    }
    Ok(Scope {
        from_where: sql,
        params,
        ignored_filters: ignored,
        catalog,
        joins,
    })
}

pub fn compile<'a>(
    q: &SemanticQuery,
    catalog: &'a Catalog,
    links: impl Into<Links<'a>>,
) -> Result<Compiled, QueryError> {
    compile_on(q, catalog, links, local_today())
}

/// Like `compile`, with the day that relative date filters count from.
pub fn compile_on<'a>(
    q: &SemanticQuery,
    catalog: &'a Catalog,
    links: impl Into<Links<'a>>,
    today: NaiveDate,
) -> Result<Compiled, QueryError> {
    let scope = scope_on(q, catalog, links, today)?;

    let mut select = Vec::new();
    let mut columns = Vec::new();
    // Dimensions that read in order: dates, and date parts.
    let mut ordered = Vec::new();
    for d in &q.dimensions {
        let (expr, dtype) = scope.expr_via(&d.table, d.via.as_deref(), &d.column, d.grain)?;
        select.push(format!(
            "{expr} AS {}",
            ident(&format!("d{}", columns.len()))
        ));
        ordered.push(
            dtype.is_temporal()
                || (d.grain.is_some_and(Grain::is_part)
                    && lookup(catalog, &d.table)
                        .and_then(|t| col(t, &d.column))
                        .is_ok_and(|c| c.dtype.is_temporal())),
        );
        columns.push(OutColumn {
            name: d.column.clone(),
            role: "dimension".into(),
            dtype,
            unit: None,
        });
    }
    for m in &q.measures {
        let (expr, dtype, name) = scope.measure(m)?;
        select.push(format!(
            "{expr} AS {}",
            ident(&format!("m{}", columns.len()))
        ));
        columns.push(OutColumn {
            name,
            role: "measure".into(),
            dtype,
            unit: None,
        });
    }

    let mut sql = String::from("SELECT ");
    if q.measures.is_empty() {
        sql.push_str("DISTINCT ");
    }
    sql.push_str(&select.join(", "));
    sql.push_str(&scope.from_where);
    let nd = q.dimensions.len();
    if nd > 0 && !q.measures.is_empty() {
        let g: Vec<String> = (1..=nd).map(|i| i.to_string()).collect();
        sql.push_str(&format!(" GROUP BY {}", g.join(", ")));
    }

    let mut order: Vec<String> = q
        .sort
        .iter()
        .filter(|s| s.by < columns.len())
        .map(|s| {
            format!(
                "{} {} NULLS LAST",
                s.by + 1,
                if s.desc { "DESC" } else { "ASC" }
            )
        })
        .collect();
    if order.is_empty() && !columns.is_empty() {
        // Time reads left to right; otherwise biggest first.
        if let Some(i) = ordered.iter().position(|&o| o) {
            order.push(format!("{} ASC", i + 1));
        } else if nd > 0 && !q.measures.is_empty() {
            order.push(format!("{} DESC NULLS LAST", nd + 1));
        } else if nd > 0 {
            order.push("1 ASC".into());
        }
    }
    // Ties are broken by every other dimension, so the same question always
    // returns rows in the same order.
    for i in 1..=nd {
        if !order
            .iter()
            .any(|o| o.split(' ').next() == Some(&i.to_string()))
        {
            order.push(format!("{i} ASC NULLS LAST"));
        }
    }
    if !order.is_empty() {
        sql.push_str(&format!(" ORDER BY {}", order.join(", ")));
    }
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    sql.push_str(&format!(" LIMIT {limit}"));

    Ok(Compiled {
        sql,
        params: scope.params,
        columns,
        ignored_filters: scope.ignored_filters,
    })
}

fn lookup<'a>(catalog: &'a Catalog, id: &str) -> Result<&'a Table, QueryError> {
    catalog
        .table(id)
        .ok_or_else(|| QueryError::UnknownTable(id.to_string()))
}

fn label(catalog: &Catalog, id: &str) -> String {
    catalog
        .table(id)
        .map(|t| t.label.clone())
        .unwrap_or_else(|| id.to_string())
}

fn col<'a>(t: &'a Table, c: &str) -> Result<&'a crate::catalog::Column, QueryError> {
    t.column(c)
        .ok_or_else(|| QueryError::UnknownColumn(t.label.clone(), c.to_string()))
}

/// One `LEFT JOIN` of a query: the link crossed, the alias that joins and the alias it joins from.
struct Step {
    edge: Edge,
    /// The number of the alias of the table the join starts from (`t{from}`).
    from: usize,
    /// The number of the alias of the table that is joined (`t{alias}`).
    alias: usize,
}

/// The tables a query joins, each under an alias named after the path that reaches it, not
/// after the table: one table joined through two links is two aliases (V4-D28). The base table
/// is `t0`; the others are numbered in the order they are first needed.
struct Joins<'a> {
    base: String,
    paths: Paths<'a>,
    /// The path of each alias, as the link ids crossed from the base. `keys[0]` is empty.
    keys: Vec<Vec<String>>,
    list: Vec<Step>,
}

impl<'a> Joins<'a> {
    fn new(base: &str, links: Links<'a>) -> Self {
        Self {
            base: base.to_string(),
            paths: Paths::new(base, links),
            keys: vec![Vec::new()],
            list: Vec::new(),
        }
    }

    /// Adds the joins that reach `table`, crossing the link `via` when one is named.
    fn add(&mut self, table: &str, via: Option<&str>) -> Result<(), PathError> {
        let path = self.paths.path(table, via)?;
        let mut key: Vec<String> = Vec::new();
        let mut from = 0;
        for e in path {
            key.push(e.rel.clone());
            from = match self.keys.iter().position(|k| *k == key) {
                Some(i) => i,
                None => {
                    self.keys.push(key.clone());
                    let alias = self.keys.len() - 1;
                    self.list.push(Step {
                        edge: e,
                        from,
                        alias,
                    });
                    alias
                }
            };
        }
        Ok(())
    }

    /// The alias of a table reached the way `via` says. A table that was never added reads as the
    /// base; the callers have checked the field first.
    fn alias(&self, table: &str, via: Option<&str>) -> String {
        let i = self
            .paths
            .path(table, via)
            .ok()
            .and_then(|p| {
                let key: Vec<String> = p.iter().map(|e| e.rel.clone()).collect();
                self.keys.iter().position(|k| *k == key)
            })
            .unwrap_or(0);
        format!("t{i}")
    }
}

fn dim_expr(
    catalog: &Catalog,
    joins: &Joins,
    table: &str,
    via: Option<&str>,
    column: &str,
    grain: Option<Grain>,
) -> Result<(String, DType), QueryError> {
    let t = lookup(catalog, table)?;
    let c = col(t, column)?;
    let r = format!("{}.{}", joins.alias(table, via), ident(column));
    Ok(match (c.dtype, grain) {
        (d, Some(g)) if d.is_temporal() && g.is_part() => (g.sql(&r), DType::Number),
        (d, Some(g)) if d.is_temporal() => (g.sql(&r), DType::Date),
        (DType::Date, None) => (format!("strftime({r}, '%Y-%m-%d')"), DType::Date),
        (DType::DateTime, None) => (
            format!("strftime({r}, '%Y-%m-%dT%H:%M:%S')"),
            DType::DateTime,
        ),
        (DType::Number, _) => (format!("CAST({r} AS DOUBLE)"), DType::Number),
        (DType::Bool, _) => (r, DType::Bool),
        _ => (format!("CAST({r} AS VARCHAR)"), DType::Text),
    })
}

fn measure_expr(
    catalog: &Catalog,
    joins: &Joins,
    m: &Measure,
) -> Result<(String, DType, String), QueryError> {
    let top = agg_expr(
        catalog,
        joins,
        &m.table,
        m.via.as_deref(),
        m.column.as_deref(),
        m.agg,
    )?;
    let Some(per) = &m.per else {
        return Ok(top);
    };
    // A ratio divides one aggregate by another in the same SELECT. A zero below gives a gap,
    // never an error. Totals ask the same question without groups, so they are a ratio of
    // totals and never a sum of ratios.
    let bottom = agg_expr(
        catalog,
        joins,
        &per.table,
        per.via.as_deref(),
        per.column.as_deref(),
        per.agg,
    )?;
    for (_, dtype, name) in [&top, &bottom] {
        if *dtype != DType::Number {
            return Err(QueryError::BadAgg(name.clone(), "divided"));
        }
    }
    let low = per.column.clone().unwrap_or_else(|| "row".into());
    Ok((
        format!("({}) / NULLIF(({}), 0)", top.0, bottom.0),
        DType::Number,
        format!("{} per {low}", top.2),
    ))
}

/// One aggregate of one column (or of the rows of a table) as SQL, its type and its name.
fn agg_expr(
    catalog: &Catalog,
    joins: &Joins,
    table: &str,
    via: Option<&str>,
    column: Option<&str>,
    agg: Agg,
) -> Result<(String, DType, String), QueryError> {
    let t = lookup(catalog, table)?;
    let Some(cname) = column else {
        return match agg {
            Agg::Count => Ok((
                "CAST(count(*) AS DOUBLE)".to_string(),
                DType::Number,
                "Rows".into(),
            )),
            _ => Err(QueryError::BadAgg("Rows".into(), "summed")),
        };
    };
    let c = col(t, cname)?;
    let r = format!("{}.{}", joins.alias(table, via), ident(cname));
    let name = cname.to_string();
    match agg {
        Agg::Count => Ok((format!("CAST(count({r}) AS DOUBLE)"), DType::Number, name)),
        Agg::CountDistinct => Ok((
            format!("CAST(count(DISTINCT {r}) AS DOUBLE)"),
            DType::Number,
            name,
        )),
        Agg::Sum | Agg::Avg | Agg::Median | Agg::P90 | Agg::Stdev | Agg::Variance => {
            if c.dtype != DType::Number {
                return Err(QueryError::BadAgg(
                    name,
                    match agg {
                        Agg::Sum => "summed",
                        Agg::Avg => "averaged",
                        Agg::Median => "used for a median",
                        Agg::P90 => "used for a 90th percentile",
                        _ => "used for a spread",
                    },
                ));
            }
            let call = match agg {
                Agg::Sum => format!("sum({r})"),
                Agg::Avg => format!("avg({r})"),
                Agg::Median => format!("median({r})"),
                Agg::P90 => format!("quantile_cont({r}, 0.9)"),
                Agg::Stdev => format!("stddev_samp({r})"),
                _ => format!("var_samp({r})"),
            };
            Ok((format!("CAST({call} AS DOUBLE)"), DType::Number, name))
        }
        Agg::Min | Agg::Max => {
            let f = if agg == Agg::Min { "min" } else { "max" };
            match c.dtype {
                DType::Number => Ok((format!("CAST({f}({r}) AS DOUBLE)"), DType::Number, name)),
                DType::Date => Ok((format!("strftime({f}({r}), '%Y-%m-%d')"), DType::Date, name)),
                DType::DateTime => Ok((
                    format!("strftime({f}({r}), '%Y-%m-%dT%H:%M:%S')"),
                    DType::DateTime,
                    name,
                )),
                _ => Ok((format!("{f}(CAST({r} AS VARCHAR))"), DType::Text, name)),
            }
        }
    }
}

fn to_param(v: &serde_json::Value, dtype: DType) -> Result<Param, QueryError> {
    use serde_json::Value as J;
    Ok(match (v, dtype) {
        (J::Null, _) => Param::Null,
        (J::Number(n), DType::Number) => Param::Double(n.as_f64().ok_or(QueryError::BadValue)?),
        (J::String(s), DType::Number) => {
            Param::Double(s.trim().parse().map_err(|_| QueryError::BadValue)?)
        }
        (J::Bool(b), DType::Bool) => Param::Bool(*b),
        (J::String(s), DType::Bool) => Param::Bool(s == "true"),
        (J::String(s), _) => Param::Text(s.clone()),
        (J::Number(n), _) => Param::Text(n.to_string()),
        (J::Bool(b), _) => Param::Text(b.to_string()),
        _ => return Err(QueryError::BadValue),
    })
}

fn filter_sql(
    expr: &str,
    dtype: DType,
    f: &Filter,
    today: NaiveDate,
    params: &mut Vec<Param>,
) -> Result<String, QueryError> {
    // These ops read their value as text, whatever the column holds.
    match f.op {
        FilterOp::Contains | FilterOp::StartsWith => {
            let text = match f.values.first() {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Number(n)) => n.to_string(),
                _ => return Err(QueryError::BadValue),
            };
            let esc = like_escape(&text);
            params.push(Param::Text(if f.op == FilterOp::Contains {
                format!("%{esc}%")
            } else {
                format!("{esc}%")
            }));
            let text_expr = if dtype == DType::Text {
                expr.to_string()
            } else {
                format!("CAST({expr} AS VARCHAR)")
            };
            return Ok(format!("{text_expr} ILIKE ? ESCAPE '\\'"));
        }
        FilterOp::Blank | FilterOp::NotBlank => {
            // Text is blank when it is empty too. Other types are blank when they are NULL.
            let blank = if dtype == DType::Text {
                format!("({expr} IS NULL OR {expr} = '')")
            } else {
                format!("{expr} IS NULL")
            };
            return Ok(if f.op == FilterOp::Blank {
                blank
            } else {
                format!("NOT {blank}")
            });
        }
        FilterOp::Relative => {
            if !dtype.is_temporal() {
                return Err(QueryError::NotADate(f.column.clone()));
            }
            let (lo, hi) = f
                .values
                .first()
                .and_then(|v| v.as_str())
                .and_then(|p| relative_range(p, today))
                .ok_or(QueryError::BadValue)?;
            // The ends are day starts, so a date-time on the last day is inside: its text sorts
            // before the start of the next day.
            params.push(Param::Text(lo.format("%Y-%m-%d").to_string()));
            params.push(Param::Text(hi.format("%Y-%m-%d").to_string()));
            return Ok(format!("({expr} >= ? AND {expr} < ?)"));
        }
        _ => {}
    }
    let vals: Vec<Param> = f
        .values
        .iter()
        .map(|v| to_param(v, dtype))
        .collect::<Result<_, _>>()?;
    let has_null = vals.contains(&Param::Null);
    let non_null: Vec<Param> = vals.into_iter().filter(|p| *p != Param::Null).collect();
    match f.op {
        FilterOp::In | FilterOp::NotIn => {
            let mut parts = Vec::new();
            if !non_null.is_empty() {
                let marks = vec!["?"; non_null.len()].join(", ");
                parts.push(format!("{expr} IN ({marks})"));
                params.extend(non_null);
            }
            if has_null {
                parts.push(format!("{expr} IS NULL"));
            }
            let inner = if parts.is_empty() {
                "FALSE".to_string()
            } else {
                format!("({})", parts.join(" OR "))
            };
            Ok(if f.op == FilterOp::NotIn {
                format!("NOT COALESCE({inner}, FALSE)")
            } else {
                inner
            })
        }
        FilterOp::Between => {
            if non_null.len() != 2 {
                return Err(QueryError::BadValue);
            }
            params.extend(non_null);
            Ok(format!("{expr} BETWEEN ? AND ?"))
        }
        FilterOp::Range => {
            if non_null.len() != 2 {
                return Err(QueryError::BadValue);
            }
            params.extend(non_null);
            Ok(format!("({expr} >= ? AND {expr} < ?)"))
        }
        FilterOp::Contains
        | FilterOp::StartsWith
        | FilterOp::Blank
        | FilterOp::NotBlank
        | FilterOp::Relative => unreachable!("handled above"),
        FilterOp::Gte | FilterOp::Lte => {
            let v = non_null.into_iter().next().ok_or(QueryError::BadValue)?;
            params.push(v);
            Ok(format!(
                "{expr} {} ?",
                if f.op == FilterOp::Gte { ">=" } else { "<=" }
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Column;
    use crate::workspace::{FieldRef, Relationship};

    fn table(id: &str, cols: &[(&str, DType)]) -> Table {
        let (src, name) = id.split_once('/').unwrap();
        Table {
            id: id.into(),
            source_id: src.into(),
            name: name.into(),
            label: name.into(),
            sql_ref: format!("\"s_{src}\".\"{name}\""),
            columns: cols
                .iter()
                .map(|(n, d)| Column::new(n.to_string(), *d, String::new()))
                .collect(),
            rows: None,
            notes: vec![],
            header_row: None,
            prep: false,
        }
    }

    fn fixture() -> (Catalog, Vec<Relationship>) {
        use DType::*;
        let cat = Catalog {
            tables: vec![
                table(
                    "s1/orders",
                    &[
                        ("date", Date),
                        ("store_id", Number),
                        ("customer", Text),
                        ("amount", Number),
                    ],
                ),
                table(
                    "s2/stores",
                    &[("id", Number), ("region_code", Text), ("city", Text)],
                ),
                table("s3/regions", &[("code", Text), ("name", Text)]),
                table("s4/customers", &[("name", Text), ("tier", Text)]),
            ],
        };
        let rel = |id: &str, a: (&str, &str), b: (&str, &str)| Relationship {
            id: id.into(),
            from: FieldRef {
                table: a.0.into(),
                column: a.1.into(),
            },
            to: FieldRef {
                table: b.0.into(),
                column: b.1.into(),
            },
            name: None,
        };
        let rels = vec![
            rel("r1", ("s1/orders", "store_id"), ("s2/stores", "id")),
            rel("r2", ("s2/stores", "region_code"), ("s3/regions", "code")),
        ];
        (cat, rels)
    }

    fn sum_amount() -> Measure {
        Measure {
            table: "s1/orders".into(),
            column: Some("amount".into()),
            agg: Agg::Sum,
            per: None,
            via: None,
        }
    }

    #[test]
    fn joins_two_hops_and_groups() {
        let (cat, rels) = fixture();
        let q = SemanticQuery {
            measures: vec![sum_amount()],
            dimensions: vec![Dimension {
                table: "s3/regions".into(),
                column: "name".into(),
                grain: None,
                via: None,
            }],
            ..Default::default()
        };
        let c = compile(&q, &cat, &rels).unwrap();
        assert_eq!(
            c.sql,
            "SELECT CAST(t2.\"name\" AS VARCHAR) AS \"d0\", CAST(sum(t0.\"amount\") AS DOUBLE) AS \"m1\" \
             FROM \"s_s1\".\"orders\" AS t0 \
             LEFT JOIN \"s_s2\".\"stores\" AS t1 ON t0.\"store_id\" = t1.\"id\" \
             LEFT JOIN \"s_s3\".\"regions\" AS t2 ON t1.\"region_code\" = t2.\"code\" \
             GROUP BY 1 ORDER BY 2 DESC NULLS LAST, 1 ASC NULLS LAST LIMIT 1000"
        );
    }

    #[test]
    fn date_grain_orders_by_time() {
        let (cat, rels) = fixture();
        let q = SemanticQuery {
            measures: vec![sum_amount()],
            dimensions: vec![Dimension {
                table: "s1/orders".into(),
                column: "date".into(),
                grain: Some(Grain::Month),
                via: None,
            }],
            ..Default::default()
        };
        let c = compile(&q, &cat, &rels).unwrap();
        assert!(
            c.sql
                .contains("strftime(date_trunc('month', t0.\"date\"), '%Y-%m-%d')")
        );
        assert!(c.sql.contains("ORDER BY 1 ASC"));
    }

    #[test]
    fn unrelated_table_is_a_clear_error() {
        let (cat, rels) = fixture();
        let q = SemanticQuery {
            measures: vec![sum_amount()],
            dimensions: vec![Dimension {
                table: "s4/customers".into(),
                column: "tier".into(),
                grain: None,
                via: None,
            }],
            ..Default::default()
        };
        assert_eq!(
            compile(&q, &cat, &rels).unwrap_err(),
            QueryError::Unreachable("orders".into(), "customers".into())
        );
    }

    #[test]
    fn unreachable_filter_is_ignored_and_reported() {
        let (cat, rels) = fixture();
        let q = SemanticQuery {
            measures: vec![Measure {
                table: "s2/stores".into(),
                column: None,
                agg: Agg::Count,
                per: None,
                via: None,
            }],
            filters: vec![Filter {
                table: "s1/orders".into(),
                column: "customer".into(),
                grain: None,
                op: FilterOp::In,
                values: vec!["x".into()],
                via: None,
            }],
            ..Default::default()
        };
        let c = compile(&q, &cat, &rels).unwrap();
        assert_eq!(c.ignored_filters, vec![0]);
        assert!(!c.sql.contains("WHERE"));
    }

    #[test]
    fn filter_values_are_parameters_never_sql() {
        let (cat, rels) = fixture();
        let evil = "x'); DROP TABLE orders; --";
        let q = SemanticQuery {
            measures: vec![sum_amount()],
            filters: vec![
                Filter {
                    table: "s2/stores".into(),
                    column: "city".into(),
                    grain: None,
                    op: FilterOp::In,
                    values: vec![evil.into(), serde_json::Value::Null],
                    via: None,
                },
                Filter {
                    table: "s1/orders".into(),
                    column: "amount".into(),
                    grain: None,
                    op: FilterOp::Gte,
                    values: vec![10.into()],
                    via: None,
                },
            ],
            ..Default::default()
        };
        let c = compile(&q, &cat, &rels).unwrap();
        assert!(!c.sql.contains("DROP"));
        assert!(c.sql.contains(
            "WHERE (CAST(t1.\"city\" AS VARCHAR) IN (?) OR CAST(t1.\"city\" AS VARCHAR) IS NULL) AND CAST(t0.\"amount\" AS DOUBLE) >= ?"
        ));
        assert_eq!(
            c.params,
            vec![Param::Text(evil.into()), Param::Double(10.0)]
        );
    }

    #[test]
    fn hostile_column_names_are_quoted() {
        let mut cat = Catalog {
            tables: vec![table("s1/t", &[("a\"; DROP", DType::Number)])],
        };
        cat.tables[0].label = "t".into();
        let q = SemanticQuery {
            measures: vec![Measure {
                table: "s1/t".into(),
                column: Some("a\"; DROP".into()),
                agg: Agg::Sum,
                per: None,
                via: None,
            }],
            ..Default::default()
        };
        let c = compile(&q, &cat, &[]).unwrap();
        assert!(c.sql.contains("sum(t0.\"a\"\"; DROP\")"));
    }

    #[test]
    fn summing_text_is_refused() {
        let (cat, rels) = fixture();
        let q = SemanticQuery {
            measures: vec![Measure {
                table: "s1/orders".into(),
                column: Some("customer".into()),
                agg: Agg::Sum,
                per: None,
                via: None,
            }],
            ..Default::default()
        };
        assert!(matches!(
            compile(&q, &cat, &rels),
            Err(QueryError::BadAgg(_, "summed"))
        ));
    }

    #[test]
    fn scope_keeps_values_as_parameters() {
        let (cat, rels) = fixture();
        let evil = "x'); DROP TABLE orders; --";
        let q = SemanticQuery {
            measures: vec![sum_amount()],
            dimensions: vec![Dimension {
                table: "s3/regions".into(),
                column: "name".into(),
                grain: None,
                via: None,
            }],
            filters: vec![Filter {
                table: "s2/stores".into(),
                column: "city".into(),
                grain: None,
                op: FilterOp::In,
                values: vec![evil.into()],
                via: None,
            }],
            ..Default::default()
        };
        let sc = scope(&q, &cat, &rels).unwrap();
        assert!(!sc.from_where.contains("DROP"));
        assert_eq!(sc.params, vec![Param::Text(evil.into())]);
        assert!(
            sc.from_where
                .starts_with(" FROM \"s_s1\".\"orders\" AS t0 LEFT JOIN")
        );
        assert!(
            sc.from_where
                .contains("WHERE (CAST(t1.\"city\" AS VARCHAR) IN (?))")
        );
        // A custom SELECT builds on the same aliases.
        let (e, t) = sc.expr("s3/regions", "name", None).unwrap();
        assert_eq!(e, "CAST(t2.\"name\" AS VARCHAR)");
        assert_eq!(t, DType::Text);
        // compile() is scope() plus a SELECT, so both agree.
        let c = compile(&q, &cat, &rels).unwrap();
        assert!(c.sql.contains(&sc.from_where));
        assert_eq!(c.params, sc.params);
    }

    #[test]
    fn range_is_half_open() {
        let (cat, rels) = fixture();
        let q = SemanticQuery {
            measures: vec![sum_amount()],
            filters: vec![Filter {
                table: "s1/orders".into(),
                column: "amount".into(),
                grain: None,
                op: FilterOp::Range,
                values: vec![10.into(), 20.into()],
                via: None,
            }],
            ..Default::default()
        };
        let c = compile(&q, &cat, &rels).unwrap();
        assert!(c.sql.contains(
            "WHERE (CAST(t0.\"amount\" AS DOUBLE) >= ? AND CAST(t0.\"amount\" AS DOUBLE) < ?)"
        ));
        assert_eq!(c.params, vec![Param::Double(10.0), Param::Double(20.0)]);
        // Two bounds are required.
        let mut bad = q.clone();
        bad.filters[0].values = vec![10.into()];
        assert_eq!(
            compile(&bad, &cat, &rels).unwrap_err(),
            QueryError::BadValue
        );
    }

    #[test]
    fn date_parts_are_numbers_in_ascending_order() {
        let (cat, rels) = fixture();
        for (grain, sql) in [
            (Grain::MonthOfYear, "CAST(month(t0.\"date\") AS DOUBLE)"),
            (Grain::Weekday, "CAST(isodow(t0.\"date\") AS DOUBLE)"),
            (Grain::Hour, "CAST(hour(t0.\"date\") AS DOUBLE)"),
        ] {
            let q = SemanticQuery {
                measures: vec![sum_amount()],
                dimensions: vec![Dimension {
                    table: "s1/orders".into(),
                    column: "date".into(),
                    grain: Some(grain),
                    via: None,
                }],
                filters: vec![Filter {
                    table: "s1/orders".into(),
                    column: "date".into(),
                    grain: Some(grain),
                    op: FilterOp::In,
                    values: vec![3.into()],
                    via: None,
                }],
                ..Default::default()
            };
            let c = compile(&q, &cat, &rels).unwrap();
            assert!(
                c.sql.contains(&format!("SELECT {sql} AS \"d0\"")),
                "{}",
                c.sql
            );
            // The cross-filter on a part uses the same expression, with a number.
            assert!(
                c.sql.contains(&format!("WHERE ({sql} IN (?))")),
                "{}",
                c.sql
            );
            assert_eq!(c.params, vec![Param::Double(3.0)]);
            assert!(c.sql.contains("ORDER BY 1 ASC"), "{}", c.sql);
            assert_eq!(c.columns[0].dtype, DType::Number);
        }
        let g: Grain = serde_json::from_str("\"month_of_year\"").unwrap();
        assert_eq!(g, Grain::MonthOfYear);
        assert_eq!(
            serde_json::to_string(&Grain::Weekday).unwrap(),
            "\"weekday\""
        );
    }

    #[test]
    fn dimensions_only_is_distinct() {
        let (cat, rels) = fixture();
        let q = SemanticQuery {
            dimensions: vec![Dimension {
                table: "s2/stores".into(),
                column: "city".into(),
                grain: None,
                via: None,
            }],
            ..Default::default()
        };
        let c = compile(&q, &cat, &rels).unwrap();
        assert!(c.sql.starts_with("SELECT DISTINCT"));
    }

    fn where_of(
        op: FilterOp,
        table: &str,
        column: &str,
        values: Vec<serde_json::Value>,
    ) -> Compiled {
        let (cat, rels) = fixture();
        let q = SemanticQuery {
            measures: vec![sum_amount()],
            filters: vec![Filter {
                table: table.into(),
                column: column.into(),
                grain: None,
                op,
                values,
                via: None,
            }],
            ..Default::default()
        };
        compile_on(
            &q,
            &cat,
            &rels,
            NaiveDate::from_ymd_opt(2026, 10, 14).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn contains_is_an_escaped_ilike_parameter() {
        let c = where_of(
            FilterOp::Contains,
            "s2/stores",
            "city",
            vec!["50%_a\\b".into()],
        );
        assert!(
            c.sql
                .contains("CAST(t1.\"city\" AS VARCHAR) ILIKE ? ESCAPE '\\'")
        );
        assert_eq!(c.params, vec![Param::Text("%50\\%\\_a\\\\b%".into())]);
        let c = where_of(
            FilterOp::StartsWith,
            "s1/orders",
            "customer",
            vec!["Ab".into()],
        );
        assert_eq!(c.params, vec![Param::Text("Ab%".into())]);
    }

    #[test]
    fn blank_means_null_or_empty_text_and_null_otherwise() {
        let c = where_of(FilterOp::Blank, "s1/orders", "customer", vec![]);
        assert!(c.sql.contains(
            "(CAST(t0.\"customer\" AS VARCHAR) IS NULL OR CAST(t0.\"customer\" AS VARCHAR) = '')"
        ));
        let c = where_of(FilterOp::NotBlank, "s1/orders", "customer", vec![]);
        assert!(
            c.sql
                .contains("NOT (CAST(t0.\"customer\" AS VARCHAR) IS NULL OR ")
        );
        let c = where_of(FilterOp::Blank, "s1/orders", "amount", vec![]);
        assert!(
            c.sql
                .contains("WHERE CAST(t0.\"amount\" AS DOUBLE) IS NULL")
        );
        assert!(!c.sql.contains("= ''"));
        assert!(c.params.is_empty());
    }

    #[test]
    fn relative_becomes_a_half_open_range_of_days() {
        let c = where_of(
            FilterOp::Relative,
            "s1/orders",
            "date",
            vec!["last_7_days".into()],
        );
        assert!(c.sql.contains(
            "(strftime(t0.\"date\", '%Y-%m-%d') >= ? AND strftime(t0.\"date\", '%Y-%m-%d') < ?)"
        ));
        assert_eq!(
            c.params,
            vec![
                Param::Text("2026-10-08".into()),
                Param::Text("2026-10-15".into())
            ]
        );
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn presets_resolve_against_today() {
        let r = |p: &str, today: &str| {
            let (lo, hi) = relative_range(p, d(today)).unwrap();
            (lo.to_string(), hi.to_string())
        };
        let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(
            r("last_7_days", "2026-10-10"),
            pair("2026-10-04", "2026-10-11")
        );
        assert_eq!(
            r("last_30_days", "2026-10-10"),
            pair("2026-09-11", "2026-10-11")
        );
        assert_eq!(
            r("this_month", "2026-10-10"),
            pair("2026-10-01", "2026-10-11")
        );
        assert_eq!(
            r("last_month", "2026-10-10"),
            pair("2026-09-01", "2026-10-01")
        );
        assert_eq!(
            r("this_quarter", "2026-10-10"),
            pair("2026-10-01", "2026-10-11")
        );
        assert_eq!(
            r("last_quarter", "2026-10-10"),
            pair("2026-07-01", "2026-10-01")
        );
        assert_eq!(r("ytd", "2026-10-10"), pair("2026-01-01", "2026-10-11"));
        assert_eq!(
            r("last_12_months", "2026-10-10"),
            pair("2025-10-11", "2026-10-11")
        );
        // January looks back into the year before.
        assert_eq!(
            r("last_month", "2026-01-15"),
            pair("2025-12-01", "2026-01-01")
        );
        assert_eq!(
            r("last_quarter", "2026-02-15"),
            pair("2025-10-01", "2026-01-01")
        );
        // The last day of a year, a leap day, and a short month.
        assert_eq!(
            r("last_7_days", "2026-12-31"),
            pair("2026-12-25", "2027-01-01")
        );
        assert_eq!(
            r("last_12_months", "2024-02-29"),
            pair("2023-03-01", "2024-03-01")
        );
        assert_eq!(
            r("last_month", "2026-03-31"),
            pair("2026-02-01", "2026-03-01")
        );
        assert!(relative_range("next_week", d("2026-10-10")).is_none());
    }

    #[test]
    fn a_relative_filter_can_be_written_as_a_range() {
        let f = Filter {
            table: "t".into(),
            column: "c".into(),
            grain: None,
            op: FilterOp::Relative,
            values: vec!["ytd".into()],
            via: None,
        };
        let r = resolve_relative(&f, d("2026-10-10"));
        assert_eq!(r.op, FilterOp::Range);
        assert_eq!(
            r.values,
            vec![
                serde_json::json!("2026-01-01"),
                serde_json::json!("2026-10-11")
            ]
        );
        // Other filters are left alone.
        let g = Filter {
            op: FilterOp::In,
            ..f
        };
        assert_eq!(resolve_relative(&g, d("2026-10-10")).op, FilterOp::In);
    }

    // ---- links that stay honest (V4-D26 to D28) -------------------------------------------

    use crate::links::{Health, LinkHealth};

    /// Transfers say which store sent and which store received: two links to one table.
    fn two_links() -> (Catalog, Vec<Relationship>) {
        use DType::*;
        let cat = Catalog {
            tables: vec![
                table(
                    "s1/transfers",
                    &[
                        ("from_store", Number),
                        ("to_store", Number),
                        ("qty", Number),
                    ],
                ),
                table("s2/stores", &[("id", Number), ("region", Text)]),
            ],
        };
        let rel = |id: &str, col: &str| Relationship {
            id: id.into(),
            from: FieldRef {
                table: "s1/transfers".into(),
                column: col.into(),
            },
            to: FieldRef {
                table: "s2/stores".into(),
                column: "id".into(),
            },
            name: None,
        };
        (cat, vec![rel("r1", "from_store"), rel("r2", "to_store")])
    }

    fn qty() -> Measure {
        Measure {
            table: "s1/transfers".into(),
            column: Some("qty".into()),
            agg: Agg::Sum,
            per: None,
            via: None,
        }
    }

    fn region(via: Option<&str>) -> Dimension {
        Dimension {
            table: "s2/stores".into(),
            column: "region".into(),
            grain: None,
            via: via.map(str::to_string),
        }
    }

    #[test]
    fn one_table_joined_through_two_links_is_two_aliases() {
        let (cat, rels) = two_links();
        let q = SemanticQuery {
            measures: vec![qty()],
            dimensions: vec![region(Some("r1")), region(Some("r2"))],
            ..Default::default()
        };
        let c = compile(&q, &cat, &rels).unwrap();
        assert_eq!(
            c.sql,
            "SELECT CAST(t1.\"region\" AS VARCHAR) AS \"d0\", CAST(t2.\"region\" AS VARCHAR) AS \"d1\", \
             CAST(sum(t0.\"qty\") AS DOUBLE) AS \"m2\" \
             FROM \"s_s1\".\"transfers\" AS t0 \
             LEFT JOIN \"s_s2\".\"stores\" AS t1 ON t0.\"from_store\" = t1.\"id\" \
             LEFT JOIN \"s_s2\".\"stores\" AS t2 ON t0.\"to_store\" = t2.\"id\" \
             GROUP BY 1, 2 ORDER BY 3 DESC NULLS LAST, 1 ASC NULLS LAST, 2 ASC NULLS LAST LIMIT 1000"
        );
    }

    #[test]
    fn no_via_means_the_first_link_and_the_same_numbers_as_before() {
        let (cat, rels) = two_links();
        let by = |via: Option<&str>| SemanticQuery {
            measures: vec![qty()],
            dimensions: vec![region(via)],
            ..Default::default()
        };
        let plain = compile(&by(None), &cat, &rels).unwrap().sql;
        assert!(plain.contains("t0.\"from_store\" = t1.\"id\""), "{plain}");
        assert_eq!(plain, compile(&by(Some("r1")), &cat, &rels).unwrap().sql);
        let to = compile(&by(Some("r2")), &cat, &rels).unwrap().sql;
        assert!(to.contains("t0.\"to_store\" = t1.\"id\""), "{to}");
    }

    #[test]
    fn a_filter_through_a_link_filters_that_role_only() {
        let (cat, rels) = two_links();
        let q = SemanticQuery {
            measures: vec![qty()],
            dimensions: vec![region(Some("r1"))],
            filters: vec![Filter {
                table: "s2/stores".into(),
                column: "region".into(),
                grain: None,
                op: FilterOp::In,
                values: vec!["West".into()],
                via: Some("r2".into()),
            }],
            ..Default::default()
        };
        let c = compile(&q, &cat, &rels).unwrap();
        assert!(
            c.sql
                .contains("WHERE (CAST(t2.\"region\" AS VARCHAR) IN (?))"),
            "{}",
            c.sql
        );
        assert!(c.sql.contains("t0.\"to_store\" = t2.\"id\""), "{}", c.sql);
        assert!(c.ignored_filters.is_empty());
    }

    #[test]
    fn a_link_that_is_gone_or_unusable_says_so() {
        let (cat, rels) = two_links();
        let q = |via: &str| SemanticQuery {
            measures: vec![qty()],
            dimensions: vec![region(Some(via))],
            ..Default::default()
        };
        assert_eq!(
            compile(&q("r9"), &cat, &rels).unwrap_err(),
            QueryError::LinkGone
        );
        // The one side is mostly repeats and the many side repeats: many to many.
        let mut health = Health::new();
        health.insert(
            "r2".into(),
            LinkHealth {
                one_rows: 4,
                one_dups: 3,
                many_dups: 5,
                ..LinkHealth::default()
            },
        );
        let links = Links::new(&rels, Some(&health));
        assert_eq!(
            compile(&q("r2"), &cat, links).unwrap_err(),
            QueryError::LinkUnusable("transfers".into(), "stores".into())
        );
        // The other link still works, and a field with no via takes it.
        assert!(compile(&q("r1"), &cat, links).is_ok());
        let plain = SemanticQuery {
            measures: vec![qty()],
            dimensions: vec![region(None)],
            ..Default::default()
        };
        assert!(compile(&plain, &cat, links).is_ok());
    }

    #[test]
    fn a_repeated_key_on_the_one_side_reads_one_row_per_key() {
        let (mut cat, rels) = fixture();
        // A local table (a file, a sheet): the first row of a key in the order of the file.
        cat.tables[1].prep = true;
        let mut health = Health::new();
        health.insert(
            "r1".into(),
            LinkHealth {
                one_rows: 11,
                one_dups: 1,
                one_keys: 1,
                many_dups: 30,
                ..LinkHealth::default()
            },
        );
        let q = SemanticQuery {
            measures: vec![sum_amount()],
            dimensions: vec![Dimension {
                table: "s2/stores".into(),
                column: "city".into(),
                grain: None,
                via: None,
            }],
            ..Default::default()
        };
        let c = compile(&q, &cat, Links::new(&rels, Some(&health))).unwrap();
        assert_eq!(
            c.sql,
            "SELECT CAST(t1.\"city\" AS VARCHAR) AS \"d0\", CAST(sum(t0.\"amount\") AS DOUBLE) AS \"m1\" \
             FROM \"s_s1\".\"orders\" AS t0 \
             LEFT JOIN (SELECT * FROM (SELECT *, row_number() OVER () AS \"__tenun_row\" FROM \"s_s2\".\"stores\") \
             QUALIFY row_number() OVER (PARTITION BY \"id\" ORDER BY \"__tenun_row\") = 1) AS t1 \
             ON t0.\"store_id\" = t1.\"id\" \
             GROUP BY 1 ORDER BY 2 DESC NULLS LAST, 1 ASC NULLS LAST LIMIT 1000"
        );
        // The warning rides with the tile.
        assert_eq!(
            crossed_dirty(&q, &cat, Links::new(&rels, Some(&health))),
            ["r1"]
        );
        assert!(crossed_dirty(&q, &cat, &rels).is_empty());
        // A database table has no file order: it keeps the row that sorts first.
        let mut live = cat.clone();
        live.tables[1].prep = false;
        let c = compile(&q, &live, Links::new(&rels, Some(&health))).unwrap();
        assert!(
            c.sql.contains(
                "(SELECT * FROM \"s_s2\".\"stores\" QUALIFY row_number() OVER (PARTITION BY \"id\" \
                 ORDER BY \"id\", CAST(\"region_code\" AS VARCHAR), CAST(\"city\" AS VARCHAR)) = 1) AS t1"
            ),
            "{}",
            c.sql
        );
    }

    #[test]
    fn a_one_to_one_link_is_crossed_from_either_table() {
        let (cat, rels) = fixture();
        let mut health = Health::new();
        health.insert(
            "r1".into(),
            LinkHealth {
                one_rows: 20,
                ..LinkHealth::default()
            },
        );
        // Stores to orders: not possible when the data says nothing, possible for one to one.
        let q = SemanticQuery {
            measures: vec![Measure {
                table: "s2/stores".into(),
                column: None,
                agg: Agg::Count,
                per: None,
                via: None,
            }],
            dimensions: vec![Dimension {
                table: "s1/orders".into(),
                column: "customer".into(),
                grain: None,
                via: None,
            }],
            ..Default::default()
        };
        assert!(matches!(
            compile(&q, &cat, &rels),
            Err(QueryError::Unreachable(..))
        ));
        let c = compile(&q, &cat, Links::new(&rels, Some(&health))).unwrap();
        assert!(
            c.sql.contains(
                "FROM \"s_s2\".\"stores\" AS t0 LEFT JOIN \"s_s1\".\"orders\" AS t1 ON t0.\"id\" = t1.\"store_id\""
            ),
            "{}",
            c.sql
        );
        // It stops being one to one when the orders side repeats a store: one way again.
        health.get_mut("r1").unwrap().many_dups = 3;
        assert!(matches!(
            compile(&q, &cat, Links::new(&rels, Some(&health))),
            Err(QueryError::Unreachable(..))
        ));
    }
}
