//! Links that stay honest (V4-D26 to V4-D28, V4-D50, V4-D51): what the data says about each
//! relationship, and the paths a query may take across them.
//!
//! A link is stored one way: many rows of `from` point at one row of `to`. The data decides what
//! that means today. The engine checks it once for every load and keeps the answer
//! ([`LinkHealth`]); the query compiler reads it through [`Links`].
//!
//! - The `to` side has a few repeated keys: a join into it reads one row per key, so no row
//!   multiplies and no total doubles. The link carries a warning.
//! - The `from` side has no repeated keys: the link also works the other way, because every
//!   row of `to` then matches at most one row of `from`.
//! - The `to` side is **not a key**: more than half of its distinct keys repeat (V4-D50). The
//!   link cannot be read the stored way. If the `from` side is unique the link reads the other
//!   way round. If both sides repeat, the link is many to many: it stays saved and is not used.
//!
//! Two things are never left to the check (V4-D51). A join into a live database table always
//! reads one row per key, whatever the last check said. A join into any table whose link has no
//! health record (a reload is running, or the check failed) reads one row per key too. A
//! [`Links`] built with no health at all (a unit test) reads every link as stored, joined as it is.
//!
//! A table marked as a **link table** (`TableSetup.link_table`, V4-D29, V4-D112) may be crossed from the
//! one side of one of its links to the table itself, which is the many side: a programme has several
//! funders through the table `programme_funders`. A path crosses one link table at most. A [`Links`]
//! built without the marks (a formula, a calculated column) never crosses one.

use crate::catalog::{Catalog, Table, ident};
use crate::workspace::{Relationship, TableSetup};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// The most repeated keys a health record names.
pub const SAMPLE_KEYS: usize = 5;

/// What the data of one relationship says. Numbers only: the sentences are written when they
/// are shown, so a renamed column or a renamed file reads right.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkHealth {
    /// Rows on the `to` side that have a key (blank keys never match a row).
    pub one_rows: u64,
    /// Rows on the `to` side beyond the first of each key: `count(key) - count(DISTINCT key)`.
    pub one_dups: u64,
    /// How many keys of the `to` side appear more than once.
    pub one_keys: u64,
    /// Up to [`SAMPLE_KEYS`] of those keys as text, the most repeated first.
    pub sample: Vec<serde_json::Value>,
    /// How many rows each key of `sample` has.
    pub sample_rows: Vec<u64>,
    /// Rows on the `from` side that have a key.
    pub many_rows: u64,
    /// Rows on the `from` side beyond the first of each key.
    pub many_dups: u64,
    /// Rows on the `from` side whose key is one of the repeated keys of the `to` side.
    pub many_hits: u64,
    /// Only the first rows of a live database table were read: how many (V4-D51).
    pub limit: Option<u64>,
    /// The kind this link had before a reload changed it, while it has not changed back.
    pub was: Option<&'static str>,
}

impl LinkHealth {
    /// Both sides are unique: the link reads the same in either direction.
    pub fn one_to_one(&self) -> bool {
        self.one_dups == 0 && self.many_dups == 0
    }

    /// The keys of the `to` side that are not repeated.
    pub fn one_distinct(&self) -> u64 {
        self.one_rows.saturating_sub(self.one_dups)
    }

    /// The `to` side is not a key any more: more than half of its distinct keys repeat (V4-D50).
    /// A few repeated keys are a warning; a column that is mostly repeats is another column.
    pub fn to_is_not_a_key(&self) -> bool {
        self.one_keys * 2 > self.one_distinct()
    }

    /// The link may be crossed the stored way, from the many side to the one side.
    pub fn forward_ok(&self) -> bool {
        !self.to_is_not_a_key()
    }

    /// The link may be crossed from the one side to the many side: no key repeats on the many
    /// side, so every row of the one side matches at most one row there.
    pub fn backward_ok(&self) -> bool {
        self.many_dups == 0
    }

    /// Neither way works: many to many. The link is saved and not used.
    pub fn unusable(&self) -> bool {
        !self.forward_ok() && !self.backward_ok()
    }

    /// The stored way does not work, but the other way does: the many side is the unique one.
    pub fn reversed(&self) -> bool {
        !self.forward_ok() && self.backward_ok()
    }

    /// What the data says the link is. `one-one` only when both sides are unique.
    pub fn kind(&self) -> &'static str {
        if self.unusable() {
            "many-many"
        } else if self.reversed() {
            "many-one"
        } else if self.one_to_one() {
            "one-one"
        } else {
            "one-many"
        }
    }
}

/// The kind of a link in words.
pub fn kind_words(kind: &str) -> &'static str {
    match kind {
        "one-one" => "one to one",
        "many-one" => "many to one",
        "many-many" => "many to many",
        _ => "one to many",
    }
}

/// The health of every relationship that was checked, by relationship id. A link with no record
/// is **unknown**: a reload is running, or the check failed (V4-D51).
pub type Health = BTreeMap<String, LinkHealth>;

/// The relationships of a workspace with what the data says about them.
#[derive(Clone, Copy)]
pub struct Links<'a> {
    pub rels: &'a [Relationship],
    /// `None`: nothing is known about the data and every link is read as stored. `Some`: a link
    /// with no record in it is unknown, and a join into it reads one row per key.
    pub health: Option<&'a Health>,
    /// Count a link that works neither way as if it worked the stored way. Only to find out
    /// whether such a link is what stands between two tables.
    lenient: bool,
    /// Every join into a table reads one row per key, whatever the health says, in both
    /// directions. The SQL of a view is fixed when the view is made and cannot follow a later
    /// change of the data, so it must be safe for any data (V4-D63).
    one_row: bool,
    /// The setup of each table, for the marks of link tables. `None`: no table is crossed this way.
    marks: Option<&'a BTreeMap<String, TableSetup>>,
    /// The path already crossed a link table, so it may not cross another (V4-D112).
    crossed: bool,
}

impl<'a> Links<'a> {
    pub fn new(rels: &'a [Relationship], health: Option<&'a Health>) -> Self {
        Links {
            rels,
            health,
            lenient: false,
            one_row: false,
            marks: None,
            crossed: false,
        }
    }

    /// The same links, where a table marked as a link table may be crossed from the one side of
    /// one of its links (V4-D112). Only a question to the data does this; a formula never does.
    pub fn with_link_tables(self, setups: &'a BTreeMap<String, TableSetup>) -> Self {
        Links {
            marks: Some(setups),
            ..self
        }
    }

    /// The same links, where no link table is crossed any more: the rest of a path that has
    /// crossed one (V4-D112).
    pub(crate) fn no_crossing(self) -> Self {
        Links {
            crossed: true,
            ..self
        }
    }

    /// The table is marked as a link table and a path may still cross one.
    pub(crate) fn crosses(&self, table: &str) -> bool {
        !self.crossed
            && self
                .marks
                .and_then(|m| m.get(table))
                .is_some_and(|s| s.link_table)
    }

    /// Some table is marked, so a search has more to find.
    fn any_link_table(&self) -> bool {
        !self.crossed && self.marks.is_some_and(|m| m.values().any(|s| s.link_table))
    }

    /// The same links, where every join reads one row per key (V4-D63). For the SQL of a view.
    pub fn one_row_per_key(self) -> Self {
        Links {
            one_row: true,
            ..self
        }
    }

    /// The same links, where a link that works neither way still counts the stored way.
    pub(crate) fn lenient(self) -> Self {
        Links {
            lenient: true,
            ..self
        }
    }

    pub fn find(&self, id: &str) -> Option<&'a Relationship> {
        self.rels.iter().find(|r| r.id == id)
    }

    pub fn health(&self, id: &str) -> Option<&'a LinkHealth> {
        self.health.and_then(|h| h.get(id))
    }

    /// A link that works neither way (many to many) is not used.
    pub fn usable(&self, r: &Relationship) -> bool {
        self.lenient || !self.health(&r.id).is_some_and(LinkHealth::unusable)
    }

    /// The link may be crossed the stored way. Without a health record it may.
    pub fn forward_ok(&self, r: &Relationship) -> bool {
        self.health(&r.id)
            .is_none_or(|h| h.forward_ok() || (self.lenient && h.unusable()))
    }

    /// The link may be crossed from `to` to `from`: no key repeats on the `from` side.
    pub fn backward_ok(&self, r: &Relationship) -> bool {
        self.health(&r.id).is_some_and(LinkHealth::backward_ok)
    }

    /// Joins into the `to` side read one row per key: the data says it repeats a key, or the
    /// data has not been read (V4-D51). With no health at all, nothing is known and nothing changes.
    pub fn dedup(&self, r: &Relationship) -> bool {
        if self.one_row {
            return true;
        }
        match self.health {
            None => false,
            Some(h) => h.get(&r.id).is_none_or(|x| x.one_dups > 0),
        }
    }

    /// The relationships a query may cross, in the order they were made.
    pub fn usable_rels(&self) -> impl Iterator<Item = &'a Relationship> + '_ {
        self.rels.iter().filter(|r| self.usable(r))
    }

    /// One crossing of a link. `forward` goes from the many side to the one side.
    pub(crate) fn edge(&self, r: &Relationship, forward: bool) -> Edge {
        if forward {
            Edge {
                rel: r.id.clone(),
                from_table: r.from.table.clone(),
                from_col: r.from.column.clone(),
                to_table: r.to.table.clone(),
                to_col: r.to.column.clone(),
                dedup: self.dedup(r),
                warn: self.health(&r.id).is_some_and(|h| h.one_dups > 0),
                cross: false,
            }
        } else {
            Edge {
                rel: r.id.clone(),
                from_table: r.to.table.clone(),
                from_col: r.to.column.clone(),
                to_table: r.from.table.clone(),
                to_col: r.from.column.clone(),
                // No key repeats on this side today; a view cannot count on that tomorrow.
                dedup: self.one_row,
                // The link is read this way because the stored way stopped working.
                warn: self.health(&r.id).is_some_and(LinkHealth::reversed),
                cross: false,
            }
        }
    }

    /// The crossing of a link into its many side, which is a link table: from the one side to
    /// every row of the link table that names it (V4-D112). Rows are not multiplied here: the
    /// query reads the link table as distinct pairs, so a pair written twice counts once.
    pub(crate) fn cross_edge(&self, r: &Relationship) -> Edge {
        Edge {
            rel: r.id.clone(),
            from_table: r.to.table.clone(),
            from_col: r.to.column.clone(),
            to_table: r.from.table.clone(),
            to_col: r.from.column.clone(),
            dedup: false,
            warn: false,
            cross: true,
        }
    }
}

impl<'a> From<&'a [Relationship]> for Links<'a> {
    fn from(rels: &'a [Relationship]) -> Self {
        Links::new(rels, None)
    }
}

impl<'a> From<&'a Vec<Relationship>> for Links<'a> {
    fn from(rels: &'a Vec<Relationship>) -> Self {
        Links::new(rels, None)
    }
}

impl<'a, const N: usize> From<&'a [Relationship; N]> for Links<'a> {
    fn from(rels: &'a [Relationship; N]) -> Self {
        Links::new(rels, None)
    }
}

/// One crossing of a link, in the direction a query takes it.
#[derive(Debug, Clone)]
pub(crate) struct Edge {
    /// The id of the relationship.
    pub(crate) rel: String,
    pub(crate) from_table: String,
    pub(crate) from_col: String,
    pub(crate) to_table: String,
    pub(crate) to_col: String,
    /// The table joined has repeated keys: read one row per key.
    pub(crate) dedup: bool,
    /// The reader of a result that crosses this link should be told what the data did to it.
    pub(crate) warn: bool,
    /// The edge goes from the one side of a link into a link table, its many side (V4-D112). What
    /// follows is behind the link table: a row can be under several of its values.
    pub(crate) cross: bool,
}

/// The condition of a join. Columns of different types meet as text.
pub(crate) fn join_on(
    left: &str,
    right: &str,
    left_type: crate::catalog::DType,
    right_type: crate::catalog::DType,
) -> String {
    if left_type == right_type {
        format!("{left} = {right}")
    } else {
        format!("CAST({left} AS VARCHAR) = CAST({right} AS VARCHAR)")
    }
}

impl Edge {
    /// What a join reads: the table itself, or a copy of it with one row for each key (V4-D26).
    /// `as_text` is true when the join compares the keys as text.
    ///
    /// A **live database table** (attached, not copied into memory) always reads one row per
    /// key, `DISTINCT ON` the key and ordered by the key only: the database does the work, and a
    /// key that is repeated tomorrow cannot double a total today (V4-D51). Which of the rows of
    /// a key is used is not defined.
    ///
    /// Local data (a file, a sheet, a pasted table, a web link) keeps the first row of each key
    /// in the order of the file, but only when its keys repeat or are not known.
    pub(crate) fn target_sql(&self, to: &Table, as_text: bool) -> String {
        if !to.prep {
            return live_one_row_per_key(to, &self.to_col, as_text);
        }
        if !self.dedup {
            return to.sql_ref.clone();
        }
        one_row_per_key(to, &self.to_col, as_text)
    }
}

fn key_expr(key: &str, as_text: bool) -> String {
    if as_text {
        format!("CAST({} AS VARCHAR)", ident(key))
    } else {
        ident(key)
    }
}

/// The rows of a live database table with one row for each value of `key` (V4-D51).
pub(crate) fn live_one_row_per_key(t: &Table, key: &str, as_text: bool) -> String {
    let k = key_expr(key, as_text);
    format!(
        "(SELECT DISTINCT ON ({k}) * FROM {} ORDER BY {k})",
        t.sql_ref
    )
}

/// The rows of a local table with one row for each value of `key`: the first in the file.
pub(crate) fn one_row_per_key(t: &Table, key: &str, as_text: bool) -> String {
    let k = key_expr(key, as_text);
    let n = ident("__tenun_row");
    format!(
        "(SELECT * FROM (SELECT *, row_number() OVER () AS {n} FROM {}) \
         QUALIFY row_number() OVER (PARTITION BY {k} ORDER BY {n}) = 1)",
        t.sql_ref
    )
}

/// What one search from a base table finds.
pub(crate) struct Search {
    /// Every reachable table and the edge that reaches it by the default path.
    pub(crate) edge: HashMap<String, Option<Edge>>,
    /// Every reachable table and the first link of **every** shortest path to it, in the order
    /// the links were made. More than one means a field of that table needs a name.
    pub(crate) firsts: HashMap<String, Vec<String>>,
    /// The tables the default path reaches by crossing a link table, the link table included
    /// (V4-D112). A row of the base table can be under several values of any of them. The
    /// TypeScript search returns the same set, and the shared vectors check both.
    #[allow(dead_code)]
    pub(crate) behind: HashSet<String>,
}

fn add_all(into: &mut Vec<String>, from: &[String]) {
    for id in from {
        if !into.contains(id) {
            into.push(id.clone());
        }
    }
}

/// The one definition of "the shortest way" (V4-D28, V4-D42, V4-D112), used for the default path of a
/// query, for the formulas, and for the field list (the TypeScript mirror in `ui/src/lib/links.ts`
/// follows it, and both are tested against `ui/tests/fixtures/paths.json`).
///
/// **Pass 1** crosses the links from the many side to the one side only, breadth first, exactly as
/// before links could work both ways, so a table that was reachable keeps its path and its SQL.
/// **Pass 2** starts from every table pass 1 found (in the order it found them) and crosses
/// links both ways, breadth first, to find the tables pass 1 did not. **Pass 3** runs only when a
/// table is marked as a link table: from every table the first two passes found, a path may cross a
/// marked table from the one side of one of its links, then goes on as pass 2 does, and crosses no
/// second link table. A table the first two passes found keeps its path. In each pass the links are
/// tried in the order they were made, the stored way first; the first to reach a table is its
/// default path, and the links that reach it as quickly as that are all counted in `firsts`.
pub(crate) fn search<'a>(base: &str, links: impl Into<Links<'a>>) -> Search {
    let links: Links = links.into();
    let mut edge: HashMap<String, Option<Edge>> = HashMap::from([(base.to_string(), None)]);
    let mut firsts: HashMap<String, Vec<String>> = HashMap::new();
    let mut dist: HashMap<String, usize> = HashMap::from([(base.to_string(), 0)]);
    let mut order = vec![base.to_string()];
    // What a path that leaves `t` through the link `r` starts with.
    let start = |firsts: &HashMap<String, Vec<String>>, t: &str, r: &Relationship| -> Vec<String> {
        if t == base {
            vec![r.id.clone()]
        } else {
            firsts.get(t).cloned().unwrap_or_default()
        }
    };
    // Pass 1.
    let mut queue = VecDeque::from([base.to_string()]);
    while let Some(t) = queue.pop_front() {
        let d = dist[&t];
        for r in links.usable_rels() {
            if r.from.table != t || !links.forward_ok(r) {
                continue;
            }
            let next = &r.to.table;
            let here = start(&firsts, &t, r);
            match dist.get(next) {
                None => {
                    dist.insert(next.clone(), d + 1);
                    edge.insert(next.clone(), Some(links.edge(r, true)));
                    firsts.insert(next.clone(), here);
                    order.push(next.clone());
                    queue.push_back(next.clone());
                }
                Some(&nd) if nd == d + 1 => {
                    add_all(firsts.entry(next.clone()).or_default(), &here);
                }
                _ => {}
            }
        }
    }
    // Pass 2.
    let mut dist2: HashMap<String, usize> = order.iter().map(|t| (t.clone(), 0)).collect();
    let mut found = order.clone();
    let mut queue: VecDeque<String> = order.into();
    while let Some(t) = queue.pop_front() {
        let d = dist2[&t];
        for r in links.usable_rels() {
            let mut steps: Vec<(bool, &String)> = Vec::new();
            if r.from.table == t && links.forward_ok(r) {
                steps.push((true, &r.to.table));
            }
            if r.to.table == t && links.backward_ok(r) {
                steps.push((false, &r.from.table));
            }
            for (forward, next) in steps {
                let here = start(&firsts, &t, r);
                match dist2.get(next) {
                    None => {
                        dist2.insert(next.clone(), d + 1);
                        edge.insert(next.clone(), Some(links.edge(r, forward)));
                        firsts.insert(next.clone(), here);
                        found.push(next.clone());
                        queue.push_back(next.clone());
                    }
                    Some(&nd) if nd == d + 1 => {
                        add_all(firsts.entry(next.clone()).or_default(), &here);
                    }
                    _ => {}
                }
            }
        }
    }
    // Pass 3.
    let mut behind: HashSet<String> = HashSet::new();
    if links.any_link_table() {
        let mut dist3: HashMap<String, usize> = found.iter().map(|t| (t.clone(), 0)).collect();
        let mut queue: VecDeque<String> = found.into();
        while let Some(t) = queue.pop_front() {
            let d = dist3[&t];
            let inside = behind.contains(&t);
            for r in links.usable_rels() {
                // Before a crossing, the only new step is the crossing; after it, the steps of pass 2.
                let mut steps: Vec<(Edge, &String)> = Vec::new();
                if inside {
                    if r.from.table == t && links.forward_ok(r) {
                        steps.push((links.edge(r, true), &r.to.table));
                    }
                    if r.to.table == t && links.backward_ok(r) {
                        steps.push((links.edge(r, false), &r.from.table));
                    }
                } else if r.to.table == t && links.crosses(&r.from.table) && links.forward_ok(r) {
                    steps.push((links.cross_edge(r), &r.from.table));
                }
                for (e, next) in steps {
                    let here = start(&firsts, &t, r);
                    match dist3.get(next) {
                        None => {
                            dist3.insert(next.clone(), d + 1);
                            edge.insert(next.clone(), Some(e));
                            firsts.insert(next.clone(), here);
                            behind.insert(next.clone());
                            queue.push_back(next.clone());
                        }
                        Some(&nd) if nd == d + 1 => {
                            add_all(firsts.entry(next.clone()).or_default(), &here);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    let rank = |id: &String| links.rels.iter().position(|r| &r.id == id);
    for f in firsts.values_mut() {
        f.sort_by_key(rank);
    }
    Search {
        edge,
        firsts,
        behind,
    }
}

/// Breadth first from the base table. Maps every reachable table to the edge that reached it.
/// See [`search`].
pub(crate) fn reach<'a>(base: &str, links: impl Into<Links<'a>>) -> HashMap<String, Option<Edge>> {
    search(base, links).edge
}

/// The links a path to `table` may start with: the first link of every shortest path from `base`.
/// One entry means the way is clear. Two or more mean a field of `table` can mean either, and
/// a person has to say which (a named link). See [`search`].
pub(crate) fn first_links<'a>(base: &str, table: &str, links: impl Into<Links<'a>>) -> Vec<String> {
    if base == table {
        return Vec::new();
    }
    search(base, links).firsts.remove(table).unwrap_or_default()
}

/// Why a path could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathError {
    /// There is no way from the base table to the table.
    No,
    /// The field names a link that no longer exists.
    Gone,
    /// The field names a link that cannot be used now (many to many).
    Unusable,
    /// The field names a link that is not on any way from the base table to the table: its id.
    OffPath(String),
}

/// The paths from one base table, with the shortest as the default (V4-D28).
pub(crate) struct Paths<'a> {
    links: Links<'a>,
    base: String,
    default: HashMap<String, Option<Edge>>,
}

impl<'a> Paths<'a> {
    pub(crate) fn new(base: &str, links: Links<'a>) -> Self {
        Paths {
            links,
            base: base.to_string(),
            default: reach(base, links),
        }
    }

    /// The tables a query based on this table can reach by the default way.
    pub(crate) fn reached(&self) -> impl Iterator<Item = &String> {
        self.default.keys()
    }

    /// The edges from the base table to `table`, base first. With `via`, the path crosses that
    /// link: the default path to the link, the link, then the default path from it.
    pub(crate) fn path(&self, table: &str, via: Option<&str>) -> Result<Vec<Edge>, PathError> {
        if table == self.base {
            return Ok(Vec::new());
        }
        match via {
            None => chain(&self.default, table).ok_or(PathError::No),
            Some(id) => self.via_path(table, id),
        }
    }

    fn via_path(&self, table: &str, id: &str) -> Result<Vec<Edge>, PathError> {
        let r = self.links.find(id).ok_or(PathError::Gone)?;
        if !self.links.usable(r) {
            return Err(PathError::Unusable);
        }
        // The ways to cross the link: the stored way, the other way when the many side is unique,
        // and into a link table from its one side (V4-D112).
        let mut ways = Vec::new();
        if self.links.forward_ok(r) {
            ways.push((true, false));
        }
        if self.links.backward_ok(r) {
            ways.push((false, false));
        }
        if self.links.crosses(&r.from.table) && self.links.forward_ok(r) {
            ways.push((false, true));
        }
        for (forward, cross) in ways {
            let e = if cross {
                self.links.cross_edge(r)
            } else {
                self.links.edge(r, forward)
            };
            let Some(mut path) = chain(&self.default, &e.from_table) else {
                continue;
            };
            // A path crosses one link table at most: after the crossing, the rest may not cross.
            let after = if e.cross || path.iter().any(|x| x.cross) {
                self.links.no_crossing()
            } else {
                self.links
            };
            let tail = if e.to_table == table {
                Some(Vec::new())
            } else {
                chain(&reach(&e.to_table, after), table)
            };
            let Some(tail) = tail else { continue };
            path.push(e);
            path.extend(tail);
            if path.iter().filter(|x| x.cross).count() > 1 {
                continue;
            }
            return Ok(path);
        }
        Err(PathError::OffPath(id.to_string()))
    }
}

/// The edges from the root of `map` to `table`, root first. Empty for the root itself.
fn chain(map: &HashMap<String, Option<Edge>>, table: &str) -> Option<Vec<Edge>> {
    if !map.contains_key(table) {
        return None;
    }
    let mut out = Vec::new();
    let mut cur = table.to_string();
    while let Some(Some(e)) = map.get(&cur) {
        out.push(e.clone());
        cur = e.from_table.clone();
    }
    out.reverse();
    Some(out)
}

/// What a link is called: its name, or the one-side table with the many-side column, like
/// "Stores (ship_store_id)". The UI proposes the same words for a new name.
pub fn link_label(r: &Relationship, catalog: &Catalog) -> String {
    match r.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => default_name(r, catalog),
    }
}

/// The name Tenun proposes for a link that has none.
pub fn default_name(r: &Relationship, catalog: &Catalog) -> String {
    let one = catalog
        .table(&r.to.table)
        .map_or(r.to.table.as_str(), |t| t.label.as_str());
    format!("{one} ({})", r.from.column)
}

// ---- the words ----------------------------------------------------------------------------

/// What the app shows of one link's health: the facts and, when something is wrong, the
/// sentence. The Model view and the tile notes use these words as they are.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HealthOut {
    /// Both sides are unique.
    #[serde(rename = "oneToOne")]
    pub one_to_one: bool,
    /// Rows on the one side beyond the first of each key.
    pub duplicates: u64,
    /// Up to five of the keys that repeat, as text.
    pub sample: Vec<serde_json::Value>,
    /// How many rows each of those keys has.
    #[serde(rename = "sampleRows", skip_serializing_if = "Vec::is_empty")]
    pub sample_rows: Vec<u64>,
    /// How many keys repeat.
    #[serde(skip_serializing_if = "is_zero")]
    pub keys: u64,
    /// No key repeats on the many side, so the link also works from the one side.
    #[serde(rename = "fromUnique")]
    pub from_unique: bool,
    /// What the data says the link is: `one-many` (the usual), `one-one` (both sides unique),
    /// `many-one` (the stored way does not work, so it reads the other way round) or `many-many`
    /// (it is not used).
    pub kind: &'static str,
    /// Both ways fail: the link is not used.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unusable: bool,
    /// The warning in plain words. Not set when the link is healthy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

type Sources<'a> = &'a dyn Fn(&str) -> Option<(String, usize)>;

/// Where a table lives, for a sentence in the app: the file name when the file holds this table
/// alone, else the table with its file.
fn place(t: &Table, sources: Sources) -> String {
    match sources(&t.source_id) {
        Some((name, 1)) => name,
        Some((name, _)) => format!("{} in {name}", t.label),
        None => t.label.clone(),
    }
}

fn shown(t: &Table, column: &str) -> String {
    t.column(column)
        .and_then(|c| c.label.clone())
        .unwrap_or_else(|| column.to_string())
}

fn key_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// 3000 as "3,000".
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "1, 2 and 3", or "1, 2, 3, 4, 5 and 2 more".
fn listed_keys(h: &LinkHealth) -> String {
    let mut keys: Vec<String> = h.sample.iter().map(key_text).collect();
    let more = h.one_keys.saturating_sub(keys.len() as u64);
    if more > 0 {
        format!("{} and {more} more", keys.join(", "))
    } else if keys.len() > 1 {
        let last = keys.pop().unwrap_or_default();
        format!("{} and {last}", keys.join(", "))
    } else {
        keys.join(", ")
    }
}

/// How many rows of the many side meet a repeated key: the number a reader can weigh.
fn meeting(many: &Table, h: &LinkHealth) -> String {
    if h.many_rows == 0 {
        return String::new();
    }
    if h.many_hits == 0 {
        return format!(" No {} row meets a repeated key.", many.label);
    }
    format!(
        " {} of {} {} rows meet a repeated key.",
        thousands(h.many_hits),
        thousands(h.many_rows),
        many.label
    )
}

fn checked(h: &LinkHealth) -> String {
    match h.limit {
        Some(n) => format!(" Tenun checked only the first {} rows.", thousands(n)),
        None => String::new(),
    }
}

/// The warning of one link as the app shows it, with the values and the file names: on a tile and
/// in the sheet of the link. `None` when the link is healthy. `sources` gives the name of a source
/// and the number of tables it holds.
pub fn tile_note(
    r: &Relationship,
    h: &LinkHealth,
    catalog: &Catalog,
    sources: Sources,
) -> Option<String> {
    let one = catalog.table(&r.to.table)?;
    let many = catalog.table(&r.from.table)?;
    let col = shown(one, &r.to.column);
    if h.unusable() {
        return Some(format!(
            "{} and {} both repeat the values of this link ({} and {}), so Tenun cannot tell which row to use. Repeated {col}: {}. The link is not used until one of them is fixed.{}",
            place(many, sources),
            place(one, sources),
            shown(many, &r.from.column),
            col,
            listed_keys(h),
            checked(h),
        ));
    }
    if h.reversed() {
        return Some(format!(
            "{} repeats most of its values of {col} ({}), so it is not a key. Tenun reads this link the other way round: {} is the side with one row for each value. Fix the file to remove this note.{}",
            place(one, sources),
            listed_keys(h),
            place(many, sources),
            checked(h),
        ));
    }
    if h.one_dups == 0 {
        return None;
    }
    let at = place(one, sources);
    let (pick_one, pick_many, fix) = if one.prep {
        (
            "Tenun used the first.",
            "Tenun used the first row of each.",
            "Fix the file to remove this note.",
        )
    } else {
        let p = "Tenun used one row of each. Which one can change.";
        (p, p, "Fix the table to remove this note.")
    };
    let meet = meeting(many, h);
    let more = checked(h);
    if h.one_keys == 1
        && let (Some(k), Some(n)) = (h.sample.first(), h.sample_rows.first())
    {
        return Some(format!(
            "{at} has {n} rows with {col} {}. {pick_one}{meet} {fix}{more}",
            key_text(k)
        ));
    }
    Some(format!(
        "{at} repeats {col}: {} ({} rows too many). {pick_many}{meet} {fix}{more}",
        listed_keys(h),
        h.one_dups,
    ))
}

/// The same warning with no key value and no file name, for what leaves the computer: exported
/// and published pages, pictures and Excel files (V4-D52). Table labels and column names only.
pub fn public_note(r: &Relationship, h: &LinkHealth, catalog: &Catalog) -> Option<String> {
    let one = catalog.table(&r.to.table)?;
    let many = catalog.table(&r.from.table)?;
    let col = shown(one, &r.to.column);
    if h.unusable() {
        return Some(format!(
            "The {} and {} tables both repeat the values of this link ({} and {}). The link is not used.",
            many.label,
            one.label,
            shown(many, &r.from.column),
            col,
        ));
    }
    let values = |n: u64| {
        if n == 1 {
            "1 value".to_string()
        } else {
            format!("{} values", thousands(n))
        }
    };
    if h.reversed() {
        return Some(format!(
            "The {} table repeats {} of {col}, so it is not a key. Tenun reads this link the other way round.",
            one.label,
            values(h.one_keys),
        ));
    }
    if h.one_dups == 0 {
        return None;
    }
    let pick = if one.prep {
        if h.one_keys == 1 {
            "Tenun used the first row of it."
        } else {
            "Tenun used the first row of each."
        }
    } else {
        "Tenun used one row of each."
    };
    Some(format!(
        "The {} table repeats {} of {col}. {pick}{}",
        one.label,
        values(h.one_keys),
        meeting(many, h),
    ))
}

/// What the Model view and the sheet of a link say: the warning, and what changed since the
/// link last had another kind.
pub fn link_note(
    r: &Relationship,
    h: &LinkHealth,
    catalog: &Catalog,
    sources: Sources,
) -> Option<String> {
    let warning = tile_note(r, h, catalog, sources);
    let Some(was) = h.was.filter(|w| *w != h.kind()) else {
        return warning;
    };
    let changed = format!(
        "This link was {} and is now {}.{}",
        kind_words(was),
        kind_words(h.kind()),
        if was == "one-one" {
            " A tile that reads the many side from the one side cannot do it now."
        } else {
            ""
        }
    );
    Some(match warning {
        Some(w) => format!("{changed} {w}"),
        None => changed,
    })
}

/// The health of a link as the app shows it.
pub fn health_out(
    r: &Relationship,
    h: &LinkHealth,
    catalog: &Catalog,
    sources: Sources,
) -> HealthOut {
    HealthOut {
        one_to_one: h.one_to_one(),
        duplicates: h.one_dups,
        sample: h.sample.clone(),
        sample_rows: h.sample_rows.clone(),
        keys: h.one_keys,
        from_unique: h.many_dups == 0,
        kind: h.kind(),
        unusable: h.unusable(),
        note: link_note(r, h, catalog, sources),
    }
}

// ---- tests ----------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Column, DType};
    use crate::workspace::FieldRef;

    fn rel(id: &str, from: (&str, &str), to: (&str, &str)) -> Relationship {
        Relationship {
            id: id.into(),
            from: FieldRef {
                table: from.0.into(),
                column: from.1.into(),
            },
            to: FieldRef {
                table: to.0.into(),
                column: to.1.into(),
            },
            name: None,
        }
    }

    /// A health record for a one side of 100 rows where `one` rows repeat a key that another
    /// row has, each repeated key twice.
    fn health(one: u64, many: u64) -> LinkHealth {
        LinkHealth {
            one_rows: 100,
            one_dups: one,
            one_keys: one,
            many_rows: 500,
            many_dups: many,
            ..LinkHealth::default()
        }
    }

    #[test]
    fn without_health_a_link_is_read_as_stored_and_joined_as_it_is() {
        let rels = vec![rel("r1", ("a", "x"), ("b", "y"))];
        assert!(reach("a", &rels).contains_key("b"));
        assert!(!reach("b", &rels).contains_key("a"));
        let none = Links::from(&rels);
        assert!(!none.dedup(&rels[0]));
        assert!(!none.edge(&rels[0], true).dedup);
    }

    #[test]
    fn a_link_with_no_record_is_unknown_and_joins_one_row_per_key() {
        let rels = vec![rel("r1", ("a", "x"), ("b", "y"))];
        let mut h = Health::new();
        let unknown = Links::new(&rels, Some(&h));
        assert!(unknown.dedup(&rels[0]));
        assert!(unknown.edge(&rels[0], true).dedup);
        // It still crosses the stored way and no other, and it has nothing to say yet.
        assert!(reach("a", unknown).contains_key("b"));
        assert!(!reach("b", unknown).contains_key("a"));
        assert!(!unknown.edge(&rels[0], true).warn);
        // A record that says the keys are unique takes the dedupe away; a repeat brings it back.
        h.insert("r1".into(), health(0, 0));
        assert!(!Links::new(&rels, Some(&h)).dedup(&rels[0]));
        h.insert("r1".into(), health(1, 0));
        assert!(Links::new(&rels, Some(&h)).dedup(&rels[0]));
        // A crossing the other way never needs one row per key: the many side is unique.
        assert!(!Links::new(&rels, Some(&h)).edge(&rels[0], false).dedup);
    }

    fn table(prep: bool) -> Table {
        Table {
            id: "s1/stores".into(),
            source_id: "s1".into(),
            name: "stores".into(),
            label: "stores".into(),
            sql_ref: "\"s_s1\".\"stores\"".into(),
            columns: vec![Column::new("id".into(), DType::Number, String::new())],
            rows: None,
            notes: vec![],
            header_row: None,
            prep,
        }
    }

    #[test]
    fn a_live_table_is_always_read_one_row_per_key_by_the_key_alone() {
        let rels = vec![rel("r1", ("a", "x"), ("s1/stores", "id"))];
        let clean = Health::from([("r1".to_string(), health(0, 3))]);
        let links = Links::new(&rels, Some(&clean));
        let forward = links.edge(&rels[0], true);
        let backward = links.edge(&rels[0], false);
        // A local table with clean keys is joined as it is.
        assert_eq!(
            forward.target_sql(&table(true), false),
            "\"s_s1\".\"stores\""
        );
        // A live table is not, whatever the check said, and a crossing into it from the other
        // side is no different.
        let live = "(SELECT DISTINCT ON (\"id\") * FROM \"s_s1\".\"stores\" ORDER BY \"id\")";
        assert_eq!(forward.target_sql(&table(false), false), live);
        // The crossing from the other side joins into the many side, by its own column.
        assert_eq!(
            backward.target_sql(&table(false), false),
            "(SELECT DISTINCT ON (\"x\") * FROM \"s_s1\".\"stores\" ORDER BY \"x\")"
        );
        assert_eq!(
            forward.target_sql(&table(false), true),
            "(SELECT DISTINCT ON (CAST(\"id\" AS VARCHAR)) * FROM \"s_s1\".\"stores\" ORDER BY CAST(\"id\" AS VARCHAR))"
        );
    }

    #[test]
    fn the_rule_for_not_a_key_counts_keys_not_rows() {
        // Five real ids and ten rows of one key: one key repeats, six keys are distinct.
        let mut h = LinkHealth {
            one_rows: 15,
            one_dups: 9,
            one_keys: 1,
            many_dups: 40,
            ..LinkHealth::default()
        };
        assert!(h.forward_ok() && !h.reversed() && !h.unusable());
        assert_eq!(h.kind(), "one-many");
        // Four of seven distinct keys repeat: more than half.
        h = LinkHealth {
            one_rows: 14,
            one_dups: 7,
            one_keys: 4,
            many_dups: 40,
            ..LinkHealth::default()
        };
        assert!(h.to_is_not_a_key() && h.unusable());
        assert_eq!(h.kind(), "many-many");
        // Exactly half does not count.
        h = LinkHealth {
            one_rows: 8,
            one_dups: 4,
            one_keys: 2,
            ..LinkHealth::default()
        };
        assert!(!h.to_is_not_a_key());
        // The kind says one to one only when both sides are unique.
        assert_eq!(health(0, 0).kind(), "one-one");
        assert_eq!(health(1, 0).kind(), "one-many");
        assert_eq!(health(0, 5).kind(), "one-many");
        assert_eq!(health(40, 0).kind(), "many-one");
    }

    #[test]
    fn a_one_to_one_link_works_both_ways_and_a_unique_from_side_is_enough() {
        let rels = vec![rel("r1", ("a", "x"), ("b", "y"))];
        let mut h = Health::new();
        h.insert("r1".into(), health(0, 0));
        let both = Links::new(&rels, Some(&h));
        assert!(reach("b", both).contains_key("a"));
        // The one side gained a duplicate: forward is still the stored way (one row per key),
        // and the unique from side still lets the link be crossed the other way.
        h.insert("r1".into(), health(2, 0));
        let both = Links::new(&rels, Some(&h));
        assert!(reach("a", both).contains_key("b"));
        assert!(reach("b", both).contains_key("a"));
        // The one side repeats a few keys and the many side repeats keys, as it does for any
        // one to many link: one row for each key, forward only.
        h.insert("r1".into(), health(2, 3));
        let usual_with_a_typo = Links::new(&rels, Some(&h));
        assert!(reach("a", usual_with_a_typo).contains_key("b"));
        assert!(!reach("b", usual_with_a_typo).contains_key("a"));
        // The one side is not a key. Both sides repeat: many to many, not used either way.
        h.insert("r1".into(), health(40, 3));
        let none = Links::new(&rels, Some(&h));
        assert!(!reach("a", none).contains_key("b"));
        assert!(!reach("b", none).contains_key("a"));
        // Seen as stored, the same link would connect them: that is how an error learns it.
        assert!(reach("a", none.lenient()).contains_key("b"));
        // The one side is not a key but the many side is unique: it reads the other way.
        h.insert("r1".into(), health(40, 0));
        let flipped = Links::new(&rels, Some(&h));
        assert!(!reach("a", flipped).contains_key("b"));
        assert!(reach("b", flipped).contains_key("a"));
        // The from side repeats and the to side is unique: the usual one to many.
        h.insert("r1".into(), health(0, 3));
        let usual = Links::new(&rels, Some(&h));
        assert!(reach("a", usual).contains_key("b"));
        assert!(!reach("b", usual).contains_key("a"));
    }

    #[test]
    fn a_path_that_was_reachable_keeps_its_edge_when_a_link_can_go_both_ways() {
        // c is reached from a through b (forward). The one to one link a-d and d-c would give
        // a shorter way, but the old path must not change.
        let rels = vec![
            rel("r1", ("a", "x"), ("b", "y")),
            rel("r2", ("b", "y"), ("c", "z")),
            rel("r3", ("d", "x"), ("a", "x")),
            rel("r4", ("d", "w"), ("c", "z")),
        ];
        let plain = reach("a", &rels);
        let mut h = Health::new();
        for id in ["r1", "r2", "r3", "r4"] {
            h.insert(id.into(), health(0, 0));
        }
        let both = reach("a", Links::new(&rels, Some(&h)));
        for t in ["a", "b", "c"] {
            assert_eq!(
                plain[t].as_ref().map(|e| e.rel.clone()),
                both[t].as_ref().map(|e| e.rel.clone()),
                "{t}"
            );
        }
        // d was not reachable before; now it is, through r3 backwards.
        assert!(!plain.contains_key("d"));
        assert_eq!(both["d"].as_ref().unwrap().rel, "r3");
    }

    #[test]
    fn the_default_path_always_starts_with_one_of_the_first_links() {
        // The field list and the default path must agree: one search makes both.
        let rels = vec![
            rel("r1", ("t", "a"), ("s", "id")),
            rel("r2", ("t", "b"), ("s", "id")),
            rel("r3", ("s", "region"), ("g", "code")),
            rel("r4", ("g", "owner"), ("o", "id")),
        ];
        let links = Links::from(&rels);
        let found = search("t", links);
        for table in ["s", "g", "o"] {
            let first_of_default = chain(&found.edge, table).unwrap()[0].rel.clone();
            assert!(found.firsts[table].contains(&first_of_default), "{table}");
        }
        assert_eq!(found.firsts["o"], ["r1", "r2"]);
        assert!(!found.firsts.contains_key("t"));
    }

    #[test]
    fn a_path_through_a_named_link_crosses_that_link() {
        let rels = vec![
            rel("r1", ("t", "from_store"), ("s", "id")),
            rel("r2", ("t", "to_store"), ("s", "id")),
        ];
        let links = Links::from(&rels);
        let paths = Paths::new("t", links);
        // No via: the first link, as before.
        assert_eq!(paths.path("s", None).unwrap()[0].rel, "r1");
        assert_eq!(paths.path("s", Some("r2")).unwrap()[0].rel, "r2");
        assert_eq!(paths.path("s", Some("r1")).unwrap()[0].rel, "r1");
        assert_eq!(paths.path("s", Some("r9")).unwrap_err(), PathError::Gone);
        // A table can always reach itself.
        assert!(paths.path("t", Some("r2")).unwrap().is_empty());
        assert_eq!(first_links("t", "s", &rels), ["r1", "r2"]);
        assert_eq!(first_links("t", "t", &rels), Vec::<String>::new());
    }

    #[test]
    fn a_link_that_is_not_on_the_way_is_told_apart() {
        let rels = vec![
            rel("r1", ("t", "a"), ("s", "id")),
            rel("r2", ("u", "k"), ("v", "id")),
        ];
        let paths = Paths::new("t", Links::from(&rels));
        assert_eq!(
            paths.path("s", Some("r2")).unwrap_err(),
            PathError::OffPath("r2".into())
        );
        assert_eq!(
            paths.path("v", Some("r2")).unwrap_err(),
            PathError::OffPath("r2".into())
        );
        assert_eq!(paths.path("v", None).unwrap_err(), PathError::No);
    }

    #[test]
    fn a_link_with_a_via_continues_down_the_chain() {
        let rels = vec![
            rel("r1", ("t", "a"), ("s", "id")),
            rel("r2", ("t", "b"), ("s", "id")),
            rel("r3", ("s", "region"), ("g", "code")),
        ];
        let paths = Paths::new("t", Links::from(&rels));
        let p = paths.path("g", Some("r2")).unwrap();
        assert_eq!(
            p.iter().map(|e| e.rel.as_str()).collect::<Vec<_>>(),
            ["r2", "r3"]
        );
        // The way to g starts with either link.
        assert_eq!(first_links("t", "g", &rels), ["r1", "r2"]);
    }

    fn marks(ids: &[&str]) -> BTreeMap<String, TableSetup> {
        ids.iter()
            .map(|id| {
                (
                    id.to_string(),
                    TableSetup {
                        link_table: true,
                        ..TableSetup::default()
                    },
                )
            })
            .collect()
    }

    fn steps(path: Vec<Edge>) -> Vec<(String, bool)> {
        path.into_iter().map(|e| (e.rel, e.cross)).collect()
    }

    #[test]
    fn a_link_table_is_crossed_once_on_the_default_path_and_on_a_named_one() {
        let rels = vec![
            rel("r1", ("s", "programme"), ("p", "id")),
            rel("r2", ("pf", "programme"), ("p", "id")),
            rel("r3", ("pf", "funder"), ("f", "id")),
            rel("r4", ("pf2", "funder"), ("f", "id")),
            rel("r5", ("pf2", "region"), ("g", "code")),
        ];
        let setups = marks(&["pf", "pf2"]);
        // Nobody has read the data yet: every link is read as stored, and a crossing is allowed.
        let links = Links::from(&rels).with_link_tables(&setups);
        let paths = Paths::new("s", links);
        let want = vec![
            ("r1".to_string(), false),
            ("r2".to_string(), true),
            ("r3".to_string(), false),
        ];
        assert_eq!(steps(paths.path("f", None).unwrap()), want);
        assert_eq!(steps(paths.path("f", Some("r3")).unwrap()), want);
        // The second link table would be a second crossing, so the table behind it is out of reach,
        // and naming its link does not change that.
        assert_eq!(paths.path("g", None).unwrap_err(), PathError::No);
        assert_eq!(paths.path("pf2", None).unwrap_err(), PathError::No);
        assert_eq!(
            paths.path("g", Some("r5")).unwrap_err(),
            PathError::OffPath("r5".into())
        );
        // From a table that is not behind one, each link table is crossed alone.
        let from_funders = Paths::new("f", links);
        assert_eq!(
            steps(from_funders.path("g", None).unwrap()),
            vec![("r4".to_string(), true), ("r5".to_string(), false)]
        );
        // Without the marks, none of it is reachable, and a formula (no marks) never crosses.
        let plain = Paths::new("s", Links::from(&rels));
        assert_eq!(plain.path("f", None).unwrap_err(), PathError::No);
        // The tables found behind a link table.
        let found = search("s", links);
        let mut behind: Vec<&String> = found.behind.iter().collect();
        behind.sort();
        assert_eq!(behind, ["f", "pf"]);
    }

    #[test]
    fn a_link_table_with_a_link_that_is_not_a_key_is_not_crossed() {
        let rels = vec![
            rel("r1", ("s", "programme"), ("p", "id")),
            rel("r2", ("pf", "programme"), ("p", "id")),
            rel("r3", ("pf", "funder"), ("f", "id")),
        ];
        let setups = marks(&["pf"]);
        let mut h = Health::new();
        for id in ["r1", "r2", "r3"] {
            h.insert(id.into(), health(0, 40));
        }
        let ok = Links::new(&rels, Some(&h)).with_link_tables(&setups);
        assert!(reach("s", ok).contains_key("f"));
        // The link table is the start of the query: its own links are crossed the usual way.
        let own = search("pf", ok);
        assert!(own.behind.is_empty());
        assert!(own.edge.contains_key("p") && own.edge.contains_key("f"));
        // The programmes repeat their ids and so does the link table: that link is not used at all.
        h.insert("r2".into(), health(40, 3));
        let flipped = Links::new(&rels, Some(&h)).with_link_tables(&setups);
        assert!(!reach("s", flipped).contains_key("f"));
    }

    fn world() -> (Catalog, Relationship) {
        let mut cat = Catalog::default();
        let mk = |id: &str, label: &str, col: &str| Table {
            id: id.into(),
            source_id: id.split('/').next().unwrap().into(),
            name: label.into(),
            label: label.into(),
            sql_ref: String::new(),
            columns: vec![Column::new(col.to_string(), DType::Number, String::new())],
            rows: None,
            notes: vec![],
            header_row: None,
            prep: true,
        };
        cat.tables.push(mk("s1/orders", "orders", "store_id"));
        cat.tables.push(mk("s2/stores", "stores", "id"));
        let r = rel("r1", ("s1/orders", "store_id"), ("s2/stores", "id"));
        (cat, r)
    }

    fn files(id: &str) -> Option<(String, usize)> {
        match id {
            "s2" => Some(("stores.csv".to_string(), 1)),
            "s1" => Some(("orders.xlsx".to_string(), 3)),
            _ => None,
        }
    }

    #[test]
    fn words_for_one_repeated_key_and_for_several_name_the_keys_and_the_rows_they_touch() {
        let (cat, r) = world();
        let mut h = LinkHealth {
            one_rows: 10,
            one_dups: 1,
            one_keys: 1,
            sample: vec![serde_json::json!("7")],
            sample_rows: vec![2],
            many_rows: 40,
            many_hits: 12,
            ..LinkHealth::default()
        };
        assert_eq!(
            tile_note(&r, &h, &cat, &files).unwrap(),
            "stores.csv has 2 rows with id 7. Tenun used the first. 12 of 40 orders rows meet a repeated key. Fix the file to remove this note."
        );
        assert_eq!(
            public_note(&r, &h, &cat).unwrap(),
            "The stores table repeats 1 value of id. Tenun used the first row of it. 12 of 40 orders rows meet a repeated key."
        );
        h.one_rows = 100;
        h.one_dups = 7;
        h.one_keys = 7;
        h.many_rows = 3000;
        h.many_hits = 0;
        h.sample = ["1", "2", "3", "4", "5"]
            .iter()
            .map(|n| serde_json::json!(n))
            .collect();
        h.sample_rows = vec![2; 5];
        assert_eq!(
            tile_note(&r, &h, &cat, &files).unwrap(),
            "stores.csv repeats id: 1, 2, 3, 4, 5 and 2 more (7 rows too many). Tenun used the first row of each. No orders row meets a repeated key. Fix the file to remove this note."
        );
        h.one_keys = 3;
        h.one_dups = 3;
        h.many_hits = 1200;
        h.sample.truncate(3);
        assert_eq!(
            tile_note(&r, &h, &cat, &files).unwrap(),
            "stores.csv repeats id: 1, 2 and 3 (3 rows too many). Tenun used the first row of each. 1,200 of 3,000 orders rows meet a repeated key. Fix the file to remove this note."
        );
        // What leaves the computer has no key and no file name (V4-D52).
        let public = public_note(&r, &h, &cat).unwrap();
        assert_eq!(
            public,
            "The stores table repeats 3 values of id. Tenun used the first row of each. 1,200 of 3,000 orders rows meet a repeated key."
        );
        assert!(!public.contains("stores.csv") && !public.contains("1, 2"));
        assert!(tile_note(&r, &LinkHealth::default(), &cat, &files).is_none());
        // A table in a live database says the row it used is not defined.
        let mut live = cat.clone();
        live.tables[1].prep = false;
        let n = tile_note(&r, &h, &live, &files).unwrap();
        assert!(
            n.contains("Tenun used one row of each. Which one can change."),
            "{n}"
        );
        assert!(n.ends_with("Fix the table to remove this note."), "{n}");
    }

    #[test]
    fn a_link_that_is_not_used_still_names_the_keys() {
        let (cat, r) = world();
        let h = LinkHealth {
            one_rows: 14,
            one_dups: 7,
            one_keys: 4,
            sample: ["1", "2", "4", "5"]
                .iter()
                .map(|k| serde_json::json!(k))
                .collect(),
            sample_rows: vec![3, 2, 2, 2],
            many_rows: 40,
            many_dups: 30,
            ..LinkHealth::default()
        };
        assert!(h.unusable());
        assert_eq!(
            tile_note(&r, &h, &cat, &files).unwrap(),
            "orders in orders.xlsx and stores.csv both repeat the values of this link (store_id and id), so Tenun cannot tell which row to use. Repeated id: 1, 2, 4 and 5. The link is not used until one of them is fixed."
        );
        assert_eq!(
            public_note(&r, &h, &cat).unwrap(),
            "The orders and stores tables both repeat the values of this link (store_id and id). The link is not used."
        );
        // The other way round, with the keys too.
        let flipped = LinkHealth { many_dups: 0, ..h };
        let n = tile_note(&r, &flipped, &cat, &files).unwrap();
        assert!(
            n.contains("(1, 2, 4 and 5)") && n.contains("the other way round"),
            "{n}"
        );
        assert_eq!(
            public_note(&r, &flipped, &cat).unwrap(),
            "The stores table repeats 4 values of id, so it is not a key. Tenun reads this link the other way round."
        );
    }

    #[test]
    fn a_link_that_changed_kind_says_so() {
        let (cat, r) = world();
        let mut h = health(0, 4);
        assert!(link_note(&r, &h, &cat, &files).is_none());
        h.was = Some("one-one");
        assert_eq!(
            link_note(&r, &h, &cat, &files).unwrap(),
            "This link was one to one and is now one to many. A tile that reads the many side from the one side cannot do it now."
        );
        // Back to what it was: nothing to say.
        h.was = Some("one-many");
        assert!(link_note(&r, &h, &cat, &files).is_none());
    }

    /// The vectors of `ui/tests/fixtures/paths.json`: the TypeScript mirror of the search reads
    /// the same file and must give the same answers.
    #[test]
    fn the_search_gives_the_answers_of_the_shared_vectors() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../ui/tests/fixtures/paths.json")).unwrap();
        assert!(cases.len() >= 20);
        let of_kind = |kind: &str| match kind {
            "one-many" => LinkHealth {
                one_rows: 100,
                many_rows: 100,
                many_dups: 50,
                ..LinkHealth::default()
            },
            "one-one" => LinkHealth {
                one_rows: 100,
                many_rows: 100,
                ..LinkHealth::default()
            },
            "many-one" => LinkHealth {
                one_rows: 14,
                one_dups: 7,
                one_keys: 4,
                many_rows: 14,
                ..LinkHealth::default()
            },
            "many-many" => LinkHealth {
                one_rows: 14,
                one_dups: 7,
                one_keys: 4,
                many_rows: 14,
                many_dups: 9,
                ..LinkHealth::default()
            },
            other => panic!("no such kind {other}"),
        };
        for case in &cases {
            let name = case["name"].as_str().unwrap();
            fn field(v: &serde_json::Value) -> (&str, &str) {
                (v[0].as_str().unwrap(), v[1].as_str().unwrap())
            }
            let rels: Vec<Relationship> = case["rels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| {
                    rel(
                        r["id"].as_str().unwrap(),
                        field(&r["from"]),
                        field(&r["to"]),
                    )
                })
                .collect();
            let health: Health = case["health"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(id, kind)| {
                    let h = of_kind(kind.as_str().unwrap());
                    assert_eq!(h.kind(), kind.as_str().unwrap(), "{name}");
                    (id.clone(), h)
                })
                .collect();
            let base = case["base"].as_str().unwrap();
            // The tables the vector marks as link tables.
            let setups: BTreeMap<String, TableSetup> = case["linkTables"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| {
                    (
                        t.as_str().unwrap().to_string(),
                        TableSetup {
                            link_table: true,
                            ..TableSetup::default()
                        },
                    )
                })
                .collect();
            let links = Links::new(&rels, Some(&health)).with_link_tables(&setups);
            let found = search(base, links);
            let mut got_behind: Vec<String> = found.behind.iter().cloned().collect();
            got_behind.sort();
            let want_behind: Vec<String> = case["behind"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| t.as_str().unwrap().to_string())
                .collect();
            assert_eq!(
                got_behind, want_behind,
                "{name}: the tables behind a link table"
            );
            let got_reach: std::collections::BTreeMap<String, Option<String>> = found
                .edge
                .iter()
                .map(|(t, e)| (t.clone(), e.as_ref().map(|e| e.rel.clone())))
                .collect();
            let want_reach: std::collections::BTreeMap<String, Option<String>> = case["reach"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(t, e)| (t.clone(), e.as_str().map(str::to_string)))
                .collect();
            assert_eq!(got_reach, want_reach, "{name}: the way to each table");
            let got_first: std::collections::BTreeMap<String, Vec<String>> = found
                .firsts
                .iter()
                .map(|(t, f)| (t.clone(), f.clone()))
                .collect();
            let want_first: std::collections::BTreeMap<String, Vec<String>> = case["first"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(t, f)| {
                    (
                        t.clone(),
                        f.as_array()
                            .unwrap()
                            .iter()
                            .map(|x| x.as_str().unwrap().to_string())
                            .collect(),
                    )
                })
                .collect();
            assert_eq!(got_first, want_first, "{name}: the first links");
            for (t, f) in &want_first {
                assert_eq!(&first_links(base, t, links), f, "{name}");
            }
        }
    }
}
