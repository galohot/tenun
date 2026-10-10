//! The engine: one in-memory DuckDB, the open workspace, and its catalog.

use crate::catalog::{Catalog, Column, DType, Table, ident, lit};
use crate::excel::{self, Cell, ColType};
use crate::fetch;
use crate::folder;
use crate::paste;
use crate::prep::{self, ColInfo};
use crate::query::{self, Param, SemanticQuery};
use crate::secrets::Secrets;
use crate::suggest::{self, Suggestion};
use crate::workspace::{
    DEFAULT_PATTERN, DbConn, Every, FieldRef, Locale, Relationship, Source, SourceKind, TableSetup,
    Transform, Workspace, resolve_path, store_path,
};
use chrono::NaiveDate;
use duckdb::Connection;
use duckdb::types::Value;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime};

pub struct Engine {
    db: Mutex<Connection>,
    state: RwLock<State>,
    secrets: Secrets,
    /// Replaces today's date for relative date filters. Tests set it; the app never does.
    today: Mutex<Option<NaiveDate>>,
}

#[derive(Default)]
struct State {
    ws: Workspace,
    path: Option<PathBuf>,
    catalog: Catalog,
    /// Source id → the last load error, shown next to the source.
    errors: BTreeMap<String, String>,
    /// Source id → when it last loaded without error (ISO 8601, UTC).
    loaded_at: BTreeMap<String, String>,
    /// Source id → when a CSV link was last fetched (or tried), for its refresh schedule.
    fetched: BTreeMap<String, SystemTime>,
    dirty: bool,
}

/// Everything the UI needs to draw the workspace.
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub workspace: Workspace,
    pub path: Option<String>,
    pub tables: Vec<Table>,
    pub errors: BTreeMap<String, String>,
    /// Source id → when it last loaded without error (ISO 8601, UTC). Kept
    /// out of the source itself so it never reaches the `.tenun` file.
    pub loaded_at: BTreeMap<String, String>,
    pub dirty: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AddSource {
    /// Any supported file; the kind is chosen from the extension.
    File { path: String },
    /// A folder of CSV and Excel files that stack into one table.
    Folder {
        path: String,
        #[serde(default)]
        pattern: Option<String>,
    },
    /// A CSV file on the web (https only).
    Url {
        url: String,
        #[serde(default)]
        every: Every,
        #[serde(default)]
        name: Option<String>,
    },
    Postgres {
        #[serde(flatten)]
        conn: DbConn,
        #[serde(default)]
        password: String,
    },
    Mysql {
        #[serde(flatten)]
        conn: DbConn,
        #[serde(default)]
        password: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryResult {
    pub columns: Vec<query::OutColumn>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub ignored_filters: Vec<usize>,
    pub ms: u128,
    /// Number tiles: the figure and its fair comparison.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kpi: Option<crate::snapshot::KpiInfo>,
    /// Time charts: the last period is not over yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<crate::snapshot::Partial>,
    /// The last group is "Other": everything outside the top N (key `"\u0000other"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other: Option<bool>,
    /// The groups that are not in "Other", for a `not_in` filter.
    #[serde(rename = "otherOf", skip_serializing_if = "Option::is_none")]
    pub other_of: Option<Vec<serde_json::Value>>,
    /// Histograms: where the first bin starts and how wide every bin is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bins: Option<Bins>,
    /// Tables and pivots: totals computed with the same measures, so ratios and averages are
    /// right and never a sum of cells.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totals: Option<Totals>,
    /// Time charts with `tile.compare`: the same question one year (or one period) earlier, with
    /// its keys moved forward so they line up with the rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compare: Option<Compare>,
}

impl QueryResult {
    /// A result with these columns and rows and none of the extras.
    pub fn plain(columns: Vec<query::OutColumn>, rows: Vec<Vec<serde_json::Value>>) -> Self {
        Self {
            columns,
            rows,
            ignored_filters: vec![],
            ms: 0,
            kpi: None,
            partial: None,
            other: None,
            other_of: None,
            bins: None,
            totals: None,
            compare: None,
        }
    }
}

/// The comparison series of a time chart. `rows` have the shape of the result's rows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Compare {
    pub label: String,
    pub rows: Vec<Vec<serde_json::Value>>,
}

/// Totals of a table or pivot. Every row holds measure values only, in the order of the measures.
/// `grand` has no groups. `rows` (pivot) has one entry per group of the row fields: the row
/// values first, then the measures. `cols` (pivot) has one entry per column value: that value
/// first, then the measures. `cols` is also the list of the column values, in the engine's order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Totals {
    pub grand: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<Vec<serde_json::Value>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cols: Option<Vec<Vec<serde_json::Value>>>,
}

/// The bins of a histogram: bin `i` covers `[start + i·width, start + (i+1)·width)`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Bins {
    pub start: f64,
    pub width: f64,
}

#[derive(Debug, Serialize)]
pub struct Preview {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<serde_json::Value>>,
}

pub const FILE_EXTENSIONS: &[&str] = &[
    "xlsx", "xlsm", "xlsb", "xls", "ods", "csv", "tsv", "txt", "parquet", "sqlite", "sqlite3",
    "db", "duckdb", "tenun",
];

impl Engine {
    pub fn new(secrets: Secrets) -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        // Do not let a query wander into the network or the file system on its own.
        conn.execute_batch("SET autoinstall_known_extensions = false;")
            .ok();
        Ok(Self {
            db: Mutex::new(conn),
            state: RwLock::new(State::default()),
            secrets,
            today: Mutex::new(None),
        })
    }

    pub fn duckdb_version(&self) -> String {
        self.db
            .lock()
            .unwrap()
            .query_row("SELECT version()", [], |r| r.get::<_, String>(0))
            .unwrap_or_default()
    }

    pub fn snapshot(&self) -> Snapshot {
        let s = self.state.read().unwrap();
        Snapshot {
            workspace: s.ws.clone(),
            path: s.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            tables: s.catalog.tables.clone(),
            errors: s.errors.clone(),
            loaded_at: s.loaded_at.clone(),
            dirty: s.dirty,
        }
    }

    pub fn workspace_path(&self) -> Option<PathBuf> {
        self.state.read().unwrap().path.clone()
    }

    fn base_dir(&self) -> Option<PathBuf> {
        self.state
            .read()
            .unwrap()
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
    }

    /// Opens a `.tenun` file and loads every source. A source that fails to
    /// load is kept, with its error, so the user can fix it.
    pub fn open(&self, path: &Path) -> anyhow::Result<()> {
        let ws = Workspace::load(path)?;
        self.replace(ws, Some(path.to_path_buf()));
        Ok(())
    }

    /// A new workspace reads numbers and dates in the language of the computer.
    pub fn new_workspace(&self) {
        self.new_workspace_in(Locale::from_env());
    }

    pub fn new_workspace_in(&self, locale: Locale) {
        self.replace(
            Workspace {
                locale: Some(locale),
                ..Workspace::default()
            },
            None,
        );
    }

    /// The number and date format of the open workspace; English when it has none.
    pub fn locale(&self) -> Locale {
        self.state.read().unwrap().ws.locale.unwrap_or_default()
    }

    pub fn set_locale(&self, locale: Locale) {
        self.state.write().unwrap().ws.locale = Some(locale);
        self.persist();
    }

    /// Replaces the workspace (used by open, new and "open sample").
    pub fn replace(&self, ws: Workspace, path: Option<PathBuf>) {
        {
            let db = self.db.lock().unwrap();
            let s = self.state.read().unwrap();
            for src in &s.ws.sources {
                drop_source(&db, &src.id);
            }
        }
        {
            let mut s = self.state.write().unwrap();
            *s = State {
                ws: ws.clone(),
                path,
                ..State::default()
            };
        }
        for src in &ws.sources {
            self.load_with(&src.id, false);
        }
    }

    /// Saves to `path`, or to the current file. Returns the path written.
    pub fn save(&self, path: Option<PathBuf>) -> anyhow::Result<PathBuf> {
        let mut s = self.state.write().unwrap();
        let target = path
            .or_else(|| s.path.clone())
            .ok_or_else(|| anyhow::anyhow!("choose where to save the workspace"))?;
        // Moving the file changes the folder that relative paths are measured from.
        let old_base = s
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let new_base = target.parent().map(Path::to_path_buf);
        if old_base != new_base {
            for src in &mut s.ws.sources {
                rebase(&mut src.kind, old_base.as_deref(), new_base.as_deref());
            }
        }
        if s.ws.name == "Untitled"
            && let Some(stem) = target.file_stem()
        {
            s.ws.name = stem.to_string_lossy().into_owned();
        }
        s.ws.save(&target)?;
        s.path = Some(target.clone());
        s.dirty = false;
        Ok(target)
    }

    /// Saves if the workspace has a file; otherwise marks it unsaved.
    fn persist(&self) {
        let has_path = {
            let mut s = self.state.write().unwrap();
            s.dirty = true;
            s.path.is_some()
        };
        if has_path && let Err(e) = self.save(None) {
            tracing::warn!("autosave failed: {e}");
        }
    }

    pub fn add_source(&self, req: AddSource) -> anyhow::Result<String> {
        let base = self.base_dir();
        let (name, kind) = match req {
            AddSource::File { path } => {
                let abs = PathBuf::from(&path);
                if !abs.is_file() {
                    anyhow::bail!("{path} is not a file");
                }
                let ext = abs
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                let stored = store_path(&abs, base.as_deref());
                let name = abs
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                let kind = match ext.as_str() {
                    "xlsx" | "xlsm" | "xlsb" | "xls" | "ods" => SourceKind::Excel { path: stored },
                    "csv" | "tsv" | "txt" => SourceKind::Csv { path: stored },
                    "parquet" => SourceKind::Parquet { path: stored },
                    "sqlite" | "sqlite3" | "db" => SourceKind::Sqlite { path: stored },
                    "duckdb" => SourceKind::Duckdb { path: stored },
                    _ => anyhow::bail!(
                        "Tenun cannot read .{ext} files yet. Try Excel, CSV, Parquet, SQLite or DuckDB."
                    ),
                };
                (name, kind)
            }
            AddSource::Folder { path, pattern } => {
                let abs = PathBuf::from(&path);
                if !abs.is_dir() {
                    anyhow::bail!("{path} is not a folder");
                }
                let name = abs
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                let pattern = pattern
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .unwrap_or_else(|| DEFAULT_PATTERN.into());
                (
                    name,
                    SourceKind::Folder {
                        path: store_path(&abs, base.as_deref()),
                        pattern,
                    },
                )
            }
            AddSource::Url { url, every, name } => {
                let host = fetch::check(&url)?;
                let name = name
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .unwrap_or(host);
                (
                    name,
                    SourceKind::Url {
                        url: url.trim().to_string(),
                        every,
                    },
                )
            }
            AddSource::Postgres { conn, password } => {
                if !password.is_empty() {
                    self.secrets
                        .set(&Secrets::key("postgres", &conn), &password)?;
                }
                (conn.database.clone(), SourceKind::Postgres(conn))
            }
            AddSource::Mysql { conn, password } => {
                if !password.is_empty() {
                    self.secrets.set(&Secrets::key("mysql", &conn), &password)?;
                }
                (conn.database.clone(), SourceKind::Mysql(conn))
            }
        };
        let id = {
            let mut s = self.state.write().unwrap();
            if let Some(existing) = s.ws.sources.iter().find(|x| x.kind == kind) {
                return Ok(existing.id.clone());
            }
            let id = s.ws.next_id("s");
            s.ws.sources.push(Source {
                id: id.clone(),
                name,
                kind,
            });
            id
        };
        self.load(&id);
        let err = self.state.read().unwrap().errors.get(&id).cloned();
        if let Some(e) = err {
            // A source that fails on first add is removed again: the user sees the error.
            self.remove_source(&id);
            anyhow::bail!(e);
        }
        self.persist();
        Ok(id)
    }

    pub fn remove_source(&self, id: &str) {
        drop_source(&self.db.lock().unwrap(), id);
        {
            let mut s = self.state.write().unwrap();
            // The downloaded copy of a CSV link is Tenun's own file, so it goes with the source.
            if s.ws
                .source(id)
                .is_some_and(|x| matches!(x.kind, SourceKind::Url { .. }))
            {
                let _ = std::fs::remove_file(cache_file(s.path.as_deref(), id));
            }
            s.fetched.remove(id);
            s.ws.sources.retain(|x| x.id != id);
            let prefix = format!("{id}/");
            s.ws.relationships
                .retain(|r| !r.from.table.starts_with(&prefix) && !r.to.table.starts_with(&prefix));
            s.catalog.tables.retain(|t| t.source_id != id);
            s.ws.tables.retain(|k, _| !k.starts_with(&prefix));
            s.errors.remove(id);
            s.loaded_at.remove(id);
        }
        self.persist();
    }

    /// (Re)loads one source into DuckDB and refreshes its catalog entries. A CSV link is fetched
    /// again. Returns the ids of its tables.
    pub fn load(&self, id: &str) -> Vec<String> {
        self.load_with(id, true)
    }

    /// `fetch` makes a CSV link download again. Without it, a copy that is still fresh is read
    /// as it is, so opening a workspace does not download every link.
    fn load_with(&self, id: &str, fetch: bool) -> Vec<String> {
        let (src, base, setups, cache, cached_at) = {
            let s = self.state.read().unwrap();
            let Some(src) = s.ws.source(id).cloned() else {
                return Vec::new();
            };
            let cache = cache_file(s.path.as_deref(), id);
            let cached_at = std::fs::metadata(&cache).and_then(|m| m.modified()).ok();
            (
                src,
                s.path
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::to_path_buf)),
                setups_of(&s.ws, id),
                cache,
                cached_at,
            )
        };
        // A CSV link is fetched first. If that fails, the copy from the last fetch is still read
        // and the error is shown beside the source.
        let mut fetch_error = None;
        let mut fetched = false;
        if let SourceKind::Url { url, every } = &src.kind {
            let fresh = cached_at.is_some_and(|t| {
                every
                    .secs()
                    .is_none_or(|n| t.elapsed().is_ok_and(|e| e < Duration::from_secs(n)))
            });
            if fetch || !fresh {
                fetched = true;
                if let Err(e) = fetch::download(url, &cache) {
                    fetch_error = Some(friendly(&e));
                }
            }
        }
        let result = {
            let db = self.db.lock().unwrap();
            load_source(&db, &src, base.as_deref(), &self.secrets, &setups, &cache)
        };
        let mut s = self.state.write().unwrap();
        if fetched {
            s.fetched.insert(id.to_string(), SystemTime::now());
        }
        s.catalog.tables.retain(|t| t.source_id != id);
        match result {
            Ok(mut tables) => {
                for t in &mut tables {
                    stamp_setup(t, s.ws.tables.get(&t.id));
                }
                match fetch_error {
                    Some(e) => {
                        s.errors.insert(id.to_string(), e);
                    }
                    None => {
                        s.errors.remove(id);
                        s.loaded_at.insert(id.to_string(), now_iso());
                    }
                }
                let ids = tables.iter().map(|t| t.id.clone()).collect();
                s.catalog.tables.extend(tables);
                let names: BTreeMap<String, String> =
                    s.ws.sources
                        .iter()
                        .map(|x| (x.id.clone(), x.name.clone()))
                        .collect();
                s.catalog
                    .relabel(&|sid| names.get(sid).cloned().unwrap_or_default());
                ids
            }
            Err(e) => {
                s.errors
                    .insert(id.to_string(), fetch_error.unwrap_or_else(|| friendly(&e)));
                Vec::new()
            }
        }
    }

    /// Absolute paths of every single-file source, for the watcher.
    pub fn watched_files(&self) -> Vec<(String, PathBuf)> {
        let s = self.state.read().unwrap();
        let base = s
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        s.ws.sources
            .iter()
            .filter_map(|src| {
                src.kind
                    .path()
                    .map(|p| (src.id.clone(), resolve_path(p, base.as_deref())))
            })
            .collect()
    }

    /// Absolute paths of every folder source, for the watcher.
    pub fn watched_folders(&self) -> Vec<(String, PathBuf)> {
        let s = self.state.read().unwrap();
        let base = s
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        s.ws.sources
            .iter()
            .filter_map(|src| {
                src.kind
                    .folder()
                    .map(|(p, _)| (src.id.clone(), resolve_path(p, base.as_deref())))
            })
            .collect()
    }

    /// The sources that read one of the changed paths: the file itself, or a file in the folder.
    /// The paths come from the watcher, so they are in canonical form.
    pub fn sources_for(&self, changed: &BTreeSet<PathBuf>) -> Vec<String> {
        let s = self.state.read().unwrap();
        let base = s
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        s.ws.sources
            .iter()
            .filter(|src| {
                if let Some(p) = src.kind.path() {
                    changed.contains(&crate::watch::canonical(&resolve_path(p, base.as_deref())))
                } else if let Some((p, pattern)) = src.kind.folder() {
                    let dir = crate::watch::canonical(&resolve_path(p, base.as_deref()));
                    changed
                        .iter()
                        .any(|c| folder::is_touched_by(c, &dir, pattern))
                } else {
                    false
                }
            })
            .map(|src| src.id.clone())
            .collect()
    }

    /// Fetches the CSV links whose time has come, by their refresh interval. Returns the ids of
    /// the sources that were fetched again (with or without an error).
    pub fn refresh_due_urls(&self) -> Vec<String> {
        let due: Vec<String> = {
            let s = self.state.read().unwrap();
            let now = SystemTime::now();
            s.ws.sources
                .iter()
                .filter_map(|src| {
                    let SourceKind::Url { every, .. } = &src.kind else {
                        return None;
                    };
                    let last = s.fetched.get(&src.id).copied().or_else(|| {
                        std::fs::metadata(cache_file(s.path.as_deref(), &src.id))
                            .and_then(|m| m.modified())
                            .ok()
                    });
                    url_due(*every, last, now).then(|| src.id.clone())
                })
                .collect()
        };
        for id in &due {
            self.load(id);
        }
        due
    }

    /// Changes how often a CSV link is fetched.
    pub fn set_url_every(&self, id: &str, every: Every) -> anyhow::Result<()> {
        {
            let mut s = self.state.write().unwrap();
            match s
                .ws
                .sources
                .iter_mut()
                .find(|x| x.id == id)
                .map(|x| &mut x.kind)
            {
                Some(SourceKind::Url { every: e, .. }) => *e = every,
                _ => anyhow::bail!("{id} is not a CSV link"),
            }
        }
        self.persist();
        Ok(())
    }

    /// Writes a pasted table as a CSV file in the data folder next to the workspace file
    /// (`<workspace> data/`) and adds that file as a normal CSV source. Returns the source id.
    pub fn add_pasted(&self, name: &str, text: &str, header: bool) -> anyhow::Result<String> {
        let path = self.state.read().unwrap().path.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "Save the workspace first. Tenun keeps a pasted table in a folder next to the workspace file."
            )
        })?;
        let pasted = paste::parse(text)
            .ok_or_else(|| anyhow::anyhow!("There is no table in what you pasted."))?;
        let (names, rows) = paste::split(&pasted, header);
        let dir = data_dir(&path);
        std::fs::create_dir_all(&dir)?;
        let stem = paste::file_stem(name);
        let file = (1..)
            .map(|n| {
                dir.join(if n == 1 {
                    format!("{stem}.csv")
                } else {
                    format!("{stem} {n}.csv")
                })
            })
            .find(|p| !p.exists())
            .expect("infinite range");
        std::fs::write(&file, paste::to_csv(&names, &rows))?;
        match self.add_source(AddSource::File {
            path: file.to_string_lossy().into_owned(),
        }) {
            Ok(id) => Ok(id),
            Err(e) => {
                let _ = std::fs::remove_file(&file);
                Err(e)
            }
        }
    }

    /// Adds a relationship between two columns. The direction is decided from
    /// the data: the side with unique values is the "one" side. If neither side
    /// is unique the join would multiply rows and every sum would be wrong, so
    /// it is refused with an explanation.
    pub fn add_relationship(&self, a: FieldRef, b: FieldRef) -> anyhow::Result<String> {
        let (dupes_a, dupes_b, label_a, label_b) = {
            let s = self.state.read().unwrap();
            let ta = s
                .catalog
                .table(&a.table)
                .ok_or_else(|| anyhow::anyhow!("unknown table {}", a.table))?;
            let tb = s
                .catalog
                .table(&b.table)
                .ok_or_else(|| anyhow::anyhow!("unknown table {}", b.table))?;
            let conn = self.conn()?;
            let dupes = |t: &Table, c: &str| -> anyhow::Result<f64> {
                let q = ident(c);
                let rows = run(
                    &conn,
                    &format!(
                        "SELECT CAST(count({q}) - count(DISTINCT {q}) AS DOUBLE) FROM {}",
                        t.sql_ref
                    ),
                    &[],
                )?;
                Ok(rows
                    .first()
                    .and_then(|r| r.first())
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0))
            };
            (
                dupes(ta, &a.column)?,
                dupes(tb, &b.column)?,
                format!("{}.{}", ta.label, a.column),
                format!("{}.{}", tb.label, b.column),
            )
        };
        let (from, to) = match (dupes_a > 0.0, dupes_b > 0.0) {
            (true, true) => anyhow::bail!(
                "Neither {label_a} nor {label_b} has unique values. One side of a relationship must be unique, like an ID, or totals would be counted twice."
            ),
            (false, true) => (b, a),
            _ => (a, b),
        };
        let id = {
            let mut s = self.state.write().unwrap();
            for f in [&from, &to] {
                let t = s
                    .catalog
                    .table(&f.table)
                    .ok_or_else(|| anyhow::anyhow!("unknown table {}", f.table))?;
                if t.column(&f.column).is_none() {
                    anyhow::bail!("{} has no column {}", t.label, f.column);
                }
            }
            if from.table == to.table {
                anyhow::bail!("a table cannot be related to itself");
            }
            if let Some(r) =
                s.ws.relationships
                    .iter()
                    .find(|r| (r.from == from && r.to == to) || (r.from == to && r.to == from))
            {
                return Ok(r.id.clone());
            }
            let id = s.ws.next_id("r");
            s.ws.relationships.push(Relationship {
                id: id.clone(),
                from,
                to,
            });
            id
        };
        self.persist();
        Ok(id)
    }

    pub fn remove_relationship(&self, id: &str) {
        self.state
            .write()
            .unwrap()
            .ws
            .relationships
            .retain(|r| r.id != id);
        self.persist();
    }

    /// Saves the dashboards the UI sends. The publish status that only the
    /// publisher writes (cloud id, link, last time, last error) is kept from the
    /// current copy, so a UI save never rolls it back.
    pub fn set_dashboards(&self, mut dashboards: Vec<serde_json::Value>) {
        {
            let mut s = self.state.write().unwrap();
            for d in &mut dashboards {
                let id = d.get("id").and_then(|v| v.as_str()).map(str::to_string);
                let old = id.and_then(|id| {
                    s.ws.dashboards
                        .iter()
                        .find(|o| o.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
                        .and_then(|o| o.get("publish").cloned())
                });
                if let Some(p) = d.get_mut("publish").and_then(|p| p.as_object_mut()) {
                    for k in ["id", "url", "last", "error"] {
                        match old.as_ref().and_then(|o| o.get(k)) {
                            Some(v) => {
                                p.insert(k.to_string(), v.clone());
                            }
                            None => {
                                p.remove(k);
                            }
                        }
                    }
                }
            }
            s.ws.dashboards = dashboards;
        }
        self.persist();
    }

    pub fn set_layout(&self, layout: serde_json::Value) {
        self.state.write().unwrap().ws.layout = layout;
        self.persist();
    }

    pub fn rename(&self, name: String) {
        self.state.write().unwrap().ws.name = name;
        self.persist();
    }

    fn conn(&self) -> anyhow::Result<Connection> {
        Ok(self.db.lock().unwrap().try_clone()?)
    }

    /// The day relative date filters count from: this computer's date, unless a test fixed it.
    pub fn today(&self) -> NaiveDate {
        self.today
            .lock()
            .unwrap()
            .unwrap_or_else(query::local_today)
    }

    /// Fixes the day relative date filters count from (`None` goes back to the computer's date).
    pub fn set_today(&self, day: Option<NaiveDate>) {
        *self.today.lock().unwrap() = day;
    }

    pub fn query(&self, q: &SemanticQuery) -> anyhow::Result<QueryResult> {
        let today = self.today();
        let compiled = {
            let s = self.state.read().unwrap();
            query::compile_on(q, &s.catalog, &s.ws.relationships, today)?
        };
        let start = std::time::Instant::now();
        let conn = self.conn()?;
        let rows = run(&conn, &compiled.sql, &compiled.params)?;
        Ok(QueryResult {
            columns: compiled.columns,
            rows,
            ignored_filters: compiled.ignored_filters,
            ms: start.elapsed().as_millis(),
            kpi: None,
            partial: None,
            other: None,
            other_of: None,
            bins: None,
            totals: None,
            compare: None,
        })
    }

    /// Runs a custom SELECT that builds on the scope of a semantic query (its FROM, JOINs and
    /// WHERE). `build` returns the SQL and its parameters, in the order the `?` marks appear.
    /// Also returns the indexes of the filters that could not reach the table.
    pub fn query_custom(
        &self,
        q: &SemanticQuery,
        build: impl FnOnce(&query::Scope<'_>) -> Result<(String, Vec<Param>), query::QueryError>,
    ) -> anyhow::Result<(Vec<Vec<serde_json::Value>>, Vec<usize>)> {
        let today = self.today();
        let (sql, params, ignored) = {
            let s = self.state.read().unwrap();
            let sc = query::scope_on(q, &s.catalog, &s.ws.relationships, today)?;
            let (sql, params) = build(&sc)?;
            (sql, params, sc.ignored_filters.clone())
        };
        let conn = self.conn()?;
        Ok((run(&conn, &sql, &params)?, ignored))
    }

    /// A dashboard stored in the workspace, by id.
    pub fn dashboard(&self, id: &str) -> Option<serde_json::Value> {
        self.state
            .read()
            .unwrap()
            .ws
            .dashboards
            .iter()
            .find(|d| d.get("id").and_then(|v| v.as_str()) == Some(id))
            .cloned()
    }

    /// Changes one dashboard in place (for example its publish settings) and saves.
    pub fn update_dashboard(&self, id: &str, f: impl FnOnce(&mut serde_json::Value)) -> bool {
        let found = {
            let mut s = self.state.write().unwrap();
            match s
                .ws
                .dashboards
                .iter_mut()
                .find(|d| d.get("id").and_then(|v| v.as_str()) == Some(id))
            {
                Some(d) => {
                    f(d);
                    true
                }
                None => false,
            }
        };
        if found {
            self.persist();
        }
        found
    }

    pub fn dashboards(&self) -> Vec<serde_json::Value> {
        self.state.read().unwrap().ws.dashboards.clone()
    }

    pub fn workspace_name(&self) -> String {
        self.state.read().unwrap().ws.name.clone()
    }

    pub fn preview(&self, table_id: &str, limit: u32) -> anyhow::Result<Preview> {
        let table = self
            .state
            .read()
            .unwrap()
            .catalog
            .table(table_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown table {table_id}"))?;
        preview_of(&self.conn()?, &table.sql_ref, table.columns, limit)
    }

    /// The columns of a table and how a "Turn these columns into rows" would change it, for the
    /// preview. `None` when the table is already turned, or cannot be.
    pub fn detect_unpivot(&self, table_id: &str) -> Option<prep::UnpivotPlan> {
        let s = self.state.read().unwrap();
        let t = s.catalog.table(table_id).filter(|t| t.prep)?;
        if s.ws
            .tables
            .get(table_id)
            .is_some_and(|x| !x.transforms.is_empty())
        {
            return None;
        }
        let cols: Vec<(String, DType)> = t
            .columns
            .iter()
            .map(|c| (c.name.clone(), c.dtype))
            .collect();
        prep::detect_unpivot(&cols)
    }

    /// The first rows of the table as they would read after the columns turn into rows.
    pub fn unpivot_preview(
        &self,
        table_id: &str,
        keep: Vec<String>,
        name: String,
        value: String,
        limit: u32,
    ) -> anyhow::Result<Preview> {
        let table = self
            .state
            .read()
            .unwrap()
            .catalog
            .table(table_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown table {table_id}"))?;
        let setup = TableSetup {
            transforms: vec![Transform::Unpivot { keep, name, value }],
            ..TableSetup::default()
        };
        let cols: Vec<ColInfo> = table
            .columns
            .iter()
            .map(|c| ColInfo {
                name: c.name.clone(),
                dtype: c.dtype,
            })
            .collect();
        let plan = prep::plan_view(&table.sql_ref, &cols, &setup)
            .ok_or_else(|| anyhow::anyhow!("There is nothing to turn into rows."))?;
        let columns = plan
            .columns
            .iter()
            .map(|c| Column::new(c.name.clone(), c.dtype, String::new()))
            .collect();
        preview_of(
            &self.conn()?,
            &format!("({}) AS \"__p\"", plan.sql),
            columns,
            limit,
        )
    }

    /// What a column holds: blanks, distinct values, range, common values or a histogram.
    pub fn profile(&self, table_id: &str, column: &str) -> anyhow::Result<prep::Profile> {
        let (sql_ref, col) = {
            let s = self.state.read().unwrap();
            let t = s
                .catalog
                .table(table_id)
                .ok_or_else(|| anyhow::anyhow!("unknown table {table_id}"))?;
            let c = t
                .column(column)
                .ok_or_else(|| anyhow::anyhow!("{} has no column {column}", t.label))?;
            (t.sql_ref.clone(), c.clone())
        };
        prep::profile(&self.conn()?, &sql_ref, &col.name, col.dtype, col.mixed)
    }

    /// Sets how one table is prepared: the header row, column overrides and the unpivot. `None`
    /// clears them. The table's source loads again when the data changes (header row, types,
    /// unpivot); labels, hidden flags and descriptions apply at once. Returns whether it loaded
    /// again.
    pub fn set_table_setup(
        &self,
        table_id: &str,
        setup: Option<TableSetup>,
    ) -> anyhow::Result<bool> {
        let mut setup = setup.unwrap_or_default();
        setup.tidy();
        for t in &setup.transforms {
            let Transform::Unpivot { name, value, .. } = t;
            if name.trim().is_empty() || value.trim().is_empty() || name.trim() == value.trim() {
                anyhow::bail!("Give the two new columns different names.");
            }
        }
        let reload = {
            let mut s = self.state.write().unwrap();
            let t = s
                .catalog
                .table(table_id)
                .ok_or_else(|| anyhow::anyhow!("unknown table {table_id}"))?;
            if !t.prep && (setup.header_row.is_some() || setup.changes_view()) {
                anyhow::bail!("This table is read live, so its columns cannot be changed here.");
            }
            let source_id = t.source_id.clone();
            let retyped = |x: &TableSetup| -> BTreeMap<String, _> {
                x.columns
                    .iter()
                    .filter_map(|(n, c)| Some((n.clone(), (c.dtype?, c.date_format))))
                    .collect()
            };
            let old = s.ws.tables.get(table_id).cloned().unwrap_or_default();
            let reload = old.header_row != setup.header_row
                || old.transforms != setup.transforms
                || retyped(&old) != retyped(&setup);
            if setup.is_empty() {
                s.ws.tables.remove(table_id);
            } else {
                s.ws.tables.insert(table_id.to_string(), setup);
            }
            if !reload {
                let st = &mut *s;
                let setup = st.ws.tables.get(table_id);
                if let Some(t) = st.catalog.tables.iter_mut().find(|t| t.id == table_id) {
                    stamp_setup(t, setup);
                }
            }
            reload.then_some(source_id)
        };
        let reloaded = reload.is_some();
        if let Some(id) = reload {
            // A link that is still fresh is read from its copy, not downloaded inside this request.
            self.load_with(&id, false);
        }
        self.persist();
        Ok(reloaded)
    }

    pub fn suggest(&self) -> anyhow::Result<Vec<Suggestion>> {
        let (catalog, rels) = {
            let s = self.state.read().unwrap();
            (s.catalog.clone(), s.ws.relationships.clone())
        };
        let conn = self.conn()?;
        suggest::suggest(&catalog, &rels, &|sql| {
            let rows = run(&conn, sql, &[])?;
            Ok(rows
                .first()
                .and_then(|r| r.first())
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0))
        })
    }

    /// Runs one statement for the export, which needs every tile's data.
    /// The type of one column, if the table and column exist.
    pub fn column_dtype(&self, table: &str, column: &str) -> Option<DType> {
        let s = self.state.read().unwrap();
        s.catalog.table(table)?.column(column).map(|c| c.dtype)
    }

    pub fn catalog(&self) -> Catalog {
        self.state.read().unwrap().catalog.clone()
    }
}

/// The first rows of a relation, written the way the Data view shows them.
fn preview_of(
    conn: &Connection,
    from: &str,
    columns: Vec<Column>,
    limit: u32,
) -> anyhow::Result<Preview> {
    let cols: Vec<String> = columns
        .iter()
        .map(|c| {
            let r = ident(&c.name);
            match c.dtype {
                DType::Number if c.raw_type.to_ascii_uppercase().starts_with("DECIMAL") => {
                    format!("CAST({r} AS DOUBLE)")
                }
                DType::Number | DType::Bool => r,
                DType::Date => format!("strftime({r}, '%Y-%m-%d')"),
                DType::DateTime => format!("strftime({r}, '%Y-%m-%d %H:%M:%S')"),
                DType::Text => format!("CAST({r} AS VARCHAR)"),
            }
        })
        .collect();
    let sql = format!(
        "SELECT {} FROM {from} LIMIT {}",
        cols.join(", "),
        limit.clamp(1, 1000)
    );
    let rows = run(conn, &sql, &[])?;
    Ok(Preview { columns, rows })
}

fn rebase(kind: &mut SourceKind, old: Option<&Path>, new: Option<&Path>) {
    let p = match kind {
        SourceKind::Excel { path }
        | SourceKind::Csv { path }
        | SourceKind::Parquet { path }
        | SourceKind::Sqlite { path }
        | SourceKind::Duckdb { path }
        | SourceKind::Folder { path, .. } => path,
        _ => return,
    };
    let abs = resolve_path(p, old);
    *p = store_path(&abs, new);
}

/// `<workspace name> data/`, next to the workspace file. Pasted tables and downloaded links go here.
fn data_dir(workspace_file: &Path) -> PathBuf {
    let stem = workspace_file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".into());
    workspace_file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{stem} data"))
}

/// Where the downloaded copy of a CSV link lives. A workspace that is not saved yet keeps it in
/// the temporary folder.
fn cache_file(workspace_file: Option<&Path>, id: &str) -> PathBuf {
    match workspace_file {
        Some(p) => data_dir(p).join(format!("link-{id}.csv")),
        None => std::env::temp_dir()
            .join(format!("tenun-links-{}", std::process::id()))
            .join(format!("link-{id}.csv")),
    }
}

/// True when a CSV link should be fetched again. A link never fetched is due.
pub fn url_due(every: Every, last: Option<SystemTime>, now: SystemTime) -> bool {
    let Some(secs) = every.secs() else {
        return false;
    };
    match last {
        None => true,
        Some(t) => now
            .duration_since(t)
            .is_ok_and(|d| d >= Duration::from_secs(secs)),
    }
}

fn schema_of(id: &str) -> String {
    format!("s_{id}")
}

fn attach_name(id: &str) -> String {
    format!("src_{id}")
}

fn now_iso() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    chrono::DateTime::from_timestamp(now.as_secs() as i64, 0)
        .map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

fn drop_source(db: &Connection, id: &str) {
    let _ = db.execute_batch(&format!(
        "DROP SCHEMA IF EXISTS {} CASCADE; DETACH DATABASE IF EXISTS {};",
        ident(&schema_of(id)),
        ident(&attach_name(id))
    ));
}

fn friendly(e: &anyhow::Error) -> String {
    let msg = format!("{e:#}");
    if msg.contains("Failed to download extension") || msg.contains("Failed to install") {
        "Tenun needs to download this connector once. Connect to the internet and try again.".into()
    } else {
        msg
    }
}

fn ensure_extension(db: &Connection, name: &str) -> anyhow::Result<()> {
    if db.execute_batch(&format!("LOAD {name};")).is_ok() {
        return Ok(());
    }
    db.execute_batch(&format!("INSTALL {name}; LOAD {name};"))
        .map_err(|e| anyhow::anyhow!("Failed to install the {name} connector: {e}"))
}

/// The setups of one source's tables, by table name.
fn setups_of(ws: &Workspace, source_id: &str) -> BTreeMap<String, TableSetup> {
    let prefix = format!("{source_id}/");
    ws.tables
        .iter()
        .filter_map(|(k, v)| Some((k.strip_prefix(&prefix)?.to_string(), v.clone())))
        .collect()
}

/// Puts the label, hidden flag and description of the column overrides on the catalog.
fn stamp_setup(t: &mut Table, setup: Option<&TableSetup>) {
    for c in &mut t.columns {
        let o = setup.and_then(|s| s.columns.get(&c.name));
        c.label = o.and_then(|o| o.label.clone()).filter(|l| !l.is_empty());
        c.hidden = o.is_some_and(|o| o.hidden == Some(true));
        c.description = o
            .and_then(|o| o.description.clone())
            .filter(|d| !d.is_empty());
    }
}

/// Name of the table that holds the sheet as it was read, when a view sits on top of it.
fn raw_name(name: &str) -> String {
    format!("__raw_{name}")
}

/// Shows what building the view found: unreadable cells, or why the changes did not apply.
fn stamp_outcome(t: &mut Table, o: &ViewOutcome) {
    for (col, n) in &o.failed {
        if let Some(c) = t.columns.iter_mut().find(|c| c.name == *col) {
            c.failed = Some(*n);
        }
    }
    if let Some(p) = &o.problem {
        t.notes.push(p.clone());
    }
}

/// Removes a table or a view, whichever it is.
fn drop_any(db: &Connection, qs: &str, name: &str) {
    for kind in ["VIEW", "TABLE"] {
        let _ = db.execute_batch(&format!("DROP {kind} IF EXISTS {qs}.{};", ident(name)));
    }
}

/// Names of the tables and views of a schema, and whether each is a view.
fn local_relations(db: &Connection, schema: &str) -> anyhow::Result<Vec<(String, bool)>> {
    let mut stmt = db.prepare(
        "SELECT table_name, table_type = 'VIEW' FROM information_schema.tables \
         WHERE table_catalog = current_database() AND table_schema = ?",
    )?;
    let rows = stmt
        .query_map([schema], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// What building the view of one table gave: how many cells each retyped column could not read,
/// and a problem to show when the changes could not be applied.
#[derive(Default)]
struct ViewOutcome {
    failed: BTreeMap<String, u64>,
    problem: Option<String>,
}

/// Creates the view `name` over `raw`: the table with its transforms and column types applied.
/// When the changes cannot be applied, the view shows the raw table and says why.
fn make_view(
    db: &Connection,
    schema: &str,
    name: &str,
    raw: &str,
    setup: &TableSetup,
) -> ViewOutcome {
    let qs = ident(schema);
    let base = format!("{qs}.{}", ident(raw));
    let create = |select: &str| {
        db.execute_batch(&format!(
            "CREATE OR REPLACE VIEW {qs}.{} AS {select};",
            ident(name)
        ))
    };
    let mut out = ViewOutcome::default();
    let cols = columns_where(
        db,
        "current_database()",
        &format!(
            "table_schema = {} AND table_name = {}",
            lit(schema),
            lit(raw)
        ),
    )
    .map(|rows| {
        rows.into_iter()
            .map(|(_, _, name, ty)| ColInfo {
                name,
                dtype: DType::from_duckdb(&ty),
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    if let Some(plan) = prep::plan_view(&base, &cols, setup) {
        match create(&plan.sql) {
            Ok(()) => {
                for (col, sql) in &plan.checks {
                    let n = run(db, sql, &[])
                        .ok()
                        .and_then(|r| r.first()?.first()?.as_u64())
                        .unwrap_or(0);
                    if n > 0 {
                        out.failed.insert(col.clone(), n);
                    }
                }
                return out;
            }
            Err(e) => out.problem = Some(format!("The changes could not be applied: {e}")),
        }
    }
    if let Err(e) = create(&format!("SELECT * FROM {base}")) {
        out.problem = Some(format!("{e}"));
    }
    out
}

fn load_source(
    db: &Connection,
    src: &Source,
    base: Option<&Path>,
    secrets: &Secrets,
    setups: &BTreeMap<String, TableSetup>,
    cache: &Path,
) -> anyhow::Result<Vec<Table>> {
    let schema = schema_of(&src.id);
    let qs = ident(&schema);
    match &src.kind {
        SourceKind::Excel { path } | SourceKind::Sqlite { path } => {
            let abs = resolve_path(path, base);
            if !abs.is_file() {
                anyhow::bail!("{} is missing", abs.display());
            }
            let sheets = if matches!(src.kind, SourceKind::Excel { .. }) {
                let header_rows: BTreeMap<String, u32> = setups
                    .iter()
                    .filter_map(|(n, s)| Some((n.clone(), s.header_row?)))
                    .collect();
                excel::read_workbook(&abs, &header_rows)?
            } else {
                crate::sqlite::read_database(&abs)?
            };
            db.execute_batch(&format!("CREATE SCHEMA IF NOT EXISTS {qs};"))?;
            let existing = local_relations(db, &schema)?;
            let mut expected = BTreeSet::new();
            let mut outcomes: BTreeMap<String, ViewOutcome> = BTreeMap::new();
            for sheet in &sheets {
                let name = &sheet.name;
                let setup = setups.get(name).filter(|s| s.changes_view());
                let was = existing.iter().find(|(n, _)| n == name).map(|e| e.1);
                if let Some(setup) = setup {
                    // The sheet as read sits under a view that holds the changes.
                    let raw = raw_name(name);
                    if was == Some(false) {
                        drop_any(db, &qs, name);
                    }
                    write_sheet(db, &schema, sheet, &raw)?;
                    outcomes.insert(name.clone(), make_view(db, &schema, name, &raw, setup));
                    expected.insert(raw);
                } else {
                    if was == Some(true) {
                        drop_any(db, &qs, name);
                    }
                    write_sheet(db, &schema, sheet, name)?;
                }
                expected.insert(name.clone());
            }
            // Sheets that were deleted from the workbook go away too, and so do raw tables
            // that no view needs any more.
            for (existing, _) in &existing {
                if !expected.contains(existing) && !existing.starts_with("__stage_") {
                    drop_any(db, &qs, existing);
                }
            }
            let mut tables = catalog_local(db, src, &schema)?;
            for t in &mut tables {
                let Some(sheet) = sheets.iter().find(|s| s.name == t.name) else {
                    continue;
                };
                t.notes = sheet.notes.clone();
                t.header_row = sheet.header_row;
                for (c, (name, _)) in sheet.columns.iter().enumerate() {
                    if let Some(col) = t.columns.iter_mut().find(|x| x.name == *name) {
                        col.mixed = sheet.mixed.get(c).copied().unwrap_or(false);
                    }
                }
                if let Some(o) = outcomes.get(&t.name) {
                    stamp_outcome(t, o);
                }
            }
            Ok(tables)
        }
        SourceKind::Csv { path } | SourceKind::Parquet { path } => {
            let abs = resolve_path(path, base);
            if !abs.is_file() {
                anyhow::bail!("{} is missing", abs.display());
            }
            let reader = if matches!(src.kind, SourceKind::Csv { .. }) {
                "read_csv_auto"
            } else {
                "read_parquet"
            };
            let stem = abs
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "data".into());
            let select = format!("SELECT * FROM {reader}({})", lit(&abs.to_string_lossy()));
            db.execute_batch(&format!("CREATE SCHEMA IF NOT EXISTS {qs};"))?;
            let outcome = make_table(db, &schema, &stem, &select, setups)?;
            let mut tables = catalog_local(db, src, &schema)?;
            stamp_table_outcome(&mut tables, &stem, outcome);
            Ok(tables)
        }
        SourceKind::Url { .. } => {
            if !cache.is_file() {
                anyhow::bail!("Nothing has been downloaded from this link yet.");
            }
            let name = crate::paste::file_stem(&src.name);
            let select = format!(
                "SELECT * FROM read_csv_auto({}, null_padding = true)",
                lit(&cache.to_string_lossy())
            );
            db.execute_batch(&format!("CREATE SCHEMA IF NOT EXISTS {qs};"))?;
            let outcome = make_table(db, &schema, &name, &select, setups)?;
            let mut tables = catalog_local(db, src, &schema)?;
            stamp_table_outcome(&mut tables, &name, outcome);
            Ok(tables)
        }
        SourceKind::Folder { path, pattern } => {
            let abs = resolve_path(path, base);
            load_folder(db, src, &abs, pattern, setups)
        }
        SourceKind::Duckdb { path } => {
            let abs = resolve_path(path, base);
            if !abs.is_file() {
                anyhow::bail!("{} is missing", abs.display());
            }
            let name = attach_name(&src.id);
            db.execute_batch(&format!(
                "DETACH DATABASE IF EXISTS {n}; ATTACH {} AS {n} (READ_ONLY);",
                lit(&abs.to_string_lossy()),
                n = ident(&name)
            ))?;
            catalog_attached(db, src, &name, &["main"])
        }
        SourceKind::Postgres(c) | SourceKind::Mysql(c) => {
            let pg = matches!(src.kind, SourceKind::Postgres(_));
            let (ext, kind) = if pg {
                ("postgres", "postgres")
            } else {
                ("mysql", "mysql")
            };
            ensure_extension(db, ext)?;
            let password = secrets.get(&Secrets::key(kind, c)).unwrap_or_default();
            let mut parts = vec![
                format!("host={}", conninfo(&c.host)),
                format!("user={}", conninfo(&c.user)),
            ];
            parts.push(if pg {
                format!("dbname={}", conninfo(&c.database))
            } else {
                format!("database={}", conninfo(&c.database))
            });
            if let Some(p) = c.port {
                parts.push(format!("port={p}"));
            }
            if !password.is_empty() {
                parts.push(format!("password={}", conninfo(&password)));
            }
            let name = attach_name(&src.id);
            let schema_opt = match (&c.schema, pg) {
                (Some(s), true) if !s.is_empty() => format!(", SCHEMA {}", lit(s)),
                _ => String::new(),
            };
            db.execute_batch(&format!(
                "DETACH DATABASE IF EXISTS {n}; ATTACH {} AS {n} (TYPE {kind}, READ_ONLY{schema_opt});",
                lit(&parts.join(" ")),
                n = ident(&name)
            ))?;
            let default_schema = if pg { "public" } else { c.database.as_str() };
            catalog_attached(db, src, &name, &[default_schema])
        }
    }
}

/// Creates the table `name` as a view over a SELECT. When the table has changes (types, unpivot)
/// the SELECT sits under a second view that holds them.
fn make_table(
    db: &Connection,
    schema: &str,
    name: &str,
    select: &str,
    setups: &BTreeMap<String, TableSetup>,
) -> anyhow::Result<Option<ViewOutcome>> {
    let qs = ident(schema);
    let raw = raw_name(name);
    match setups.get(name).filter(|s| s.changes_view()) {
        Some(setup) => {
            db.execute_batch(&format!(
                "CREATE OR REPLACE VIEW {qs}.{} AS {select};",
                ident(&raw)
            ))?;
            Ok(Some(make_view(db, schema, name, &raw, setup)))
        }
        None => {
            drop_any(db, &qs, &raw);
            db.execute_batch(&format!(
                "CREATE OR REPLACE VIEW {qs}.{} AS {select};",
                ident(name)
            ))?;
            Ok(None)
        }
    }
}

fn stamp_table_outcome(tables: &mut [Table], name: &str, outcome: Option<ViewOutcome>) {
    if let Some(o) = outcome
        && let Some(t) = tables.iter_mut().find(|t| t.name == name)
    {
        stamp_outcome(t, &o);
    }
}

/// One file of a folder, read into the schema as the relation `rel`.
struct Part {
    file: String,
    rel: String,
    columns: Vec<String>,
}

/// Stacks the files of a folder into one table named after the folder, with the file name in the
/// first column. Columns are matched by name; a file that lacks one leaves it blank. A file that
/// shares no column with the others is skipped and named in the table's notes.
fn load_folder(
    db: &Connection,
    src: &Source,
    dir: &Path,
    pattern: &str,
    setups: &BTreeMap<String, TableSetup>,
) -> anyhow::Result<Vec<Table>> {
    if !dir.is_dir() {
        anyhow::bail!("{} is missing", dir.display());
    }
    let files = folder::files(dir, pattern)?;
    if files.is_empty() {
        anyhow::bail!("No files in this folder match {pattern}.");
    }
    let schema = schema_of(&src.id);
    let qs = ident(&schema);
    db.execute_batch(&format!(
        "DROP SCHEMA IF EXISTS {qs} CASCADE; CREATE SCHEMA {qs};"
    ))?;
    let mut notes: Vec<String> = Vec::new();
    let mut parts: Vec<Part> = Vec::new();

    // Excel files first: the sheet that most workbooks have is the one that stacks.
    let mut books: Vec<(String, Vec<excel::Sheet>)> = Vec::new();
    for f in &files {
        let name = f
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if is_book(f) {
            match excel::read_workbook(f, &BTreeMap::new()) {
                Ok(sheets) => books.push((name, sheets)),
                Err(e) => notes.push(format!("Skipped {name}: {e:#}")),
            }
        }
    }
    let mut tally: Vec<(String, usize)> = Vec::new();
    for (_, sheets) in &books {
        for sh in sheets.iter().filter(|s| !s.columns.is_empty()) {
            match tally.iter_mut().find(|(n, _)| *n == sh.name) {
                Some(t) => t.1 += 1,
                None => tally.push((sh.name.clone(), 1)),
            }
        }
    }
    let target = tally
        .iter()
        .fold(None::<&(String, usize)>, |best, t| match best {
            Some(b) if b.1 >= t.1 => Some(b),
            _ => Some(t),
        })
        .map(|t| t.0.clone());

    for f in &files {
        let name = f
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let rel = format!("__raw_#{}", parts.len());
        let read: anyhow::Result<()> = if let Some((_, sheets)) =
            books.iter().find(|(n, _)| *n == name)
        {
            let sheet = sheets
                .iter()
                .filter(|s| !s.columns.is_empty())
                .find(|s| Some(&s.name) == target.as_ref())
                .or_else(|| sheets.iter().find(|s| !s.columns.is_empty()));
            match sheet {
                Some(sh) => {
                    for n in sh.notes.iter().take(3) {
                        notes.push(format!("{name}: {n}"));
                    }
                    write_sheet(db, &schema, sh, &rel)
                }
                None => Err(anyhow::anyhow!("it has no data")),
            }
        } else if is_book(f) {
            continue; // a workbook that could not be opened is already in the notes
        } else if folder::is_readable(f) {
            db.execute_batch(&format!(
                    "CREATE OR REPLACE VIEW {qs}.{} AS SELECT * FROM read_csv_auto({}, null_padding = true);",
                    ident(&rel),
                    lit(&f.to_string_lossy())
                ))
                .map_err(Into::into)
        } else {
            Err(anyhow::anyhow!("Tenun cannot read this kind of file"))
        };
        match read {
            Ok(()) => {
                let columns = columns_where(
                    db,
                    "current_database()",
                    &format!(
                        "table_schema = {} AND table_name = {}",
                        lit(&schema),
                        lit(&rel)
                    ),
                )?
                .into_iter()
                .map(|c| c.2)
                .collect();
                parts.push(Part {
                    file: name,
                    rel,
                    columns,
                });
            }
            Err(e) => {
                drop_any(db, &qs, &rel);
                notes.push(format!("Skipped {name}: {e:#}"));
            }
        }
    }

    // Keep the files that share a column with the biggest group of files.
    let lower =
        |p: &Part| -> BTreeSet<String> { p.columns.iter().map(|c| c.to_lowercase()).collect() };
    let sets: Vec<BTreeSet<String>> = parts.iter().map(lower).collect();
    let linked = |a: usize, b: usize| !sets[a].is_disjoint(&sets[b]);
    let seed = (0..parts.len()).max_by_key(|&i| {
        (
            (0..parts.len()).filter(|&j| linked(i, j)).count(),
            std::cmp::Reverse(i),
        )
    });
    let mut keep: Vec<bool> = vec![false; parts.len()];
    if let Some(seed) = seed {
        let mut known = sets[seed].clone();
        keep[seed] = true;
        // A file joins when it shares a column with what is stacked so far.
        let mut changed = true;
        while changed {
            changed = false;
            for i in 0..parts.len() {
                if !keep[i] && !sets[i].is_disjoint(&known) {
                    keep[i] = true;
                    known.extend(sets[i].iter().cloned());
                    changed = true;
                }
            }
        }
    }
    for (i, p) in parts.iter().enumerate() {
        if !keep[i] {
            notes.push(format!(
                "Skipped {}: it has no column in common with the other files.",
                p.file
            ));
        }
    }
    let used: Vec<&Part> = parts
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, p)| p)
        .collect();
    if used.is_empty() {
        let why = notes.first().cloned().unwrap_or_default();
        anyhow::bail!("Tenun could not read any file in this folder. {why}");
    }

    // The file name comes first. A column of the files that is itself called "file" is renamed.
    let own = |c: &str| {
        if c.eq_ignore_ascii_case("file") {
            "file 2".to_string()
        } else {
            c.to_string()
        }
    };
    let stack = |as_text: bool| -> String {
        used.iter()
            .map(|p| {
                let cols: String = p
                    .columns
                    .iter()
                    .map(|c| {
                        let q = ident(c);
                        if as_text {
                            format!(", CAST({q} AS VARCHAR) AS {}", ident(&own(c)))
                        } else if c.eq_ignore_ascii_case("file") {
                            format!(", {q} AS {}", ident(&own(c)))
                        } else {
                            format!(", {q}")
                        }
                    })
                    .collect();
                format!(
                    "SELECT {} AS \"file\"{cols} FROM {qs}.{}",
                    lit(&p.file),
                    ident(&p.rel)
                )
            })
            .collect::<Vec<_>>()
            .join(" UNION ALL BY NAME ")
    };
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "files".into());
    let name = crate::paste::file_stem(&name);
    // Files can disagree about a type (numbers in one, words in the next). Then every column is text.
    let outcome = match make_table(db, &schema, &name, &stack(false), setups) {
        Ok(o) => o,
        Err(_) => {
            notes.push(
                "The files disagree about some column types, so Tenun reads every column as text."
                    .into(),
            );
            make_table(db, &schema, &name, &stack(true), setups)?
        }
    };
    let mut tables = catalog_local(db, src, &schema)?;
    stamp_table_outcome(&mut tables, &name, outcome);
    notes.truncate(30);
    if let Some(t) = tables.iter_mut().find(|t| t.name == name) {
        t.notes = notes;
    }
    Ok(tables)
}

fn is_book(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| matches!(e.as_str(), "xlsx" | "xlsm" | "xlsb" | "xls" | "ods"))
}

/// libpq-style value quoting: wrap in single quotes, escape \ and '.
fn conninfo(v: &str) -> String {
    format!("'{}'", v.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Writes a sheet into the schema as the table `target`.
fn write_sheet(
    db: &Connection,
    schema: &str,
    sheet: &excel::Sheet,
    target: &str,
) -> anyhow::Result<()> {
    let qs = ident(schema);
    let stage = format!("__stage_{}", std::process::id());
    let stage_cols: Vec<String> = sheet
        .columns
        .iter()
        .map(|(n, t)| {
            let ty = match t {
                ColType::Date | ColType::Timestamp => "VARCHAR",
                other => other.duckdb(),
            };
            format!("{} {ty}", ident(n))
        })
        .collect();
    db.execute_batch(&format!(
        "CREATE OR REPLACE TABLE {qs}.{} ({});",
        ident(&stage),
        stage_cols.join(", ")
    ))?;
    {
        let mut app = db.appender_to_db(&stage, schema)?;
        for row in &sheet.rows {
            let vals: Vec<Value> = row
                .iter()
                .map(|c| match c {
                    Cell::Null => Value::Null,
                    Cell::Int(i) => Value::BigInt(*i),
                    Cell::Float(f) => Value::Double(*f),
                    Cell::Bool(b) => Value::Boolean(*b),
                    Cell::Time(t) => Value::Text(t.format("%Y-%m-%d %H:%M:%S").to_string()),
                    Cell::Text(s) => Value::Text(s.clone()),
                })
                .collect();
            app.append_row(duckdb::appender_params_from_iter(vals))?;
        }
        app.flush()?;
    }
    let select: Vec<String> = sheet
        .columns
        .iter()
        .map(|(n, t)| {
            let q = ident(n);
            match t {
                ColType::Date => format!("CAST(CAST({q} AS TIMESTAMP) AS DATE) AS {q}"),
                ColType::Timestamp => format!("CAST({q} AS TIMESTAMP) AS {q}"),
                _ => q,
            }
        })
        .collect();
    db.execute_batch(&format!(
        "CREATE OR REPLACE TABLE {qs}.{} AS SELECT {} FROM {qs}.{s}; DROP TABLE {qs}.{s};",
        ident(target),
        select.join(", "),
        s = ident(&stage)
    ))?;
    Ok(())
}

type ColumnRows = Vec<(String, String, String, String)>;

fn columns_where(db: &Connection, catalog_sql: &str, filter: &str) -> anyhow::Result<ColumnRows> {
    let sql = format!(
        "SELECT table_schema, table_name, column_name, data_type FROM information_schema.columns \
         WHERE table_catalog = {catalog_sql} AND {filter} \
         AND table_schema NOT IN ('information_schema', 'pg_catalog') \
         ORDER BY table_schema, table_name, ordinal_position"
    );
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn group(rows: ColumnRows) -> Vec<(String, String, Vec<Column>)> {
    let mut out: Vec<(String, String, Vec<Column>)> = Vec::new();
    for (schema, table, col, ty) in rows {
        let column = Column::new(col, DType::from_duckdb(&ty), ty.clone());
        match out.last_mut() {
            Some((s, t, cols)) if *s == schema && *t == table => cols.push(column),
            _ => out.push((schema, table, vec![column])),
        }
    }
    out
}

fn catalog_local(db: &Connection, src: &Source, schema: &str) -> anyhow::Result<Vec<Table>> {
    let rows = columns_where(
        db,
        "current_database()",
        &format!("table_schema = {}", lit(schema)),
    )?;
    let mut tables = Vec::new();
    for (s, name, mut columns) in group(rows) {
        if name.starts_with("__stage_") || name.starts_with("__raw_") {
            continue;
        }
        let sql_ref = format!("{}.{}", ident(&s), ident(&name));
        // One pass gives the row count and the filled cells of every column.
        let counts = format!(
            "SELECT count(*){} FROM {sql_ref}",
            columns
                .iter()
                .map(|c| format!(", count({})", ident(&c.name)))
                .collect::<String>()
        );
        let counted = run(db, &counts, &[])
            .ok()
            .and_then(|r| r.into_iter().next());
        let count = counted
            .as_ref()
            .and_then(|r| r.first()?.as_i64())
            .unwrap_or(0);
        if let Some(row) = &counted
            && count > 0
        {
            for (i, c) in columns.iter_mut().enumerate() {
                let filled = row.get(i + 1).and_then(|v| v.as_i64()).unwrap_or(count);
                if filled < count {
                    c.blank = Some((count - filled) as f64 / count as f64);
                }
            }
        }
        tables.push(Table {
            id: format!("{}/{}", src.id, name),
            source_id: src.id.clone(),
            label: name.clone(),
            name,
            sql_ref,
            columns,
            rows: Some(count),
            notes: Vec::new(),
            header_row: None,
            prep: true,
        });
    }
    Ok(tables)
}

fn catalog_attached(
    db: &Connection,
    src: &Source,
    attach: &str,
    default_schemas: &[&str],
) -> anyhow::Result<Vec<Table>> {
    let rows = columns_where(db, &lit(attach), "TRUE")?;
    let local = matches!(src.kind, SourceKind::Duckdb { .. });
    let mut tables = Vec::new();
    for (schema, table, columns) in group(rows) {
        let name = if default_schemas.contains(&schema.as_str()) {
            table.clone()
        } else {
            format!("{schema}.{table}")
        };
        let sql_ref = format!("{}.{}.{}", ident(attach), ident(&schema), ident(&table));
        let rows = if local {
            db.query_row(&format!("SELECT count(*) FROM {sql_ref}"), [], |r| r.get(0))
                .ok()
        } else {
            None
        };
        tables.push(Table {
            id: format!("{}/{}", src.id, name),
            source_id: src.id.clone(),
            label: name.clone(),
            name,
            sql_ref,
            columns,
            rows,
            notes: Vec::new(),
            header_row: None,
            prep: false,
        });
    }
    if tables.is_empty() {
        anyhow::bail!("no tables found");
    }
    Ok(tables)
}

fn to_duck(p: &Param) -> Value {
    match p {
        Param::Null => Value::Null,
        Param::Text(s) => Value::Text(s.clone()),
        Param::Double(d) => Value::Double(*d),
        Param::Bool(b) => Value::Boolean(*b),
    }
}

pub(crate) fn run(
    conn: &Connection,
    sql: &str,
    params: &[Param],
) -> anyhow::Result<Vec<Vec<serde_json::Value>>> {
    let mut stmt = conn.prepare(sql)?;
    let values: Vec<Value> = params.iter().map(to_duck).collect();
    let mut rows = stmt.query(duckdb::params_from_iter(values))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let n = row.as_ref().column_count();
        let mut r = Vec::with_capacity(n);
        for i in 0..n {
            r.push(to_json(row.get::<_, Value>(i)?));
        }
        out.push(r);
    }
    Ok(out)
}

fn to_json(v: Value) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        Value::Null => J::Null,
        Value::Boolean(b) => J::Bool(b),
        Value::TinyInt(i) => i.into(),
        Value::SmallInt(i) => i.into(),
        Value::Int(i) => i.into(),
        Value::BigInt(i) => i.into(),
        Value::UTinyInt(i) => i.into(),
        Value::USmallInt(i) => i.into(),
        Value::UInt(i) => i.into(),
        Value::UBigInt(i) => i.into(),
        Value::HugeInt(i) => J::String(i.to_string()),
        Value::UHugeInt(i) => J::String(i.to_string()),
        Value::Float(f) => serde_json::Number::from_f64(f as f64).map_or(J::Null, J::Number),
        Value::Double(f) => serde_json::Number::from_f64(f).map_or(J::Null, J::Number),
        Value::Text(s) => J::String(s),
        other => J::String(format!("{other:?}")),
    }
}
