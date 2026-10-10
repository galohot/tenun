//! Finds likely relationships so the user only has to click "Accept".
//!
//! A candidate needs a name that looks like a key (`store_id` → `stores.id`,
//! or the same key-like name on both sides). Then the data must agree: the
//! target column is unique, and at least 90 % of a sample of source values
//! exist in it.

use crate::catalog::{Catalog, Column, DType, Table, ident};
use crate::workspace::{FieldRef, Relationship};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub from: FieldRef,
    pub to: FieldRef,
    /// 0..1; shown as "likely" / "possible".
    pub score: f64,
    pub reason: String,
}

const MIN_OVERLAP: f64 = 0.9;

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn singular(s: &str) -> String {
    if let Some(x) = s.strip_suffix("ies") {
        format!("{x}y")
    } else if let Some(x) = s.strip_suffix("ses") {
        format!("{x}s")
    } else if let Some(x) = s.strip_suffix('s') {
        x.to_string()
    } else {
        s.to_string()
    }
}

const KEY_WORDS: &[&str] = &["id", "code", "key", "no", "number", "num", "sku"];

fn key_like(n: &str) -> bool {
    KEY_WORDS.iter().any(|k| n.ends_with(k))
}

/// Name score for "column `a` of table A points at column `b` of table B".
pub fn name_score(a: &Column, b: &Column, b_table: &Table) -> Option<(f64, String)> {
    let (an, bn) = (norm(&a.name), norm(&b.name));
    let tn = norm(b_table.name.rsplit('.').next().unwrap_or(&b_table.name));
    let ts = singular(&tn);
    if KEY_WORDS.contains(&bn.as_str()) && (an == format!("{ts}{bn}") || an == format!("{tn}{bn}"))
    {
        return Some((
            0.95,
            format!("{} looks like a key of {}", a.name, b_table.label),
        ));
    }
    if an == bn && key_like(&an) {
        return Some((0.85, format!("both are called {}", a.name)));
    }
    if (an == ts || an == tn)
        && (bn == ts || bn == tn || bn == "name" || KEY_WORDS.contains(&bn.as_str()))
    {
        return Some((
            0.7,
            format!("{} matches the table {}", a.name, b_table.label),
        ));
    }
    None
}

fn compatible(a: DType, b: DType) -> bool {
    a == b
        || matches!(
            (a, b),
            (DType::Number, DType::Text) | (DType::Text, DType::Number)
        )
}

/// `probe` runs one SQL statement and returns the first value as a number.
pub fn suggest(
    catalog: &Catalog,
    existing: &[Relationship],
    probe: &dyn Fn(&str) -> anyhow::Result<f64>,
) -> anyhow::Result<Vec<Suggestion>> {
    let related = |x: &str, y: &str| {
        existing.iter().any(|r| {
            (r.from.table == x && r.to.table == y) || (r.from.table == y && r.to.table == x)
        })
    };
    let mut out: Vec<Suggestion> = Vec::new();
    for a_t in &catalog.tables {
        for b_t in &catalog.tables {
            if a_t.id == b_t.id || related(&a_t.id, &b_t.id) {
                continue;
            }
            for a in &a_t.columns {
                for b in &b_t.columns {
                    if !compatible(a.dtype, b.dtype) || a.dtype.is_temporal() {
                        continue;
                    }
                    let Some((score, reason)) = name_score(a, b, b_t) else {
                        continue;
                    };
                    let (ar, br) = (ident(&a.name), ident(&b.name));
                    // The one side must be unique.
                    let dupes = probe(&format!(
                        "SELECT count({br}) - count(DISTINCT {br}) FROM {}",
                        b_t.sql_ref
                    ))?;
                    let filled = probe(&format!("SELECT count({br}) FROM {}", b_t.sql_ref))?;
                    if dupes > 0.0 || filled == 0.0 {
                        continue;
                    }
                    let overlap = probe(&format!(
                        "SELECT coalesce(avg(CASE WHEN b.k IS NULL THEN 0 ELSE 1 END), 0) FROM \
                         (SELECT DISTINCT CAST({ar} AS VARCHAR) AS k FROM {} WHERE {ar} IS NOT NULL LIMIT 2000) a \
                         LEFT JOIN (SELECT DISTINCT CAST({br} AS VARCHAR) AS k FROM {}) b ON a.k = b.k",
                        a_t.sql_ref, b_t.sql_ref
                    ))?;
                    if overlap < MIN_OVERLAP {
                        continue;
                    }
                    out.push(Suggestion {
                        from: FieldRef {
                            table: a_t.id.clone(),
                            column: a.name.clone(),
                        },
                        to: FieldRef {
                            table: b_t.id.clone(),
                            column: b.name.clone(),
                        },
                        score: score * overlap,
                        reason,
                    });
                }
            }
        }
    }
    // One suggestion per pair of tables: the best one. A 1:1 pair shows once.
    out.sort_by(|x, y| y.score.total_cmp(&x.score));
    let mut kept: Vec<Suggestion> = Vec::new();
    for s in out {
        let dup = kept.iter().any(|k| {
            (k.from.table == s.from.table && k.to.table == s.to.table)
                || (k.from.table == s.to.table && k.to.table == s.from.table)
        });
        if !dup {
            kept.push(s);
        }
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(n: &str) -> Column {
        Column::new(n.into(), DType::Number, String::new())
    }
    fn t(n: &str) -> Table {
        Table {
            id: format!("s/{n}"),
            source_id: "s".into(),
            name: n.into(),
            label: n.into(),
            sql_ref: String::new(),
            columns: vec![],
            rows: None,
            notes: vec![],
            header_row: None,
            prep: false,
        }
    }

    #[test]
    fn names() {
        assert!(
            name_score(&c("store_id"), &c("id"), &t("stores"))
                .unwrap()
                .0
                > 0.9
        );
        assert!(name_score(&c("Category ID"), &c("id"), &t("categories")).is_some());
        assert!(name_score(&c("customer_id"), &c("customer_id"), &t("customers")).is_some());
        assert!(name_score(&c("amount"), &c("amount"), &t("x")).is_none());
        assert!(name_score(&c("name"), &c("name"), &t("x")).is_none());
        assert!(name_score(&c("store"), &c("store"), &t("stores")).is_some());
    }
}
