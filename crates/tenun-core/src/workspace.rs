//! The `.tenun` workspace file: sources, relationships and dashboards.
//!
//! The file is meant to be shared and committed to git, so it never holds a
//! password. Database passwords live in the [`crate::secrets`] store.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE_VERSION: u32 = 1;

/// How numbers and dates read in this workspace. Only numbers and dates change;
/// the app itself stays in English. A file without a locale reads as English.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    Id,
    #[default]
    En,
}

impl Locale {
    /// `id` for a language tag that starts with "id" ("id", "id-ID", "id_ID.UTF-8"), else `en`.
    pub fn guess(lang: &str) -> Self {
        if lang.trim().to_ascii_lowercase().starts_with("id") {
            Locale::Id
        } else {
            Locale::En
        }
    }

    /// The language of the operating system, read from the usual environment variables.
    pub fn from_env() -> Self {
        for key in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
            if let Ok(v) = std::env::var(key)
                && !v.is_empty()
            {
                return Self::guess(&v);
            }
        }
        Locale::En
    }
}

/// A locale the file names but this version does not know reads as "not set".
fn lenient_locale<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Locale>, D::Error> {
    let v = Option::<serde_json::Value>::deserialize(d)?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Workspace {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub sources: Vec<Source>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
    /// Dashboards are owned by the UI; the core only stores them.
    #[serde(default)]
    pub dashboards: Vec<serde_json::Value>,
    /// Model-view node positions and other UI state worth keeping.
    #[serde(default)]
    pub layout: serde_json::Value,
    /// Number and date format. Not written until it is set (V3-D11).
    #[serde(
        default,
        deserialize_with = "lenient_locale",
        skip_serializing_if = "Option::is_none"
    )]
    pub locale: Option<Locale>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            name: "Untitled".into(),
            sources: Vec::new(),
            relationships: Vec::new(),
            dashboards: Vec::new(),
            layout: serde_json::Value::Null,
            locale: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Source {
    pub id: String,
    /// Display name, e.g. the file name or the database name.
    pub name: String,
    #[serde(flatten)]
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SourceKind {
    /// .xlsx / .xlsm / .xls / .ods — every sheet becomes a table.
    Excel {
        path: String,
    },
    /// .csv / .tsv / .txt
    Csv {
        path: String,
    },
    Parquet {
        path: String,
    },
    Sqlite {
        path: String,
    },
    /// A DuckDB database file, attached read-only.
    Duckdb {
        path: String,
    },
    Postgres(DbConn),
    Mysql(DbConn),
}

impl SourceKind {
    pub fn path(&self) -> Option<&str> {
        match self {
            SourceKind::Excel { path }
            | SourceKind::Csv { path }
            | SourceKind::Parquet { path }
            | SourceKind::Sqlite { path }
            | SourceKind::Duckdb { path } => Some(path),
            _ => None,
        }
    }

    pub fn is_file(&self) -> bool {
        self.path().is_some()
    }

    pub fn label(&self) -> &'static str {
        match self {
            SourceKind::Excel { .. } => "excel",
            SourceKind::Csv { .. } => "csv",
            SourceKind::Parquet { .. } => "parquet",
            SourceKind::Sqlite { .. } => "sqlite",
            SourceKind::Duckdb { .. } => "duckdb",
            SourceKind::Postgres(_) => "postgres",
            SourceKind::Mysql(_) => "mysql",
        }
    }
}

/// Connection details without the password.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DbConn {
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    pub database: String,
    pub user: String,
    /// Postgres only; defaults to every schema the user can see.
    #[serde(default)]
    pub schema: Option<String>,
}

/// Many-to-one: many rows of `from` point at one row of `to`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Relationship {
    pub id: String,
    pub from: FieldRef,
    pub to: FieldRef,
}

/// A column of a table. `table` is a stable table id: `<source id>/<table name>`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FieldRef {
    pub table: String,
    pub column: String,
}

impl Workspace {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let ws: Workspace = serde_json::from_str(&text)?;
        if ws.version > FILE_VERSION {
            anyhow::bail!(
                "this file was made by a newer Tenun (file version {}); please update",
                ws.version
            );
        }
        Ok(ws)
    }

    /// Writes atomically (temp file + rename) so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("tenun.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn source(&self, id: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    pub fn next_id(&self, prefix: &str) -> String {
        let used: Vec<&str> = self
            .sources
            .iter()
            .map(|s| s.id.as_str())
            .chain(self.relationships.iter().map(|r| r.id.as_str()))
            .collect();
        (1..)
            .map(|n| format!("{prefix}{n}"))
            .find(|id| !used.contains(&id.as_str()))
            .expect("infinite range")
    }
}

/// Resolves a stored path against the folder of the workspace file.
pub fn resolve_path(stored: &str, base: Option<&Path>) -> PathBuf {
    let p = PathBuf::from(stored);
    match base {
        Some(dir) if p.is_relative() => dir.join(p),
        _ => p,
    }
}

/// Stores a path relative to the workspace folder when the file sits inside it,
/// so a folder with the `.tenun` file and its data can be moved as one.
pub fn store_path(abs: &Path, base: Option<&Path>) -> String {
    if let Some(dir) = base
        && let Ok(rel) = abs.strip_prefix(dir)
    {
        return rel.to_string_lossy().replace('\\', "/");
    }
    abs.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_keeps_no_password_field() {
        let mut ws = Workspace::default();
        ws.sources.push(Source {
            id: "s1".into(),
            name: "sales.xlsx".into(),
            kind: SourceKind::Excel {
                path: "data/sales.xlsx".into(),
            },
        });
        ws.sources.push(Source {
            id: "s2".into(),
            name: "shop".into(),
            kind: SourceKind::Postgres(DbConn {
                host: "localhost".into(),
                port: Some(5432),
                database: "shop".into(),
                user: "me".into(),
                schema: None,
            }),
        });
        let json = serde_json::to_string(&ws).unwrap();
        assert!(!json.to_lowercase().contains("password"));
        let back: Workspace = serde_json::from_str(&json).unwrap();
        assert_eq!(ws, back);
    }

    #[test]
    fn locale_is_optional_and_kept() {
        // An old file has none, reads as English and is written back without one.
        let old: Workspace = serde_json::from_str(r#"{"version":1,"name":"x"}"#).unwrap();
        assert_eq!(old.locale, None);
        assert!(!serde_json::to_string(&old).unwrap().contains("locale"));
        let ws: Workspace =
            serde_json::from_str(r#"{"version":1,"name":"x","locale":"id"}"#).unwrap();
        assert_eq!(ws.locale, Some(Locale::Id));
        let back: Workspace = serde_json::from_str(&serde_json::to_string(&ws).unwrap()).unwrap();
        assert_eq!(back.locale, Some(Locale::Id));
        // A value this version does not know is ignored, not an error.
        let odd: Workspace =
            serde_json::from_str(r#"{"version":1,"name":"x","locale":"fr"}"#).unwrap();
        assert_eq!(odd.locale, None);
    }

    #[test]
    fn language_tags_pick_a_locale() {
        assert_eq!(Locale::guess("id"), Locale::Id);
        assert_eq!(Locale::guess("id-ID"), Locale::Id);
        assert_eq!(Locale::guess("id_ID.UTF-8"), Locale::Id);
        assert_eq!(Locale::guess("en-US"), Locale::En);
        assert_eq!(Locale::guess("C"), Locale::En);
        assert_eq!(Locale::guess(""), Locale::En);
    }

    #[test]
    fn relative_paths() {
        let base = Path::new("/home/a/proj");
        assert_eq!(
            store_path(Path::new("/home/a/proj/data/x.xlsx"), Some(base)),
            "data/x.xlsx"
        );
        assert_eq!(
            store_path(Path::new("/tmp/x.csv"), Some(base)),
            "/tmp/x.csv"
        );
        assert_eq!(
            resolve_path("data/x.xlsx", Some(base)),
            PathBuf::from("/home/a/proj/data/x.xlsx")
        );
    }

    #[test]
    fn next_id_skips_used() {
        let mut ws = Workspace::default();
        ws.sources.push(Source {
            id: "s1".into(),
            name: "a".into(),
            kind: SourceKind::Csv { path: "a".into() },
        });
        assert_eq!(ws.next_id("s"), "s2");
    }
}
