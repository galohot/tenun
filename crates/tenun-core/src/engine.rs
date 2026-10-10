//! The engine: one in-memory DuckDB, the open workspace, and its catalog.

use crate::catalog::{Catalog, Column, DType, Table, ident, lit};
use crate::excel::{self, Cell, ColType};
use crate::query::{self, Param, SemanticQuery};
use crate::secrets::Secrets;
use crate::suggest::{self, Suggestion};
use crate::workspace::{
    DbConn, FieldRef, Relationship, Source, SourceKind, Workspace, resolve_path, store_path,
};
use duckdb::Connection;
use duckdb::types::Value;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

pub struct Engine {
    db: Mutex<Connection>,
    state: RwLock<State>,
    secrets: Secrets,
}

#[derive(Default)]
struct State {
    ws: Workspace,
    path: Option<PathBuf>,
    catalog: Catalog,
    /// Source id → the last load error, shown next to the source.
    errors: BTreeMap<String, String>,
    dirty: bool,
}

/// Everything the UI needs to draw the workspace.
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub workspace: Workspace,
    pub path: Option<String>,
    pub tables: Vec<Table>,
    pub errors: BTreeMap<String, String>,
    pub dirty: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AddSource {
    /// Any supported file; the kind is chosen from the extension.
    File { path: String },
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

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<query::OutColumn>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub ignored_filters: Vec<usize>,
    pub ms: u128,
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

    pub fn new_workspace(&self) {
        self.replace(Workspace::default(), None);
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
            self.load(&src.id);
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
            s.ws.sources.retain(|x| x.id != id);
            let prefix = format!("{id}/");
            s.ws.relationships
                .retain(|r| !r.from.table.starts_with(&prefix) && !r.to.table.starts_with(&prefix));
            s.catalog.tables.retain(|t| t.source_id != id);
            s.errors.remove(id);
        }
        self.persist();
    }

    /// (Re)loads one source into DuckDB and refreshes its catalog entries.
    /// Returns the ids of its tables.
    pub fn load(&self, id: &str) -> Vec<String> {
        let (src, base) = {
            let s = self.state.read().unwrap();
            let Some(src) = s.ws.source(id).cloned() else {
                return Vec::new();
            };
            (
                src,
                s.path
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::to_path_buf)),
            )
        };
        let result = {
            let db = self.db.lock().unwrap();
            load_source(&db, &src, base.as_deref(), &self.secrets)
        };
        let mut s = self.state.write().unwrap();
        s.catalog.tables.retain(|t| t.source_id != id);
        match result {
            Ok(tables) => {
                s.errors.remove(id);
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
                s.errors.insert(id.to_string(), friendly(&e));
                Vec::new()
            }
        }
    }

    /// Absolute paths of every file source, for the watcher.
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

    pub fn set_dashboards(&self, dashboards: Vec<serde_json::Value>) {
        self.state.write().unwrap().ws.dashboards = dashboards;
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

    pub fn query(&self, q: &SemanticQuery) -> anyhow::Result<QueryResult> {
        let compiled = {
            let s = self.state.read().unwrap();
            query::compile(q, &s.catalog, &s.ws.relationships)?
        };
        let start = std::time::Instant::now();
        let conn = self.conn()?;
        let rows = run(&conn, &compiled.sql, &compiled.params)?;
        Ok(QueryResult {
            columns: compiled.columns,
            rows,
            ignored_filters: compiled.ignored_filters,
            ms: start.elapsed().as_millis(),
        })
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
        let cols: Vec<String> = table
            .columns
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
            "SELECT {} FROM {} LIMIT {}",
            cols.join(", "),
            table.sql_ref,
            limit.clamp(1, 1000)
        );
        let rows = run(&self.conn()?, &sql, &[])?;
        Ok(Preview {
            columns: table.columns,
            rows,
        })
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
    pub fn catalog(&self) -> Catalog {
        self.state.read().unwrap().catalog.clone()
    }
}

fn rebase(kind: &mut SourceKind, old: Option<&Path>, new: Option<&Path>) {
    let p = match kind {
        SourceKind::Excel { path }
        | SourceKind::Csv { path }
        | SourceKind::Parquet { path }
        | SourceKind::Sqlite { path }
        | SourceKind::Duckdb { path } => path,
        _ => return,
    };
    let abs = resolve_path(p, old);
    *p = store_path(&abs, new);
}

fn schema_of(id: &str) -> String {
    format!("s_{id}")
}

fn attach_name(id: &str) -> String {
    format!("src_{id}")
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

fn load_source(
    db: &Connection,
    src: &Source,
    base: Option<&Path>,
    secrets: &Secrets,
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
                excel::read_workbook(&abs)?
            } else {
                crate::sqlite::read_database(&abs)?
            };
            db.execute_batch(&format!("CREATE SCHEMA IF NOT EXISTS {qs};"))?;
            let mut names = Vec::new();
            for sheet in &sheets {
                write_sheet(db, &schema, sheet)?;
                names.push(sheet.name.clone());
            }
            // Sheets that were deleted from the workbook go away too.
            for existing in local_tables(db, &schema)? {
                if !names.contains(&existing) {
                    db.execute_batch(&format!("DROP TABLE IF EXISTS {qs}.{};", ident(&existing)))?;
                }
            }
            catalog_local(db, src, &schema)
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
            db.execute_batch(&format!(
                "CREATE SCHEMA IF NOT EXISTS {qs}; CREATE OR REPLACE VIEW {qs}.{} AS SELECT * FROM {reader}({});",
                ident(&stem),
                lit(&abs.to_string_lossy())
            ))?;
            catalog_local(db, src, &schema)
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

/// libpq-style value quoting: wrap in single quotes, escape \ and '.
fn conninfo(v: &str) -> String {
    format!("'{}'", v.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn write_sheet(db: &Connection, schema: &str, sheet: &excel::Sheet) -> anyhow::Result<()> {
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
        ident(&sheet.name),
        select.join(", "),
        s = ident(&stage)
    ))?;
    Ok(())
}

fn local_tables(db: &Connection, schema: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = db.prepare(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_catalog = current_database() AND table_schema = ?",
    )?;
    let names = stmt
        .query_map([schema], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names)
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
        let column = Column {
            name: col,
            dtype: DType::from_duckdb(&ty),
            raw_type: ty,
        };
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
    for (s, name, columns) in group(rows) {
        if name.starts_with("__stage_") {
            continue;
        }
        let sql_ref = format!("{}.{}", ident(&s), ident(&name));
        let count: i64 = db
            .query_row(&format!("SELECT count(*) FROM {sql_ref}"), [], |r| r.get(0))
            .unwrap_or(0);
        tables.push(Table {
            id: format!("{}/{}", src.id, name),
            source_id: src.id.clone(),
            label: name.clone(),
            name,
            sql_ref,
            columns,
            rows: Some(count),
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
