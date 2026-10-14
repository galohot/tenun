//! The names the app gives values (`ui/src/lib/format.ts`: `measureLabel`, `fieldLabel`): "Amount", "Number of
//! orders", "Average amount", "Amount per order". Words that leave the app — an alert, an email update, an answer to
//! an AI assistant — use the same names (v5 builds 12–14).

use crate::catalog::Catalog;
use crate::query::{Agg, Measure};
use crate::workspace::NamedMeasure;

/// "unit_price" → "Unit price". A name with a capital after its first letter stays as it is ("orderValue").
pub fn pretty(name: &str) -> String {
    let mut s = String::new();
    let mut gap = false;
    for c in name.chars() {
        if c == '_' {
            gap = true;
            continue;
        }
        if gap {
            s.push(' ');
            gap = false;
        }
        s.push(c);
    }
    let s = s.trim().to_string();
    if s.chars().skip(1).any(|c| c.is_ascii_uppercase()) {
        return s;
    }
    let mut cs = s.chars();
    match cs.next() {
        Some(first) => first.to_uppercase().chain(cs).collect(),
        None => s,
    }
}

/// The label the user gave a column, if any.
fn label_of(catalog: &Catalog, table: &str, column: &str) -> Option<String> {
    catalog
        .table(table)?
        .column(column)?
        .label
        .clone()
        .filter(|l| !l.trim().is_empty())
}

/// The name of a field: its label, else the column name made readable.
pub fn field_label(catalog: &Catalog, table: &str, column: &str) -> String {
    label_of(catalog, table, column).unwrap_or_else(|| pretty(column))
}

/// A field inside a sentence: its label as typed, else the column name in lower case.
fn field_word(catalog: &Catalog, table: &str, column: &str) -> String {
    label_of(catalog, table, column).unwrap_or_else(|| pretty(column).to_lowercase())
}

/// "customer_id" → "customer": what an id column counts.
fn id_base(column: &str) -> Option<String> {
    let lower = column.to_ascii_lowercase();
    let base = column.get(..column.len().checked_sub(2)?)?;
    if !lower.ends_with("id") {
        return None;
    }
    let base = base.strip_suffix([' ', '_', '-']).unwrap_or(base);
    (base.chars().count() > 1).then(|| pretty(base).to_lowercase())
}

fn agg_words(agg: Agg) -> &'static str {
    match agg {
        Agg::Sum => "Total",
        Agg::Avg => "Average",
        Agg::Min => "Lowest",
        Agg::Max => "Highest",
        Agg::Count => "Count of",
        Agg::CountDistinct => "Unique",
        Agg::Median => "Median",
        Agg::P90 => "90th percentile of",
        Agg::Stdev => "Spread of",
        Agg::Variance => "Variance of",
        Agg::Closing => "Closing",
        Agg::Opening => "Opening",
    }
}

/// "orders" → "order", "categories" → "category".
fn singular(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    if lower.ends_with("ies") {
        return format!("{}y", &s[..s.len() - 3]);
    }
    if lower.ends_with('s') {
        return s[..s.len() - 1].to_string();
    }
    s.to_string()
}

fn table_label(catalog: &Catalog, table: &str) -> Option<String> {
    catalog.table(table).map(|t| t.label.clone())
}

fn plain_label(catalog: &Catalog, named: &[NamedMeasure], m: &Measure) -> String {
    if let Some(id) = &m.named {
        return named
            .iter()
            .find(|n| &n.id == id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "A deleted measure".into());
    }
    let Some(column) = &m.column else {
        let t = table_label(catalog, &m.table).unwrap_or_else(|| "rows".into());
        return format!("Number of {}", t.to_lowercase());
    };
    let base = if label_of(catalog, &m.table, column).is_some() {
        None
    } else {
        id_base(column)
    };
    if let Some(base) = base {
        match m.agg {
            Agg::CountDistinct => return format!("Unique {base}s"),
            Agg::Count => return format!("Number of {base}s"),
            _ => {}
        }
    }
    if m.agg == Agg::Sum {
        return field_label(catalog, &m.table, column);
    }
    format!(
        "{} {}",
        agg_words(m.agg),
        field_word(catalog, &m.table, column)
    )
}

/// The name of a value, as the app shows it: "Amount", "Average amount", "Amount per order".
pub fn measure_label(catalog: &Catalog, named: &[NamedMeasure], m: &Measure) -> String {
    let top = plain_label(catalog, named, m);
    let Some(p) = &m.per else {
        return top;
    };
    let word = match &p.column {
        None if p.table != m.table => "row".to_string(),
        None => singular(
            &table_label(catalog, &p.table)
                .unwrap_or_else(|| "row".into())
                .to_lowercase(),
        ),
        Some(c) => plain_label(
            catalog,
            named,
            &Measure {
                table: p.table.clone(),
                column: Some(c.clone()),
                agg: p.agg,
                per: None,
                via: None,
                named: None,
            },
        )
        .to_lowercase(),
    };
    format!("{top} per {word}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_read_as_the_app_writes_them() {
        assert_eq!(pretty("unit_price"), "Unit price");
        assert_eq!(pretty("orderValue"), "orderValue");
        assert_eq!(pretty("__amount"), "Amount");
        assert_eq!(id_base("customer_id"), Some("customer".into()));
        assert_eq!(id_base("StoreID"), Some("store".into()));
        assert_eq!(id_base("id"), None);
        assert_eq!(id_base("paid"), Some("pa".into()));
        assert_eq!(singular("categories"), "category");
        assert_eq!(singular("orders"), "order");
        assert_eq!(singular("staff"), "staff");
    }
}
