//! Calculated columns (V4-D3): formulas that become real columns.
//!
//! A table that has calculated columns has a second view on top of its own, in the schema of its
//! source and named `__calc_<table>`. The view is a stack of SELECTs, one for each column, so a
//! column may use the ones above it and nothing else:
//!
//! ```sql
//! SELECT t0.*, <the second column> AS "b" FROM (
//!   SELECT t0.*, <the first column> AS "a" FROM (SELECT * FROM <the table>) AS t0 <joins>
//! ) AS t0 <joins>
//! ```
//!
//! The catalog entry of the table points at that view (`sql_ref`) and lists the new columns, so
//! a chart, a filter, a join, the profile and an export read them like any column.
//!
//! - **Related columns** (`[stores].[region]`) are `LEFT JOIN`s to the related table's own view,
//!   and they read **one row per key, always** (V4-D63): the SQL of a view is fixed when it is
//!   made, so it must be safe for any data tomorrow. A view never has more rows than its table.
//! - **Order.** A table whose column reads another table's calculated column is built after it.
//!   Two tables that read each other are refused, with words that name both columns.
//! - **A column that fails** (a name that is gone, a type that no longer fits, a link that was
//!   removed) is a blank column and a sentence on the table. It never fails a load.
//! - **SQL** columns (Advanced) go through the check of V4-D62 and run inside `TRY`.

use crate::catalog::{Catalog, Column, DType, Table, ident};
use crate::formula::check::{Found, find_column, find_table};
use crate::formula::lex::{Tok, lex};
use crate::formula::{self, Body, Context, FormulaError, Join, Kind, sqlcheck};
use crate::links::{Health, Links, Paths};
use crate::workspace::{CalcColumn, ColumnSetup, Relationship, TableSetup};
use chrono::{NaiveDate, NaiveDateTime};
use duckdb::Connection;
use std::collections::{BTreeMap, BTreeSet};

/// The most letters a calculated column's name may have.
pub const MAX_NAME: usize = 100;

/// The most calculated columns one table may have. Each is a layer of the view.
pub const MAX_COLUMNS: usize = 50;

/// A note on a table about a calculated column that could not be made at all, because its name
/// cannot be a column of the view (empty, or the name of a column the view has). The notes of this
/// kind are made again with every pass.
pub const NOT_MADE: &str = "A calculated column was not made: ";

/// What the passes remember between one and the next, and across a reload. A reload gives the
/// catalog fresh tables with no calculated columns, so what a column was cannot be read from it.
#[derive(Debug, Clone, Default)]
pub struct Memory {
    /// Table id → column name → the type the column had the last time it was computed. A column
    /// that cannot be computed now is blank with this type, so a chart that used it still draws
    /// (V4-D67).
    pub types: BTreeMap<String, BTreeMap<String, DType>>,
    /// Table id → the tables its view read at the last pass. A table whose source fails or goes
    /// away is no longer in the catalog, but the tables that read it are still found by this.
    pub reads: BTreeMap<String, BTreeSet<String>>,
}

/// What a pass reads.
pub struct World<'a> {
    pub catalog: &'a Catalog,
    pub rels: &'a [Relationship],
    pub health: &'a Health,
    /// The setup of every table, by table id. A table with no entry has no calculated column.
    pub setups: &'a BTreeMap<String, TableSetup>,
    /// Table id → the reference of the table under its calculated-column view. A table has an
    /// entry only while its catalog entry points at such a view.
    pub bases: &'a BTreeMap<String, String>,
    pub memory: &'a Memory,
    pub today: NaiveDate,
    pub now: NaiveDateTime,
}

/// Which tables a pass rebuilds. The tables that read them are rebuilt too.
pub enum Roots<'a> {
    /// Every table with calculated columns.
    All,
    /// These tables.
    Tables(&'a [String]),
    /// The tables of one source, which has just loaded.
    Source(&'a str),
}

/// One table after a pass.
pub struct Built {
    /// The catalog entry as it is now: its columns and the view it points at.
    pub table: Table,
    /// The reference of the table under its view; `None` when it has no calculated column.
    pub base: Option<String>,
    /// One for each calculated column, in order.
    pub columns: Vec<Made>,
    /// The ids of the tables the view reads: those the formulas name, and the tables on the way.
    pub reads: BTreeSet<String>,
}

/// One calculated column after a pass.
pub struct Made {
    pub name: String,
    pub error: Option<FormulaError>,
    /// It reads another table, through a link, or a column that does. Such a column cannot be the
    /// key of a link (V4-D63).
    pub related: bool,
    /// The formula uses `TODAY()` or `NOW()`, or reads a column that does.
    pub uses_time: bool,
    /// The type of the column, when it was computed. Not set for a column that could not be.
    pub dtype: Option<DType>,
}

pub struct Output {
    pub built: Vec<Built>,
    /// The tables that had a formula with a related column. A link that was unknown during the
    /// pass may be known now, so the engine builds these again after the links are checked.
    pub related_tables: Vec<String>,
}

pub fn schema_of(source: &str) -> String {
    format!("s_{source}")
}

fn view_of(t: &Table) -> (String, String) {
    let schema = schema_of(&t.source_id);
    let name = format!("__calc_{}", t.name);
    (schema, name)
}

fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    a == b || a.to_lowercase() == b.to_lowercase()
}

fn duck_type(d: DType) -> &'static str {
    match d {
        DType::Number => "DOUBLE",
        DType::Date => "DATE",
        DType::DateTime => "TIMESTAMP",
        DType::Bool => "BOOLEAN",
        DType::Text => "VARCHAR",
    }
}

/// Does the formula use `TODAY()` or `NOW()`? (V4-D34)
pub fn uses_time(formula: &str) -> bool {
    lex(formula).iter().any(|t| match &t.tok {
        Tok::Name(n) => n.eq_ignore_ascii_case("TODAY") || n.eq_ignore_ascii_case("NOW"),
        _ => false,
    })
}

// ---- who reads whom ------------------------------------------------------------------------

/// One table a calculated column reads: the index of the column, and the table.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Edge {
    column: usize,
    on: String,
}

/// The tables each calculated column of `t` reads, from the names in its formula. This is only
/// for the order of the work (the compiler does the real check): a `[Table].[Column]` names a
/// table or a link, and the tables on the way to it are read too.
fn edges_of(w: &World<'_>, t: &Table, calcs: &[CalcColumn]) -> BTreeSet<Edge> {
    let links = Links::new(w.rels, Some(w.health)).one_row_per_key();
    let paths = Paths::new(&t.id, links);
    let mut out = BTreeSet::new();
    for (k, c) in calcs.iter().enumerate() {
        if c.sql {
            continue;
        }
        for tk in lex(&c.formula) {
            let Tok::Ref { table: Some(q), .. } = &tk.tok else {
                continue;
            };
            let (target, via) = match find_table(w.catalog, q) {
                Some(x) => (x.id.clone(), None),
                None => match w
                    .rels
                    .iter()
                    .find(|r| r.name.as_deref().is_some_and(|n| same(n, q)))
                {
                    Some(r) => (r.to.table.clone(), Some(r.id.clone())),
                    None => continue,
                },
            };
            if target == t.id {
                continue;
            }
            match paths.path(&target, via.as_deref()) {
                Ok(edges) => {
                    for e in edges {
                        out.insert(Edge {
                            column: k,
                            on: e.to_table,
                        });
                    }
                }
                Err(_) => {
                    out.insert(Edge {
                        column: k,
                        on: target,
                    });
                }
            }
        }
    }
    out
}

/// Tables that are in a cycle with each other, as groups (Tarjan). Only groups of two or more.
fn cycles(nodes: &[String], next: &BTreeMap<String, BTreeSet<String>>) -> Vec<BTreeSet<String>> {
    struct S<'a> {
        next: &'a BTreeMap<String, BTreeSet<String>>,
        index: BTreeMap<String, usize>,
        low: BTreeMap<String, usize>,
        stack: Vec<String>,
        on: BTreeSet<String>,
        n: usize,
        out: Vec<BTreeSet<String>>,
    }
    fn visit(s: &mut S<'_>, v: &str) {
        s.index.insert(v.to_string(), s.n);
        s.low.insert(v.to_string(), s.n);
        s.n += 1;
        s.stack.push(v.to_string());
        s.on.insert(v.to_string());
        let outs: Vec<String> = s
            .next
            .get(v)
            .map(|x| x.iter().cloned().collect())
            .unwrap_or_default();
        for w in outs {
            if !s.index.contains_key(&w) {
                visit(s, &w);
                let low = s.low[v].min(s.low[&w]);
                s.low.insert(v.to_string(), low);
            } else if s.on.contains(&w) {
                let low = s.low[v].min(s.index[&w]);
                s.low.insert(v.to_string(), low);
            }
        }
        if s.low[v] == s.index[v] {
            let mut group = BTreeSet::new();
            while let Some(x) = s.stack.pop() {
                s.on.remove(&x);
                group.insert(x.clone());
                if x == v {
                    break;
                }
            }
            if group.len() > 1 {
                s.out.push(group);
            }
        }
    }
    let mut s = S {
        next,
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        stack: Vec::new(),
        on: BTreeSet::new(),
        n: 0,
        out: Vec::new(),
    };
    for v in nodes {
        if !s.index.contains_key(v) {
            visit(&mut s, v);
        }
    }
    s.out
}

// ---- the pass ------------------------------------------------------------------------------

/// Builds the calculated-column views of the tables `roots` names and of the tables that read
/// them, in the order they need. With `commit` false nothing is written to DuckDB: the columns
/// are worked out and typed, and the answer says which ones fail (the check before a save).
/// With `dependents` false the tables that read the roots are left alone.
pub fn rebuild(
    db: &Connection,
    w: &World<'_>,
    roots: &Roots<'_>,
    commit: bool,
    dependents: bool,
) -> Output {
    let calcs_of = |id: &str| -> &[CalcColumn] { w.setups.get(id).map_or(&[], |s| &s.calc) };
    let with_calc: Vec<&Table> = w
        .catalog
        .tables
        .iter()
        .filter(|t| !calcs_of(&t.id).is_empty())
        .collect();
    let restore: Vec<&Table> = w
        .catalog
        .tables
        .iter()
        .filter(|t| calcs_of(&t.id).is_empty() && w.bases.contains_key(&t.id))
        .collect();

    // Who reads whom, for every table that has calculated columns.
    let edges: BTreeMap<String, BTreeSet<Edge>> = with_calc
        .iter()
        .map(|t| (t.id.clone(), edges_of(w, t, calcs_of(&t.id))))
        .collect();
    let has_calc = |id: &str| edges.contains_key(id);

    // The tables that changed: every table of a source that loaded, even one with no calculated
    // column, because a column of another table may read it. A source that failed to load or went
    // away has no tables in the catalog; its tables are known from what the views read before.
    let changed: BTreeSet<String> = match roots {
        Roots::All => with_calc
            .iter()
            .chain(restore.iter())
            .map(|t| t.id.clone())
            .collect(),
        Roots::Tables(ids) => ids.iter().cloned().collect(),
        Roots::Source(src) => {
            let prefix = format!("{src}/");
            w.catalog
                .tables
                .iter()
                .map(|t| t.id.clone())
                .chain(w.memory.reads.values().flatten().cloned())
                .chain(w.setups.keys().cloned())
                .filter(|id| id.starts_with(&prefix))
                .collect()
        }
    };
    // The tables to build: the changed ones that have columns (or had them), and the tables that
    // read a changed table, and so on.
    let mut affected: BTreeSet<String> = with_calc
        .iter()
        .chain(restore.iter())
        .filter(|t| changed.contains(&t.id))
        .map(|t| t.id.clone())
        .collect();
    if dependents {
        let reads_affected = |id: &str, affected: &BTreeSet<String>| {
            edges.get(id).is_some_and(|es| {
                es.iter()
                    .any(|e| changed.contains(&e.on) || affected.contains(&e.on))
            }) || w.memory.reads.get(id).is_some_and(|r| {
                r.iter()
                    .any(|x| changed.contains(x) || affected.contains(x))
            })
        };
        loop {
            let more: Vec<String> = with_calc
                .iter()
                .map(|t| t.id.clone())
                .filter(|id| !affected.contains(id) && reads_affected(id, &affected))
                .collect();
            if more.is_empty() {
                break;
            }
            affected.extend(more);
        }
    }
    if affected.is_empty() {
        return Output {
            built: Vec::new(),
            related_tables: Vec::new(),
        };
    }

    // Cycles: two tables whose columns read each other. The columns of the tables being built
    // that lie on a cycle are blank, with words (V4-D63).
    let graph: BTreeMap<String, BTreeSet<String>> = edges
        .iter()
        .map(|(id, es)| {
            (
                id.clone(),
                es.iter()
                    .filter(|e| has_calc(&e.on))
                    .map(|e| e.on.clone())
                    .collect(),
            )
        })
        .collect();
    let nodes: Vec<String> = with_calc.iter().map(|t| t.id.clone()).collect();
    let mut cyclic: BTreeMap<(String, usize), String> = BTreeMap::new();
    for group in cycles(&nodes, &graph) {
        for id in group.iter().filter(|id| affected.contains(*id)) {
            for e in &edges[id] {
                if group.contains(&e.on) {
                    let words = cycle_words(w, &edges, &group, id, e);
                    cyclic.insert((id.clone(), e.column), words);
                }
            }
        }
    }

    // The order: a table after the tables of its columns that are built in this pass.
    let mut order: Vec<String> = Vec::new();
    let mut left: Vec<String> = w
        .catalog
        .tables
        .iter()
        .filter(|t| affected.contains(&t.id))
        .map(|t| t.id.clone())
        .collect();
    while !left.is_empty() {
        let ready: Vec<String> = left
            .iter()
            .filter(|id| {
                edges.get(*id).is_none_or(|es| {
                    es.iter().all(|e| {
                        !left.contains(&e.on)
                            || e.on == **id
                            || cyclic.contains_key(&((*id).clone(), e.column))
                    })
                })
            })
            .cloned()
            .collect();
        // A cycle that is not flagged cannot happen; if it does, build what is left as it is.
        let take = if ready.is_empty() {
            left.clone()
        } else {
            ready
        };
        left.retain(|id| !take.contains(id));
        order.extend(take);
    }

    // The working catalog: the tables being built are back to their own columns.
    let mut wc = w.catalog.clone();
    for t in wc.tables.iter_mut().filter(|t| affected.contains(&t.id)) {
        if let Some(base) = w.bases.get(&t.id) {
            t.sql_ref = base.clone();
        }
        t.columns.retain(|c| !c.calc);
    }

    let mut built = Vec::new();
    let mut related_tables = Vec::new();
    for id in &order {
        let Some(t) = wc.table(id).cloned() else {
            continue;
        };
        let calcs = calcs_of(id);
        let b = if calcs.is_empty() {
            restore_table(db, &t, commit)
        } else {
            if edges.get(id).is_some_and(|e| !e.is_empty()) {
                related_tables.push(id.clone());
            }
            build_table(db, w, &wc, &t, calcs, &cyclic, commit)
        };
        if let Some(slot) = wc.tables.iter_mut().find(|x| x.id == *id) {
            *slot = b.table.clone();
        }
        built.push(b);
    }
    Output {
        built,
        related_tables,
    }
}

/// "Orders › Region reads Stores, and Stores › Zone reads Orders. Two tables cannot read each other…"
/// for a column on a cycle (V4-D66).
fn cycle_words(
    w: &World<'_>,
    edges: &BTreeMap<String, BTreeSet<Edge>>,
    group: &BTreeSet<String>,
    from: &str,
    first: &Edge,
) -> String {
    let label = |id: &str| {
        w.catalog
            .table(id)
            .map_or_else(|| id.to_string(), |t| t.label.clone())
    };
    let col_name = |id: &str, k: usize| {
        w.setups
            .get(id)
            .and_then(|s| s.calc.get(k))
            .map_or_else(String::new, |c| c.name.clone())
    };
    // The shortest way back from the table that is read to this one, inside the group.
    let mut path: Vec<(String, Edge)> = vec![(from.to_string(), first.clone())];
    let mut queue: std::collections::VecDeque<Vec<(String, Edge)>> =
        std::collections::VecDeque::from([path.clone()]);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some(p) = queue.pop_front() {
        let last = p.last().expect("a path has a step").1.clone();
        if last.on == from {
            path = p;
            break;
        }
        if !seen.insert(last.on.clone()) {
            continue;
        }
        for e in edges
            .get(&last.on)
            .into_iter()
            .flatten()
            .filter(|e| group.contains(&e.on))
        {
            let mut q = p.clone();
            q.push((last.on.clone(), e.clone()));
            queue.push_back(q);
        }
    }
    let steps: Vec<String> = path
        .iter()
        .map(|(id, e)| {
            format!(
                "{} › {} reads {}",
                label(id),
                col_name(id, e.column),
                label(&e.on)
            )
        })
        .collect();
    let list = match steps.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{}, and {last}", rest.join(", ")),
        _ => steps.join(""),
    };
    if path.len() == 2 {
        format!("{list}. Two tables cannot read each other. Change or remove one of these columns.")
    } else {
        format!(
            "{list}. These tables read each other in a circle, and a table cannot read itself. Change or remove one of these columns."
        )
    }
}

/// The table without a calculated-column view: its own columns and its own reference.
fn restore_table(db: &Connection, t: &Table, commit: bool) -> Built {
    if commit {
        let (schema, view) = view_of(t);
        let _ = db.execute_batch(&format!(
            "DROP VIEW IF EXISTS {}.{};",
            ident(&schema),
            ident(&view)
        ));
    }
    let mut table = t.clone();
    table.notes.retain(|n| !n.starts_with(NOT_MADE));
    Built {
        table,
        base: None,
        columns: Vec::new(),
        reads: BTreeSet::new(),
    }
}

/// The type a column had the last time it was computed (V4-D67).
fn old_type(w: &World<'_>, table: &str, name: &str) -> Option<DType> {
    w.memory.types.get(table)?.get(name).copied()
}

/// Does the formula read a calculated column that is as of a day: one with `TODAY()` or `NOW()`,
/// or one that reads such a column? Then it is as of that day too (V4-D34). A raw SQL column reads
/// the columns of its own table, so it is judged by the names in its text.
fn reads_time_column(c: &CalcColumn, cat: &Catalog, own: &Table, rels: &[Relationship]) -> bool {
    let mine = cat.table(&own.id).unwrap_or(own);
    if c.sql {
        let text = c.formula.to_lowercase();
        return mine
            .columns
            .iter()
            .any(|x| x.as_of.is_some() && text.contains(&x.name.to_lowercase()));
    }
    lex(&c.formula).iter().any(|tk| {
        let Tok::Ref { table, column } = &tk.tok else {
            return false;
        };
        let t = match table {
            None => Some(mine),
            Some(q) => find_table(cat, q).or_else(|| {
                let r = rels
                    .iter()
                    .find(|r| r.name.as_deref().is_some_and(|n| same(n, q)))?;
                cat.table(&r.to.table)
            }),
        };
        t.is_some_and(|t| matches!(find_column(t, column), Found::One(x) if x.as_of.is_some()))
    })
}

/// What one calculated column came to before its layer is tried.
struct Tried {
    /// The SQL expression.
    expr: String,
    /// The `LEFT JOIN`s it needs, as text.
    joins: String,
    /// It reads another table, or a column that does.
    related: bool,
    /// The type the formula gives (a SQL column has none until DuckDB says).
    dtype: Option<DType>,
    /// The ids of the tables it reads.
    reads: Vec<String>,
}

fn clash(name: &str, known: &[String]) -> bool {
    known.iter().any(|k| same(k, name))
}

/// The calculated column `name` of a table: a reference to a column that comes after it, as
/// the place to underline, if the formula has one.
fn later_reference(
    formula: &str,
    table: &Table,
    known: &Table,
    later: &[&str],
) -> Option<(String, usize, usize)> {
    for tk in lex(formula) {
        let Tok::Ref { table: q, column } = &tk.tok else {
            continue;
        };
        let mine = match q {
            None => true,
            Some(q) => same(q, &table.label) || same(q, &table.id),
        };
        if mine
            && later.iter().any(|n| same(n, column))
            && matches!(find_column(known, column), Found::No)
        {
            return Some((column.clone(), tk.at, tk.len));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn build_table(
    db: &Connection,
    w: &World<'_>,
    wc: &Catalog,
    t: &Table,
    calcs: &[CalcColumn],
    cyclic: &BTreeMap<(String, usize), String>,
    commit: bool,
) -> Built {
    let base_ref = t.sql_ref.clone();
    let setup = w.setups.get(&t.id);
    let cols_setup =
        |name: &str| -> Option<&ColumnSetup> { setup.and_then(|s| s.columns.get(name)) };
    // The columns of the layer so far, with their DuckDB types.
    let mut layer = format!("SELECT * FROM {base_ref}");
    let mut known: Vec<(String, String)> = match sqlcheck::describe(db, &layer) {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!(
                "calculated columns: the table {} could not be read: {e}",
                t.label
            );
            let error = FormulaError::new(
                "The data of this table could not be read. Check that its source is there, then try again.",
                0,
                0,
            );
            return all_failed(t, calcs, error, w);
        }
    };
    // The table as the formulas see it: its own columns, then the calculated ones above.
    let mut own = wc.clone();
    let mut not_made: Vec<String> = Vec::new();
    let mut made: Vec<Made> = Vec::new();
    let mut related_names: BTreeSet<String> = BTreeSet::new();
    let mut shown_names: Vec<String> = t
        .columns
        .iter()
        .flat_map(|c| std::iter::once(c.name.clone()).chain(c.label.clone()))
        .collect();
    let mut reads: BTreeSet<String> = BTreeSet::new();
    // The limits of a file that is opened hold as much as those of a save (V4-D24, V4-D75): a column
    // past the 50th, or with more than 10,000 characters, stays in the file and is not built.
    if calcs.len() > MAX_COLUMNS {
        let extra = calcs.len() - MAX_COLUMNS;
        not_made.push(format!(
            "{NOT_MADE}{extra} calculated column{} after the first {MAX_COLUMNS} {} not built. The file keeps {}.",
            if extra == 1 { "" } else { "s" },
            if extra == 1 { "was" } else { "were" },
            if extra == 1 { "it" } else { "them" },
        ));
    }
    for (k, c) in calcs.iter().enumerate() {
        if k >= MAX_COLUMNS {
            made.push(Made {
                name: c.name.trim().to_string(),
                error: Some(FormulaError::new(
                    format!("A table builds up to {MAX_COLUMNS} calculated columns."),
                    0,
                    0,
                )),
                related: false,
                uses_time: false,
                dtype: None,
            });
            continue;
        }
        if c.formula.chars().count() > sqlcheck::MAX_CHARS {
            let error = FormulaError::new(
                format!(
                    "The formula has more than {} characters, so it was not built.",
                    sqlcheck::MAX_CHARS
                ),
                0,
                0,
            );
            not_made.push(format!("{NOT_MADE}{}: {}", c.name.trim(), error.message));
            made.push(Made {
                name: c.name.trim().to_string(),
                error: Some(error),
                related: false,
                uses_time: false,
                dtype: None,
            });
            continue;
        }
        let own_time = !c.sql && uses_time(&c.formula);
        let uses_time = own_time;
        // This column and the ones below it: a formula cannot read them.
        let later: Vec<&str> = calcs[k..].iter().map(|x| x.name.as_str()).collect();
        let table_now = own.table(&t.id).cloned().unwrap_or_else(|| t.clone());
        // One column: the expression, its joins, and whether it reads another table.
        let attempt: Result<Tried, FormulaError> = (|| {
            let name = c.name.trim();
            if name.is_empty() {
                return Err(FormulaError::new("Give the column a name.", 0, 0));
            }
            if name.chars().count() > MAX_NAME {
                return Err(FormulaError::new(
                    format!("Keep the name of a column to {MAX_NAME} letters or fewer."),
                    0,
                    0,
                ));
            }
            if name.contains('[') || name.contains(']') {
                return Err(FormulaError::new(
                    "Leave the square brackets [ ] out of the name. Formulas use them to write names.",
                    0,
                    0,
                ));
            }
            if name.starts_with("__") {
                return Err(FormulaError::new(
                    "A name cannot start with two underscores.",
                    0,
                    0,
                ));
            }
            if clash(name, &shown_names) {
                return Err(FormulaError::new(
                    format!(
                        "The table {} has a column called {name} already. Choose another name.",
                        t.label
                    ),
                    0,
                    0,
                ));
            }
            if let Some(words) = cyclic.get(&(t.id.clone(), k)) {
                return Err(FormulaError::new(words.clone(), 0, 0));
            }
            if c.sql {
                let columns: Vec<String> = known.iter().map(|(n, _)| n.clone()).collect();
                sqlcheck::check(db, &c.formula, &columns, &t.label, &layer)?;
                return Ok(Tried {
                    expr: sqlcheck::wrap(&c.formula),
                    joins: String::new(),
                    related: false,
                    dtype: None,
                    reads: Vec::new(),
                });
            }
            if let Some((col, at, len)) = later_reference(&c.formula, t, &table_now, &later) {
                let message = if same(&col, name) {
                    format!("[{col}] is this column. A column cannot use itself.")
                } else {
                    format!(
                        "[{col}] is a calculated column that comes after this one. A column can use the ones above it, not the ones below it."
                    )
                };
                return Err(FormulaError::new(message, at, len));
            }
            let cx = Context {
                catalog: &own,
                rels: w.rels,
                health: w.health,
                today: w.today,
                now: w.now,
                view: true,
                // A calculated column is built once, at load: `PARAM` is refused in it (v5 build 05).
                params: &[],
                setups: Some(w.setups),
            };
            let compiled = formula::compile(&c.formula, Kind::Column, Some(&t.id), &cx)?;
            let Body::Column(col) = compiled.body else {
                return Err(FormulaError::new("This is not a column formula.", 0, 0));
            };
            let joins: String = col.joins.iter().map(Join::to_sql).collect();
            // A column that reads an earlier column that reads another table does too.
            let reads_related = !col.joins.is_empty()
                || lex(&c.formula).iter().any(|tk| match &tk.tok {
                    Tok::Ref {
                        table: None,
                        column,
                    } => related_names.iter().any(|r| same(r, column)),
                    _ => false,
                });
            let mut read: Vec<String> = compiled.uses.clone();
            read.extend(col.joins.iter().map(|j| j.table.clone()));
            Ok(Tried {
                expr: col.sql,
                joins,
                related: reads_related,
                dtype: Some(compiled.dtype),
                reads: read,
            })
        })();

        let mut error: Option<FormulaError> = None;
        let mut related = false;
        let mut done = false;
        let mut compiled_type = None;
        if let Ok(Tried {
            expr,
            joins,
            related: reads_related,
            dtype,
            reads: read,
        }) = &attempt
        {
            related = *reads_related;
            compiled_type = *dtype;
            reads.extend(read.iter().filter(|r| **r != t.id).cloned());
            let try_layer = format!(
                "SELECT t0.*, {expr} AS {} FROM ({layer}) AS t0{joins}",
                ident(c.name.trim())
            );
            match sqlcheck::describe(db, &try_layer) {
                Ok(cols) => {
                    layer = try_layer;
                    known = cols;
                    done = true;
                }
                Err(e) => {
                    tracing::warn!("calculated column {} of {}: {e}", c.name, t.label);
                    error = Some(FormulaError::new(
                        "The data could not be read with this formula. Check the kinds of the columns it uses.",
                        0,
                        c.formula.chars().count(),
                    ));
                }
            }
        } else if let Err(e) = attempt {
            error = Some(e);
        }
        if !done
            && (c.name.trim().is_empty()
                || known
                    .iter()
                    .any(|(n, _)| n.to_lowercase() == c.name.trim().to_lowercase()))
        {
            // The name is a column of the view already (or empty): a second one would be a
            // duplicate. The column is left out, and the table says so.
            let error =
                error.unwrap_or_else(|| FormulaError::new("This column could not be made.", 0, 0));
            not_made.push(format!("{NOT_MADE}{}: {}", c.name.trim(), error.message));
            made.push(Made {
                name: c.name.clone(),
                error: Some(error),
                related: false,
                uses_time,
                dtype: None,
            });
            continue;
        }
        if !done {
            // A blank column of the kind it had before, else the kind the formula gives, else text.
            let ty = old_type(w, &t.id, c.name.trim())
                .or(compiled_type)
                .map_or("VARCHAR", duck_type);
            let try_layer = format!(
                "SELECT t0.*, {} FROM ({layer}) AS t0",
                sqlcheck::blank(ty, c.name.trim())
            );
            match sqlcheck::describe(db, &try_layer) {
                Ok(cols) => {
                    layer = try_layer;
                    known = cols;
                }
                Err(e) => {
                    // Only a name DuckDB cannot take gets here. Say so and leave the column out.
                    tracing::warn!("calculated column {} of {}: {e}", c.name, t.label);
                    let error = error.unwrap_or_else(|| {
                        FormulaError::new("This column could not be made.", 0, 0)
                    });
                    not_made.push(format!("{NOT_MADE}{}: {}", c.name.trim(), error.message));
                    made.push(Made {
                        name: c.name.clone(),
                        error: Some(error),
                        related: false,
                        uses_time,
                        dtype: None,
                    });
                    continue;
                }
            }
            related = false;
        }
        if related {
            related_names.insert(c.name.trim().to_string());
        }
        // The next formulas see this column, with the kind DuckDB gave it.
        let (name, raw) = known
            .iter()
            .find(|(n, _)| n == c.name.trim())
            .cloned()
            .unwrap_or_else(|| (c.name.trim().to_string(), "VARCHAR".into()));
        let col_type = DType::from_duckdb(&raw);
        let mut col = Column::new(name.clone(), col_type, raw);
        col.calc = true;
        let o = cols_setup(&name);
        col.label = o.and_then(|o| o.label.clone()).filter(|l| !l.is_empty());
        col.hidden = o.is_some_and(|o| o.hidden == Some(true));
        col.description = o
            .and_then(|o| o.description.clone())
            .or_else(|| c.description.clone())
            .filter(|d| !d.is_empty());
        col.error = error.as_ref().map(|e| e.message.clone());
        // Reading a column that is as of a day makes this one as of that day (V4-D34).
        let uses_time = error.is_none() && (own_time || reads_time_column(c, &own, t, w.rels));
        col.as_of = uses_time.then(|| w.today.to_string());
        shown_names.push(name.clone());
        if let Some(l) = &col.label {
            shown_names.push(l.clone());
        }
        if let Some(slot) = own.tables.iter_mut().find(|x| x.id == t.id) {
            slot.columns.push(col);
        }
        let dtype = error.is_none().then_some(col_type);
        made.push(Made {
            name,
            error,
            related,
            uses_time,
            dtype,
        });
    }

    // The columns are in `own`; the view is made from the last layer.
    let mut table = own.table(&t.id).cloned().unwrap_or_else(|| t.clone());
    table.notes.retain(|n| !n.starts_with(NOT_MADE));
    table.notes.extend(not_made);
    let mut base = None;
    if !made.is_empty() && commit {
        let (schema, view) = view_of(t);
        let create = format!(
            "CREATE SCHEMA IF NOT EXISTS {s}; CREATE OR REPLACE VIEW {s}.{v} AS {layer};",
            s = ident(&schema),
            v = ident(&view)
        );
        match db.execute_batch(&create) {
            Ok(()) => {
                table.sql_ref = format!("{}.{}", ident(&schema), ident(&view));
                base = Some(base_ref.clone());
                share_of_blanks(db, &mut table);
            }
            Err(e) => {
                tracing::warn!(
                    "calculated columns: the view of {} was not made: {e}",
                    t.label
                );
                let error = FormulaError::new("The columns of this table could not be made.", 0, 0);
                return all_failed(t, calcs, error, w);
            }
        }
    } else if !made.is_empty() {
        // A check only: the table keeps the reference it has.
        table.sql_ref = base_ref;
    }
    Built {
        table,
        base,
        columns: made,
        reads,
    }
}

/// Every calculated column of the table is blank, with the same reason.
fn all_failed(t: &Table, calcs: &[CalcColumn], error: FormulaError, w: &World<'_>) -> Built {
    let mut table = t.clone();
    table.notes.retain(|n| !n.starts_with(NOT_MADE));
    let mut made = Vec::new();
    for c in calcs {
        // A name the table has already is not added a second time.
        if table
            .columns
            .iter()
            .any(|x| x.name.to_lowercase() == c.name.trim().to_lowercase())
        {
            made.push(Made {
                name: c.name.trim().to_string(),
                error: Some(error.clone()),
                related: false,
                uses_time: false,
                dtype: None,
            });
            continue;
        }
        let dtype = old_type(w, &t.id, c.name.trim()).unwrap_or(DType::Text);
        let mut col = Column::new(c.name.trim().to_string(), dtype, duck_type(dtype).into());
        col.calc = true;
        col.error = Some(error.message.clone());
        col.description = c.description.clone().filter(|d| !d.is_empty());
        let o = w
            .setups
            .get(&t.id)
            .and_then(|s| s.columns.get(c.name.trim()));
        col.label = o.and_then(|o| o.label.clone()).filter(|l| !l.is_empty());
        col.hidden = o.is_some_and(|o| o.hidden == Some(true));
        table.columns.push(col);
        made.push(Made {
            name: c.name.trim().to_string(),
            error: Some(error.clone()),
            related: false,
            uses_time: false,
            dtype: None,
        });
    }
    Built {
        table,
        base: None,
        columns: made,
        reads: BTreeSet::new(),
    }
}

/// The share of blank cells of each calculated column, in one pass over the view. Only for local
/// data, like the share of the other columns.
fn share_of_blanks(db: &Connection, table: &mut Table) {
    let Some(rows) = table.rows.filter(|n| *n > 0) else {
        return;
    };
    let calc: Vec<usize> = table
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.calc && c.error.is_none())
        .map(|(i, _)| i)
        .collect();
    if calc.is_empty() {
        return;
    }
    let select = calc
        .iter()
        .map(|i| format!("count({})", ident(&table.columns[*i].name)))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("SELECT {select} FROM {}", table.sql_ref);
    let Ok(counts) = db.query_row(&sql, [], |r| {
        (0..calc.len())
            .map(|i| r.get::<_, i64>(i))
            .collect::<Result<Vec<_>, _>>()
    }) else {
        return;
    };
    for (i, filled) in calc.iter().zip(counts) {
        if filled < rows {
            table.columns[*i].blank = Some((rows - filled) as f64 / rows as f64);
        }
    }
}

// ---- the check before a save -----------------------------------------------------------------

/// What the check of a setup found out about one of its calculated columns.
pub struct Checked {
    pub index: usize,
    pub name: String,
    pub error: FormulaError,
}

/// The result of [`check_columns`].
pub struct CalcCheck {
    /// The columns that cannot be computed, with the reason.
    pub failing: Vec<Checked>,
    /// The names of the columns that read another table.
    pub related: BTreeSet<String>,
}

/// Works out the calculated columns of `table` against the workspace as it is (nothing is
/// written) and says which ones fail, with the reason. The caller keeps the ones that are new or
/// changed.
pub fn check_columns(db: &Connection, w: &World<'_>, table: &str) -> CalcCheck {
    let out = rebuild(db, w, &Roots::Tables(&[table.to_string()]), false, false);
    let mut check = CalcCheck {
        failing: Vec::new(),
        related: BTreeSet::new(),
    };
    for b in out.built.into_iter().filter(|b| b.table.id == table) {
        for (i, m) in b.columns.into_iter().enumerate() {
            if m.related {
                check.related.insert(m.name.clone());
            }
            if let Some(error) = m.error {
                check.failing.push(Checked {
                    index: i,
                    name: m.name,
                    error,
                });
            }
        }
    }
    check
}

/// A setup with a calculated column that cannot be saved: the column, the reason and the place
/// to underline (in characters of the formula). It is the error of [`Engine::set_table_setup`];
/// the API answers it as a normal reply, with the place in UTF-16 units (V4-D23, V4-D35).
///
/// [`Engine::set_table_setup`]: crate::engine::Engine::set_table_setup
#[derive(Debug, Clone)]
pub struct CalcRefused {
    pub index: usize,
    pub name: String,
    pub formula: String,
    pub error: FormulaError,
}

impl std::fmt::Display for CalcRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.error.message)
    }
}

impl std::error::Error for CalcRefused {}

impl CalcRefused {
    /// The reply of `PUT /api/tables/setup` for a column that cannot be saved.
    pub fn reply(&self) -> serde_json::Value {
        let e = self.error.in_utf16(&self.formula);
        serde_json::json!({
            "ok": false,
            "calc": self.index,
            "name": self.name,
            "error": { "message": e.message, "at": e.at, "len": e.len },
        })
    }
}

// ---- names that change -------------------------------------------------------------------------

/// The columns of `table` whose name in a formula changes when the setup goes from `old` to `new`:
/// a name shown that was given and is now another or gone, and the two columns an unpivot makes.
/// A column that had no name shown keeps its real name, which a formula still finds (V4-D30), so
/// nothing is rewritten for it.
pub fn renamed_columns(table: &Table, old: &TableSetup, new: &TableSetup) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for c in &table.columns {
        let was = old.columns.get(&c.name).and_then(|o| o.label.clone());
        let now = new.columns.get(&c.name).and_then(|o| o.label.clone());
        let Some(was) = was.filter(|l| !l.trim().is_empty()) else {
            continue;
        };
        let now = now
            .filter(|l| !l.trim().is_empty())
            .unwrap_or_else(|| c.name.clone());
        if !same(&was, &now) {
            out.push((was, now));
        }
    }
    out.extend(unpivot_renames(old, new));
    out
}

/// The two columns an unpivot makes, when the setup gives them other names.
pub fn unpivot_renames(old: &TableSetup, new: &TableSetup) -> Vec<(String, String)> {
    let unpivot = |s: &TableSetup| {
        s.transforms
            .iter()
            .map(|t| match t {
                crate::workspace::Transform::Unpivot { name, value, .. } => {
                    (name.clone(), value.clone())
                }
            })
            .next()
    };
    let mut out = Vec::new();
    if let (Some((on, ov)), Some((nn, nv))) = (unpivot(old), unpivot(new)) {
        if on != nn {
            out.push((on, nn));
        }
        if ov != nv {
            out.push((ov, nv));
        }
    }
    out
}

/// A raw SQL column reads real names, and an unpivot that gives its two columns other names
/// changes them. The SQL is not rewritten (it is not a formula), so the rename is refused, with
/// words that name the column (V4-D78).
pub fn unpivot_sql_problem(old: &TableSetup, new: &TableSetup) -> Option<String> {
    for (was, now) in unpivot_renames(old, new) {
        for c in new.calc.iter().filter(|c| c.sql) {
            if mentions(&c.formula, &was) {
                return Some(format!(
                    "Renaming {was} to {now} would break the SQL of the calculated column {}. Change the SQL first.",
                    c.name
                ));
            }
        }
    }
    None
}

/// Does the text write the name, between double quotes or as a word of its own?
fn mentions(text: &str, name: &str) -> bool {
    let (t, n) = (text.to_lowercase(), name.trim().to_lowercase());
    if n.is_empty() {
        return false;
    }
    let word = |c: char| c.is_alphanumeric() || c == '_';
    t.match_indices(&n).any(|(i, m)| {
        let before = t[..i].chars().next_back();
        let after = t[i + m.len()..].chars().next();
        let quoted = before == Some('"') && after == Some('"');
        quoted || (!before.is_some_and(word) && !after.is_some_and(word))
    })
}

/// A name shown that another column of the table answers to already: it would make `[name]` mean
/// two columns, and the formulas that use it could not tell which (V4-D38). Only a name that this
/// setup gives or changes is looked at, so an old file with a clash can still be saved. Names are
/// compared the way a formula finds a column: ignoring case and the spaces at the ends (V4-D77).
pub fn label_problem(table: &Table, old: &TableSetup, new: &TableSetup) -> Option<String> {
    // Every column the table will have: its own, and the calculated ones of the new setup.
    let mut cols: Vec<(String, Option<String>)> = table
        .columns
        .iter()
        .filter(|c| !c.calc)
        .map(|c| (c.name.clone(), None))
        .collect();
    cols.extend(new.calc.iter().map(|c| (c.name.trim().to_string(), None)));
    for col in &mut cols {
        col.1 = new
            .columns
            .get(&col.0)
            .and_then(|o| o.label.clone())
            .filter(|l| !l.trim().is_empty());
    }
    for (i, (real, label)) in cols.iter().enumerate() {
        let Some(label) = label else { continue };
        let was = old.columns.get(real).and_then(|o| o.label.as_deref());
        if was == Some(label.as_str()) {
            continue;
        }
        for (j, (other, other_label)) in cols.iter().enumerate() {
            if i != j
                && (same(label, other) || other_label.as_deref().is_some_and(|l| same(label, l)))
            {
                return Some(format!(
                    "The name {} is already used by another column of {}. Choose another name.",
                    label.trim(),
                    table.label
                ));
            }
        }
    }
    None
}

/// The place (`at` and `len`, in characters) of a problem found in `rewritten`, as it sits in
/// `sent`. The two texts have the same tokens; a rename changes how long some of them are. Used
/// so an error is underlined in the text the user wrote, not in the one the engine rewrote
/// (V4-D77).
pub fn map_back(sent: &str, rewritten: &str, at: usize, len: usize) -> (usize, usize) {
    if sent == rewritten {
        return (at, len);
    }
    let (a, b) = (lex(sent), lex(rewritten));
    if a.len() != b.len() {
        return (at, len);
    }
    let map = |pos: usize| -> usize {
        match b.iter().rposition(|t| t.at <= pos) {
            None => pos,
            Some(i) if pos < b[i].at + b[i].len => a[i].at + (pos - b[i].at).min(a[i].len),
            Some(i) => a[i].at + a[i].len + (pos - (b[i].at + b[i].len)),
        }
    };
    let (start, end) = (map(at), map(at + len));
    (start, end.saturating_sub(start))
}

/// The formula after the columns in `renames` of `table` got other names. For a formula of the
/// table itself (`own`), a bare `[old]` is that column too. For a formula of another table, a
/// column is written after the table, or after the name of a link that leads to the table
/// (`links`, V4-D28): `[orders].[old]`, `[Sold at].[old]`.
///
/// All the names change in one sweep (through a name nobody uses), so two columns can swap names
/// and a formula keeps its meaning (V4-D77).
pub fn follow_renames(
    formula: &str,
    table: &Table,
    renames: &[(String, String)],
    own: bool,
    links: &[String],
) -> String {
    let tmp = |i: usize| format!("\u{e000}col{i}");
    let mut qualifiers: Vec<&str> = vec![&table.label, &table.id];
    if !own {
        qualifiers.extend(links.iter().map(String::as_str));
    }
    let step = |text: String, from: &str, to: &str| {
        let mut text = text;
        for q in &qualifiers {
            text = if own {
                formula::rename_column(&text, q, from, to)
            } else {
                formula::rename_column_qualified(&text, q, from, to)
            };
        }
        text
    };
    let mut text = formula.to_string();
    for (i, (old, _)) in renames.iter().enumerate() {
        text = step(text, old, &tmp(i));
    }
    for (i, (_, new)) in renames.iter().enumerate() {
        text = step(text, &tmp(i), new);
    }
    text
}

/// The formula after links got other names. All the names change in one sweep, so two links can
/// swap names.
pub fn rename_links_in(formula: &str, renames: &[(String, String)]) -> String {
    let tmp = |i: usize| format!("\u{e000}link{i}");
    let mut text = formula.to_string();
    for (i, (old, _)) in renames.iter().enumerate() {
        text = formula::rename_link(&text, old, &tmp(i));
    }
    for (i, (_, new)) in renames.iter().enumerate() {
        text = formula::rename_link(&text, &tmp(i), new);
    }
    text
}
