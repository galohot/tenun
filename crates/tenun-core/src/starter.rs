//! Starter dashboards (0.7 build 04, AD-D8): a dashboard made for a kind of data (sales, a budget,
//! stock, attendance, a survey) with **roles** where the columns go. The engine guesses which
//! column plays each role; the person confirms or changes each guess on one screen; then the roles
//! become the real columns of their table.
//!
//! A starter is a JSON file in `starters/`: its roles, and one dashboard whose fields name the
//! table `@table` and the columns `@<role>`. Each starter has a small sample CSV beside it.

use crate::catalog::{Column, DType, Table, ident};
use crate::suggest::{norm, singular};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The starters, in the order the app shows them: id, definition, sample.
const FILES: &[(&str, &str, &str)] = &[
    (
        "sales",
        include_str!("../starters/sales.json"),
        include_str!("../starters/sales.csv"),
    ),
    (
        "budget",
        include_str!("../starters/budget.json"),
        include_str!("../starters/budget.csv"),
    ),
    (
        "stock",
        include_str!("../starters/stock.json"),
        include_str!("../starters/stock.csv"),
    ),
    (
        "attendance",
        include_str!("../starters/attendance.json"),
        include_str!("../starters/attendance.csv"),
    ),
    (
        "survey",
        include_str!("../starters/survey.json"),
        include_str!("../starters/survey.csv"),
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoleKind {
    Date,
    Number,
    Text,
}

impl RoleKind {
    /// The column types that can play a role of this kind. A text role groups, and any column
    /// can group, so it takes every type; the guess still prefers text.
    pub fn accepts(self, t: DType) -> bool {
        match self {
            RoleKind::Date => t.is_temporal(),
            RoleKind::Number => t == DType::Number,
            RoleKind::Text => true,
        }
    }

    fn words(self) -> &'static str {
        match self {
            RoleKind::Date => "dates",
            RoleKind::Number => "numbers",
            RoleKind::Text => "text",
        }
    }
}

/// How many different values a text role usually has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    /// A few groups: products, regions, statuses.
    Few,
    /// Many: people, items.
    Many,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// The id the dashboard uses: `@<name>`.
    pub name: String,
    /// What the app calls it (in English; the app translates it).
    pub label: String,
    pub kind: RoleKind,
    pub required: bool,
    /// One line that says what goes here.
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<Shape>,
    /// Column names that usually play this role, in English and Indonesian. The guess reads them.
    #[serde(default, skip_serializing)]
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Starter {
    pub id: String,
    pub name: String,
    pub description: String,
    /// The file name of the sample.
    pub sample: String,
    pub roles: Vec<Role>,
    /// The dashboard, with `@table` and `@<role>` in place of the table and the columns.
    #[serde(skip_serializing)]
    pub dashboard: Value,
}

/// Every starter.
pub fn all() -> Vec<Starter> {
    FILES
        .iter()
        .map(|(id, json, _)| {
            serde_json::from_str(json).unwrap_or_else(|e| panic!("starter {id}: {e}"))
        })
        .collect()
}

pub fn get(id: &str) -> Option<Starter> {
    all().into_iter().find(|s| s.id == id)
}

/// The sample CSV of a starter.
pub fn sample(id: &str) -> Option<&'static str> {
    FILES
        .iter()
        .find(|(i, _, _)| *i == id)
        .map(|(_, _, csv)| *csv)
}

/// The column the engine guessed for one role, and how sure it is (0..1). No column: nothing fits.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Guess {
    pub role: String,
    pub column: Option<String>,
    pub score: f64,
}

/// How well a column name matches the names of a role: 1 the same word, less for a word inside the
/// other, 0 nothing. Names are compared as `suggest` compares them: letters and digits, lower case,
/// singular.
pub fn name_match(role: &Role, column: &Column) -> f64 {
    let shown = column.label.as_deref().unwrap_or(&column.name);
    let mut best: f64 = 0.0;
    for raw in [shown, column.name.as_str()] {
        let n = norm(raw);
        if n.is_empty() {
            continue;
        }
        let ns = singular(&n);
        for h in role.names.iter().chain(std::iter::once(&role.name)) {
            let h = norm(h);
            let score = if n == h || ns == h {
                1.0
            } else if h.len() >= 3 && (n.starts_with(&h) || n.ends_with(&h)) {
                0.8
            } else if h.len() >= 4 && n.contains(&h) {
                0.7
            } else if n.len() >= 4 && h.contains(&n) {
                0.5
            } else {
                0.0
            };
            best = best.max(score);
        }
    }
    best
}

/// Guesses a column of `table` for each role. `probe` runs one SQL statement and gives its first
/// value as a number (the number of different values of a text column).
///
/// Each column plays at most one role. The best pairs are taken first, so a column called `budget`
/// goes to Budget even when Actual would also take a number. A required role takes any column of the
/// right type when no name matches, because the person checks it next; an optional role is only
/// guessed from its name, so an unrelated column never fills it by chance.
pub fn guess(
    roles: &[Role],
    table: &Table,
    probe: &dyn Fn(&str) -> anyhow::Result<f64>,
) -> Vec<Guess> {
    let mut distinct: BTreeMap<String, Option<f64>> = BTreeMap::new();
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    let rows = table.rows.map(|r| r as f64);
    for (ri, role) in roles.iter().enumerate() {
        for (ci, col) in table.columns.iter().enumerate() {
            if col.hidden || col.error.is_some() {
                continue;
            }
            let fits = match role.kind {
                RoleKind::Text => matches!(col.dtype, DType::Text | DType::Bool),
                k => k.accepts(col.dtype),
            };
            if !fits {
                continue;
            }
            let name = name_match(role, col);
            if name == 0.0 && (!role.required || (role.kind == RoleKind::Number && id_like(col))) {
                continue;
            }
            // The data: a column that is mostly blank fits less, and a text role prefers a column with
            // as many different values as it expects.
            let mut data = 1.0 - col.blank.unwrap_or(0.0);
            if col.mixed {
                data *= 0.7;
            }
            // A column with too many different values for a role that groups (or too few for one that
            // names each row) fits much less, whatever its name: a bar per row is no chart.
            let mut fit = 1.0;
            if let Some(shape) = role.shape {
                let d = *distinct.entry(col.name.clone()).or_insert_with(|| {
                    probe(&format!(
                        "SELECT count(DISTINCT {}) FROM {}",
                        ident(&col.name),
                        table.sql_ref
                    ))
                    .ok()
                });
                if let Some(d) = d {
                    // One value for every row groups nothing.
                    if d < 2.0 {
                        continue;
                    }
                    let few = d <= 60.0 || rows.is_some_and(|r| d <= r * 0.2);
                    fit = match shape {
                        Shape::Few if few => 1.0,
                        Shape::Many if d >= 3.0 => 1.0,
                        _ => 0.4,
                    };
                }
            }
            let score = (0.3 + 0.55 * name + 0.15 * data) * fit;
            pairs.push((score, ri, ci));
        }
    }
    // Best first; on a tie, the role and the column that come first.
    pairs.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut out: Vec<Guess> = roles
        .iter()
        .map(|r| Guess {
            role: r.name.clone(),
            column: None,
            score: 0.0,
        })
        .collect();
    let mut used = BTreeSet::new();
    for (score, ri, ci) in pairs {
        if out[ri].column.is_some() || used.contains(&ci) {
            continue;
        }
        used.insert(ci);
        out[ri].column = Some(table.columns[ci].name.clone());
        out[ri].score = (score * 100.0).round() / 100.0;
    }
    out
}

/// A column that numbers the rows (`id`, `order_id`, `No`, `kode`): never an amount by chance.
fn id_like(col: &Column) -> bool {
    let n = norm(&col.name);
    n == "id"
        || n.ends_with("id")
        || ["no", "nr", "nomor", "number", "kode", "code", "key"].contains(&n.as_str())
}

/// The dashboard of `starter` on `table`, with the column of each role from `mapping` (role → column).
///
/// Fails, in words, when a required role has no column, when a column is not in the table, or when
/// a column holds the wrong kind of value; nothing is made then. A tile that needs an optional role
/// with no column is left out.
pub fn apply(
    starter: &Starter,
    table: &Table,
    mapping: &BTreeMap<String, String>,
    id: &str,
    title: &str,
) -> anyhow::Result<Value> {
    let mut cols: BTreeMap<String, String> = BTreeMap::new();
    for role in &starter.roles {
        let Some(c) = mapping
            .get(&role.name)
            .map(|c| c.trim())
            .filter(|c| !c.is_empty())
        else {
            if role.required {
                anyhow::bail!(
                    "Choose a column for {}. The {} starter needs it.",
                    role.label,
                    starter.name
                );
            }
            continue;
        };
        let col = table
            .column(c)
            .ok_or_else(|| anyhow::anyhow!("There is no column {c} in {}.", table.label))?;
        if !role.kind.accepts(col.dtype) {
            anyhow::bail!(
                "{} needs a column of {}, and {c} holds {}.",
                role.label,
                role.kind.words(),
                dtype_words(col.dtype)
            );
        }
        cols.insert(role.name.clone(), c.to_string());
    }
    let mut dash = starter.dashboard.clone();
    let tiles = dash
        .get_mut("tiles")
        .and_then(|t| t.as_array_mut())
        .map(std::mem::take)
        .unwrap_or_default();
    let mut kept = Vec::new();
    for mut tile in tiles {
        if fill(&mut tile, &table.id, &cols) {
            kept.push(tile);
        }
    }
    dash["tiles"] = Value::Array(kept);
    dash["id"] = Value::String(id.to_string());
    dash["title"] = Value::String(title.to_string());
    Ok(dash)
}

fn dtype_words(t: DType) -> &'static str {
    match t {
        DType::Number => "numbers",
        DType::Date | DType::DateTime => "dates",
        DType::Text => "text",
        DType::Bool => "yes and no",
    }
}

/// Puts the table and the columns in place of `@table` and `@<role>`. False when the value names a
/// role that has no column.
fn fill(v: &mut Value, table: &str, cols: &BTreeMap<String, String>) -> bool {
    match v {
        Value::String(s) if s.starts_with('@') => {
            let key = &s[1..];
            let to = if key == "table" {
                Some(table.to_string())
            } else {
                cols.get(key).cloned()
            };
            match to {
                Some(t) => {
                    *s = t;
                    true
                }
                None => false,
            }
        }
        Value::Array(a) => a.iter_mut().all(|x| fill(x, table, cols)),
        Value::Object(o) => o.values_mut().all(|x| fill(x, table, cols)),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(n: &str, t: DType) -> Column {
        Column::new(n.into(), t, String::new())
    }

    #[test]
    fn every_starter_reads_and_names_only_its_roles() {
        let list = all();
        assert_eq!(
            list.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["sales", "budget", "stock", "attendance", "survey"]
        );
        for s in &list {
            assert!(sample(&s.id).is_some_and(|c| c.lines().count() > 50));
            let names: BTreeSet<&str> = s.roles.iter().map(|r| r.name.as_str()).collect();
            let mut used = BTreeSet::new();
            fn walk(v: &Value, out: &mut BTreeSet<String>) {
                match v {
                    Value::String(s) if s.starts_with('@') => {
                        out.insert(s[1..].to_string());
                    }
                    Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
                    Value::Object(o) => o.values().for_each(|x| walk(x, out)),
                    _ => {}
                }
            }
            walk(&s.dashboard, &mut used);
            for u in &used {
                assert!(u == "table" || names.contains(u.as_str()), "{}: @{u}", s.id);
            }
            for r in &s.roles {
                assert!(used.contains(&r.name), "{}: {} is not used", s.id, r.name);
            }
        }
    }

    #[test]
    fn names_match_by_word() {
        let s = get("sales").unwrap();
        let amount = s.roles.iter().find(|r| r.name == "amount").unwrap();
        assert_eq!(name_match(amount, &col("Amount", DType::Number)), 1.0);
        assert_eq!(name_match(amount, &col("Total (IDR)", DType::Number)), 0.8);
        assert_eq!(
            name_match(amount, &col("Nilai_Penjualan", DType::Number)),
            0.8
        );
        assert_eq!(name_match(amount, &col("x", DType::Number)), 0.0);
    }
}
