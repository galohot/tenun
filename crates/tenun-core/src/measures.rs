//! Named measures: formulas of totals that work across tables (build 03, V4-D4, V4-D5, V4-D85).
//!
//! A tile that uses a named measure (`{ named: "m1" }`) asks the same question as any tile: some
//! values, grouped by some fields, under some filters. The formula says what is added up, and the
//! parts of it may come from different tables (`SUM([Sales].[amount]) / SUM([Targets].[target])`).
//! Joining those tables first and adding up after would count a target once for each sale. So each
//! **home table** is totalled on its own, in a sub-query of its own that reaches the groups and the
//! filters the way a plain question from that table does (`query::scope_on`, so the one-row-per-key
//! rule of V4-D50 and V4-D51 holds in every part), and the sub-queries meet on the group values.
//! Then the formula combines the parts in an outer query, because `TRY` cannot wrap an aggregate
//! (V4-D32).
//!
//! - **One table, every group reachable from it, and a formula that is one total** (`SUM(...)`, no
//!   arithmetic around it): one SELECT, the shape a plain measure has (the fast path).
//! - **Otherwise**: one sub-query for each home table (`g0`, `g1`…), each grouped by the groups it
//!   can reach. The group values of the tile come from the sub-queries that reach all of them
//!   (`FULL OUTER JOIN` on `IS NOT DISTINCT FROM`, so a blank group is a group). A sub-query that
//!   reaches only some of the groups is joined on those, so its value repeats across the rest.
//!   A group that is missing from a part gives 0 for a sum or a count (V4-D22) and blank for the rest.
//! - **A group a table cannot reach** is dropped from that table's sub-query. A time group (a date at
//!   a grain) is read from the table's main date instead (V4-D5), and so is a filter on a date. Any
//!   other group or filter is left out of that part, and the result says so in `parts` (V4-D85).
//!
//! A plain measure beside a named one is a part of its own and reads as it does alone.

use crate::catalog::{Catalog, Column, DType, ident};
use crate::formula::{self, AggregateSql, Body, Join, Kind, PartScope};
use crate::links::{Health, Links, PathError};
use crate::query::{
    self, Agg, Compiled, Dimension, Filter, Grain, Joins, Measure, OutColumn, PartNote, QueryError,
    SemanticQuery, explain, label, lookup, order_limit, scope_on,
};
use crate::snapshot::count_rows;
use crate::workspace::{NamedMeasure, Relationship, TableSetup};
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use std::collections::BTreeMap;

/// A workspace uses at most this many named measures. The rest stay in the file and are not
/// worked out, and a tile that uses one says why (V4-D85c).
pub const MAX_MEASURES: usize = 200;
/// A formula of a measure has at most this many characters (V4-D24).
pub const MAX_FORMULA: usize = 10_000;

/// What a question about named measures is read against.
pub struct Env<'a> {
    pub measures: &'a [NamedMeasure],
    /// The setup of each table, for its main date.
    pub tables: &'a BTreeMap<String, TableSetup>,
    pub rels: &'a [Relationship],
    pub health: &'a Health,
    /// What `NOW()` is. `TODAY()` is its date.
    pub now: NaiveDateTime,
    /// The main dates that hold only the first day of a month, a quarter or a year, as (table id,
    /// column) → the period (V4-D101). Read from the data once for each load, by the engine. A
    /// main date not in the map is read as it is: any day.
    pub grains: &'a BTreeMap<(String, String), Period>,
    /// The parameters a measure can read (v5 build 05).
    pub params: &'a [crate::params::Param],
}

/// The period a main date counts in when every one of its values is the first day of a period
/// (V4-D101): a table of targets with a row for each month has a monthly main date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Period {
    Month,
    Quarter,
    Year,
}

impl Period {
    fn word(self) -> &'static str {
        match self {
            Period::Month => "month",
            Period::Quarter => "quarter",
            Period::Year => "year",
        }
    }

    /// The first day of the period that holds `d`.
    fn first(self, d: NaiveDate) -> NaiveDate {
        let month = match self {
            Period::Month => d.month(),
            Period::Quarter => (d.month() - 1) / 3 * 3 + 1,
            Period::Year => 1,
        };
        NaiveDate::from_ymd_opt(d.year(), month, 1).unwrap_or(d)
    }

    /// The last day of the period that holds `d`.
    fn last(self, d: NaiveDate) -> NaiveDate {
        let next = match self {
            Period::Month => 1,
            Period::Quarter => 3,
            Period::Year => 12,
        };
        self.first(d)
            .checked_add_months(chrono::Months::new(next))
            .and_then(|n| n.pred_opt())
            .unwrap_or(d)
    }

    /// The name of the period that holds `d`: "March", "Q1" or "2025".
    fn name(self, d: NaiveDate, year: bool) -> String {
        let y = if year {
            format!(" {}", d.year())
        } else {
            String::new()
        };
        match self {
            Period::Month => format!("{}{y}", d.format("%B")),
            Period::Quarter => format!("Q{}{y}", (d.month() - 1) / 3 + 1),
            Period::Year => d.year().to_string(),
        }
    }

    /// The unit a group of grain `g` counts in when that is finer than the period: a row of the
    /// period then has a value only on the first day (or month) of each period.
    fn finer_unit(self, g: Grain) -> Option<&'static str> {
        match (self, g) {
            (_, Grain::Day | Grain::Week | Grain::Weekday | Grain::Hour) => Some("day"),
            (Period::Quarter | Period::Year, Grain::Month | Grain::MonthOfYear) => Some("month"),
            (Period::Year, Grain::Quarter) => Some("quarter"),
            _ => None,
        }
    }
}

/// The SELECT that reads the grain of a main date: how many values it has, and how many of them
/// are the first day of a month, of a quarter and of a year.
pub fn grain_sql(column: &str, sql_ref: &str) -> String {
    let c = ident(column);
    let day_one = format!(
        "day({c}) = 1 AND CAST({c} AS TIMESTAMP) = date_trunc('day', CAST({c} AS TIMESTAMP))"
    );
    format!(
        "SELECT CAST(count({c}) AS DOUBLE), \
         CAST(count(*) FILTER (WHERE {day_one}) AS DOUBLE), \
         CAST(count(*) FILTER (WHERE {day_one} AND month({c}) IN (1, 4, 7, 10)) AS DOUBLE), \
         CAST(count(*) FILTER (WHERE {day_one} AND month({c}) = 1) AS DOUBLE) \
         FROM {sql_ref} WHERE {c} IS NOT NULL"
    )
}

/// The period of a main date from the row `grain_sql` gave, or `None` when it holds other days.
pub fn period_of(row: &[serde_json::Value]) -> Option<Period> {
    let n = row.first()?.as_f64()?;
    let at = |i: usize| row.get(i).and_then(|v| v.as_f64());
    if n < 1.0 {
        return None;
    }
    if at(3) == Some(n) {
        Some(Period::Year)
    } else if at(2) == Some(n) {
        Some(Period::Quarter)
    } else if at(1) == Some(n) {
        Some(Period::Month)
    } else {
        None
    }
}

impl<'a> Env<'a> {
    fn links(&self) -> Links<'a> {
        Links::new(self.rels, Some(self.health)).with_link_tables(self.tables)
    }
}

/// A named measure, checked and written as SQL.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub name: String,
    pub agg: AggregateSql,
    pub dtype: DType,
    /// The calculated columns the formula reads that could not be computed, as (table id, column).
    /// A part that reads one is blank, never 0 (V4-D103).
    pub broken: Vec<(String, String)>,
}

impl Resolved {
    /// The formula is one total and nothing else: `SUM(...)`, `AVERAGE(...)`.
    fn is_one_total(&self) -> bool {
        self.agg.parts.len() == 1 && self.agg.combine.trim() == "$p0"
    }

    /// The value for a group, or the whole, is the sum of the values of its parts: a share of the
    /// whole, a running total or a pile of bars read it right. A formula that is one sum or count
    /// is. So is a plain sum, count or difference of them (`SUM([a]) - SUM([b])`).
    pub fn is_additive(&self) -> bool {
        !self.agg.parts.is_empty()
            && self.agg.parts.iter().all(|p| p.additive)
            && combine_is_linear(&self.agg.combine)
    }

    /// The tables whose rows the formula adds up, once each, in the order it reads them.
    pub fn homes(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for p in &self.agg.parts {
            if !out.contains(&p.home_table) {
                out.push(p.home_table.clone());
            }
        }
        out
    }

    /// The value of the formula over no rows at all, as SQL: 0 for a sum or a count, blank for the
    /// rest, and what the formula makes of them. A time chart fills a quiet period with this.
    pub fn empty_sql(&self) -> String {
        let values: Vec<String> = self
            .agg
            .parts
            .iter()
            .map(|p| {
                if p.zero_when_empty {
                    "0e0".to_string()
                } else {
                    typed_null(p.dtype)
                }
            })
            .collect();
        format!("SELECT {}", out_sql(&self.agg.fill(&values), self.dtype))
    }
}

/// The formula that combines the parts is linear in them: sums and differences of parts, and a
/// part times or over a number. Then the value of a whole is the sum of the values of its groups.
/// `SUM(a) + 5` is not (5 would be added once for each group), `SUM(a) * SUM(b)` is not, and
/// neither is `ROUND(SUM(a), 2)` or `SUM(a) / SUM(b)`.
fn combine_is_linear(combine: &str) -> bool {
    #[derive(PartialEq, Clone, Copy)]
    enum Shape {
        /// A number.
        Number,
        /// Made of parts, and linear in them.
        Linear,
    }
    struct P<'a> {
        s: &'a [u8],
        at: usize,
    }
    impl P<'_> {
        fn skip(&mut self) {
            while self.s.get(self.at).is_some_and(u8::is_ascii_whitespace) {
                self.at += 1;
            }
        }
        fn eat(&mut self, c: u8) -> bool {
            self.skip();
            if self.s.get(self.at) == Some(&c) {
                self.at += 1;
                true
            } else {
                false
            }
        }
        fn expr(&mut self) -> Option<Shape> {
            let mut left = self.term()?;
            loop {
                let plus = self.eat(b'+');
                if !plus && !self.eat(b'-') {
                    return Some(left);
                }
                let right = self.term()?;
                // A part plus a number is a part with something added once, not a linear one.
                if left != right {
                    return None;
                }
                left = right;
            }
        }
        fn term(&mut self) -> Option<Shape> {
            let mut left = self.unary()?;
            loop {
                if self.eat(b'*') {
                    let right = self.unary()?;
                    left = match (left, right) {
                        (Shape::Number, Shape::Number) => Shape::Number,
                        (Shape::Linear, Shape::Number) | (Shape::Number, Shape::Linear) => {
                            Shape::Linear
                        }
                        _ => return None,
                    };
                } else if self.eat(b'/') {
                    // Over a number only.
                    if self.unary()? != Shape::Number {
                        return None;
                    }
                } else {
                    return Some(left);
                }
            }
        }
        fn unary(&mut self) -> Option<Shape> {
            if self.eat(b'-') || self.eat(b'+') {
                return self.unary();
            }
            // A number parameter's value (v5 build 05) is a number.
            if let Some(n) = crate::params::number_sql_at(&self.s[self.at..]) {
                self.at += n;
                return Some(Shape::Number);
            }
            if self.eat(b'(') {
                let inner = self.expr()?;
                return self.eat(b')').then_some(inner);
            }
            self.skip();
            let start = self.at;
            if self.s.get(self.at) == Some(&b'$') && self.s.get(self.at + 1) == Some(&b'p') {
                self.at += 2;
                while self.s.get(self.at).is_some_and(u8::is_ascii_digit) {
                    self.at += 1;
                }
                return (self.at > start + 2).then_some(Shape::Linear);
            }
            // A number, written the way SQL writes it: `1.11e0`.
            while self
                .s
                .get(self.at)
                .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E'))
            {
                self.at += 1;
            }
            (self.at > start).then_some(Shape::Number)
        }
    }
    let mut p = P {
        s: combine.as_bytes(),
        at: 0,
    };
    let shape = p.expr();
    p.skip();
    shape == Some(Shape::Linear) && p.at == combine.len()
}

fn typed_null(d: DType) -> String {
    match d {
        DType::Number => "CAST(NULL AS DOUBLE)",
        DType::Text => "CAST(NULL AS VARCHAR)",
        DType::Date => "CAST(NULL AS DATE)",
        DType::DateTime => "CAST(NULL AS TIMESTAMP)",
        DType::Bool => "CAST(NULL AS BOOLEAN)",
    }
    .into()
}

/// A value as a query column shows it: numbers as DOUBLE, dates as text.
fn out_sql(sql: &str, dtype: DType) -> String {
    match dtype {
        DType::Number => format!("CAST({sql} AS DOUBLE)"),
        DType::Text => format!("CAST({sql} AS VARCHAR)"),
        DType::Date => format!("strftime({sql}, '%Y-%m-%d')"),
        DType::DateTime => format!("strftime({sql}, '%Y-%m-%dT%H:%M:%S')"),
        DType::Bool => sql.to_string(),
    }
}

/// Finds a named measure and writes its formula as SQL. A measure past a limit (V4-D85c) or with a
/// formula that does not work now is an error in words, never a crash.
pub fn resolve(env: &Env<'_>, catalog: &Catalog, id: &str) -> Result<Resolved, QueryError> {
    let at = env
        .measures
        .iter()
        .position(|m| m.id == id)
        .ok_or(QueryError::UnknownMeasure)?;
    let nm = &env.measures[at];
    if at >= MAX_MEASURES {
        return Err(QueryError::MeasureLimit(
            nm.name.clone(),
            format!(
                "a workspace uses its first {MAX_MEASURES} measures. Delete one that is not needed."
            ),
        ));
    }
    resolve_formula(env, catalog, nm)
}

/// A plain closing or opening value (v5 build 07) as the measure `CLOSING(SUM([table].[column]))` (`COUNTROWS` with
/// no column), named after its column.
fn resolve_plain(env: &Env<'_>, catalog: &Catalog, m: &Measure) -> Result<Resolved, QueryError> {
    let t = lookup(catalog, &m.table)?;
    let f = if m.agg == Agg::Opening {
        "OPENING"
    } else {
        "CLOSING"
    };
    let table = t.id.replace(']', "]]");
    let (name, inner) = match &m.column {
        Some(c) => (
            c.clone(),
            format!("SUM([{table}].[{}])", c.replace(']', "]]")),
        ),
        None => ("Rows".to_string(), format!("COUNTROWS([{table}])")),
    };
    resolve_formula(
        env,
        catalog,
        &NamedMeasure {
            name,
            formula: format!("{f}({inner})"),
            ..NamedMeasure::default()
        },
    )
}

fn resolve_formula(
    env: &Env<'_>,
    catalog: &Catalog,
    nm: &NamedMeasure,
) -> Result<Resolved, QueryError> {
    if nm.formula.chars().count() > MAX_FORMULA {
        return Err(QueryError::MeasureLimit(
            nm.name.clone(),
            format!("its formula is longer than {MAX_FORMULA} characters. Make it shorter."),
        ));
    }
    let cx = formula::Context {
        catalog,
        rels: env.rels,
        health: env.health,
        today: env.now.date(),
        now: env.now,
        view: false,
        params: env.params,
        setups: Some(env.tables),
    };
    let c = formula::compile(&nm.formula, Kind::Measure, None, &cx)
        .map_err(|e| QueryError::MeasureBroken(nm.name.clone(), e.message))?;
    match c.body {
        Body::Aggregate(mut agg) => {
            // A part that reads a calculated column that failed has no value: its sum of blanks is
            // blank, and a blank is not the 0 of a table with no rows (V4-D22, V4-D103).
            let mut broken: Vec<(String, String)> = Vec::new();
            for p in &mut agg.parts {
                let bad: Vec<&(String, String)> = p
                    .reads
                    .iter()
                    .filter(|(t, c)| column_failed(catalog, t, c))
                    .collect();
                if !bad.is_empty() {
                    for b in bad {
                        if !broken.contains(b) {
                            broken.push(b.clone());
                        }
                    }
                    p.sql = typed_null(p.dtype);
                    p.joins.clear();
                    p.zero_when_empty = false;
                }
            }
            Ok(Resolved {
                name: nm.name.clone(),
                agg,
                dtype: c.dtype,
                broken,
            })
        }
        Body::Column(_) => Err(QueryError::MeasureBroken(
            nm.name.clone(),
            "A measure adds things up. Use SUM, AVERAGE, COUNT or another total.".into(),
        )),
    }
}

/// What the engine needs to know about one measure of a tile: the tables it adds up, whether its
/// values add up to the whole, and what it is called.
pub struct Facts {
    pub name: String,
    pub homes: Vec<String>,
    pub additive: bool,
    /// A closing or an opening value (v5 build 07): `Some(true)` for closing, `Some(false)` for opening.
    pub stock: Option<bool>,
}

pub fn facts(env: &Env<'_>, catalog: &Catalog, m: &Measure) -> Result<Facts, QueryError> {
    match &m.named {
        Some(id) => {
            let r = resolve(env, catalog, id)?;
            let stock = r.agg.parts.iter().find_map(|p| match p.scope {
                Some(PartScope::Closing) => Some(true),
                Some(PartScope::Opening) => Some(false),
                _ => None,
            });
            Ok(Facts {
                homes: r.homes(),
                additive: r.is_additive(),
                name: r.name,
                stock,
            })
        }
        None => Ok(Facts {
            name: m.column.clone().unwrap_or_else(|| "Rows".into()),
            homes: vec![m.table.clone()],
            additive: m.per.is_none() && matches!(m.agg, Agg::Sum | Agg::Count),
            stock: match m.agg {
                Agg::Closing => Some(true),
                Agg::Opening => Some(false),
                _ => None,
            },
        }),
    }
}

/// What the app needs to know about a measure of the workspace: its type, whether its values add
/// up (a share of the whole, a running total and a pile of bars are only right for these), the
/// tables it adds up, and why it does not work, if it does not. Made with every snapshot.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct MeasureInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dtype: Option<DType>,
    pub additive: bool,
    /// The ids of the tables the formula adds up.
    pub tables: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The info of every measure, by id.
pub fn infos(env: &Env<'_>, catalog: &Catalog) -> BTreeMap<String, MeasureInfo> {
    env.measures
        .iter()
        .map(|m| {
            let info = match resolve(env, catalog, &m.id) {
                Ok(r) => MeasureInfo {
                    dtype: Some(r.dtype),
                    additive: r.is_additive(),
                    tables: r.homes(),
                    error: None,
                },
                Err(e) => MeasureInfo {
                    dtype: None,
                    additive: false,
                    tables: Vec::new(),
                    error: Some(match e {
                        QueryError::MeasureBroken(_, why) | QueryError::MeasureLimit(_, why) => why,
                        other => other.to_string(),
                    }),
                },
            };
            (m.id.clone(), info)
        })
        .collect()
}

// ---- the question, read part by part -----------------------------------------------------

enum Item {
    Plain(Measure),
    Named(Resolved),
}

/// One total inside a sub-query.
#[derive(PartialEq)]
enum Member {
    /// The SQL of a part of a formula, over `t0` and the joins of its group.
    Sql(String),
    /// A plain measure, written by the scope of the group.
    Plain(Measure),
}

/// All the totals of one home table that read the same joins: one sub-query.
struct Group {
    home: String,
    joins: Vec<Join>,
    members: Vec<Member>,
    /// A plain measure with a ratio or a link of its own: its sub-query is built from that measure.
    lone: bool,
    /// `TOTAL` or `UNFILTERED` (v5 build 06): the sub-query reads fewer of the tile's groups or filters. It never
    /// gives the tile its group values; it is joined to them.
    scope: Option<PartScope>,
}

impl Group {
    fn add(&mut self, m: Member) -> usize {
        match self.members.iter().position(|x| *x == m) {
            Some(i) => i,
            None => {
                self.members.push(m);
                self.members.len() - 1
            }
        }
    }
}

/// Where the total of a measure sits: the sub-query and the place in it.
#[derive(Clone, Copy)]
struct Slot {
    group: usize,
    member: usize,
}

/// The date column a time group or a date filter uses for a table (V4-D5): the one the user picked
/// (`mainDate`), else the only date or date-time column of the table.
pub fn main_date<'a>(
    catalog: &'a Catalog,
    tables: &BTreeMap<String, TableSetup>,
    table: &str,
) -> Option<&'a Column> {
    let t = catalog.table(table)?;
    if let Some(name) = tables.get(table).and_then(|s| s.main_date.as_deref()) {
        return t.column(name).filter(|c| c.dtype.is_temporal());
    }
    let mut dates = t.columns.iter().filter(|c| c.dtype.is_temporal());
    match (dates.next(), dates.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    }
}

/// A calculated column that could not be computed, so its values are blank (V4-D67).
fn column_failed(catalog: &Catalog, table: &str, column: &str) -> bool {
    catalog
        .table(table)
        .and_then(|t| t.column(column))
        .is_some_and(|c| c.error.is_some())
}

fn is_date(catalog: &Catalog, table: &str, column: &str) -> bool {
    catalog
        .table(table)
        .and_then(|t| t.column(column))
        .is_some_and(|c| c.dtype.is_temporal())
}

fn column_label(catalog: &Catalog, table: &str, column: &str) -> String {
    catalog
        .table(table)
        .and_then(|t| t.column(column))
        .and_then(|c| c.label.clone())
        .unwrap_or_else(|| column.to_string())
}

/// What one group reads of the question: the groups and the filters it can reach (a date read from
/// the main date counts as reached).
struct Reads {
    dims: Vec<Option<Dimension>>,
    filters: Vec<Option<Filter>>,
    /// The group or the filter is read through the main date of this table, not as the question
    /// wrote it (V4-D98).
    remapped_dims: Vec<bool>,
    remapped_filters: Vec<bool>,
}

/// A date column is a main date for the question when it is the main date of its table, and either the user
/// picked it, or the table is an event table: some link starts from it (many rows to one), as the sales and the
/// targets do. A date that only describes a row of a table other tables point at (the launch of a product, the
/// day a store opened) is not a time axis, so a part is not split by it through its own dates (V4-D98, V4-D100).
fn is_main(catalog: &Catalog, env: &Env<'_>, table: &str, column: &str) -> bool {
    let Some(c) = main_date(catalog, env.tables, table) else {
        return false;
    };
    if c.name != column {
        return false;
    }
    let picked = env
        .tables
        .get(table)
        .and_then(|s| s.main_date.as_deref())
        .is_some();
    // A table that others point at and that points at none is a table of things. Any other is of events.
    picked
        || env.rels.iter().any(|r| r.from.table == table)
        || !env.rels.iter().any(|r| r.to.table == table)
}

/// The date a part of `table` is split by when a group or a filter of the question is a main date of
/// another table: the main date of `table`, if it is a main date itself (`is_main`). A table of things
/// (stores, with the day each opened) has none, so its part is not split by time (V4-D98, V4-D122).
fn time_axis<'a>(catalog: &'a Catalog, env: &Env<'_>, table: &str) -> Option<&'a Column> {
    main_date(catalog, env.tables, table).filter(|c| is_main(catalog, env, table, &c.name))
}

/// No link starts from the table or points at it.
fn unlinked(env: &Env<'_>, table: &str) -> bool {
    !env.rels
        .iter()
        .any(|r| r.from.table == table || r.to.table == table)
}

fn reads(
    catalog: &Catalog,
    env: &Env<'_>,
    links: Links<'_>,
    home: &str,
    q: &SemanticQuery,
) -> Result<Reads, QueryError> {
    let reaches = |table: &str, via: Option<&str>| -> Result<(), PathError> {
        Joins::new(home, links).add(table, via)
    };
    let main = time_axis(catalog, env, home).map(|c| c.name.clone());
    let mut dims = Vec::new();
    let mut remapped_dims = Vec::new();
    for d in &q.dimensions {
        lookup(catalog, &d.table)?;
        let mut remapped = false;
        dims.push(match reaches(&d.table, d.via.as_deref()) {
            Ok(()) => Some(d.clone()),
            Err(PathError::Gone) => return Err(QueryError::LinkGone),
            Err(_) => {
                if d.via.is_some() && reaches(&d.table, None).is_ok() {
                    // The group names a link this part does not cross. The part reaches the same
                    // table and column by its own way, and is split by that (V4-D107).
                    Some(Dimension {
                        via: None,
                        ..d.clone()
                    })
                } else {
                    match (&main, d.grain) {
                        // A date at a grain that is a main date is read from the main date of
                        // this table, at the same grain.
                        (Some(m), Some(g)) if is_main(catalog, env, &d.table, &d.column) => {
                            remapped = true;
                            Some(Dimension {
                                table: home.to_string(),
                                column: m.clone(),
                                grain: Some(g),
                                via: None,
                            })
                        }
                        _ => None,
                    }
                }
            }
        });
        remapped_dims.push(remapped);
    }
    let mut filters = Vec::new();
    let mut remapped_filters = Vec::new();
    for f in &q.filters {
        let mut remapped = false;
        filters.push(if catalog.table(&f.table).is_none() {
            None
        } else {
            match reaches(&f.table, f.via.as_deref()) {
                Ok(()) => Some(f.clone()),
                Err(_) if f.via.is_some() && reaches(&f.table, None).is_ok() => Some(Filter {
                    via: None,
                    ..f.clone()
                }),
                Err(_) => match &main {
                    Some(m) if is_main(catalog, env, &f.table, &f.column) => {
                        remapped = true;
                        Some(Filter {
                            table: home.to_string(),
                            column: m.clone(),
                            via: None,
                            ..f.clone()
                        })
                    }
                    _ => None,
                },
            }
        });
        remapped_filters.push(remapped);
    }
    Ok(Reads {
        dims,
        filters,
        remapped_dims,
        remapped_filters,
    })
}

/// The days a date filter lets through, as `(first, last)`, either open (V4-D98). Filters that are not a
/// window of days (a list of values, blank) have none.
fn window(f: &Filter, today: NaiveDate) -> Option<(Option<NaiveDate>, Option<NaiveDate>)> {
    use crate::query::FilterOp;
    let f = query::resolve_relative(f, today);
    let day = |i: usize| {
        f.values
            .get(i)
            .and_then(|v| v.as_str())
            .and_then(|s| NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok())
    };
    match f.op {
        FilterOp::Between => Some((day(0), day(1))),
        // lo <= x < hi: the last day let through is the one before hi.
        FilterOp::Range => Some((day(0), day(1).and_then(|d| d.pred_opt()))),
        FilterOp::Gte => Some((day(0), None)),
        FilterOp::Lte => Some((None, day(0))),
        _ => None,
    }
}

/// What a part read by a coarse main date cannot say about a filter window: a period counts when the
/// window holds its first day, so a window that starts inside a period leaves it out and one that
/// ends inside a period keeps it whole (V4-D98, V4-D101). No prorating.
fn window_notes(table: &str, period: Period, f: &Filter, today: NaiveDate) -> Vec<String> {
    let Some((lo, hi)) = window(f, today) else {
        return Vec::new();
    };
    // A window across years names the year, so "March" is not ambiguous.
    let year = matches!((lo, hi), (Some(a), Some(b)) if a.year() != b.year());
    let on = |d: NaiveDate| {
        if year {
            format!("{} {}", d.day(), d.format("%B %Y"))
        } else {
            format!("{} {}", d.day(), d.format("%B"))
        }
    };
    let mut out = Vec::new();
    if let Some(lo) = lo.filter(|d| period.first(*d) != *d) {
        out.push(format!(
            "{table} is read by {}: {} is left out because the window starts on {}.",
            period.word(),
            period.name(lo, year),
            on(lo)
        ));
    }
    if let Some(hi) = hi.filter(|d| period.last(*d) != *d) {
        out.push(format!(
            "{table} is read by {}: {} is counted whole because the window ends on {}.",
            period.word(),
            period.name(hi, year),
            on(hi)
        ));
    }
    out
}

/// The question as the part with this home table reads it: the groups and filters it can reach, or can read
/// through its main date, and no others.
pub fn narrow(
    q: &SemanticQuery,
    catalog: &Catalog,
    env: &Env<'_>,
    home: &str,
) -> Result<SemanticQuery, QueryError> {
    let r = reads(catalog, env, env.links(), home, q)?;
    Ok(SemanticQuery {
        dimensions: r.dims.into_iter().flatten().collect(),
        filters: r.filters.into_iter().flatten().collect(),
        ..q.clone()
    })
}

/// The main dates a question reads, as (table id, column, SQL reference), for the engine to read the
/// grain of (V4-D101): one for each table a total of the question adds up.
pub fn grain_wants(
    q: &SemanticQuery,
    catalog: &Catalog,
    env: &Env<'_>,
) -> Vec<(String, String, String)> {
    let mut homes: Vec<String> = Vec::new();
    let mut add = |t: String| {
        if !homes.contains(&t) {
            homes.push(t);
        }
    };
    for m in &q.measures {
        match &m.named {
            Some(id) => {
                if let Ok(r) = resolve(env, catalog, id) {
                    r.homes().into_iter().for_each(&mut add);
                }
            }
            None => add(m.table.clone()),
        }
    }
    homes
        .into_iter()
        .filter_map(|h| {
            let c = time_axis(catalog, env, &h)?;
            let t = catalog.table(&h)?;
            Some((h.clone(), c.name.clone(), t.sql_ref.clone()))
        })
        .collect()
}

/// Compiles a question that has a named measure in it. A question with none is compiled by
/// `query::compile_on`, unchanged.
pub fn compile(
    q: &SemanticQuery,
    catalog: &Catalog,
    today: NaiveDate,
    env: &Env<'_>,
) -> Result<Compiled, QueryError> {
    let links = env.links();
    let closing = |m: &Measure| m.named.is_none() && matches!(m.agg, Agg::Closing | Agg::Opening);
    if q.measures.iter().all(|m| m.named.is_none() && !closing(m)) {
        return query::compile_on(q, catalog, links, today);
    }
    // 1. The measures, and the totals they are made of, grouped by the table they read.
    let mut items = Vec::new();
    for m in &q.measures {
        items.push(match &m.named {
            Some(id) => Item::Named(resolve(env, catalog, id)?),
            None if closing(m) => Item::Named(resolve_plain(env, catalog, m)?),
            None => {
                lookup(catalog, &m.table)?;
                Item::Plain(m.clone())
            }
        });
    }
    // The calculated columns that failed and that a value reads: the value is blank (V4-D103).
    let mut blank_reads: Vec<(String, String)> = Vec::new();
    let mut plain_blank: Vec<bool> = Vec::new();
    for item in &items {
        let mut bad = false;
        match item {
            Item::Named(r) => {
                for b in &r.broken {
                    if !blank_reads.contains(b) {
                        blank_reads.push(b.clone());
                    }
                }
            }
            Item::Plain(m) => {
                let cols = m
                    .column
                    .iter()
                    .map(|c| (m.table.as_str(), c.as_str()))
                    .chain(
                        m.per
                            .iter()
                            .filter_map(|p| p.column.as_deref().map(|c| (p.table.as_str(), c))),
                    );
                for (t, c) in cols {
                    if column_failed(catalog, t, c) {
                        bad = true;
                        let b = (t.to_string(), c.to_string());
                        if !blank_reads.contains(&b) {
                            blank_reads.push(b);
                        }
                    }
                }
            }
        }
        plain_blank.push(bad);
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut slots: Vec<Vec<Slot>> = Vec::new();
    for item in &items {
        let mut mine = Vec::new();
        match item {
            Item::Named(r) => {
                for p in &r.agg.parts {
                    lookup(catalog, &p.home_table)?;
                    let g = match groups.iter().position(|g| {
                        !g.lone
                            && g.home == p.home_table
                            && g.joins == p.joins
                            && g.scope == p.scope
                    }) {
                        Some(g) => g,
                        None => {
                            groups.push(Group {
                                home: p.home_table.clone(),
                                joins: p.joins.clone(),
                                members: Vec::new(),
                                lone: false,
                                scope: p.scope.clone(),
                            });
                            groups.len() - 1
                        }
                    };
                    let member = groups[g].add(Member::Sql(p.sql.clone()));
                    mine.push(Slot { group: g, member });
                }
            }
            Item::Plain(m) if m.per.is_none() && m.via.is_none() => {
                let g = match groups.iter().position(|g| {
                    !g.lone && g.home == m.table && g.joins.is_empty() && g.scope.is_none()
                }) {
                    Some(g) => g,
                    None => {
                        groups.push(Group {
                            home: m.table.clone(),
                            joins: Vec::new(),
                            members: Vec::new(),
                            lone: false,
                            scope: None,
                        });
                        groups.len() - 1
                    }
                };
                let member = groups[g].add(Member::Plain(m.clone()));
                mine.push(Slot { group: g, member });
            }
            Item::Plain(m) => {
                let g = match groups
                    .iter()
                    .position(|g| g.lone && g.members == [Member::Plain(m.clone())])
                {
                    Some(g) => g,
                    None => {
                        groups.push(Group {
                            home: m.table.clone(),
                            joins: Vec::new(),
                            members: vec![Member::Plain(m.clone())],
                            lone: true,
                            scope: None,
                        });
                        groups.len() - 1
                    }
                };
                mine.push(Slot {
                    group: g,
                    member: 0,
                });
            }
        }
        slots.push(mine);
    }
    if groups.is_empty() {
        let name = items
            .iter()
            .find_map(|i| match i {
                Item::Named(r) => Some(r.name.clone()),
                Item::Plain(_) => None,
            })
            .unwrap_or_default();
        return Err(QueryError::MeasureBroken(
            name,
            "A measure adds things up. Use SUM, AVERAGE, COUNT or another total.".into(),
        ));
    }

    // A question whose totals all have a TOTAL or an UNFILTERED still has the tile's own group values: they come
    // from the rows of the first such table under all the filters (v5 build 06).
    if groups.iter().all(|g| g.scope.is_some()) {
        let first = &groups[0];
        groups.push(Group {
            home: first.home.clone(),
            joins: first.joins.clone(),
            members: vec![Member::Plain(count_rows(&first.home))],
            lone: false,
            scope: None,
        });
    }

    // 2. What each group can reach of the groups and the filters of the question.
    let nd = q.dimensions.len();
    let mut read: Vec<Reads> = Vec::new();
    for g in &groups {
        read.push(reads(catalog, env, links, &g.home, q)?);
    }
    // What a TOTAL or an UNFILTERED leaves out on purpose: no note says that a part is not split or not filtered
    // by it (v5 build 06, V5-D8).
    let mut dropped_dims = vec![vec![false; nd]; groups.len()];
    // The parts that have no value on this tile, and why.
    let mut blank_groups: Vec<(usize, String)> = Vec::new();
    let mut dropped_filters = vec![vec![false; q.filters.len()]; groups.len()];
    for (gi, g) in groups.iter().enumerate() {
        match &g.scope {
            Some(PartScope::Total(keep)) => {
                let named = |t: &str, c: &str| keep.iter().any(|k| k.0 == t && k.1 == c);
                // A column the tile is not grouped by (a tile by store, or the total row of a table): the value is
                // blank, and the tile says why. The grand total would be the wrong answer. A filter that keeps one
                // value of it (a filter tile on one region, a drill into it) is as good as the group.
                if let Some(k) = keep.iter().find(|k| {
                    !q.dimensions
                        .iter()
                        .any(|d| d.table == k.0 && d.column == k.1)
                        && !q.filters.iter().any(|f| {
                            f.table == k.0
                                && f.column == k.1
                                && f.op == query::FilterOp::In
                                && f.values.len() == 1
                        })
                }) {
                    blank_groups.push((
                        gi,
                        format!(
                            "TOTAL keeps the groups of {}, but this tile is not grouped by it, so the value is blank. Group the tile by it too, or leave it out of TOTAL.",
                            column_label(catalog, &k.0, &k.1)
                        ),
                    ));
                }
                for (i, d) in q.dimensions.iter().enumerate() {
                    if !named(&d.table, &d.column) {
                        read[gi].dims[i] = None;
                        dropped_dims[gi][i] = true;
                    }
                }
            }
            Some(PartScope::Unfiltered(drop)) => {
                for (j, f) in q.filters.iter().enumerate() {
                    if drop.is_empty() || drop.iter().any(|k| k.0 == f.table && k.1 == f.column) {
                        read[gi].filters[j] = None;
                        dropped_filters[gi][j] = true;
                    }
                }
            }
            // CLOSING and OPENING (v5 build 07) read the groups and the filters of the tile; they need a date.
            Some(PartScope::Closing | PartScope::Opening)
                if main_date(catalog, env.tables, &g.home).is_none() =>
            {
                let measure = slots
                    .iter()
                    .zip(&items)
                    .find(|(s, _)| s.iter().any(|s| s.group == gi))
                    .and_then(|(_, i)| match i {
                        Item::Named(r) => Some(r.name.clone()),
                        Item::Plain(_) => None,
                    })
                    .unwrap_or_default();
                return Err(QueryError::MeasureBroken(
                    measure,
                    format!(
                        "A closing or opening value reads the rows on the last or first date of each period, and {} has no date. Add a date column to it, or pick its main date in Data.",
                        label(catalog, &g.home)
                    ),
                ));
            }
            _ => {}
        }
    }
    // A group that no table can reach is an error, in the words a plain question gives.
    for (i, d) in q.dimensions.iter().enumerate() {
        if read.iter().all(|r| r.dims[i].is_none()) {
            let home = &groups[0].home;
            let err = Joins::new(home, links)
                .add(&d.table, d.via.as_deref())
                .err()
                .unwrap_or(PathError::No);
            return Err(explain(catalog, links, home, &d.table, err));
        }
    }
    let ignored: Vec<usize> = (0..q.filters.len())
        .filter(|&j| read.iter().all(|r| r.filters[j].is_none()))
        .collect();

    // 3. One scope for each group, as a plain question from its home table.
    let mut scopes = Vec::new();
    for (g, r) in groups.iter().zip(&read) {
        let lead = match (&g.lone, g.members.first()) {
            (true, Some(Member::Plain(m))) => m.clone(),
            _ => count_rows(&g.home),
        };
        let sq = SemanticQuery {
            measures: vec![lead],
            dimensions: r.dims.iter().flatten().cloned().collect(),
            filters: r.filters.iter().flatten().cloned().collect(),
            sort: vec![],
            limit: None,
        };
        scopes.push(scope_on(&sq, catalog, links, today)?);
    }

    // 4. The text of each group: its group values and its totals.
    let mut dim_exprs: Vec<Vec<Option<String>>> = Vec::new();
    let mut dim_types: Vec<Option<DType>> = vec![None; nd];
    let mut member_exprs: Vec<Vec<String>> = Vec::new();
    let mut plain_names: BTreeMap<(usize, usize), (String, DType)> = BTreeMap::new();
    for (gi, g) in groups.iter().enumerate() {
        let sc = &scopes[gi];
        let mut exprs = Vec::new();
        for (i, d) in read[gi].dims.iter().enumerate() {
            exprs.push(match d {
                Some(d) => {
                    let (e, t) = sc.expr_via(&d.table, d.via.as_deref(), &d.column, d.grain)?;
                    dim_types[i].get_or_insert(t);
                    Some(e)
                }
                None => None,
            });
        }
        dim_exprs.push(exprs);
        let mut ms = Vec::new();
        for (mi, m) in g.members.iter().enumerate() {
            ms.push(match m {
                Member::Sql(s) => s.clone(),
                Member::Plain(m) => {
                    let (e, t, name) = sc.measure(m)?;
                    plain_names.insert((gi, mi), (name, t));
                    e
                }
            });
        }
        member_exprs.push(ms);
    }

    // 5. The words for what a part cannot reach, or can read only by period (V4-D85, V4-D98).
    let mut notes: Vec<Vec<(String, bool)>> = vec![Vec::new(); groups.len()];
    for (gi, g) in groups.iter().enumerate() {
        let table = label(catalog, &g.home);
        let main = time_axis(catalog, env, &g.home);
        // The only date of a table of things: it is not read as a time (V4-D122).
        let thing_date = main_date(catalog, env.tables, &g.home)
            .filter(|_| main.is_none())
            .map(|c| column_label(catalog, &g.home, &c.name));
        let no_time = |what: &str, c: &str| match &thing_date {
            Some(d) => format!(
                "{table} is not {what} {c}. Its date {d} is not a main date: pick it in Data if it is one."
            ),
            None => format!("{table} has no main date, so it is not {what} {c}."),
        };
        let period = main.and_then(|c| env.grains.get(&(g.home.clone(), c.name.clone())).copied());
        for (i, d) in q.dimensions.iter().enumerate() {
            if read[gi].dims[i].is_none()
                && !dropped_dims[gi][i]
                && read.iter().any(|r| r.dims[i].is_some())
            {
                let c = column_label(catalog, &d.table, &d.column);
                let time = d.grain.is_some() && is_main(catalog, env, &d.table, &d.column);
                // This table has the same value for every group: the groups do not add up (V4-D99a).
                notes[gi].push((
                    if time && main.is_none() {
                        no_time("split by", &c)
                    } else {
                        format!("{table} is not split by {c}.")
                    },
                    true,
                ));
            }
            // A table with no link at all is a table of events (V4-D111), so its date is read as this
            // part's own date. When that table is not a part of the question, say so: it may be a table of
            // things whose link is not drawn yet (V4-D122).
            if read[gi].remapped_dims[i]
                && unlinked(env, &d.table)
                && !groups.iter().any(|x| x.home == d.table)
                && let Some(m) = main
            {
                notes[gi].push((
                    format!(
                        "{} has no link to {table}, so {table} is split by its own date, {}.",
                        label(catalog, &d.table),
                        column_label(catalog, &g.home, &m.name)
                    ),
                    false,
                ));
            }
            if read[gi].remapped_dims[i]
                && let (Some(p), Some(gr)) = (period, d.grain)
                && let Some(unit) = p.finer_unit(gr)
            {
                notes[gi].push((
                    format!(
                        "{table} is read by {}: only the first {unit} of each {} has a value, so the other {unit}s are blank.",
                        p.word(),
                        p.word()
                    ),
                    false,
                ));
            }
        }
        for (j, f) in q.filters.iter().enumerate() {
            if read[gi].filters[j].is_none()
                && !dropped_filters[gi][j]
                && catalog.table(&f.table).is_some()
                && read.iter().any(|r| r.filters[j].is_some())
            {
                let c = column_label(catalog, &f.table, &f.column);
                let time = is_main(catalog, env, &f.table, &f.column);
                notes[gi].push((
                    if time && main.is_none() {
                        no_time("filtered by", &c)
                    } else {
                        format!("{table} is not filtered by {c}.")
                    },
                    false,
                ));
            }
            if read[gi].remapped_filters[j]
                && let Some(p) = period
            {
                for n in window_notes(&table, p, f, today) {
                    notes[gi].push((n, false));
                }
            }
        }
        notes[gi].dedup();
    }
    let mut parts = Vec::new();
    for (j, mine) in slots.iter().enumerate() {
        let mut seen: Vec<usize> = Vec::new();
        for s in mine {
            if seen.contains(&s.group) {
                continue;
            }
            seen.push(s.group);
            for (n, repeats) in &notes[s.group] {
                let p = PartNote {
                    measure: j,
                    note: n.clone(),
                    repeats: *repeats,
                    overlap: false,
                    split_only: false,
                };
                if !parts.contains(&p) {
                    parts.push(p);
                }
            }
        }
    }

    // A TOTAL that keeps a group the tile does not have: the measure is blank, and says why (v5 build 06).
    for (j, mine) in slots.iter().enumerate() {
        for (g, words) in &blank_groups {
            if mine.iter().any(|s| s.group == *g) {
                let p = PartNote {
                    measure: j,
                    note: words.clone(),
                    repeats: false,
                    overlap: false,
                    split_only: false,
                };
                if !parts.contains(&p) {
                    parts.push(p);
                }
            }
        }
    }

    // A group behind a link table puts a row under several groups: the values of every part that
    // is split by it do not add up on this tile (V4-D112).
    for (j, mine) in slots.iter().enumerate() {
        let mut seen: Vec<usize> = Vec::new();
        for s in mine {
            if seen.contains(&s.group) {
                continue;
            }
            seen.push(s.group);
            for note in scopes[s.group].filter_notes() {
                let p = PartNote {
                    measure: j,
                    note,
                    repeats: false,
                    overlap: false,
                    split_only: false,
                };
                if !parts.contains(&p) {
                    parts.push(p);
                }
            }
            // The first group of the scope is the first of the tile only when this part reads it.
            let first = read[s.group].dims.first().is_some_and(Option::is_some)
                && scopes[s.group].first_behind;
            for note in scopes[s.group].overlap_notes() {
                match parts
                    .iter_mut()
                    .find(|p| p.measure == j && p.overlap && p.note == note)
                {
                    Some(p) => p.split_only &= !first,
                    None => parts.push(PartNote {
                        measure: j,
                        note,
                        repeats: false,
                        overlap: true,
                        split_only: !first,
                    }),
                }
            }
        }
    }

    // The columns of the answer.
    let mut columns = Vec::new();
    let mut ordered = Vec::new();
    for (i, d) in q.dimensions.iter().enumerate() {
        let dtype = dim_types[i].unwrap_or(DType::Text);
        ordered.push(
            dtype.is_temporal()
                || (d.grain.is_some_and(Grain::is_part) && is_date(catalog, &d.table, &d.column)),
        );
        columns.push(OutColumn {
            name: d.column.clone(),
            role: "dimension".into(),
            dtype,
            unit: None,
        });
    }
    for (j, item) in items.iter().enumerate() {
        let (name, dtype) = match item {
            Item::Named(r) => (r.name.clone(), r.dtype),
            Item::Plain(_) => {
                let s = slots[j][0];
                plain_names
                    .get(&(s.group, s.member))
                    .cloned()
                    .unwrap_or_else(|| ("Rows".into(), DType::Number))
            }
        };
        columns.push(OutColumn {
            name,
            role: "measure".into(),
            dtype,
            unit: None,
        });
    }

    // The links the sub-queries cross that have something to tell the reader.
    let mut crossed: Vec<String> = Vec::new();
    for (gi, sc) in scopes.iter().enumerate() {
        let from_formulas = groups[gi]
            .joins
            .iter()
            .filter(|j| j.warn)
            .map(|j| j.rel.clone());
        for id in sc.crossed.iter().cloned().chain(from_formulas) {
            if !crossed.contains(&id) {
                crossed.push(id);
            }
        }
    }

    // 6. The SQL.
    let one_total_each = items.iter().all(|i| match i {
        Item::Named(r) => r.is_one_total(),
        Item::Plain(_) => true,
    });
    let order = order_limit(q, columns.len(), &ordered, true);
    if groups.len() == 1 && one_total_each && groups[0].scope.is_none() {
        // The fast path: one SELECT, as a plain question from the table is.
        let sc = &scopes[0];
        let mut select = Vec::new();
        for (i, e) in dim_exprs[0].iter().enumerate() {
            let Some(e) = e else {
                // Cannot happen: a group that no table reaches was refused above.
                continue;
            };
            select.push(format!("{e} AS {}", ident(&format!("d{i}"))));
        }
        for (j, item) in items.iter().enumerate() {
            let s = slots[j][0];
            let expr = match item {
                Item::Named(r) => out_sql(
                    &r.agg.fill(std::slice::from_ref(&member_exprs[0][s.member])),
                    r.dtype,
                ),
                Item::Plain(_) => member_exprs[0][s.member].clone(),
            };
            select.push(format!("{expr} AS {}", ident(&format!("m{}", nd + j))));
        }
        let mut sql = format!("SELECT {}", select.join(", "));
        sql.push_str(&sc.from_joins);
        for j in &groups[0].joins {
            sql.push_str(&j.to_sql());
        }
        sql.push_str(&sc.where_clause);
        if nd > 0 {
            let g: Vec<String> = (1..=nd).map(|i| i.to_string()).collect();
            sql.push_str(&format!(" GROUP BY {}", g.join(", ")));
        }
        sql.push_str(&order);
        return Ok(Compiled {
            sql,
            params: sc.params.clone(),
            columns,
            ignored_filters: ignored,
            crossed,
            parts,
            blank_reads,
        });
    }

    // The general path: one sub-query for each group, then the outer query.
    let mut ctes = Vec::new();
    let mut params = Vec::new();
    for (gi, g) in groups.iter().enumerate() {
        let sc = &scopes[gi];
        let mut select = Vec::new();
        let mut kept = 0;
        for (i, e) in dim_exprs[gi].iter().enumerate() {
            if let Some(e) = e {
                select.push(format!("{e} AS {}", ident(&format!("d{i}"))));
                kept += 1;
            }
        }
        for (mi, e) in member_exprs[gi].iter().enumerate() {
            select.push(format!("{e} AS {}", ident(&format!("v{mi}"))));
        }
        let mut sql = format!("g{gi} AS (SELECT {}", select.join(", "));
        sql.push_str(&sc.from_joins);
        for j in &g.joins {
            sql.push_str(&j.to_sql());
        }
        // CLOSING and OPENING (v5 build 07, V5-D9): only the rows on the last (first) day of each period that has any
        // row under the filters. The period is the tile's time group; with none, all the days the filters keep. The
        // day is the table's, not each store's, so a store with no row that day counts as nothing.
        if let Some(scope @ (PartScope::Closing | PartScope::Opening)) = &g.scope
            && let Some(main) = main_date(catalog, env.tables, &g.home)
        {
            let (day, _) = sc.expr_via(&g.home, None, &main.name, Some(Grain::Day))?;
            let period = q
                .dimensions
                .iter()
                .enumerate()
                .find_map(|(i, _)| {
                    let d = read[gi].dims[i].as_ref()?;
                    (d.grain.is_some() && d.table == g.home && d.column == main.name)
                        .then(|| dim_exprs[gi][i].clone())
                        .flatten()
                })
                .unwrap_or_else(|| "0".into());
            let pick = if matches!(scope, PartScope::Closing) {
                "max"
            } else {
                "min"
            };
            let mut inner = format!("SELECT {period} AS p, {pick}({day}) AS d");
            inner.push_str(&sc.from_joins);
            for j in &g.joins {
                inner.push_str(&j.to_sql());
            }
            inner.push_str(&sc.where_clause);
            inner.push_str(" GROUP BY 1");
            sql.push_str(&format!(
                " JOIN ({inner}) AS cl{gi} ON cl{gi}.p IS NOT DISTINCT FROM {period} AND {day} = cl{gi}.d"
            ));
            // The filters of the inner query come first in the text.
            params.extend(sc.params.iter().cloned());
        }
        sql.push_str(&sc.where_clause);
        if kept > 0 {
            let by: Vec<String> = (1..=kept).map(|i| i.to_string()).collect();
            sql.push_str(&format!(" GROUP BY {}", by.join(", ")));
        }
        sql.push(')');
        ctes.push(sql);
        params.extend(sc.params.iter().cloned());
    }

    // The group values of the tile: the sub-queries that reach all of them, side by side.
    let has = |gi: usize, i: usize| dim_exprs[gi][i].is_some();
    // A part with a TOTAL or an UNFILTERED never gives the tile its group values: an UNFILTERED part would bring back
    // a store the filters took away. It is joined to the values the other parts give.
    let full: Vec<usize> = (0..groups.len())
        .filter(|&gi| groups[gi].scope.is_none() && (0..nd).all(|i| has(gi, i)))
        .collect();
    let d = |gi: usize, i: usize| format!("g{gi}.{}", ident(&format!("d{i}")));
    let mut from = String::new();
    let mut key: Vec<String> = Vec::new();
    let mut partial: Vec<usize> = (0..groups.len()).collect();
    if nd == 0 {
        from.push_str(&format!("g{}", 0));
        for gi in 1..groups.len() {
            from.push_str(&format!(" CROSS JOIN g{gi}"));
        }
        partial.clear();
    } else if let Some(&first) = full.first() {
        from.push_str(&format!("g{first}"));
        let mut so_far = vec![first];
        let keys_of = |so_far: &[usize], i: usize| -> String {
            if so_far.len() == 1 {
                d(so_far[0], i)
            } else {
                format!(
                    "COALESCE({})",
                    so_far
                        .iter()
                        .map(|&g| d(g, i))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        };
        for &gi in full.iter().skip(1) {
            let on: Vec<String> = (0..nd)
                .map(|i| format!("{} IS NOT DISTINCT FROM {}", d(gi, i), keys_of(&so_far, i)))
                .collect();
            from.push_str(&format!(" FULL OUTER JOIN g{gi} ON {}", on.join(" AND ")));
            so_far.push(gi);
        }
        for i in 0..nd {
            key.push(keys_of(&so_far, i));
        }
        partial.retain(|g| !full.contains(g));
        // A group value that only a part which cannot see all the groups has (a store with a target
        // and no sale, in a chart of stores by category) would vanish: the keys come from the parts
        // that reach every group. It appears once, with the groups that part cannot reach left blank
        // (V4-D108). Parts that reach no group have nothing to say about a key.
        let extra: Vec<usize> = partial
            .iter()
            .copied()
            .filter(|&g| groups[g].scope.is_none() && (0..nd).any(|i| has(g, i)))
            .collect();
        if !extra.is_empty() {
            let own: Vec<String> = (0..nd)
                .map(|i| format!("{} AS {}", key[i], ident(&format!("d{i}"))))
                .collect();
            ctes.push(format!("kf AS (SELECT {} FROM {})", own.join(", "), from));
            let kd = |i: usize| ident(&format!("d{i}"));
            let more: Vec<String> = extra
                .iter()
                .map(|&g| {
                    let cols: Vec<String> = (0..nd)
                        .map(|i| {
                            if has(g, i) {
                                format!("{} AS {}", d(g, i), kd(i))
                            } else {
                                format!("NULL AS {}", kd(i))
                            }
                        })
                        .collect();
                    let same: Vec<String> = (0..nd)
                        .filter(|&i| has(g, i))
                        .map(|i| format!("kf.{} IS NOT DISTINCT FROM {}", kd(i), d(g, i)))
                        .collect();
                    format!(
                        "SELECT {} FROM g{g} WHERE NOT EXISTS (SELECT 1 FROM kf WHERE {})",
                        cols.join(", "),
                        same.join(" AND ")
                    )
                })
                .collect();
            ctes.push(format!(
                "k AS (SELECT * FROM kf UNION ALL SELECT * FROM ({}))",
                more.join(" UNION ")
            ));
            from = "k".to_string();
            for (i, k) in key.iter_mut().enumerate() {
                *k = format!("k.{}", kd(i));
            }
            partial = (0..groups.len()).collect();
        }
    } else {
        // No sub-query reaches all the groups: every value of each group, in every combination.
        let mut sources = Vec::new();
        for i in 0..nd {
            let gi = (0..groups.len())
                .find(|&g| groups[g].scope.is_none() && has(g, i))
                .or_else(|| (0..groups.len()).find(|&g| has(g, i)))
                .unwrap_or(0);
            sources.push(format!(
                "(SELECT DISTINCT {} AS {} FROM g{gi}) AS k{i}",
                d(gi, i),
                ident(&format!("d{i}"))
            ));
            key.push(format!("k{i}.{}", ident(&format!("d{i}"))));
        }
        from.push_str(&sources.join(" CROSS JOIN "));
    }
    if nd == 0 {
        key.clear();
    } else if key.is_empty() {
        unreachable!("a question with groups has a key");
    }
    for gi in partial {
        let on: Vec<String> = (0..nd)
            .filter(|&i| has(gi, i))
            .map(|i| format!("{} IS NOT DISTINCT FROM {}", d(gi, i), key[i]))
            .collect();
        from.push_str(&format!(
            " LEFT JOIN g{gi} ON {}",
            if on.is_empty() {
                "TRUE".to_string()
            } else {
                on.join(" AND ")
            }
        ));
    }

    let mut select = Vec::new();
    for (i, k) in key.iter().enumerate() {
        select.push(format!("{k} AS {}", ident(&format!("d{i}"))));
    }
    for (j, item) in items.iter().enumerate() {
        let value = |s: &Slot| {
            if blank_groups.iter().any(|(g, _)| *g == s.group) {
                "CAST(NULL AS DOUBLE)".to_string()
            } else {
                format!("g{}.{}", s.group, ident(&format!("v{}", s.member)))
            }
        };
        let expr = match item {
            Item::Named(r) => {
                let vals: Vec<String> = slots[j].iter().map(value).collect();
                out_sql(&r.agg.fill(&vals), r.dtype)
            }
            Item::Plain(m) => {
                let v = value(&slots[j][0]);
                // A group this table has no rows for: nothing to add up is 0 for a sum or a count.
                if !plain_blank[j]
                    && m.per.is_none()
                    && matches!(m.agg, Agg::Sum | Agg::Count | Agg::CountDistinct)
                {
                    format!("COALESCE({v}, 0e0)")
                } else {
                    v
                }
            }
        };
        select.push(format!("{expr} AS {}", ident(&format!("m{}", nd + j))));
    }
    let sql = format!(
        "WITH {} SELECT {} FROM {}{}",
        ctes.join(", "),
        select.join(", "),
        from,
        order
    );
    Ok(Compiled {
        sql,
        params,
        columns,
        ignored_filters: ignored,
        crossed,
        parts,
        blank_reads,
    })
}

#[cfg(test)]
mod tests {
    use super::combine_is_linear;

    #[test]
    fn only_sums_differences_and_scaling_are_linear() {
        for yes in [
            "$p0",
            "($p0 - $p1)",
            "($p0 + $p1)",
            "($p0 * 1.11e0)",
            "(-$p0)",
            "($p0 / 1e2)",
            "(2e0 * ($p0 + $p1))",
            "$p0 - $p1 + $p2",
            "($p0 * (1e0 + (SELECT num FROM temp.main.tenun_param WHERE id = 'p1')))",
        ] {
            assert!(combine_is_linear(yes), "{yes}");
        }
        for no in [
            "($p0 + 5e0)",
            "($p0 + (SELECT num FROM temp.main.tenun_param WHERE id = 'p1'))",
            "($p0 * (SELECT txt FROM temp.main.tenun_param WHERE id = 'p1'))",
            "($p0 * $p1)",
            "($p0 / $p1)",
            "list_transform([$p0], v0 -> v0)[1]",
            "COALESCE($p0, 0e0)",
            "5e0",
            "($p0 / (1e0 - $p1))",
            "($p0",
            "",
        ] {
            assert!(!combine_is_linear(no), "{no}");
        }
    }
}
