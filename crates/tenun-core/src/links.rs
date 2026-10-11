//! Links that stay honest (V4-D26 to V4-D28): what the data says about each relationship, and the
//! paths a query may take across them.
//!
//! A link is stored one way: many rows of `from` point at one row of `to`. The data decides what
//! that means today. The engine checks it once for every load and keeps the answer
//! ([`LinkHealth`]); the query compiler reads it through [`Links`].
//!
//! - The `to` side has a few repeated keys: a join into it reads one row per key (the first
//!   one), so no row multiplies and no total doubles. The link carries a warning.
//! - The `from` side has no repeated keys: the link also works the other way, because every
//!   row of `to` then matches at most one row of `from`.
//! - The `to` side repeats most of its values, so it is not a key any more: the link cannot
//!   be read the stored way. If the `from` side is unique the link reads the other way round.
//!   If both sides repeat, the link is many to many: it stays saved and is not used.
//!
//! Without a health record (a unit test, or a link that was not checked yet) every link is
//! read as stored: forward only, joined as it is.

use crate::catalog::{Catalog, Table, ident};
use crate::workspace::Relationship;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, VecDeque};

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
    /// Up to [`SAMPLE_KEYS`] of those keys, the most repeated first.
    pub sample: Vec<serde_json::Value>,
    /// How many rows each key of `sample` has.
    pub sample_rows: Vec<u64>,
    /// Rows on the `from` side beyond the first of each key.
    pub many_dups: u64,
}

impl LinkHealth {
    /// Both sides are unique: the link reads the same in either direction.
    pub fn one_to_one(&self) -> bool {
        self.one_dups == 0 && self.many_dups == 0
    }

    /// The `to` side repeats at least half of its rows: it is not a key any more, and reading
    /// one row per key would throw most of it away (V4-D41).
    pub fn to_is_not_a_key(&self) -> bool {
        self.one_dups > 0 && self.one_dups * 2 >= self.one_rows
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
}

/// The health of every relationship that was checked, by relationship id.
pub type Health = BTreeMap<String, LinkHealth>;

/// The relationships of a workspace with what the data says about them.
#[derive(Clone, Copy)]
pub struct Links<'a> {
    pub rels: &'a [Relationship],
    pub health: Option<&'a Health>,
}

impl<'a> Links<'a> {
    pub fn new(rels: &'a [Relationship], health: Option<&'a Health>) -> Self {
        Links { rels, health }
    }

    pub fn find(&self, id: &str) -> Option<&'a Relationship> {
        self.rels.iter().find(|r| r.id == id)
    }

    pub fn health(&self, id: &str) -> Option<&'a LinkHealth> {
        self.health.and_then(|h| h.get(id))
    }

    /// A link that works neither way (many to many) is not used.
    pub fn usable(&self, r: &Relationship) -> bool {
        !self.health(&r.id).is_some_and(LinkHealth::unusable)
    }

    /// The link may be crossed the stored way. Without a health record it may.
    pub fn forward_ok(&self, r: &Relationship) -> bool {
        self.health(&r.id).is_none_or(LinkHealth::forward_ok)
    }

    /// The link may be crossed from `to` to `from`: no key repeats on the `from` side.
    pub fn backward_ok(&self, r: &Relationship) -> bool {
        self.health(&r.id).is_some_and(LinkHealth::backward_ok)
    }

    /// Joins into the `to` side read one row per key.
    pub fn dedup(&self, r: &Relationship) -> bool {
        self.health(&r.id).is_some_and(|h| h.one_dups > 0)
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
                warn: self.dedup(r),
            }
        } else {
            Edge {
                rel: r.id.clone(),
                from_table: r.to.table.clone(),
                from_col: r.to.column.clone(),
                to_table: r.from.table.clone(),
                to_col: r.from.column.clone(),
                dedup: false,
                // The link is read this way because the stored way stopped working.
                warn: self.health(&r.id).is_some_and(LinkHealth::reversed),
            }
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
    /// What a join reads: the table itself, or when its keys repeat a copy with one row for
    /// each key (V4-D26). `as_text` is true when the join compares the keys as text.
    ///
    /// Local data (a file, a sheet, a pasted table, a web link) keeps the first row of each key
    /// in the order of the file. A database table has no order a person can see, so it keeps the
    /// row that sorts first when its columns are read left to right: the same row every time.
    pub(crate) fn target_sql(&self, to: &Table, as_text: bool) -> String {
        if !self.dedup {
            return to.sql_ref.clone();
        }
        one_row_per_key(to, &self.to_col, as_text)
    }
}

/// The rows of `t` with one row for each value of `key`.
pub(crate) fn one_row_per_key(t: &Table, key: &str, as_text: bool) -> String {
    let k = if as_text {
        format!("CAST({} AS VARCHAR)", ident(key))
    } else {
        ident(key)
    };
    if t.prep {
        let n = ident("__tenun_row");
        format!(
            "(SELECT * FROM (SELECT *, row_number() OVER () AS {n} FROM {}) \
             QUALIFY row_number() OVER (PARTITION BY {k} ORDER BY {n}) = 1)",
            t.sql_ref
        )
    } else {
        let order: Vec<String> = t
            .columns
            .iter()
            .map(|c| match c.dtype {
                crate::catalog::DType::Text => format!("CAST({} AS VARCHAR)", ident(&c.name)),
                _ => ident(&c.name),
            })
            .collect();
        format!(
            "(SELECT * FROM {} QUALIFY row_number() OVER (PARTITION BY {k} ORDER BY {}) = 1)",
            t.sql_ref,
            order.join(", ")
        )
    }
}

/// Breadth-first from the base table. Maps every reachable table to the edge that reached it.
///
/// First the links are crossed from the many side to the one side only, exactly as before links
/// could work both ways, so a table that was reachable keeps its path and its SQL. Then the links
/// that may also be crossed from the one side to the many side add the tables that were not
/// reachable before.
pub(crate) fn reach<'a>(base: &str, links: impl Into<Links<'a>>) -> HashMap<String, Option<Edge>> {
    let links: Links = links.into();
    let mut seen: HashMap<String, Option<Edge>> = HashMap::new();
    seen.insert(base.to_string(), None);
    let mut order = vec![base.to_string()];
    let mut queue = VecDeque::from([base.to_string()]);
    while let Some(t) = queue.pop_front() {
        for r in links
            .usable_rels()
            .filter(|r| r.from.table == t && links.forward_ok(r))
        {
            if !seen.contains_key(&r.to.table) {
                seen.insert(r.to.table.clone(), Some(links.edge(r, true)));
                order.push(r.to.table.clone());
                queue.push_back(r.to.table.clone());
            }
        }
    }
    let mut queue: VecDeque<String> = order.into();
    while let Some(t) = queue.pop_front() {
        for r in links.usable_rels() {
            if r.from.table == t && links.forward_ok(r) && !seen.contains_key(&r.to.table) {
                seen.insert(r.to.table.clone(), Some(links.edge(r, true)));
                queue.push_back(r.to.table.clone());
            } else if r.to.table == t && !seen.contains_key(&r.from.table) && links.backward_ok(r) {
                seen.insert(r.from.table.clone(), Some(links.edge(r, false)));
                queue.push_back(r.from.table.clone());
            }
        }
    }
    seen
}

/// Why a path could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathError {
    /// There is no way from the base table to the table.
    No,
    /// The field names a link that no longer exists.
    Gone,
    /// The field names a link that cannot be used now (both sides repeat keys).
    Unusable,
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
        let mut ways = Vec::new();
        if self.links.forward_ok(r) {
            ways.push(true);
        }
        if self.links.backward_ok(r) {
            ways.push(false);
        }
        for forward in ways {
            let e = self.links.edge(r, forward);
            let Some(mut path) = chain(&self.default, &e.from_table) else {
                continue;
            };
            let tail = if e.to_table == table {
                Some(Vec::new())
            } else {
                chain(&reach(&e.to_table, self.links), table)
            };
            let Some(tail) = tail else { continue };
            path.push(e);
            path.extend(tail);
            return Ok(path);
        }
        Err(PathError::No)
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

/// The links a path to `table` may start with: the first link of every shortest path from `base`.
/// One entry means the way is clear. Two or more mean a field of `table` can mean either, and
/// a person has to say which (a named link).
pub(crate) fn first_links<'a>(base: &str, table: &str, links: impl Into<Links<'a>>) -> Vec<String> {
    let links: Links = links.into();
    if base == table {
        return Vec::new();
    }
    // Distance from the base, and the set of first links of the shortest paths to each table.
    let mut dist: HashMap<String, usize> = HashMap::from([(base.to_string(), 0)]);
    let mut first: HashMap<String, Vec<String>> = HashMap::new();
    let mut queue = VecDeque::from([base.to_string()]);
    while let Some(t) = queue.pop_front() {
        let d = dist[&t];
        let own = first.get(&t).cloned().unwrap_or_default();
        for r in links.usable_rels() {
            let steps = [
                (r.from.table == t && links.forward_ok(r)).then_some(&r.to.table),
                (r.to.table == t && links.backward_ok(r)).then_some(&r.from.table),
            ];
            for next in steps.into_iter().flatten() {
                let via_here = if t == base {
                    vec![r.id.clone()]
                } else {
                    own.clone()
                };
                match dist.get(next) {
                    None => {
                        dist.insert(next.clone(), d + 1);
                        first.insert(next.clone(), via_here);
                        queue.push_back(next.clone());
                    }
                    Some(&nd) if nd == d + 1 => {
                        let f = first.entry(next.clone()).or_default();
                        for id in via_here {
                            if !f.contains(&id) {
                                f.push(id);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let mut out = first.remove(table).unwrap_or_default();
    // In the order the links were made.
    out.sort_by_key(|id| links.rels.iter().position(|r| &r.id == id));
    out
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
    /// Up to five of the keys that repeat.
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
    /// What the data says the link is: `one-many` (the usual), `one-one`, `many-one` (the stored
    /// way does not work, so it reads the other way round) or `many-many` (it is not used).
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

/// Where a table lives, for a sentence: the file name when the file holds this table alone,
/// else the table with its file.
fn place(t: &Table, sources: &dyn Fn(&str) -> Option<(String, usize)>) -> String {
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
        serde_json::Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

/// The warning of one link, or `None` when it is healthy. `sources` gives the name of a source
/// and the number of tables it holds.
pub fn note(
    r: &Relationship,
    h: &LinkHealth,
    catalog: &Catalog,
    sources: &dyn Fn(&str) -> Option<(String, usize)>,
) -> Option<String> {
    let one = catalog.table(&r.to.table)?;
    let many = catalog.table(&r.from.table)?;
    if h.unusable() {
        return Some(format!(
            "{} and {} both repeat the values of this link ({} and {}), so Tenun cannot tell which row to use. The link is not used until one of them is fixed.",
            place(many, sources),
            place(one, sources),
            shown(many, &r.from.column),
            shown(one, &r.to.column),
        ));
    }
    if h.reversed() {
        return Some(format!(
            "{} repeats most of its values of {}, so it is not a key. Tenun reads this link the other way round: {} is the side with one row for each value. Fix the file to remove this note.",
            place(one, sources),
            shown(one, &r.to.column),
            place(many, sources),
        ));
    }
    if h.one_dups == 0 {
        return None;
    }
    let at = place(one, sources);
    let col = shown(one, &r.to.column);
    let pick = if one.prep {
        "Tenun used the first."
    } else {
        "Tenun used one row of each, picked by the order of the columns."
    };
    let fix = if one.prep {
        "Fix the file to remove this note."
    } else {
        "Fix the table to remove this note."
    };
    if h.one_keys == 1
        && let (Some(k), Some(n)) = (h.sample.first(), h.sample_rows.first())
    {
        return Some(format!(
            "{at} has {n} rows with {col} {}. {pick} {fix}",
            key_text(k)
        ));
    }
    let mut keys: Vec<String> = h.sample.iter().map(key_text).collect();
    let more = h.one_keys.saturating_sub(keys.len() as u64);
    let listed = if more > 0 {
        format!("{} and {more} more", keys.join(", "))
    } else if keys.len() > 1 {
        let last = keys.pop().unwrap_or_default();
        format!("{} and {last}", keys.join(", "))
    } else {
        keys.join(", ")
    };
    Some(format!(
        "{at} repeats {col}: {listed} ({} rows too many). {} {fix}",
        h.one_dups,
        if one.prep {
            "Tenun used the first row of each."
        } else {
            "Tenun used one row of each, picked by the order of the columns."
        }
    ))
}

/// The health of a link as the app shows it.
pub fn health_out(
    r: &Relationship,
    h: &LinkHealth,
    catalog: &Catalog,
    sources: &dyn Fn(&str) -> Option<(String, usize)>,
) -> HealthOut {
    HealthOut {
        one_to_one: h.one_to_one(),
        duplicates: h.one_dups,
        sample: h.sample.clone(),
        sample_rows: h.sample_rows.clone(),
        keys: h.one_keys,
        from_unique: h.many_dups == 0,
        kind: if h.unusable() {
            "many-many"
        } else if h.reversed() {
            "many-one"
        } else if h.many_dups == 0 {
            "one-one"
        } else {
            "one-many"
        },
        unusable: h.unusable(),
        note: note(r, h, catalog, sources),
    }
}

// ---- tests ----------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
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

    /// A health record for a one side of 100 rows with `one` repeated rows.
    fn health(one: u64, many: u64) -> LinkHealth {
        LinkHealth {
            one_rows: 100,
            one_dups: one,
            many_dups: many,
            ..LinkHealth::default()
        }
    }

    #[test]
    fn without_health_a_link_is_read_as_stored() {
        let rels = vec![rel("r1", ("a", "x"), ("b", "y"))];
        assert!(reach("a", &rels).contains_key("b"));
        assert!(!reach("b", &rels).contains_key("a"));
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
        // The one side is mostly repeats: it is not a key. Both sides repeat: many to many,
        // not used either way.
        h.insert("r1".into(), health(60, 3));
        let none = Links::new(&rels, Some(&h));
        assert!(!reach("a", none).contains_key("b"));
        assert!(!reach("b", none).contains_key("a"));
        // The one side is mostly repeats but the many side is unique: it reads the other way.
        h.insert("r1".into(), health(60, 0));
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

    #[test]
    fn words_for_one_repeated_key_and_for_several() {
        let mut cat = Catalog::default();
        let mk = |id: &str, label: &str, cols: &[&str]| Table {
            id: id.into(),
            source_id: id.split('/').next().unwrap().into(),
            name: label.into(),
            label: label.into(),
            sql_ref: String::new(),
            columns: cols
                .iter()
                .map(|c| {
                    crate::catalog::Column::new(
                        c.to_string(),
                        crate::catalog::DType::Number,
                        String::new(),
                    )
                })
                .collect(),
            rows: None,
            notes: vec![],
            header_row: None,
            prep: true,
        };
        cat.tables.push(mk("s1/orders", "orders", &["store_id"]));
        cat.tables.push(mk("s2/stores", "stores", &["id"]));
        let r = rel("r1", ("s1/orders", "store_id"), ("s2/stores", "id"));
        let files = |id: &str| match id {
            "s2" => Some(("stores.csv".to_string(), 1)),
            "s1" => Some(("orders.xlsx".to_string(), 3)),
            _ => None,
        };
        let mut h = LinkHealth {
            one_rows: 10,
            one_dups: 1,
            one_keys: 1,
            sample: vec![serde_json::json!(7)],
            sample_rows: vec![2],
            many_dups: 0,
        };
        assert_eq!(
            note(&r, &h, &cat, &files).unwrap(),
            "stores.csv has 2 rows with id 7. Tenun used the first. Fix the file to remove this note."
        );
        h.one_rows = 100;
        h.one_dups = 7;
        h.one_keys = 7;
        h.sample = vec![1, 2, 3, 4, 5]
            .into_iter()
            .map(|n| serde_json::json!(n))
            .collect();
        h.sample_rows = vec![2; 5];
        assert_eq!(
            note(&r, &h, &cat, &files).unwrap(),
            "stores.csv repeats id: 1, 2, 3, 4, 5 and 2 more (7 rows too many). Tenun used the first row of each. Fix the file to remove this note."
        );
        h.one_keys = 3;
        h.one_dups = 3;
        h.sample.truncate(3);
        assert_eq!(
            note(&r, &h, &cat, &files).unwrap(),
            "stores.csv repeats id: 1, 2 and 3 (3 rows too many). Tenun used the first row of each. Fix the file to remove this note."
        );
        // Mostly repeated, and the many side repeats too: not used.
        h.many_dups = 4;
        h.one_rows = 4;
        h.one_dups = 3;
        assert_eq!(
            note(&r, &h, &cat, &files).unwrap(),
            "orders in orders.xlsx and stores.csv both repeat the values of this link (store_id and id), so Tenun cannot tell which row to use. The link is not used until one of them is fixed."
        );
        assert!(note(&r, &LinkHealth::default(), &cat, &files).is_none());
    }
}
