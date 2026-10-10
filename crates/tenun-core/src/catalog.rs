//! What the user can query: tables, their columns, and a simple type for each.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DType {
    Number,
    Date,
    DateTime,
    Text,
    Bool,
}

impl DType {
    /// Maps a DuckDB type name (as in information_schema) to a simple type.
    pub fn from_duckdb(t: &str) -> DType {
        let t = t.to_ascii_uppercase();
        if t.starts_with("TIMESTAMP") || t == "DATETIME" {
            DType::DateTime
        } else if t == "DATE" {
            DType::Date
        } else if t == "BOOLEAN" || t == "BOOL" {
            DType::Bool
        } else if [
            "TINYINT",
            "SMALLINT",
            "INTEGER",
            "INT",
            "BIGINT",
            "HUGEINT",
            "UTINYINT",
            "USMALLINT",
            "UINTEGER",
            "UBIGINT",
            "UHUGEINT",
            "FLOAT",
            "DOUBLE",
            "REAL",
        ]
        .contains(&t.as_str())
            || t.starts_with("DECIMAL")
            || t.starts_with("NUMERIC")
        {
            DType::Number
        } else {
            DType::Text
        }
    }

    pub fn is_temporal(self) -> bool {
        matches!(self, DType::Date | DType::DateTime)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Column {
    pub name: String,
    pub dtype: DType,
    /// The database's own type name, shown in previews.
    pub raw_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Table {
    /// Stable id used in relationships and queries: `<source id>/<name>`.
    pub id: String,
    pub source_id: String,
    pub name: String,
    /// Unique among all tables; `name` plus the source name when two collide.
    pub label: String,
    /// Fully quoted DuckDB reference, e.g. `"s_s1"."Orders"`.
    #[serde(skip)]
    pub sql_ref: String,
    pub columns: Vec<Column>,
    /// Known for local data; `None` for live databases (counting can be slow).
    pub rows: Option<i64>,
}

impl Table {
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Catalog {
    pub tables: Vec<Table>,
}

impl Catalog {
    pub fn table(&self, id: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.id == id)
    }

    /// Gives every table a unique label after the list changes.
    pub fn relabel(&mut self, source_names: &dyn Fn(&str) -> String) {
        let names: Vec<String> = self.tables.iter().map(|t| t.name.clone()).collect();
        for t in &mut self.tables {
            let dup = names.iter().filter(|n| **n == t.name).count() > 1;
            t.label = if dup {
                format!("{} ({})", t.name, source_names(&t.source_id))
            } else {
                t.name.clone()
            };
        }
    }
}

/// Quotes an SQL identifier.
pub fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Quotes an SQL string literal (used only where DuckDB takes no parameters,
/// such as `ATTACH` and view definitions).
pub fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types() {
        assert_eq!(DType::from_duckdb("DECIMAL(18,3)"), DType::Number);
        assert_eq!(
            DType::from_duckdb("TIMESTAMP WITH TIME ZONE"),
            DType::DateTime
        );
        assert_eq!(DType::from_duckdb("DATE"), DType::Date);
        assert_eq!(DType::from_duckdb("VARCHAR"), DType::Text);
        assert_eq!(DType::from_duckdb("UUID"), DType::Text);
    }

    #[test]
    fn quoting() {
        assert_eq!(ident("a\"b"), "\"a\"\"b\"");
        assert_eq!(lit("it's"), "'it''s'");
    }
}
