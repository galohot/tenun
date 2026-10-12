//! Formulas: the language of calculated columns and measures.
//!
//! A formula is written the way Excel writes it, with `[Column]` and `[Table].[Column]` for
//! data. It goes through four steps, one file each:
//!
//! 1. `lex` cuts the text into tokens, each with its character offset.
//! 2. `parse` builds a syntax tree (a Pratt parser, written by hand).
//! 3. `check` resolves every name against the catalog and the relationships, infers a type for
//!    every part, and enforces the rules of the context. Its errors are plain English with the
//!    place of the culprit (`at` and `len`, in characters).
//! 4. `sql` writes a DuckDB expression. Column names are quoted identifiers that came from the
//!    catalog, never from the user. The only user text in the SQL is a number or an escaped
//!    string literal. `functions` holds the function list and what each one does.
//!
//! Failures give blank, never an error: dividing by zero, `VALUE("abc")`, a date that does not
//! exist. A query must never stop because one row holds odd data.
//!
//! # The three contexts
//!
//! - `Kind::Column` is row-wise over one table. The table is required. Aggregates are errors.
//!   `[Table].[Column]` reads one row of a related table, when the table is reached from this
//!   one on the many side of a relationship (many to one, any number of steps).
//! - `Kind::Measure` is a formula of aggregates. Every column sits inside one, so
//!   `SUM([Orders].[amount]) / SUM([Targets].[target])` works across tables. The table is
//!   optional: with it, `[amount]` means a column of that table.
//! - `Kind::Group` is a measure for a group (a new table). The table is required, and every
//!   aggregate reads that table, or tables related to it.
//!
//! # What the compiler hands to the next build files
//!
//! `compile` returns `Compiled { dtype, uses, body }`.
//!
//! **A column** (`Body::Column`) is one SQL expression plus the joins it needs. The current
//! table is the alias `t0`. A related table is `r1`, `r2`, … (each is a `LEFT JOIN` on the
//! many-to-one path, so the row count never changes). A view is
//! `SELECT t0.*, <sql> AS "name" FROM <table> AS t0 <joins>`. The `r` aliases never clash with
//! the `t` aliases of the query compiler.
//!
//! **A measure or a group value** (`Body::Aggregate`) is a list of **parts** and an expression
//! that combines them. Each part is one aggregate, with the table whose rows it reads
//! (`home_table`, a table id), as the alias `t0`, and the joins it needs to reach related
//! tables (`r1`…). The combining expression holds the placeholders `$p0`, `$p1`… for the
//! parts, in order. The caller runs each part on its own, grouped by the tile's groups, then
//! joins the results on the group values and fills the placeholders (see
//! `AggregateSql::fill`). The combining expression must read the results as columns of an
//! outer query. DuckDB does not allow `TRY` (which keeps failures blank) around an aggregate.
//! This is V4-D4: every table is totalled on its own, so a number on the one side of a
//! relationship is never counted once per row on the many side.
//! `AggregateSql::grand_total` gives the one-cell answer with no groups.

pub mod check;
pub mod functions;
pub mod lex;
pub mod parse;
pub mod rename;
pub mod sql;
pub mod sqlcheck;

pub use rename::{rename_column, rename_column_qualified, rename_link};

#[cfg(test)]
mod tests;

use crate::catalog::{Catalog, DType};
use crate::links::{Health, Links};
use crate::workspace::Relationship;
use chrono::{NaiveDate, NaiveDateTime};
use duckdb::Connection;
use serde::{Deserialize, Serialize};

/// A problem in a formula, in plain words, with the place to underline.
///
/// `at` and `len` count Unicode code points (what Rust calls `char`) from the start of the
/// formula, everywhere inside Rust. `POST /api/formula/check` converts them once, at the edge,
/// to UTF-16 code units with [`FormulaError::in_utf16`], so JavaScript's `slice` underlines the
/// right characters (V4-D23). A `len` of 0 means a place, not a range: the formula stops here,
/// or there is nothing to underline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct FormulaError {
    pub message: String,
    pub at: usize,
    pub len: usize,
}

impl FormulaError {
    pub fn new(message: impl Into<String>, at: usize, len: usize) -> Self {
        FormulaError {
            message: message.into(),
            at,
            len,
        }
    }

    /// The same error with `at` and `len` counted in UTF-16 code units of `formula`. A
    /// character outside the Basic Multilingual Plane, such as an emoji, takes two. A place
    /// past the end of the text counts one unit for each missing character.
    pub fn in_utf16(&self, formula: &str) -> FormulaError {
        let chars: Vec<char> = formula.chars().collect();
        let unit = |i: usize| chars.get(i).map_or(1, |c| c.len_utf16());
        FormulaError {
            message: self.message.clone(),
            at: (0..self.at).map(unit).sum(),
            len: (self.at..self.at + self.len).map(unit).sum(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Column,
    Measure,
    Group,
}

/// What a formula is read against: the tables, how they relate, and the clock.
pub struct Context<'a> {
    pub catalog: &'a Catalog,
    pub rels: &'a [Relationship],
    /// What the data says about each relationship (V4-D26). It is required: a join into a link
    /// with no record, or into a live database table, reads one row per key (V4-D51), and a
    /// context with no health could lose that without a sign. Use the engine's own, or an empty
    /// map to read every link as unknown.
    pub health: &'a Health,
    /// What `TODAY()` is. A date literal goes into the SQL, so a view is stable while it is used.
    pub today: NaiveDate,
    /// What `NOW()` is.
    pub now: NaiveDateTime,
    /// The formula becomes part of a view, a calculated column (V4-D63): every join into another
    /// table reads one row per key, whatever the health says, because the SQL of a view cannot
    /// follow a later change of the data.
    pub view: bool,
}

impl<'a> Context<'a> {
    /// A context that reads the computer's clock.
    pub fn new(catalog: &'a Catalog, rels: &'a [Relationship], health: &'a Health) -> Self {
        let now = chrono::Local::now().naive_local();
        Context {
            catalog,
            rels,
            health,
            today: now.date(),
            now,
            view: false,
        }
    }

    /// The relationships with what the data says about them.
    pub fn links(&self) -> Links<'a> {
        let links = Links::new(self.rels, Some(self.health));
        if self.view {
            links.one_row_per_key()
        } else {
            links
        }
    }
}

/// A formula, checked and written as SQL.
#[derive(Debug, Clone)]
pub struct Compiled {
    pub dtype: DType,
    /// The ids of the tables the formula reads, in the order it first reads them.
    pub uses: Vec<String>,
    pub body: Body,
}

#[derive(Debug, Clone)]
pub enum Body {
    Column(ColumnSql),
    Aggregate(AggregateSql),
}

/// A row-wise expression over the alias `t0` (the table of the formula) and the joins to the
/// related tables it reads.
#[derive(Debug, Clone)]
pub struct ColumnSql {
    pub sql: String,
    pub joins: Vec<Join>,
}

/// One `LEFT JOIN` along a many-to-one relationship.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Join {
    /// The id of the table that is joined.
    pub table: String,
    /// Its fully quoted DuckDB reference.
    pub sql_ref: String,
    pub alias: String,
    /// The condition after `ON`, with the aliases of both sides.
    pub on: String,
    /// The id of the relationship this join crosses.
    pub rel: String,
    /// The data of the link has something to tell the reader of a result that crosses it (a key
    /// repeats on the side joined, V4-D26). The measures in 03 collect these for the tile note.
    pub warn: bool,
}

impl Join {
    /// ` LEFT JOIN <table> AS <alias> ON <condition>`, with a leading space.
    pub fn to_sql(&self) -> String {
        format!(
            " LEFT JOIN {} AS {} ON {}",
            self.sql_ref, self.alias, self.on
        )
    }
}

/// The aggregates of a measure and the expression that combines them.
#[derive(Debug, Clone)]
pub struct AggregateSql {
    pub parts: Vec<Part>,
    /// SQL over the placeholders `$p0`, `$p1`… (one for each part). Row-wise functions,
    /// operators and literals are in it; aggregates are not.
    pub combine: String,
}

/// One aggregate, such as `sum(t0."amount")`, over the rows of one table.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// The id of the table whose rows are aggregated. It is the alias `t0` in `sql`.
    pub home_table: String,
    pub sql: String,
    /// The joins `sql` needs to reach related tables (`r1`…).
    pub joins: Vec<Join>,
    pub dtype: DType,
    /// A sum or a count: over no rows it is 0, not blank (V4-D22). `fill` and `grand_total`
    /// wrap such a part as `COALESCE(part, 0)`; the join on group values in 03 does the same
    /// for a group that has no rows in this part's table.
    pub zero_when_empty: bool,
    /// The values of the groups add up to the value of the whole: a sum or a count of rows or of
    /// filled cells. An average, a middle value, a distinct count, a least or a most do not. A
    /// share of the whole, a running total and a pile of bars are only right for these.
    pub additive: bool,
}

impl AggregateSql {
    /// The combining expression with each placeholder replaced by `values[n]`. A part that is a
    /// sum or a count gives 0 where `values[n]` is blank.
    pub fn fill(&self, values: &[String]) -> String {
        let wrapped: Vec<String> = values
            .iter()
            .enumerate()
            .map(|(i, v)| match self.parts.get(i) {
                Some(p) if p.zero_when_empty => format!("COALESCE({v}, 0e0)"),
                _ => v.clone(),
            })
            .collect();
        fill(&self.combine, &wrapped)
    }

    /// A whole SELECT for the answer with no groups and no filters: one cell, shown as `ty`
    /// shows it. Each part is a scalar subquery over its own table, so nothing is counted
    /// twice, and the combining expression reads the results as plain columns.
    pub fn grand_total(&self, catalog: &Catalog, ty: check::Ty) -> Option<String> {
        let mut parts = Vec::new();
        let mut names = Vec::new();
        for (i, p) in self.parts.iter().enumerate() {
            let home = catalog.table(&p.home_table)?;
            let joins: String = p.joins.iter().map(Join::to_sql).collect();
            parts.push(format!(
                "(SELECT {} FROM {} AS t0{joins}) AS p{i}",
                p.sql, home.sql_ref
            ));
            names.push(format!("g.p{i}"));
        }
        Some(format!(
            "SELECT {} FROM (SELECT {}) AS g",
            sql::display(&self.fill(&names), ty),
            parts.join(", ")
        ))
    }
}

/// Replaces `$p0`, `$p1`… in `text`. A number is read to its last digit, so `$p1` never cuts
/// into `$p10`. Text between quotes is left alone, so a user's `"$p0"` stays what they wrote.
pub fn fill(text: &str, values: &[String]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match quote {
            // A doubled quote closes the text and opens it again, which keeps it inside.
            Some(q) => {
                out.push(c);
                if c == q {
                    quote = None;
                }
                i += 1;
            }
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                out.push(c);
                i += 1;
            }
            None if c == '$' && chars.get(i + 1) == Some(&'p') => {
                let mut j = i + 2;
                while chars.get(j).is_some_and(char::is_ascii_digit) {
                    j += 1;
                }
                let digits: String = chars[i + 2..j].iter().collect();
                match digits.parse::<usize>().ok().and_then(|n| values.get(n)) {
                    Some(v) => out.push_str(v),
                    None => out.extend(&chars[i..j]),
                }
                i = j;
            }
            None => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Checks a formula and writes it as SQL. `table` is a table label or id.
pub fn compile(
    src: &str,
    kind: Kind,
    table: Option<&str>,
    cx: &Context<'_>,
) -> Result<Compiled, FormulaError> {
    let typed = check::check(src, kind, table, cx)?;
    sql::compile(&typed, cx)
}

// ---- POST /api/formula/check ---------------------------------------------------------

/// The body of `POST /api/formula/check`.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckRequest {
    pub formula: String,
    pub kind: Kind,
    /// A table id (or label). Required for a column and a group.
    #[serde(default)]
    pub table: Option<String>,
}

/// The answer. It holds a type, a preview and the tables read, or a plain error. It never
/// holds SQL.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CheckReply {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dtype: Option<DType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<FormulaError>,
    /// A column: the first 5 values of the table. A measure: the grand total, one cell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses: Option<Vec<String>>,
}

impl CheckReply {
    pub(crate) fn failed(e: FormulaError) -> Self {
        CheckReply {
            ok: false,
            dtype: None,
            error: Some(e),
            preview: None,
            uses: None,
        }
    }
}

/// Answers a check request. The formula is compiled, and run on the database for a preview.
/// Every error leaves here with its place in UTF-16 units (V4-D23). A formula that is fine
/// but fails on the data says so without showing the database's own message, which would
/// hold SQL: a problem with the kinds of the columns is underlined over the formula, a
/// problem with the source (offline, missing) has no place to underline (`len` 0).
pub fn answer(req: &CheckRequest, cx: &Context<'_>, db: &Connection) -> CheckReply {
    let fail = |e: FormulaError| CheckReply::failed(e.in_utf16(&req.formula));
    let len = req.formula.chars().count();
    let compiled = match compile(&req.formula, req.kind, req.table.as_deref(), cx) {
        Ok(c) => c,
        Err(e) => return fail(e),
    };
    let select = match preview_sql(&compiled, req.table.as_deref(), cx) {
        Some(s) => s,
        None => {
            return fail(FormulaError::new(
                "The table of this formula is not loaded.",
                0,
                0,
            ));
        }
    };
    match crate::engine::run(db, &select, &[]) {
        Ok(rows) => CheckReply {
            ok: true,
            dtype: Some(compiled.dtype),
            error: None,
            preview: Some(
                rows.into_iter()
                    .filter_map(|r| r.into_iter().next())
                    .collect(),
            ),
            uses: Some(compiled.uses),
        },
        Err(e) => {
            tracing::warn!("formula preview failed: {e:#}");
            fail(if source_problem(&format!("{e:#}")) {
                FormulaError::new(
                    "The data of this table could not be read. Check that its source is there, then try again.",
                    0,
                    0,
                )
            } else {
                FormulaError::new(
                    "The data could not be read with this formula. Check the kinds of the columns it uses.",
                    0,
                    len,
                )
            })
        }
    }
}

/// Answers a check request for a column written as SQL (Advanced, V4-D62): the rules of
/// `sqlcheck`, then the type and the first 5 values. Like [`answer`], it never shows the
/// database's own message about the data.
pub fn answer_sql(req: &CheckRequest, cx: &Context<'_>, db: &Connection) -> CheckReply {
    let fail = |e: FormulaError| CheckReply::failed(e.in_utf16(&req.formula));
    let len = req.formula.chars().count();
    if req.kind != Kind::Column {
        return fail(FormulaError::new("SQL works in a column only.", 0, 0));
    }
    let Some(t) = req
        .table
        .as_deref()
        .and_then(|t| check::find_table(cx.catalog, t))
    else {
        return fail(FormulaError::new(
            "Pick the table this formula belongs to first.",
            0,
            0,
        ));
    };
    let columns: Vec<String> = t.columns.iter().map(|c| c.name.clone()).collect();
    let source = format!("SELECT * FROM {}", t.sql_ref);
    if let Err(e) = sqlcheck::check(db, &req.formula, &columns, &t.label, &source) {
        return fail(e);
    }
    let wrapped = sqlcheck::wrap(&req.formula);
    let dtype = match sqlcheck::describe(
        db,
        &format!("SELECT {wrapped} AS v FROM ({source}) AS t0"),
    ) {
        Ok(cols) => cols
            .first()
            .map_or(DType::Text, |c| DType::from_duckdb(&c.1)),
        Err(_) => {
            return fail(FormulaError::new(
                "The data could not be read with this formula. Check the kinds of the columns it uses.",
                0,
                len,
            ));
        }
    };
    let shown = match dtype {
        DType::Number => format!("CAST({wrapped} AS DOUBLE)"),
        DType::Text => format!("CAST({wrapped} AS VARCHAR)"),
        other => sql::display(&wrapped, check::Ty::from_dtype(other)),
    };
    match crate::engine::run(
        db,
        &format!("SELECT {shown} FROM ({source}) AS t0 LIMIT 5"),
        &[],
    ) {
        Ok(rows) => CheckReply {
            ok: true,
            dtype: Some(dtype),
            error: None,
            preview: Some(
                rows.into_iter()
                    .filter_map(|r| r.into_iter().next())
                    .collect(),
            ),
            uses: Some(vec![t.id.clone()]),
        },
        Err(e) => {
            tracing::warn!("sql preview failed: {e:#}");
            fail(if source_problem(&format!("{e:#}")) {
                FormulaError::new(
                    "The data of this table could not be read. Check that its source is there, then try again.",
                    0,
                    0,
                )
            } else {
                FormulaError::new(
                    "The data could not be read with this formula. Check the kinds of the columns it uses.",
                    0,
                    len,
                )
            })
        }
    }
}

/// True when DuckDB's complaint is about where the data lives, not about what it holds.
fn source_problem(message: &str) -> bool {
    [
        "Catalog Error",
        "IO Error",
        "Connection Error",
        "HTTP Error",
        "does not exist",
    ]
    .iter()
    .any(|w| message.contains(w))
}

/// The SELECT for the preview: 5 rows of a column, or the grand total of a measure.
fn preview_sql(c: &Compiled, table: Option<&str>, cx: &Context<'_>) -> Option<String> {
    let ty = check::Ty::from_dtype(c.dtype);
    match &c.body {
        Body::Column(col) => {
            let t = check::find_table(cx.catalog, table?)?;
            let joins: String = col.joins.iter().map(Join::to_sql).collect();
            Some(format!(
                "SELECT {} FROM {} AS t0{joins} LIMIT 5",
                sql::display(&col.sql, ty),
                t.sql_ref
            ))
        }
        Body::Aggregate(agg) => agg.grand_total(cx.catalog, ty),
    }
}
