//! New tables made from other tables (build 04, V4-D6): append, merge and group.
//!
//! A new table is a source of kind `derived` with one table, a DuckDB view over its input tables.
//! This module writes the SELECT of the view, the words that check a spec, the order in which
//! sources load (a new table after the tables it reads), and the counts behind a merge's notes.
//! The engine makes the view and reads it into the catalog like any other table, so a new table
//! takes part in relationships, calculated columns, measures, prep and export.

use crate::catalog::{Catalog, Table, ident, lit};
use crate::formula::{self, Body, Context, Kind};
use crate::links::{Health, join_on};
use crate::workspace::{DerivedSpec, Keep, Relationship, Source, SourceKind};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// The longest name a new table or one of its values may have.
pub const MAX_NAME: usize = 64;

/// The SELECT of a new table, and what the engine counts after the view is made.
#[derive(Debug, Clone)]
pub struct Plan {
    pub sql: String,
    /// A merge: the counts behind its notes.
    pub merge: Option<MergeCounts>,
}

/// Two SELECTs a merge runs once its view is made, for the words under the table (V4-D126).
#[derive(Debug, Clone)]
pub struct MergeCounts {
    /// `SELECT <rows of left>, <rows of left that found a match>`.
    pub matched: String,
    /// `SELECT <keys of right that are on more than one row>`.
    pub repeated: String,
    pub left: String,
    pub right: String,
    pub keep: Keep,
}

impl MergeCounts {
    /// The notes of a merge from its counts: how many rows found a match, and what a repeated key
    /// on the right did.
    pub fn notes(&self, rows: i64, matched: i64, repeated: i64) -> Vec<String> {
        let mut out = vec![format!(
            "{} of {} rows of {} found a match in {}.",
            thousands(matched),
            thousands(rows),
            self.left,
            self.right
        )];
        if self.keep == Keep::Inner && rows > matched {
            out[0].push_str(&match rows - matched {
                1 => " The other row is left out.".to_string(),
                n => format!(" The other {} are left out.", thousands(n)),
            });
        }
        if repeated > 0 {
            out.push(format!(
                "{} repeats {} {}. The first row of each is used, so no row of {} is counted twice.",
                self.right,
                thousands(repeated),
                if repeated == 1 { "key" } else { "keys" },
                self.left
            ));
        }
        out
    }
}

/// `9000` as `9,000`.
pub fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// What is wrong with the name of a new table, in words. `None` when it is fine.
pub fn name_problem(name: &str) -> Option<String> {
    let n = name.trim();
    if n.is_empty() {
        return Some("Give the new table a name.".into());
    }
    if n.chars().count() > MAX_NAME {
        return Some(format!(
            "A name has {MAX_NAME} characters at most. This one has {}.",
            n.chars().count()
        ));
    }
    if n.starts_with("__") {
        return Some("A name cannot start with two underscores.".into());
    }
    None
}

/// The table `id`, or the words that say it is gone.
fn input<'a>(catalog: &'a Catalog, id: &str) -> Result<&'a Table, String> {
    catalog.table(id).ok_or_else(|| {
        let name = id.split_once('/').map_or(id, |(_, n)| n);
        format!(
            "The table {name} is not there any more. Open the new table and pick its tables again."
        )
    })
}

fn column_of<'a>(t: &'a Table, c: &str) -> Result<&'a crate::catalog::Column, String> {
    t.column(c)
        .ok_or_else(|| format!("{} has no column {c}.", t.label))
}

/// The first of `wanted`, then `wanted 2`, `wanted 3`… that is not in `taken`.
fn free_name(wanted: &str, taken: &BTreeSet<String>) -> String {
    if !taken.contains(wanted) {
        return wanted.to_string();
    }
    (2..)
        .map(|i| format!("{wanted} {i}"))
        .find(|n| !taken.contains(n))
        .expect("an unused name")
}

/// The SELECT of a new table over the tables of `catalog` (V4-D6). The words of the error say what
/// to change. `rels` and `health` are for the formulas of a group, which may read related tables.
pub fn plan(
    spec: &DerivedSpec,
    catalog: &Catalog,
    rels: &[Relationship],
    health: &Health,
) -> Result<Plan, String> {
    match spec {
        DerivedSpec::Append { tables } => append(tables, catalog),
        DerivedSpec::Merge {
            left,
            right,
            on,
            keep,
        } => merge(left, right, on, *keep, catalog),
        DerivedSpec::Group { table, by, values } => group(table, by, values, catalog, rels, health),
    }
}

/// The name of the column that says which table a row of an append came from: `source`, unless a
/// table has a column of that name.
pub fn source_column(tables: &[&Table]) -> String {
    let taken: BTreeSet<String> = tables
        .iter()
        .flat_map(|t| t.columns.iter().map(|c| c.name.clone()))
        .collect();
    free_name("source", &taken)
}

fn append(tables: &[String], catalog: &Catalog) -> Result<Plan, String> {
    if tables.len() < 2 {
        return Err("Pick two or more tables to append.".into());
    }
    let mut seen = HashSet::new();
    let mut inputs = Vec::new();
    for id in tables {
        if !seen.insert(id.as_str()) {
            return Err(format!(
                "{} is picked twice. Pick each table once.",
                input(catalog, id)?.label
            ));
        }
        inputs.push(input(catalog, id)?);
    }
    let col = ident(&source_column(&inputs));
    // Columns are matched by name; a table that lacks one gives blanks. The `source` column is first,
    // as the `file` column of a folder is.
    let parts: Vec<String> = inputs
        .iter()
        .map(|t| format!("SELECT {} AS {col}, * FROM {}", lit(&t.label), t.sql_ref))
        .collect();
    Ok(Plan {
        sql: parts.join(" UNION ALL BY NAME "),
        merge: None,
    })
}

/// A key as the join compares it: as text when the two sides differ in type, as `query.rs` does.
fn key(alias: &str, column: &str, as_text: bool) -> String {
    let c = format!("{alias}.{}", ident(column));
    if as_text {
        format!("CAST({c} AS VARCHAR)")
    } else {
        c
    }
}

fn merge(
    left: &str,
    right: &str,
    on: &[(String, String)],
    keep: Keep,
    catalog: &Catalog,
) -> Result<Plan, String> {
    let l = input(catalog, left)?;
    let r = input(catalog, right)?;
    if left == right {
        return Err("Pick two different tables to merge.".into());
    }
    if on.is_empty() {
        return Err("Pick the columns that match: one from each table.".into());
    }
    let mut conds = Vec::new();
    let mut right_keys = Vec::new();
    let mut partition = Vec::new();
    for (lc, rc) in on {
        let a = column_of(l, lc)?;
        let b = column_of(r, rc)?;
        let as_text = a.dtype != b.dtype;
        conds.push(join_on(
            &format!("l.{}", ident(lc)),
            &format!("r.{}", ident(rc)),
            a.dtype,
            b.dtype,
        ));
        right_keys.push(rc.as_str());
        partition.push(key("x", rc, as_text));
    }
    // The right table, one row per key: the first in the file for local data, any one for a live
    // database table (V4-D51, V4-D126). A match marker counts the rows that found one.
    let n = ident("__tenun_row");
    let m = ident("__tenun_match");
    let by = partition.join(", ");
    let one = if r.prep {
        format!(
            "(SELECT *, 1 AS {m} FROM (SELECT *, row_number() OVER () AS {n} FROM {} AS x) AS x \
             QUALIFY row_number() OVER (PARTITION BY {by} ORDER BY {n}) = 1)",
            r.sql_ref
        )
    } else {
        format!(
            "(SELECT DISTINCT ON ({by}) *, 1 AS {m} FROM {} AS x ORDER BY {by})",
            r.sql_ref
        )
    };
    let join = match keep {
        Keep::Left => "LEFT JOIN",
        Keep::Inner => "JOIN",
    };
    let from = format!(
        " FROM {} AS l {join} {one} AS r ON {}",
        l.sql_ref,
        conds.join(" AND ")
    );
    // The columns of the right table, without its keys, which are the left keys again. A name the left
    // table has already gets the label of the right table in front.
    let mut taken: BTreeSet<String> = l.columns.iter().map(|c| c.name.clone()).collect();
    let mut select = vec!["l.*".to_string()];
    for c in r
        .columns
        .iter()
        .filter(|c| !right_keys.contains(&c.name.as_str()))
    {
        let name = if taken.contains(&c.name) {
            free_name(&format!("{} {}", r.label, c.name), &taken)
        } else {
            c.name.clone()
        };
        taken.insert(name.clone());
        select.push(format!("r.{} AS {}", ident(&c.name), ident(&name)));
    }
    // A blank key matches nothing, so it is not a repeated key.
    let filled: Vec<String> = partition
        .iter()
        .map(|k| format!("{k} IS NOT NULL"))
        .collect();
    let repeated = format!(
        "SELECT count(*) FROM (SELECT {by} FROM {} AS x WHERE {} GROUP BY {by} HAVING count(*) > 1)",
        r.sql_ref,
        filled.join(" AND ")
    );
    let matched = format!(
        "SELECT count(*), count(r.{m}) FROM {} AS l LEFT JOIN {one} AS r ON {}",
        l.sql_ref,
        conds.join(" AND ")
    );
    Ok(Plan {
        sql: format!("SELECT {}{from}", select.join(", ")),
        merge: Some(MergeCounts {
            matched,
            repeated,
            left: l.label.clone(),
            right: r.label.clone(),
            keep,
        }),
    })
}

fn group(
    table: &str,
    by: &[String],
    values: &[crate::workspace::GroupValue],
    catalog: &Catalog,
    rels: &[Relationship],
    health: &Health,
) -> Result<Plan, String> {
    let t = input(catalog, table)?;
    if by.is_empty() && values.is_empty() {
        return Err("Pick a column to group by, or add a value.".into());
    }
    let mut names: BTreeSet<String> = BTreeSet::new();
    for b in by {
        column_of(t, b)?;
        if !names.insert(b.clone()) {
            return Err(format!("{b} is picked twice. Group by each column once."));
        }
    }
    // The formulas are part of a view, so every join reads one row per key (V4-D63).
    let cx = Context {
        view: true,
        ..Context::new(catalog, rels, health)
    };
    let mut parts = Vec::new();
    let mut outs = Vec::new();
    for v in values {
        let name = v.name.trim();
        if name.is_empty() {
            return Err("Give each value a name.".into());
        }
        if name.chars().count() > MAX_NAME {
            return Err(format!(
                "The name {name} is too long: {MAX_NAME} characters at most."
            ));
        }
        if !names.insert(name.to_string()) {
            return Err(format!(
                "Two columns would be called {name}. Give each value its own name."
            ));
        }
        let c = formula::compile(&v.formula, Kind::Group, Some(table), &cx)
            .map_err(|e| format!("{name}: {}", e.message))?;
        let Body::Aggregate(agg) = c.body else {
            return Err(format!(
                "{name}: a value adds up rows, such as SUM([amount])."
            ));
        };
        let at = parts.len();
        let filled: Vec<String> = (0..agg.parts.len())
            .map(|i| format!("g{}.{}", at + i, ident("v")))
            .collect();
        outs.push(format!("{} AS {}", agg.fill(&filled), ident(name)));
        parts.extend(agg.parts);
    }
    let bys: Vec<String> = by
        .iter()
        .enumerate()
        .map(|(i, b)| format!("t0.{} AS {}", ident(b), ident(&format!("b{i}"))))
        .collect();
    if parts.is_empty() {
        return Ok(Plan {
            sql: format!(
                "SELECT DISTINCT {} FROM {} AS t0 ORDER BY {}",
                by_names(by),
                t.sql_ref,
                (1..=by.len())
                    .map(|i| format!("{i} NULLS LAST"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            merge: None,
        });
    }
    // One sub-query for each part (each has its own joins), grouped the same way, then put side by
    // side on the group values. Every part reads the rows of the same table, so every sub-query has
    // the same groups.
    let group_by = if by.is_empty() {
        String::new()
    } else {
        format!(
            " GROUP BY {}",
            (1..=by.len())
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let mut ctes = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let mut select = bys.clone();
        select.push(format!("{} AS {}", p.sql, ident("v")));
        let joins: String = p.joins.iter().map(|j| j.to_sql()).collect();
        ctes.push(format!(
            "g{i} AS (SELECT {} FROM {} AS t0{joins}{group_by})",
            select.join(", "),
            t.sql_ref
        ));
    }
    let mut from = " FROM g0".to_string();
    for i in 1..parts.len() {
        if by.is_empty() {
            from.push_str(&format!(" CROSS JOIN g{i}"));
        } else {
            let on: Vec<String> = (0..by.len())
                .map(|b| {
                    let c = ident(&format!("b{b}"));
                    format!("g{i}.{c} IS NOT DISTINCT FROM g0.{c}")
                })
                .collect();
            from.push_str(&format!(" LEFT JOIN g{i} ON {}", on.join(" AND ")));
        }
    }
    let mut select: Vec<String> = by
        .iter()
        .enumerate()
        .map(|(i, b)| format!("g0.{} AS {}", ident(&format!("b{i}")), ident(b)))
        .collect();
    select.extend(outs);
    // The groups in order, so the table reads like a list.
    let order = if by.is_empty() {
        String::new()
    } else {
        format!(
            " ORDER BY {}",
            (1..=by.len())
                .map(|i| format!("{i} NULLS LAST"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Ok(Plan {
        sql: format!(
            "WITH {} SELECT {}{from}{order}",
            ctes.join(", "),
            select.join(", ")
        ),
        merge: None,
    })
}

fn by_names(by: &[String]) -> String {
    by.iter()
        .map(|b| format!("t0.{}", ident(b)))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---- the order of loads ----------------------------------------------------------------------

/// The source a table belongs to: the part of its id before the first `/`.
pub fn source_of(table: &str) -> &str {
    table.split_once('/').map_or(table, |(s, _)| s)
}

/// The sources a derived source reads, from its spec.
fn reads(src: &Source) -> Vec<&str> {
    match &src.kind {
        SourceKind::Derived { spec } => spec.inputs().into_iter().map(source_of).collect(),
        _ => Vec::new(),
    }
}

/// The order to load sources in: every other source as listed, then the new tables, each after the
/// sources it reads. The second list holds the new tables that read each other in a ring; they are
/// not loaded (a file written by hand may hold one; Tenun refuses one when it is saved).
pub fn load_order(sources: &[Source]) -> (Vec<String>, Vec<String>) {
    let mut order: Vec<String> = sources
        .iter()
        .filter(|s| !matches!(s.kind, SourceKind::Derived { .. }))
        .map(|s| s.id.clone())
        .collect();
    let mut left: Vec<&Source> = sources
        .iter()
        .filter(|s| matches!(s.kind, SourceKind::Derived { .. }))
        .collect();
    loop {
        let before = left.len();
        let mut i = 0;
        while i < left.len() {
            let ready = reads(left[i])
                .iter()
                .all(|r| order.iter().any(|o| o == r) || !left.iter().any(|x| x.id == *r));
            if ready {
                order.push(left.remove(i).id.clone());
            } else {
                i += 1;
            }
        }
        if left.len() == before {
            break;
        }
    }
    (order, left.into_iter().map(|s| s.id.clone()).collect())
}

/// The new tables that read the sources `changed`, directly or through other new tables, in the order
/// they load. `changed` themselves are not in it.
pub fn dependents(sources: &[Source], changed: &[String]) -> Vec<String> {
    let mut hit: BTreeSet<String> = changed.iter().cloned().collect();
    let mut found: BTreeSet<String> = BTreeSet::new();
    loop {
        let more: Vec<String> = sources
            .iter()
            .filter(|s| !hit.contains(&s.id))
            .filter(|s| reads(s).iter().any(|r| hit.contains(*r)))
            .map(|s| s.id.clone())
            .collect();
        if more.is_empty() {
            break;
        }
        for m in more {
            hit.insert(m.clone());
            found.insert(m);
        }
    }
    load_order(sources)
        .0
        .into_iter()
        .filter(|id| found.contains(id))
        .collect()
}

/// A new table `id` with this spec would read itself, directly or through other new tables.
pub fn makes_ring(sources: &[Source], id: &str, spec: &DerivedSpec) -> bool {
    let by_id: BTreeMap<&str, &Source> = sources.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut todo: Vec<&str> = spec.inputs().into_iter().map(source_of).collect();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    while let Some(s) = todo.pop() {
        if s == id {
            return true;
        }
        if !seen.insert(s) {
            continue;
        }
        if let Some(src) = by_id.get(s) {
            todo.extend(reads(src));
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::GroupValue;

    fn src(id: &str, kind: SourceKind) -> Source {
        Source {
            id: id.into(),
            name: id.into(),
            kind,
        }
    }

    fn file(id: &str) -> Source {
        src(
            id,
            SourceKind::Csv {
                path: format!("{id}.csv"),
            },
        )
    }

    fn append_of(id: &str, tables: &[&str]) -> Source {
        src(
            id,
            SourceKind::Derived {
                spec: DerivedSpec::Append {
                    tables: tables.iter().map(|t| t.to_string()).collect(),
                },
            },
        )
    }

    #[test]
    fn a_new_table_loads_after_the_tables_it_reads_and_a_ring_is_not_loaded() {
        // s4 reads s3, which reads s1 and s2; s5 and s6 read each other.
        let sources = vec![
            append_of("s4", &["s3/x", "s1/a"]),
            file("s1"),
            append_of("s3", &["s1/a", "s2/b"]),
            file("s2"),
            append_of("s5", &["s6/z", "s1/a"]),
            append_of("s6", &["s5/y", "s1/a"]),
        ];
        let (order, ring) = load_order(&sources);
        assert_eq!(order, ["s1", "s2", "s3", "s4"]);
        assert_eq!(ring, ["s5", "s6"]);
        // A reload of s2 rebuilds s3, then s4.
        assert_eq!(dependents(&sources, &["s2".into()]), ["s3", "s4"]);
        assert_eq!(dependents(&sources, &["s4".into()]), Vec::<String>::new());
        // A table that would read itself is refused when it is saved.
        let spec = DerivedSpec::Append {
            tables: vec!["s4/w".into(), "s2/b".into()],
        };
        assert!(makes_ring(&sources, "s3", &spec));
        assert!(!makes_ring(&sources, "s7", &spec));
        // A group of a merge reads the merge's source.
        let g = DerivedSpec::Group {
            table: "s3/x".into(),
            by: vec![],
            values: vec![GroupValue {
                name: "n".into(),
                formula: "COUNTROWS([x])".into(),
            }],
        };
        assert!(makes_ring(&sources, "s1", &g));
    }

    #[test]
    fn numbers_get_thousands_and_names_are_checked() {
        assert_eq!(thousands(9000), "9,000");
        assert_eq!(thousands(8912), "8,912");
        assert_eq!(thousands(12), "12");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(thousands(-1500), "-1,500");
        assert!(name_problem("  ").is_some());
        assert!(name_problem("__x").is_some());
        assert!(name_problem(&"a".repeat(65)).is_some());
        assert!(name_problem("Sales by province").is_none());
    }
}
