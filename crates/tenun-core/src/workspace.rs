//! The `.tenun` workspace file: sources, relationships and dashboards.
//!
//! The file is meant to be shared and committed to git, so it never holds a
//! password. Database passwords live in the [`crate::secrets`] store.

use crate::catalog::DType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// How each table is prepared: header row, column overrides, unpivot. The key is the table id
    /// (`<source id>/<table name>`). Not written until a table is set up (V3-D11).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tables: BTreeMap<String, TableSetup>,
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
            tables: BTreeMap::new(),
        }
    }
}

/// What the user changed about one table. Everything is optional; an empty setup is dropped.
/// The raw data is never touched: the changes live in the table's view.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TableSetup {
    /// Excel: the row (as numbered in the sheet) that holds the column names. Overrides detection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_row: Option<u32>,
    /// By real column name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub columns: BTreeMap<String, ColumnSetup>,
    /// Applied in order, before the column types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transforms: Vec<Transform>,
    /// Calculated columns (V4-D3): formulas that become real columns at the end of the view, in
    /// order. A column may use an earlier one, never a later one. Not written until one is added
    /// (V3-D11), so a file made before this field existed is written back as it was.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calc: Vec<CalcColumn>,
}

/// A column that is computed from the others, in the view of its table (V4-D3).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CalcColumn {
    /// The name of the column. It is also its real name: a chart that uses the column keeps it.
    /// It must not be a column of the table, nor the name of another calculated column.
    pub name: String,
    /// A formula in the language of build 01, or raw DuckDB SQL when `sql` is set.
    pub formula: String,
    /// The formula is one DuckDB expression over the columns of the table (Advanced, V4-D62).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sql: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl TableSetup {
    pub fn is_empty(&self) -> bool {
        self.header_row.is_none()
            && self.columns.is_empty()
            && self.transforms.is_empty()
            && self.calc.is_empty()
    }

    /// Drops overrides that say nothing, so a cleared menu leaves no trace in the file.
    pub fn tidy(&mut self) {
        for c in self.columns.values_mut() {
            if c.label.as_deref().is_some_and(|s| s.trim().is_empty()) {
                c.label = None;
            }
            if c.description
                .as_deref()
                .is_some_and(|s| s.trim().is_empty())
            {
                c.description = None;
            }
            if c.hidden == Some(false) {
                c.hidden = None;
            }
            if c.dtype.is_none() {
                c.date_format = None;
            }
            if c.date_format == Some(DateFormat::Auto) {
                c.date_format = None;
            }
        }
        self.columns.retain(|_, c| *c != ColumnSetup::default());
        for c in &mut self.calc {
            c.name = c.name.trim().to_string();
            c.formula = c.formula.trim().to_string();
            if c.description
                .as_deref()
                .is_some_and(|s| s.trim().is_empty())
            {
                c.description = None;
            }
        }
    }

    /// True when the view of the table (not only its labels) depends on this setup.
    pub fn changes_view(&self) -> bool {
        !self.transforms.is_empty() || self.columns.values().any(|c| c.dtype.is_some())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ColumnSetup {
    /// The name shown in the app. Queries keep the real name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Read the column as this type. Cells that cannot be read become blank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dtype: Option<DType>,
    /// How to read dates written as text. Only used with `dtype` date or datetime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_format: Option<DateFormat>,
    /// Out of the field pickers. Still there for relationships.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// A tooltip in the field pickers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateFormat {
    Auto,
    /// 31/12/2026
    Dmy,
    /// 12/31/2026
    Mdy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Transform {
    /// Columns become rows: the `keep` columns stay, every other column turns into one row per
    /// value, with its name in `name` and its value in `value`.
    Unpivot {
        keep: Vec<String>,
        name: String,
        value: String,
    },
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
    /// A folder of CSV and Excel files. Files of the same shape stack into one table with a
    /// `file` column. `pattern` is a list of file name patterns, separated by `;`.
    Folder {
        path: String,
        #[serde(default = "default_pattern")]
        pattern: String,
    },
    /// CSV or JSON data on the web, fetched into a CSV cache file next to the workspace. The key
    /// holds only the place and the name: its value is kept outside the file ([`crate::secrets`]).
    Url {
        url: String,
        #[serde(default)]
        every: Every,
        /// JSON: where the list of records sits, like `data.items` or `result.0.rows`. `.` is the
        /// answer itself. Empty: Tenun picks the longest list.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        records: Option<String>,
        /// JSON: how to reach the next page. Empty: one page.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pages: Option<Paging>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<LinkKey>,
    },
    Postgres(DbConn),
    Mysql(DbConn),
}

/// How a JSON link finds its next page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "by", rename_all = "lowercase")]
pub enum Paging {
    /// The `Link` header with `rel="next"`.
    Link,
    /// A field of the answer that holds the next link, complete or relative.
    Field { path: String },
    /// A field that holds a token. The token goes back as the link parameter `param`.
    Cursor { path: String, param: String },
    /// A page number in the link parameter `param`. It starts at the number in the link, or 1.
    Page { param: String },
}

/// Where a link sends its key.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeyPlace {
    Header,
    Query,
}

impl KeyPlace {
    pub fn label(self) -> &'static str {
        match self {
            KeyPlace::Header => "header",
            KeyPlace::Query => "query",
        }
    }
}

/// The place and name of a link's key. The value is never part of the workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkKey {
    pub place: KeyPlace,
    pub name: String,
}

/// The file name patterns a new folder source reads.
pub const DEFAULT_PATTERN: &str = "*.csv;*.xlsx";

fn default_pattern() -> String {
    DEFAULT_PATTERN.into()
}

/// How often a web link is fetched again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Every {
    #[serde(rename = "15min")]
    Min15,
    #[default]
    Hourly,
    Daily,
    Manual,
}

impl Every {
    /// Seconds between two fetches; `None` for manual.
    pub fn secs(self) -> Option<u64> {
        match self {
            Every::Min15 => Some(15 * 60),
            Every::Hourly => Some(60 * 60),
            Every::Daily => Some(24 * 60 * 60),
            Every::Manual => None,
        }
    }
}

impl SourceKind {
    /// The path of a single data file. A folder is not a file: see [`SourceKind::folder`].
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

    /// The path and name patterns of a folder source.
    pub fn folder(&self) -> Option<(&str, &str)> {
        match self {
            SourceKind::Folder { path, pattern } => Some((path, pattern)),
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
            SourceKind::Folder { .. } => "folder",
            SourceKind::Url { .. } => "url",
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
    /// A name for the link ("Shipped from"). When two links join the same two tables each one
    /// has a name, so a field and a formula can say which link they mean (V4-D28). Not written
    /// until the link is named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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
    fn folder_and_link_sources_round_trip_with_their_defaults() {
        let ws: Workspace = serde_json::from_str(
            r#"{"version":1,"name":"x","sources":[
                {"id":"s1","name":"sales","kind":"folder","path":"sales"},
                {"id":"s2","name":"web","kind":"url","url":"https://example.com/a.csv"},
                {"id":"s3","name":"web","kind":"url","url":"https://example.com/b.csv","every":"15min"}]}"#,
        )
        .unwrap();
        assert_eq!(
            ws.sources[0].kind.folder(),
            Some(("sales", DEFAULT_PATTERN))
        );
        assert!(matches!(
            &ws.sources[1].kind,
            SourceKind::Url {
                every: Every::Hourly,
                ..
            }
        ));
        assert!(matches!(
            &ws.sources[2].kind,
            SourceKind::Url {
                every: Every::Min15,
                ..
            }
        ));
        let back: Workspace = serde_json::from_str(&serde_json::to_string(&ws).unwrap()).unwrap();
        assert_eq!(ws, back);
    }

    #[test]
    fn a_json_link_keeps_its_settings_and_an_old_link_is_written_as_before() {
        // An old link has none of the new fields and is written back without them.
        let old = r#"{"id":"s1","name":"web","kind":"url","url":"https://example.com/a.csv","every":"hourly"}"#;
        let src: Source = serde_json::from_str(old).unwrap();
        assert_eq!(serde_json::to_string(&src).unwrap(), old);
        // The new fields round-trip. The key holds a place and a name, never a value.
        let text = r#"{"id":"s2","name":"api","kind":"url","url":"https://example.com/items","every":"daily","records":"data.items","pages":{"by":"cursor","path":"next_cursor","param":"cursor"},"key":{"place":"header","name":"Authorization"}}"#;
        let src: Source = serde_json::from_str(text).unwrap();
        let SourceKind::Url {
            records,
            pages,
            key,
            ..
        } = &src.kind
        else {
            panic!("a link");
        };
        assert_eq!(records.as_deref(), Some("data.items"));
        assert_eq!(
            pages,
            &Some(Paging::Cursor {
                path: "next_cursor".into(),
                param: "cursor".into()
            })
        );
        assert_eq!(
            key,
            &Some(LinkKey {
                place: KeyPlace::Header,
                name: "Authorization".into()
            })
        );
        assert_eq!(serde_json::to_string(&src).unwrap(), text);
        for p in [
            r#"{"by":"link"}"#,
            r#"{"by":"field","path":"paging.next"}"#,
            r#"{"by":"page","param":"page"}"#,
        ] {
            let paging: Paging = serde_json::from_str(p).unwrap();
            assert_eq!(serde_json::to_string(&paging).unwrap(), p);
        }
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
