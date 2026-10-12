//! The engine: one in-memory DuckDB, the open workspace, and its catalog.

use crate::calc::{self, CalcRefused};
use crate::catalog::{Catalog, Column, DType, Table, ident, lit};
use crate::derived;
use crate::excel::{self, Cell, ColType};
use crate::fetch;
use crate::folder;
use crate::formula;
use crate::links::{self, Health, HealthOut, LinkHealth, Links};
use crate::measures;
use crate::paste;
use crate::prep::{self, ColInfo};
use crate::query::{self, Param, SemanticQuery};
use crate::secrets::Secrets;
use crate::suggest::{self, Suggestion};
use crate::workspace::{
    CalcColumn, DEFAULT_PATTERN, DbConn, DerivedSpec, Every, FieldRef, KeyPlace, LinkKey, Locale,
    NamedMeasure, Paging, Relationship, Source, SourceKind, TableSetup, Transform, Workspace,
    resolve_path, store_path,
};
use chrono::NaiveDate;
use duckdb::Connection;
use duckdb::types::Value;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime};

/// (table id, main date column) → the data generation it was read under, and its period (V4-D101).
type GrainCache = BTreeMap<(String, String), (u64, Option<measures::Period>)>;

/// Called when a check of live database links that ran off the request path has written its answer.
type Listener = Arc<dyn Fn() + Send + Sync>;

/// A measure that cannot be saved: which one, why, and the place to underline (in characters of the
/// formula). It is the error of [`Engine::set_measures`]; the API answers it as a normal reply, with
/// the place in UTF-16 units (V4-D23, V4-D35).
#[derive(Debug, Clone)]
pub struct MeasureRefused {
    /// The place of the measure in the list that was sent.
    pub index: usize,
    pub name: String,
    /// The text that was sent, which the place counts in.
    pub formula: String,
    /// What the message is about: `name`, `formula`, or `list` (too many measures, a repeated id).
    pub field: &'static str,
    pub error: formula::FormulaError,
}

impl std::fmt::Display for MeasureRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.error.message)
    }
}

impl std::error::Error for MeasureRefused {}

impl MeasureRefused {
    /// The reply of `PUT /api/workspace/measures` for a measure that cannot be saved.
    pub fn reply(&self) -> serde_json::Value {
        let e = self.error.in_utf16(&self.formula);
        serde_json::json!({
            "ok": false,
            "measure": self.index,
            "name": self.name,
            "field": self.field,
            "error": { "message": e.message, "at": e.at, "len": e.len },
        })
    }
}

/// A tile that uses a measure.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TileRef {
    pub dashboard: String,
    #[serde(rename = "dashboardTitle")]
    pub dashboard_title: String,
    pub tile: String,
    pub title: String,
    pub kind: String,
}

/// A measure that tiles use, and the tiles.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MeasureUse {
    pub measure: String,
    pub name: String,
    pub tiles: Vec<TileRef>,
}

/// The measures a save would delete while tiles use them. Nothing was saved; the caller asks the
/// user, and sends the list again with `confirm`.
#[derive(Debug, Clone)]
pub struct MeasureInUse {
    pub used: Vec<MeasureUse>,
}

impl std::fmt::Display for MeasureInUse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tiles use a measure that this would delete.")
    }
}

impl std::error::Error for MeasureInUse {}

impl MeasureInUse {
    /// The reply of `PUT /api/workspace/measures`: `{ ok: false, used: [...] }`.
    pub fn reply(&self) -> serde_json::Value {
        serde_json::json!({ "ok": false, "used": self.used })
    }
}

/// The tiles of the dashboards that use the measure `id`.
pub fn tiles_using(dashboards: &[serde_json::Value], id: &str) -> Vec<TileRef> {
    let text = |v: &serde_json::Value, k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let mut out = Vec::new();
    for d in dashboards {
        let tiles = d.get("tiles").and_then(|t| t.as_array());
        for t in tiles.into_iter().flatten() {
            let uses = t
                .get("measures")
                .and_then(|m| m.as_array())
                .is_some_and(|ms| {
                    ms.iter()
                        .any(|m| m.get("named").and_then(|n| n.as_str()) == Some(id))
                });
            if uses {
                out.push(TileRef {
                    dashboard: text(d, "id"),
                    dashboard_title: text(d, "title"),
                    tile: text(t, "id"),
                    title: text(t, "title"),
                    kind: text(t, "type"),
                });
            }
        }
    }
    out
}

/// The most rows of a live database table that a link check reads (V4-D51).
pub const LIVE_CHECK_ROWS: u64 = 1_000_000;

/// The card styles a workspace keeps at most (build 08).
pub const MAX_CARD_STYLES: usize = 200;

pub struct Engine {
    db: Arc<Mutex<Connection>>,
    state: Arc<RwLock<State>>,
    secrets: Secrets,
    /// Replaces today's date for relative date filters. Tests set it; the app never does.
    today: Mutex<Option<NaiveDate>>,
    /// How many link checks of live tables are running in the background.
    pending: Arc<AtomicUsize>,
    /// Told when such a check has written its answer.
    listener: Arc<Mutex<Option<Listener>>>,
    /// The most rows of a live table a check reads.
    live_rows: Arc<AtomicU64>,
    /// Milliseconds a link check waits before it reads the data. Tests use it to hold the moment
    /// after a reload open; the app never sets it.
    check_delay: Arc<AtomicU64>,
    /// (table id, main date column) → the `State.data_gen` it was read under, and its period (V4-D101).
    grain_cache: Mutex<GrainCache>,
}

thread_local! {
    /// While a tile runs: the links its queries crossed that have something to tell the reader.
    static CROSSED: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

fn note_crossed(ids: &[String]) {
    CROSSED.with(|c| {
        if let Some(v) = c.borrow_mut().as_mut() {
            for id in ids {
                if !v.contains(id) {
                    v.push(id.clone());
                }
            }
        }
    });
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
    /// Relationship id → what the data of its two columns says. Made when a table loads (V4-D26),
    /// never when a query runs. A link with no record is unknown: a join into it reads one row
    /// per key (V4-D51).
    health: Health,
    /// Relationship id → how many times its record was thrown away. A check that began before
    /// the last time writes nothing.
    epoch: BTreeMap<String, u64>,
    /// Relationship id → the kind the link had at its last check, and the kind it had before the
    /// data changed it (while it has not changed back).
    kinds: BTreeMap<String, (&'static str, Option<&'static str>)>,
    /// Table id → the reference of the table under its calculated-column view (V4-D3). A table
    /// has an entry only while its catalog entry points at such a view.
    calc_base: BTreeMap<String, String>,
    /// The calculated columns that read another table, as (table id, column). Such a column cannot
    /// be the key of a link (V4-D63).
    calc_related: BTreeSet<(String, String)>,
    /// Table id → the day its calculated-column view was built, for a table with `TODAY()` or
    /// `NOW()` in a formula (V4-D34). The view is built again when the day changes.
    calc_day: BTreeMap<String, NaiveDate>,
    /// What the passes remember across a reload: the type of each calculated column, and the
    /// tables each view read (V4-D67, V4-D77). A load does not clear it.
    calc_memory: calc::Memory,
    /// Bumped when a load puts new tables in the catalog, so a pass that began before it is
    /// thrown away instead of writing old tables back.
    tables_gen: u64,
    /// Moves whenever the data a question reads may have changed: a load, a removed source, a calculated
    /// view built again, a workspace put in. What the engine read from the data once (the grain of a
    /// main date) is read again when it moves (V4-D101). Never goes back.
    data_gen: u64,
    dirty: bool,
}

/// Everything the UI needs to draw the workspace.
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub workspace: Workspace,
    pub path: Option<String>,
    pub tables: Vec<Table>,
    /// Source id → why it did not load.
    pub errors: BTreeMap<String, String>,
    /// Table id → the calculated columns of the table that could not be computed, in words; their
    /// values are blank. A table problem is not a source problem: it is not in `errors`, so what
    /// stops on a source that did not load (`tenun snapshot`) does not stop on this (V4-D77).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub table_errors: BTreeMap<String, String>,
    /// Source id → when it last loaded without error (ISO 8601, UTC). Kept
    /// out of the source itself so it never reaches the `.tenun` file.
    pub loaded_at: BTreeMap<String, String>,
    /// The web links whose key has a value stored on this computer (their ids). The value itself
    /// never leaves the secrets file.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub link_keys: BTreeSet<String>,
    /// Relationship id → what the data says about the link: repeated keys on the one side,
    /// one to one, and the warning in plain words. Not set for a link that was not checked.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub health: BTreeMap<String, HealthOut>,
    /// Link id → why its name no longer works: a table now has that name, or another link does.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub link_clashes: BTreeMap<String, String>,
    /// Measure id → its type, whether its values add up, the tables it adds up, and why it does
    /// not work (V4-D4). The app never works this out itself.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub measure_info: BTreeMap<String, measures::MeasureInfo>,
    pub dirty: bool,
}

/// A key as the Add sheet sends it. The value is not part of the `Debug` text.
#[derive(Deserialize)]
pub struct KeyIn {
    pub place: KeyPlace,
    pub name: String,
    /// Empty: use the value that is stored already, if there is one.
    #[serde(default)]
    pub value: String,
}

impl std::fmt::Debug for KeyIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyIn")
            .field("place", &self.place)
            .field("name", &self.name)
            .field("value", &"•••")
            .finish()
    }
}

/// What a change of link settings does with the key.
#[derive(Debug)]
pub enum KeyEdit {
    Keep,
    Remove,
    Set(KeyIn),
}

/// A look at the first page of a web link, before it is added.
#[derive(Debug, Deserialize)]
pub struct LinkPreview {
    pub url: String,
    #[serde(default)]
    pub records: Option<String>,
    #[serde(default)]
    pub key: Option<KeyIn>,
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
    /// CSV or JSON data on the web (https only).
    Url {
        url: String,
        #[serde(default)]
        every: Every,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        records: Option<String>,
        #[serde(default)]
        pages: Option<Paging>,
        #[serde(default)]
        key: Option<KeyIn>,
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
    /// A new table from other tables: append, merge or group (V4-D6). `name` is the table's name.
    Derived { name: String, spec: DerivedSpec },
}

/// A look at a new table before it is made: its columns, its first rows, how many rows it has and
/// the notes it would carry (build 04).
#[derive(Debug, Serialize)]
pub struct DerivedPreview {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub total: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
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
    /// Tables whose row limit cut the result: how many rows there are in all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_rows: Option<u64>,
    /// What the reader should know about the links this result crossed: a key that repeats on
    /// the one side, so the first row was used (V4-D26).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// The same notes with no key value and no file name, for what leaves the computer: pictures
    /// and exports (V4-D52). Snapshots put these in `notes`.
    #[serde(rename = "publicNotes", skip_serializing_if = "Vec::is_empty")]
    pub public_notes: Vec<String>,
    /// Small multiples (`tile.repeat`): the repeat values that have a chart, in the order they are
    /// drawn, and how many more there are. Rows are `[group, repeat value, value]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat: Option<Repeat>,
    /// Indexed lines (`tile.layout = "indexed"`): the rows before they were divided by the first
    /// value, same shape as `rows`, for the tooltip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real: Option<Vec<Vec<serde_json::Value>>>,
    /// Box plots: the outlier dots, `[group, value]`, at most 50 for each group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outliers: Option<Vec<Vec<serde_json::Value>>>,
    /// Tables with a trend column (`tile.spark`): for each row key, the last 12 periods of the first
    /// value. The key is the group values of the row joined by `\u{1f}` (`advanced::spark_key`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sparks: Option<std::collections::BTreeMap<String, Vec<Option<f64>>>>,
    /// Compare charts: what the two value columns stand for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pair: Option<Pair>,
    /// Named measures: a group or a filter that one table of the formula cannot reach, so that
    /// table is left out of it (V4-D4, V4-D85). Table labels and column names only, so the words
    /// may leave the computer as they are. The same sentences are also in `notes` and `publicNotes`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<query::PartNote>,
    /// Map tiles (build 06): the place of each row, the names that are not on the map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo: Option<crate::maptile::GeoInfo>,
    /// A text tile's live numbers (build 10): `"<measure id>"` is the value, `"<id>|compare"` the change against the
    /// period before (0.063 for +6.3 %), `"<id>|period"` the name of the period. A measure that is gone has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<BTreeMap<String, serde_json::Value>>,
}

/// A compare chart with one value reads two periods of a field. `periods` are those two values, in
/// the order of the two value columns of the rows. Not set when the chart compares two values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pair {
    pub periods: Option<Vec<serde_json::Value>>,
}

impl QueryResult {
    /// A result with these columns and rows, and none of the extras.
    pub fn new(columns: Vec<query::OutColumn>, rows: Vec<Vec<serde_json::Value>>) -> Self {
        QueryResult {
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
            total_rows: None,
            repeat: None,
            real: None,
            outliers: None,
            sparks: None,
            pair: None,
            notes: Vec::new(),
            public_notes: Vec::new(),
            parts: Vec::new(),
            geo: None,
            values: None,
        }
    }
}

/// The charts of a small-multiples tile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Repeat {
    /// The repeat values that have a chart, in drawing order.
    pub keys: Vec<serde_json::Value>,
    /// How many more values there are than charts ("and 5 more").
    pub more: u32,
}

impl QueryResult {
    /// Same as `new`.
    pub fn plain(columns: Vec<query::OutColumn>, rows: Vec<Vec<serde_json::Value>>) -> Self {
        Self::new(columns, rows)
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
    /// Expandable pivot (`tile.expand`): the cells of each group of the row fields, over each column.
    /// An entry is the group values (the first 1 to n-1 row fields), the column value, then the
    /// measures. The totals of a group are in `rows`, as entries with fewer row values than a row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<Vec<serde_json::Value>>>,
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
            db: Arc::new(Mutex::new(conn)),
            state: Arc::new(RwLock::new(State::default())),
            secrets,
            today: Mutex::new(None),
            pending: Arc::new(AtomicUsize::new(0)),
            listener: Arc::new(Mutex::new(None)),
            live_rows: Arc::new(AtomicU64::new(LIVE_CHECK_ROWS)),
            check_delay: Arc::new(AtomicU64::new(0)),
            grain_cache: Mutex::new(BTreeMap::new()),
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
            table_errors: table_errors_of(&s),
            loaded_at: s.loaded_at.clone(),
            link_keys: s
                .ws
                .sources
                .iter()
                .filter(|x| {
                    link_secret(&x.kind)
                        .is_some_and(|id| self.secrets.get(&id).is_some_and(|v| !v.is_empty()))
                })
                .map(|x| x.id.clone())
                .collect(),
            health: health_out(&s),
            link_clashes: link_clashes(&s),
            measure_info: measures::infos(&measure_env(&s, self.today(), &NO_GRAINS), &s.catalog),
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
            let gen_before = s.data_gen;
            *s = State {
                ws: ws.clone(),
                path,
                data_gen: gen_before + 1,
                ..State::default()
            };
        }
        // A new table loads after the tables it reads (V4-D6). New tables that read each other in a
        // ring are not loaded: each says so, and the rest of the workspace loads.
        let (order, ring) = derived::load_order(&ws.sources);
        for id in order {
            self.load_one(&id, false);
        }
        let mut s = self.state.write().unwrap();
        for id in ring {
            s.errors.insert(
                id,
                "This new table reads itself through another new table. Open it and pick other tables."
                    .into(),
            );
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

    /// Where boundaries downloaded for maps are kept: `geo/` beside the secrets file (V4-D19).
    pub fn geo_dir(&self) -> PathBuf {
        self.secrets.path().with_file_name("geo")
    }

    pub fn add_source(&self, req: AddSource) -> anyhow::Result<String> {
        let base = self.base_dir();
        // A link key: its secrets entry, and the value to store before the first load (none when
        // the value stored already is meant).
        let mut link_entry: Option<(String, Option<String>)> = None;
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
            AddSource::Url {
                url,
                every,
                name,
                records,
                pages,
                key,
            } => {
                let host = fetch::check(&url)?;
                let name = name
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| host.clone());
                let mut link_key = None;
                if let Some(k) = key.filter(|k| !k.name.trim().is_empty()) {
                    let lk = LinkKey {
                        place: k.place,
                        name: k.name.trim().to_string(),
                    };
                    let secret = Secrets::link_key(&host, lk.place, &lk.name);
                    if k.value.is_empty() {
                        if self.secrets.get(&secret).is_none_or(|v| v.is_empty()) {
                            anyhow::bail!("Type the key to send with the link.");
                        }
                        link_entry = Some((secret, None));
                    } else {
                        link_entry = Some((secret, Some(k.value)));
                    }
                    link_key = Some(lk);
                }
                (
                    name,
                    SourceKind::Url {
                        url: url.trim().to_string(),
                        every,
                        records: clean(records),
                        pages,
                        key: link_key,
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
            AddSource::Derived { name, spec } => {
                let name = name.trim().to_string();
                if let Some(why) = derived::name_problem(&name) {
                    anyhow::bail!(why);
                }
                // The spec is checked against the tables as they are now, before anything is saved.
                {
                    let s = self.state.read().unwrap();
                    derived::plan(&spec, &s.catalog, &s.ws.relationships, &s.health)
                        .map_err(anyhow::Error::msg)?;
                }
                return self.add_derived(name, spec);
            }
        };
        // The key goes into the secrets file before the first load, as a database password does.
        // If the load fails, the entry goes back to what it was, also when the value stored
        // already was used: another workspace may need it.
        let mut undo_key: Option<(String, Option<String>)> = None;
        if let Some((secret, value)) = link_entry {
            let before = self.secrets.get(&secret);
            if let Some(v) = value {
                self.secrets.set(&secret, &v)?;
            }
            undo_key = Some((secret, before));
        }
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
            if let Some((secret, before)) = undo_key {
                let _ = match before {
                    Some(v) => self.secrets.set(&secret, &v),
                    None => self.secrets.remove(&secret),
                };
            }
            anyhow::bail!(e);
        }
        self.persist();
        Ok(id)
    }

    /// Adds a new table (V4-D6). Two new tables may have the same spec under two names, so there is
    /// no "already added" here. A spec that fails when the view is made is not kept.
    fn add_derived(&self, name: String, spec: DerivedSpec) -> anyhow::Result<String> {
        let id = {
            let mut s = self.state.write().unwrap();
            let id = s.ws.next_id("s");
            s.ws.sources.push(Source {
                id: id.clone(),
                name,
                kind: SourceKind::Derived { spec },
            });
            id
        };
        self.load(&id);
        let err = self.state.read().unwrap().errors.get(&id).cloned();
        if let Some(e) = err {
            self.remove_source(&id);
            anyhow::bail!(e);
        }
        self.persist();
        Ok(id)
    }

    /// Changes how a new table is made. Its name stays, so its id, its links and the tiles that use
    /// it stay too. A spec that would make the table read itself is refused; one that fails when the
    /// view is made leaves the old spec in place. Returns the ids of the tables that loaded again.
    pub fn set_derived(&self, id: &str, spec: DerivedSpec) -> anyhow::Result<Vec<String>> {
        let old = {
            let s = self.state.read().unwrap();
            let src =
                s.ws.source(id)
                    .ok_or_else(|| anyhow::anyhow!("unknown source {id}"))?;
            let SourceKind::Derived { spec: old } = &src.kind else {
                anyhow::bail!("{} is not a new table.", src.name);
            };
            if derived::makes_ring(&s.ws.sources, id, &spec) {
                anyhow::bail!("{} would read itself. Pick other tables.", src.name);
            }
            derived::plan(&spec, &s.catalog, &s.ws.relationships, &s.health)
                .map_err(anyhow::Error::msg)?;
            old.clone()
        };
        let put = |spec: DerivedSpec| {
            let mut s = self.state.write().unwrap();
            if let Some(src) = s.ws.sources.iter_mut().find(|x| x.id == id) {
                src.kind = SourceKind::Derived { spec };
            }
        };
        put(spec);
        let ids = self.load(id);
        let err = self.state.read().unwrap().errors.get(id).cloned();
        if let Some(e) = err {
            put(old);
            self.load(id);
            anyhow::bail!(e);
        }
        self.persist();
        Ok(ids)
    }

    /// The columns, the first rows, the number of rows and the notes of a new table before it is made.
    pub fn preview_derived(&self, spec: &DerivedSpec) -> anyhow::Result<DerivedPreview> {
        let plan = {
            let s = self.state.read().unwrap();
            derived::plan(spec, &s.catalog, &s.ws.relationships, &s.health)
                .map_err(anyhow::Error::msg)?
        };
        let db = self.db.lock().unwrap();
        let columns = describe(&db, &plan.sql).map_err(|e| anyhow::anyhow!(friendly(&e)))?;
        let from = format!("({}) AS d", plan.sql);
        let mut out = preview_of(&db, &from, columns.clone(), 5)?;
        let total = count_of(&db, &format!("SELECT count(*) FROM {from}"));
        let notes = match &plan.merge {
            Some(m) => merge_notes(&db, m),
            None => Vec::new(),
        };
        out.columns = columns;
        Ok(DerivedPreview {
            columns: out.columns,
            rows: out.rows,
            total,
            notes,
        })
    }

    pub fn remove_source(&self, id: &str) {
        // The new tables that read this source say so after it goes.
        let readers = {
            let s = self.state.read().unwrap();
            derived::dependents(&s.ws.sources, &[id.to_string()])
        };
        self.remove_one(id);
        for r in readers {
            self.load_one(&r, false);
        }
    }

    fn remove_one(&self, id: &str) {
        drop_source(&self.db.lock().unwrap(), id);
        {
            let mut s = self.state.write().unwrap();
            // The downloaded copy of a web link is Tenun's own file, so it goes with the source.
            // Its key stays, like a database password: a link in another workspace on this
            // computer may use the same entry. Only "Remove key" deletes it (V3-D88).
            if s.ws
                .source(id)
                .is_some_and(|x| matches!(x.kind, SourceKind::Url { .. }))
            {
                let cache = cache_file(s.path.as_deref(), id);
                let _ = std::fs::remove_file(&cache);
                let _ = std::fs::remove_file(notes_file(&cache));
            }
            s.fetched.remove(id);
            s.ws.sources.retain(|x| x.id != id);
            let prefix = format!("{id}/");
            s.ws.relationships
                .retain(|r| !r.from.table.starts_with(&prefix) && !r.to.table.starts_with(&prefix));
            let kept: BTreeSet<String> = s.ws.relationships.iter().map(|r| r.id.clone()).collect();
            s.health.retain(|k, _| kept.contains(k));
            s.catalog.tables.retain(|t| t.source_id != id);
            s.ws.tables.retain(|k, _| !k.starts_with(&prefix));
            s.errors.remove(id);
            s.loaded_at.remove(id);
            s.calc_base.retain(|k, _| !k.starts_with(&prefix));
            s.calc_related.retain(|(k, _)| !k.starts_with(&prefix));
            s.calc_day.retain(|k, _| !k.starts_with(&prefix));
            s.calc_memory.types.retain(|k, _| !k.starts_with(&prefix));
            s.calc_memory.reads.retain(|k, _| !k.starts_with(&prefix));
            s.tables_gen += 1;
            s.data_gen += 1;
        }
        // Tables that read the removed ones now say so.
        self.rebuild_calc(calc::Roots::All);
        self.persist();
    }

    /// (Re)loads one source into DuckDB and refreshes its catalog entries. A CSV link is fetched
    /// again. Returns the ids of its tables.
    pub fn load(&self, id: &str) -> Vec<String> {
        self.load_with(id, true)
    }

    /// `fetch` makes a CSV link download again. Without it, a copy that is still fresh is read
    /// as it is, so opening a workspace does not download every link. The new tables that read the
    /// source are built again after it, in order (V4-D6); the ids of their tables are in the answer.
    fn load_with(&self, id: &str, fetch: bool) -> Vec<String> {
        let mut ids = self.load_one(id, fetch);
        self.load_readers(&[id.to_string()], &mut ids);
        ids
    }

    /// Builds again the new tables that read the sources `changed`, each after the tables it reads.
    fn load_readers(&self, changed: &[String], ids: &mut Vec<String>) {
        let readers = {
            let s = self.state.read().unwrap();
            derived::dependents(&s.ws.sources, changed)
        };
        for r in readers {
            ids.extend(self.load_one(&r, false));
        }
    }

    /// Loads one source and nothing that reads it.
    fn load_one(&self, id: &str, fetch: bool) -> Vec<String> {
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
        if let SourceKind::Url { every, .. } = &src.kind {
            let fresh = cached_at.is_some_and(|t| {
                every
                    .secs()
                    .is_none_or(|n| t.elapsed().is_ok_and(|e| e < Duration::from_secs(n)))
            });
            if fetch || !fresh {
                fetched = true;
                if let Err(e) = self.fetch_link(&src.kind, &cache) {
                    fetch_error = Some(friendly(&e));
                }
            }
        }
        // The data is about to change. From now until the check has read it, the links of this
        // source are unknown, so no query can join them as if they were clean (V4-D51).
        forget_links(&mut self.state.write().unwrap(), id);
        let result = match &src.kind {
            // A new table is a view over tables of the catalog, written from its spec (V4-D6).
            SourceKind::Derived { spec } => {
                let plan = {
                    let s = self.state.read().unwrap();
                    derived::plan(spec, &s.catalog, &s.ws.relationships, &s.health)
                };
                let db = self.db.lock().unwrap();
                plan.map_err(anyhow::Error::msg)
                    .and_then(|p| load_derived(&db, &src, &p, &setups))
            }
            _ => {
                let db = self.db.lock().unwrap();
                load_source(&db, &src, base.as_deref(), &self.secrets, &setups, &cache)
            }
        };
        let ids = {
            let mut s = self.state.write().unwrap();
            self.finish_load(&mut s, id, result, fetched, fetch_error)
        };
        // The calculated columns come before the links are checked, because a link may use one
        // as its key. A column that reads another table is built again after the check, because a
        // link that was unknown during the first pass is known now (V4-D3, V4-D63).
        let related = self.rebuild_calc(calc::Roots::Source(id));
        // The links of this source's tables are checked once per load (V4-D26), never per query.
        self.check_links(id);
        if !related.is_empty() {
            self.rebuild_calc(calc::Roots::Tables(&related));
        }
        ids
    }

    /// Builds the calculated-column views of the tables `roots` names and of the tables that read
    /// them (V4-D3). Returns the tables that have a column which reads another table.
    fn rebuild_calc(&self, roots: calc::Roots<'_>) -> Vec<String> {
        rebuild_calc_in(&self.db, &self.state, self.today(), roots)
    }

    /// Builds the calculated columns that use `TODAY()` or `NOW()` again when the day has changed
    /// since their view was made (V4-D34). The app calls it when a dashboard refreshes, the
    /// publisher before it publishes, and the agent on each run. Returns whether anything was
    /// built again.
    pub fn refresh_dates(&self) -> bool {
        let today = self.today();
        let stale: Vec<String> = self
            .state
            .read()
            .unwrap()
            .calc_day
            .iter()
            .filter(|(_, day)| **day != today)
            .map(|(id, _)| id.clone())
            .collect();
        if stale.is_empty() {
            return false;
        }
        self.rebuild_calc(calc::Roots::Tables(&stale));
        true
    }

    /// Puts what a load found into the state. Returns the ids of the tables.
    fn finish_load(
        &self,
        s: &mut State,
        id: &str,
        result: anyhow::Result<Vec<Table>>,
        fetched: bool,
        fetch_error: Option<String>,
    ) -> Vec<String> {
        if fetched {
            s.fetched.insert(id.to_string(), SystemTime::now());
        }
        // Under the same lock that puts the new tables in: the links of this source are unknown
        // until the check has read them. A check that began earlier writes nothing (V4-D51).
        forget_links(s, id);
        s.catalog.tables.retain(|t| t.source_id != id);
        // The tables are new, so they have no calculated columns until the next pass.
        let prefix = format!("{id}/");
        s.calc_base.retain(|k, _| !k.starts_with(&prefix));
        s.calc_related.retain(|(k, _)| !k.starts_with(&prefix));
        s.calc_day.retain(|k, _| !k.starts_with(&prefix));
        s.tables_gen += 1;
        s.data_gen += 1;
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

    /// Checks the links that touch the tables of one source against the data and keeps the answer
    /// (V4-D26). It runs once for each load, so a repeated key that a live file save adds shows up
    /// at once, and never for a query.
    ///
    /// A link between local tables is checked here, before this returns: it is quick. A link that
    /// touches a live database table is checked in the background, reading at most
    /// [`LIVE_CHECK_ROWS`] rows of each side, so a load never waits for a database (V4-D51). Until
    /// the answer is in, the link is unknown and joins one row per key.
    fn check_links(&self, source: &str) {
        let prefix = format!("{source}/");
        let (local, live) = self
            .link_jobs(|r| r.from.table.starts_with(&prefix) || r.to.table.starts_with(&prefix));
        let delay = self.check_delay.load(Ordering::Relaxed);
        let rows = self.live_rows.load(Ordering::Relaxed);
        if !local.is_empty() {
            let _ = run_checks(&self.db, &self.state, local, rows, delay);
        }
        if live.is_empty() {
            return;
        }
        let (db, state, listener) = (self.db.clone(), self.state.clone(), self.listener.clone());
        let pending = self.pending.clone();
        let today = self.today();
        pending.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            let changed = run_checks(&db, &state, live, rows, delay);
            // A calculated column that reads across a link whose way is known now is built again
            // (V4-D77): it may have waited for the answer.
            if !changed.is_empty() {
                let tables = tables_of_links(&state, &changed);
                rebuild_calc_in(&db, &state, today, calc::Roots::Tables(&tables));
            }
            pending.fetch_sub(1, Ordering::SeqCst);
            let told = listener.lock().unwrap().clone();
            if let Some(f) = told {
                f();
            }
        });
    }

    /// The links to check, copied out so no lock is held while the data is read: the ones between
    /// local tables first, then the ones that touch a live database table.
    fn link_jobs(&self, wanted: impl Fn(&Relationship) -> bool) -> (Vec<LinkJob>, Vec<LinkJob>) {
        let s = self.state.read().unwrap();
        let mut local = Vec::new();
        let mut live = Vec::new();
        for r in s.ws.relationships.iter().filter(|r| wanted(r)) {
            let sides = s
                .catalog
                .table(&r.to.table)
                .zip(s.catalog.table(&r.from.table))
                .map(|(one, many)| {
                    (
                        (one.clone(), r.to.column.clone()),
                        (many.clone(), r.from.column.clone()),
                    )
                });
            let is_live = sides
                .as_ref()
                .is_some_and(|((one, _), (many, _))| !one.prep || !many.prep);
            let job = LinkJob {
                id: r.id.clone(),
                epoch: s.epoch.get(&r.id).copied().unwrap_or(0),
                sides,
            };
            if is_live {
                live.push(job);
            } else {
                local.push(job);
            }
        }
        (local, live)
    }

    /// Reads the links of live database tables again, now (V4-D51): a refresh of a dashboard, a
    /// publish, and the agent call it, because a database changes without a load. Returns whether
    /// any answer changed. Without live links it does nothing.
    pub fn recheck_live_links(&self) -> bool {
        let (_, live) = self.link_jobs(|_| true);
        if live.is_empty() {
            return false;
        }
        let before = self.state.read().unwrap().health.clone();
        let rows = self.live_rows.load(Ordering::Relaxed);
        let delay = self.check_delay.load(Ordering::Relaxed);
        let ids: Vec<String> = live.iter().map(|j| j.id.clone()).collect();
        let changed = run_checks(&self.db, &self.state, live, rows, delay);
        if !changed.is_empty() {
            let tables = tables_of_links(&self.state, &changed);
            self.rebuild_calc(calc::Roots::Tables(&tables));
        }
        let s = self.state.read().unwrap();
        ids.iter().any(|id| before.get(id) != s.health.get(id))
    }

    /// Is a link check of a live table still running in the background?
    pub fn checks_pending(&self) -> bool {
        self.pending.load(Ordering::SeqCst) > 0
    }

    /// Calls `f` each time such a check has written its answer. The server tells the windows.
    pub fn on_links_checked(&self, f: impl Fn() + Send + Sync + 'static) {
        *self.listener.lock().unwrap() = Some(Arc::new(f));
    }

    /// Reads at most this many rows of a live table when a link is checked (tests make it small).
    pub fn set_live_check_rows(&self, rows: u64) {
        self.live_rows.store(rows, Ordering::Relaxed);
    }

    /// Makes every link check wait before it reads the data (tests hold the moment after a
    /// reload open to see that nothing is counted twice there). The app never calls it.
    pub fn slow_link_checks(&self, wait: Duration) {
        self.check_delay
            .store(wait.as_millis() as u64, Ordering::Relaxed);
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
                _ => anyhow::bail!("{id} is not a web link"),
            }
        }
        self.persist();
        Ok(())
    }

    /// Fetches a web link into its cache file, with the key stored for it. The notes of the fetch
    /// (for example "the link has more pages") are kept beside the cache.
    fn fetch_link(&self, kind: &SourceKind, cache: &Path) -> anyhow::Result<()> {
        let SourceKind::Url {
            url,
            records,
            pages,
            key,
            ..
        } = kind
        else {
            return Ok(());
        };
        let value = link_secret(kind)
            .and_then(|id| self.secrets.get(&id))
            .filter(|v| !v.is_empty());
        let spec = fetch::Spec {
            url,
            records: records.as_deref(),
            pages: pages.as_ref(),
            key: key
                .as_ref()
                .zip(value.as_deref())
                .map(|(k, v)| fetch::KeyArg {
                    place: k.place,
                    name: &k.name,
                    value: v,
                }),
        };
        let notes = fetch::download(&spec, cache)?;
        write_notes(cache, &notes);
        Ok(())
    }

    /// Looks at the first page of a web link: what format it is, which lists of records it holds,
    /// the first rows, and how its pages seem to follow each other. Nothing is saved: not the
    /// copy, and not the key. A key with no value uses the stored one, if there is one.
    pub fn link_preview(&self, req: LinkPreview) -> anyhow::Result<fetch::Preview> {
        let host = fetch::check(&req.url)?;
        let key = req.key.filter(|k| !k.name.trim().is_empty());
        let value = key.as_ref().map(|k| {
            if k.value.is_empty() {
                self.secrets
                    .get(&Secrets::link_key(&host, k.place, k.name.trim()))
                    .unwrap_or_default()
            } else {
                k.value.clone()
            }
        });
        let spec = fetch::Spec {
            url: &req.url,
            records: req.records.as_deref(),
            pages: None,
            key: key
                .as_ref()
                .zip(value.as_deref())
                .map(|(k, v)| fetch::KeyArg {
                    place: k.place,
                    name: k.name.trim(),
                    value: v,
                }),
        };
        fetch::preview(&spec)
    }

    /// Changes where a web link finds its records, how it reads its pages and which key it sends,
    /// and fetches it again. A fetch that fails keeps the last copy and shows its error beside the
    /// source, as for any link (V3-D56). Returns the ids of its tables.
    pub fn set_link(
        &self,
        id: &str,
        records: Option<String>,
        pages: Option<Paging>,
        key: KeyEdit,
    ) -> anyhow::Result<Vec<String>> {
        let (url, old) = {
            let s = self.state.read().unwrap();
            match s.ws.source(id).map(|x| &x.kind) {
                Some(SourceKind::Url { url, key, .. }) => (url.clone(), key.clone()),
                _ => anyhow::bail!("{id} is not a web link"),
            }
        };
        let host = fetch::check(&url)?;
        let old_secret = old
            .as_ref()
            .map(|k| Secrets::link_key(&host, k.place, &k.name));
        let stored = |secret: &str| self.secrets.get(secret).filter(|v| !v.is_empty());
        let (new_key, new_value) = match key {
            KeyEdit::Keep => (old, None),
            KeyEdit::Remove => (None, None),
            KeyEdit::Set(k) => {
                let name = k.name.trim().to_string();
                if name.is_empty() {
                    anyhow::bail!("Type a name for the key.");
                }
                let lk = LinkKey {
                    place: k.place,
                    name,
                };
                let secret = Secrets::link_key(&host, lk.place, &lk.name);
                // No value: keep what is stored, here or under the old name.
                let value = if k.value.is_empty() {
                    stored(&secret)
                        .or_else(|| old_secret.as_deref().and_then(stored))
                        .ok_or_else(|| anyhow::anyhow!("Type the key to send with the link."))?
                } else {
                    k.value
                };
                (Some(lk), Some((secret, value)))
            }
        };
        if let Some((secret, value)) = new_value {
            self.secrets.set(&secret, &value)?;
        }
        {
            let mut s = self.state.write().unwrap();
            let Some(SourceKind::Url {
                records: r,
                pages: p,
                key: k,
                ..
            }) =
                s.ws.sources
                    .iter_mut()
                    .find(|x| x.id == id)
                    .map(|x| &mut x.kind)
            else {
                anyhow::bail!("{id} is not a web link");
            };
            *r = clean(records);
            *p = pages;
            let removed = new_key.is_none();
            *k = new_key;
            // "Remove key" deletes the entry, unless another link here still uses it. A renamed
            // key keeps its old entry: a link in another workspace on this computer may use it.
            if removed
                && let Some(old) = old_secret
                && !s
                    .ws
                    .sources
                    .iter()
                    .any(|x| link_secret(&x.kind).as_deref() == Some(old.as_str()))
            {
                let _ = self.secrets.remove(&old);
            }
        }
        self.persist();
        Ok(self.load(id))
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
        {
            // A calculated column that reads another table cannot be a key: its value would depend
            // on the health of another link (V4-D63).
            let s = self.state.read().unwrap();
            for f in [&a, &b] {
                if s.calc_related
                    .contains(&(f.table.clone(), f.column.clone()))
                {
                    let label = s.catalog.table(&f.table).map_or(&f.table, |t| &t.label);
                    anyhow::bail!(
                        "The column {} of {label} reads another table, so it cannot be the key of a link. Use a column from the file, or a calculated column that reads only its own table.",
                        f.column
                    );
                }
            }
        }
        let rows = self.live_rows.load(Ordering::Relaxed);
        let (from, to, health) = {
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
            // How many keys repeat on each side. The answer for the chosen direction is kept
            // as the health of the new link.
            let (dupes_a, dupes_b) = (
                key_counts(&conn, ta, &a.column, live_limit(ta, rows))?.dups,
                key_counts(&conn, tb, &b.column, live_limit(tb, rows))?.dups,
            );
            let (from, to, one_table, many_table) = match (dupes_a > 0, dupes_b > 0) {
                (true, true) => anyhow::bail!(
                    "Neither {}.{} nor {}.{} has unique values. One side of a relationship must be unique, like an ID, or totals would be counted twice.",
                    ta.label,
                    a.column,
                    tb.label,
                    b.column
                ),
                (false, true) => (b.clone(), a.clone(), ta, tb),
                _ => (a.clone(), b.clone(), tb, ta),
            };
            let health = measure_link(
                &conn,
                (one_table, &to.column),
                (many_table, &from.column),
                rows,
                &mut Memo::default(),
            )?;
            (from, to, health)
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
                name: None,
            });
            record(&mut s, &id, health);
            id
        };
        // A calculated column may have been waiting for this link.
        self.rebuild_calc(calc::Roots::All);
        self.reload_groups();
        self.persist();
        Ok(id)
    }

    /// Builds again the new tables made by a group, and what reads them: a value of a group may read a
    /// related table, and a link changed (V4-D6), as a calculated column does.
    fn reload_groups(&self) {
        let groups: Vec<String> = {
            let s = self.state.read().unwrap();
            derived::load_order(&s.ws.sources)
                .0
                .into_iter()
                .filter(|id| {
                    s.ws.source(id).is_some_and(|x| {
                        matches!(
                            x.kind,
                            SourceKind::Derived {
                                spec: DerivedSpec::Group { .. }
                            }
                        )
                    })
                })
                .collect()
        };
        if groups.is_empty() {
            return;
        }
        let mut ids = Vec::new();
        for g in &groups {
            ids.extend(self.load_one(g, false));
        }
        self.load_readers(&groups, &mut ids);
    }

    pub fn remove_relationship(&self, id: &str) {
        {
            let mut s = self.state.write().unwrap();
            s.ws.relationships.retain(|r| r.id != id);
            s.health.remove(id);
            s.epoch.remove(id);
            s.kinds.remove(id);
        }
        // A calculated column that read across this link says so now.
        self.rebuild_calc(calc::Roots::All);
        self.reload_groups();
        self.persist();
    }

    /// Names a link, or clears its name (`None`). See [`Engine::rename_links`].
    pub fn rename_link(&self, id: &str, name: Option<String>) -> anyhow::Result<()> {
        self.rename_links(BTreeMap::from([(id.to_string(), name)]))
    }

    /// Names several links at once, and checks the names as a set, so two links can swap their
    /// names (V4-D46). A name is how a field or a formula says which link it means when two links
    /// join the same two tables (V4-D28), so it must be new: not another link's name, and not the
    /// label or the id of a table, which a formula reads first. A name has at most 60 letters;
    /// spaces at its ends and doubled spaces go. An empty name clears it, unless another link joins
    /// the same two tables.
    ///
    /// **Whoever stores formulas must follow a rename here.** The formulas of calculated columns
    /// and of measures are rewritten from this function with `formula::rename_link(text, old, new)`,
    /// in the same step that names the link (V4-D46, V4-D85e). Group values (04) join them when
    /// they exist. A link that gives up its name leaves its formulas as they are: they say so.
    pub fn rename_links(&self, names: BTreeMap<String, Option<String>>) -> anyhow::Result<()> {
        {
            let mut s = self.state.write().unwrap();
            // The names as they will be, so the set is checked as a whole.
            let mut next: Vec<(String, Option<String>)> =
                s.ws.relationships
                    .iter()
                    .map(|r| (r.id.clone(), r.name.clone()))
                    .collect();
            for (id, name) in &names {
                let Some(slot) = next.iter_mut().find(|(i, _)| i == id) else {
                    anyhow::bail!("There is no link {id}.");
                };
                slot.1 = name
                    .as_ref()
                    .map(|n| n.split_whitespace().collect::<Vec<_>>().join(" "))
                    .filter(|n| !n.is_empty());
            }
            for id in names.keys() {
                let name = next
                    .iter()
                    .find(|(i, _)| i == id)
                    .and_then(|(_, n)| n.clone());
                match name {
                    Some(n) => {
                        if n.chars().count() > 60 {
                            anyhow::bail!("Keep the name of a link to 60 letters or fewer.");
                        }
                        let same = |a: &str| a.trim().to_lowercase() == n.to_lowercase();
                        if s.catalog
                            .tables
                            .iter()
                            .any(|t| same(&t.label) || same(&t.id))
                        {
                            anyhow::bail!(
                                "A table is called {n}. Give the link another name, so a formula can tell them apart."
                            );
                        }
                        if next
                            .iter()
                            .any(|(i, other)| i != id && other.as_deref().is_some_and(same))
                        {
                            anyhow::bail!(
                                "Another link is called {n}. Give this one another name."
                            );
                        }
                    }
                    None => {
                        let Some(r) = s.ws.relationships.iter().find(|r| &r.id == id) else {
                            continue;
                        };
                        let pair = |x: &Relationship| {
                            (x.from.table == r.from.table && x.to.table == r.to.table)
                                || (x.from.table == r.to.table && x.to.table == r.from.table)
                        };
                        if s.ws.relationships.iter().any(|x| x.id != *id && pair(x)) {
                            anyhow::bail!(
                                "Two links join these tables, so each one needs a name. Give this link a name."
                            );
                        }
                    }
                }
            }
            // The formulas that use a link by its name follow the name.
            let renames: Vec<(String, String)> = next
                .iter()
                .filter_map(|(id, new)| {
                    let old =
                        s.ws.relationships
                            .iter()
                            .find(|r| &r.id == id)?
                            .name
                            .clone()?;
                    let new = new.clone()?;
                    (old != new).then_some((old, new))
                })
                .collect();
            for (id, name) in next {
                if let Some(r) = s.ws.relationships.iter_mut().find(|r| r.id == id) {
                    r.name = name;
                }
            }
            if !renames.is_empty() {
                for setup in s.ws.tables.values_mut() {
                    for c in setup.calc.iter_mut().filter(|c| !c.sql) {
                        c.formula = calc::rename_links_in(&c.formula, &renames);
                    }
                }
                for m in s.ws.measures.iter_mut() {
                    m.formula = calc::rename_links_in(&m.formula, &renames);
                }
            }
        }
        self.rebuild_calc(calc::Roots::All);
        self.persist();
        Ok(())
    }

    // ---- measures (build 03) ---------------------------------------------------------------

    /// The named measures of the workspace.
    pub fn measures(&self) -> Vec<NamedMeasure> {
        self.state.read().unwrap().ws.measures.clone()
    }

    /// Saves the whole list of named measures (V4-D4). A new measure (empty `id`) gets the next
    /// `m<n>`. A measure that is new or whose formula changed is checked by build 01 and refused with
    /// its place when it does not work ([`MeasureRefused`]); one that was saved before and fails now
    /// stays and says so on the tiles that use it, so a rename or a lost column never stops the user
    /// from saving another measure (as V4-D68 does for columns). A measure that tiles use cannot go
    /// without a question: without `confirm` the tiles are returned ([`MeasureInUse`]) and nothing is saved.
    pub fn set_measures(&self, mut list: Vec<NamedMeasure>, confirm: bool) -> anyhow::Result<()> {
        // The formulas as they were sent: a place in an error counts in this text, which is the text the
        // live check read, not the trimmed one that is stored (V4-D105).
        let raw: Vec<String> = list.iter().map(|m| m.formula.clone()).collect();
        let refuse =
            |index: usize, m: &NamedMeasure, field: &'static str, message: &str| MeasureRefused {
                index,
                name: m.name.clone(),
                formula: raw.get(index).cloned().unwrap_or_default(),
                field,
                error: formula::FormulaError::new(message, 0, 0),
            };
        if list.len() > measures::MAX_MEASURES {
            let at = measures::MAX_MEASURES;
            return Err(refuse(
                at,
                &list[at],
                "list",
                &format!(
                    "A workspace can have up to {at} measures. Delete one that is not needed."
                ),
            )
            .into());
        }
        for m in &mut list {
            m.name = m.name.split_whitespace().collect::<Vec<_>>().join(" ");
            m.formula = m.formula.trim().to_string();
            m.id = m.id.trim().to_string();
            if m.description
                .as_deref()
                .is_some_and(|d| d.trim().is_empty())
            {
                m.description = None;
            }
            if m.format
                .as_ref()
                .is_some_and(|f| f.as_object().is_none_or(|o| o.is_empty()))
            {
                m.format = None;
            }
        }
        let (old, catalog, rels, health, dashboards) = {
            let s = self.state.read().unwrap();
            (
                s.ws.measures.clone(),
                s.catalog.clone(),
                s.ws.relationships.clone(),
                s.health.clone(),
                s.ws.dashboards.clone(),
            )
        };
        // Ids: a new measure takes the next number; a repeated id is a mistake of the caller.
        let number = |id: &str| id.strip_prefix('m').and_then(|n| n.parse::<u64>().ok());
        // A tile may still name a measure that was deleted. Its id is not given again, or the tile would
        // quietly turn into another measure.
        let dangling: Vec<String> = dashboards
            .iter()
            .flat_map(|d| {
                d.get("tiles")
                    .and_then(|t| t.as_array())
                    .into_iter()
                    .flatten()
            })
            .flat_map(|t| {
                t.get("measures")
                    .and_then(|m| m.as_array())
                    .into_iter()
                    .flatten()
            })
            .filter_map(|m| m.get("named").and_then(|n| n.as_str()).map(str::to_string))
            .collect();
        let mut next = old
            .iter()
            .map(|m| &m.id)
            .chain(list.iter().map(|m| &m.id))
            .chain(dangling.iter())
            .filter_map(|id| number(id))
            .max()
            .unwrap_or(0);
        for m in &mut list {
            if m.id.is_empty() {
                next += 1;
                m.id = format!("m{next}");
            }
        }
        for (i, m) in list.iter().enumerate() {
            if list[..i].iter().any(|o| o.id == m.id) {
                return Err(
                    refuse(i, m, "list", &format!("Two measures have the id {}.", m.id)).into(),
                );
            }
        }
        for (i, m) in list.iter().enumerate() {
            if m.name.is_empty() {
                return Err(refuse(i, m, "name", "Give the measure a name.").into());
            }
            if m.name.chars().count() > 60 {
                return Err(refuse(
                    i,
                    m,
                    "name",
                    "Keep the name of a measure to 60 letters or fewer.",
                )
                .into());
            }
            let same = |a: &str| a.to_lowercase() == m.name.to_lowercase();
            if list[..i].iter().any(|o| same(&o.name)) {
                return Err(refuse(
                    i,
                    m,
                    "name",
                    &format!(
                        "Another measure is called {}. Give this one another name.",
                        m.name
                    ),
                )
                .into());
            }
        }
        // The measures that are new, or changed.
        let today = self.today();
        let cx = formula::Context {
            catalog: &catalog,
            rels: &rels,
            health: &health,
            today,
            now: today.and_time(chrono::Local::now().time()),
            view: false,
        };
        for (i, m) in list.iter().enumerate() {
            if old.iter().any(|o| o.id == m.id && o.formula == m.formula) {
                continue;
            }
            if m.formula.is_empty() {
                return Err(refuse(
                    i,
                    m,
                    "formula",
                    "Write a formula, such as SUM([Sales].[amount]).",
                )
                .into());
            }
            if raw[i].chars().count() > measures::MAX_FORMULA {
                return Err(refuse(
                    i,
                    m,
                    "formula",
                    &format!(
                        "Keep a formula to {} characters or fewer.",
                        measures::MAX_FORMULA
                    ),
                )
                .into());
            }
            match formula::compile(&raw[i], formula::Kind::Measure, None, &cx) {
                Ok(c) if matches!(c.body, formula::Body::Aggregate(_)) => {}
                Ok(_) => {
                    return Err(refuse(
                        i,
                        m,
                        "formula",
                        "A measure adds things up. Use SUM, AVERAGE, COUNT or another total.",
                    )
                    .into());
                }
                Err(e) => {
                    return Err(MeasureRefused {
                        index: i,
                        name: m.name.clone(),
                        formula: raw[i].clone(),
                        field: "formula",
                        error: e,
                    }
                    .into());
                }
            }
        }
        // A measure that tiles use does not go without a question.
        if !confirm {
            let mut used = Vec::new();
            for o in old.iter().filter(|o| !list.iter().any(|m| m.id == o.id)) {
                let tiles = tiles_using(&dashboards, &o.id);
                if !tiles.is_empty() {
                    used.push(MeasureUse {
                        measure: o.id.clone(),
                        name: o.name.clone(),
                        tiles,
                    });
                }
            }
            if !used.is_empty() {
                return Err(MeasureInUse { used }.into());
            }
        }
        {
            let mut s = self.state.write().unwrap();
            if s.ws.measures == list {
                return Ok(());
            }
            s.ws.measures = list;
        }
        self.persist();
        Ok(())
    }

    /// The question with the groups and filters that `home` cannot reach read as a part of a measure reads
    /// them (V4-D98), for the rows behind a number.
    pub fn narrow_for_rows(
        &self,
        q: &SemanticQuery,
        home: &str,
    ) -> Result<SemanticQuery, query::QueryError> {
        let s = self.state.read().unwrap();
        measures::narrow(
            q,
            &s.catalog,
            &measure_env(&s, self.today(), &NO_GRAINS),
            home,
        )
    }

    /// The type, the additivity and the tables of each measure, by id (see [`measures::MeasureInfo`]).
    pub fn measure_info(&self) -> BTreeMap<String, measures::MeasureInfo> {
        let s = self.state.read().unwrap();
        measures::infos(&measure_env(&s, self.today(), &NO_GRAINS), &s.catalog)
    }

    /// What a question about these measures needs to know about each (see [`measures::Facts`]).
    pub fn measure_facts(&self, m: &query::Measure) -> Result<measures::Facts, query::QueryError> {
        let s = self.state.read().unwrap();
        measures::facts(&measure_env(&s, self.today(), &NO_GRAINS), &s.catalog, m)
    }

    /// What a measure gives where there is nothing to add up: 0 for a sum or a count, and blank for
    /// the rest. A time chart fills a quiet period with it, and a trend column too. Index-aligned
    /// with `ms`.
    pub fn empty_values(&self, ms: &[query::Measure]) -> Vec<serde_json::Value> {
        ms.iter()
            .map(|m| match &m.named {
                None => match m.agg {
                    query::Agg::Sum | query::Agg::Count | query::Agg::CountDistinct
                        if m.per.is_none() =>
                    {
                        serde_json::json!(0.0)
                    }
                    _ => serde_json::Value::Null,
                },
                Some(id) => {
                    let sql = {
                        let s = self.state.read().unwrap();
                        measures::resolve(
                            &measure_env(&s, self.today(), &NO_GRAINS),
                            &s.catalog,
                            id,
                        )
                        .map(|r| r.empty_sql())
                    };
                    sql.ok()
                        .and_then(|sql| {
                            let conn = self.conn().ok()?;
                            run(&conn, &sql, &[]).ok()
                        })
                        .and_then(|rows| rows.into_iter().next()?.into_iter().next())
                        .unwrap_or(serde_json::Value::Null)
                }
            })
            .collect()
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

    /// The workspace as a site (build 12), as the app wrote it.
    pub fn site(&self) -> Option<serde_json::Value> {
        self.state.read().unwrap().ws.site.clone()
    }

    /// Keeps the site settings (build 12). They must be an object; the app cleans them up (`site.ts normaliseSite`).
    /// As with a dashboard, what only the publisher writes (`publish`: the cloud site, its link, the last time, the last
    /// error) is kept from the current copy, so a save from the app never rolls it back (build 14). Settings that name no
    /// `publish` keep the current one whole: only the publisher removes it (`update_site`).
    pub fn set_site(&self, mut site: serde_json::Value) -> anyhow::Result<()> {
        if !site.is_object() {
            anyhow::bail!("The site settings are not readable.");
        }
        {
            let mut s = self.state.write().unwrap();
            let old = s.ws.site.as_ref().and_then(|o| o.get("publish")).cloned();
            match site.get_mut("publish").and_then(|p| p.as_object_mut()) {
                Some(p) => {
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
                None => {
                    if let Some(o) = old {
                        site["publish"] = o;
                    }
                }
            }
            s.ws.site = Some(site);
        }
        self.persist();
        Ok(())
    }

    /// Changes the site settings as they are (the publisher, build 14): nothing is kept from the current copy.
    pub fn update_site(&self, f: impl FnOnce(&mut serde_json::Value)) {
        {
            let mut s = self.state.write().unwrap();
            let mut site = s.ws.site.take().unwrap_or_else(|| serde_json::json!({}));
            f(&mut site);
            s.ws.site = Some(site);
        }
        self.persist();
    }

    /// The card styles of the workspace (build 08).
    pub fn card_styles(&self) -> Vec<serde_json::Value> {
        self.state.read().unwrap().ws.card_styles.clone()
    }

    /// Replaces the card styles (build 08, V4-D10). Each has an id, a name and a card; no two share an id; a name is
    /// trimmed and its spaces folded. A refused list changes nothing.
    pub fn set_card_styles(&self, mut list: Vec<serde_json::Value>) -> anyhow::Result<()> {
        if list.len() > MAX_CARD_STYLES {
            anyhow::bail!("A workspace can have up to {MAX_CARD_STYLES} card styles.");
        }
        let mut ids = HashSet::new();
        for s in &mut list {
            let id = s
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if id.is_empty() {
                anyhow::bail!("A card style needs an id.");
            }
            let name = s
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if name.is_empty() {
                anyhow::bail!("A card style needs a name.");
            }
            if !ids.insert(id.clone()) {
                anyhow::bail!("Two card styles have the id {id}.");
            }
            if !s.get("card").is_some_and(|c| c.is_object()) {
                anyhow::bail!("The card style {name} has no card.");
            }
            s["id"] = serde_json::Value::String(id);
            s["name"] = serde_json::Value::String(name);
        }
        self.state.write().unwrap().ws.card_styles = list;
        self.persist();
        Ok(())
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
        let grains = self.grains_for(q);
        let compiled = {
            let s = self.state.read().unwrap();
            note_blank_calc(&s, q);
            measures::compile(q, &s.catalog, today, &measure_env(&s, today, &grains))?
        };
        note_crossed(&compiled.crossed);
        // A calculated column that failed and that a measure reads: the tile carries its note.
        let blank: Vec<String> = compiled
            .blank_reads
            .iter()
            .map(|(t, c)| format!("{CALC_NOTE}{t}\u{1f}{c}"))
            .collect();
        if !blank.is_empty() {
            note_crossed(&blank);
        }
        let start = std::time::Instant::now();
        let conn = self.conn()?;
        let rows = run(&conn, &compiled.sql, &compiled.params)?;
        let mut res = QueryResult::new(compiled.columns, rows);
        res.ignored_filters = compiled.ignored_filters;
        res.ms = start.elapsed().as_millis();
        res.parts = compiled.parts;
        Ok(res)
    }

    /// The main dates of the question that hold only the first day of a period, read from the data
    /// once for each `data_gen` (V4-D101). A question with no named measure has none.
    fn grains_for(&self, q: &SemanticQuery) -> BTreeMap<(String, String), measures::Period> {
        let mut out = BTreeMap::new();
        if q.measures.iter().all(|m| m.named.is_none()) {
            return out;
        }
        let today = self.today();
        let (wants, gen_now) = {
            let s = self.state.read().unwrap();
            (
                measures::grain_wants(q, &s.catalog, &measure_env(&s, today, &NO_GRAINS)),
                s.data_gen,
            )
        };
        for (table, column, sql_ref) in wants {
            let key = (table, column);
            let cached = self.grain_cache.lock().unwrap().get(&key).copied();
            let period = match cached {
                Some((g, p)) if g == gen_now => p,
                _ => {
                    // A table that cannot be read says nothing: its main date counts as any day.
                    let p = self
                        .conn()
                        .ok()
                        .and_then(|c| run(&c, &measures::grain_sql(&key.1, &sql_ref), &[]).ok())
                        .and_then(|rows| rows.first().and_then(|r| measures::period_of(r)));
                    self.grain_cache
                        .lock()
                        .unwrap()
                        .insert(key.clone(), (gen_now, p));
                    p
                }
            };
            if let Some(p) = period {
                out.insert(key, p);
            }
        }
        out
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
            note_blank_calc(&s, q);
            let sc = query::scope_on(
                q,
                &s.catalog,
                Links::new(&s.ws.relationships, Some(&s.health)).with_link_tables(&s.ws.tables),
                today,
            )?;
            note_crossed(&sc.crossed);
            note_overlap(&sc);
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
            .filter(|c| !c.calc)
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
        // The table as the file gives it, under its calculated columns.
        let base_ref = {
            let s = self.state.read().unwrap();
            s.calc_base.get(table_id).cloned()
        }
        .unwrap_or_else(|| table.sql_ref.clone());
        let cols: Vec<ColInfo> = table
            .columns
            .iter()
            .filter(|c| !c.calc)
            .map(|c| ColInfo {
                name: c.name.clone(),
                dtype: c.dtype,
            })
            .collect();
        let plan = prep::plan_view(&base_ref, &cols, &setup)
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

    /// Sets how one table is prepared: the header row, column overrides, the unpivot and the
    /// calculated columns. `None` clears them. The table's source loads again when the data changes
    /// (header row, types, unpivot); labels, hidden flags and descriptions apply at once; a change
    /// of the calculated columns builds the view of the table again. Returns whether the data of
    /// any table changed, so the tiles should ask again.
    ///
    /// The calculated columns are checked first, each with the checker of build 01 (or the SQL
    /// check of V4-D62). A new or changed one that fails is refused with a [`CalcRefused`] error
    /// that has the place to underline. One that was saved before and fails now is kept: it shows
    /// as blank with a sentence on the table.
    ///
    /// A column that is renamed (the name shown, or the name an unpivot gives) rewrites the formulas
    /// that use the old name, in this table and in the others (V4-D25).
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
        let (table, old, link_names) = {
            let s = self.state.read().unwrap();
            let t = s
                .catalog
                .table(table_id)
                .ok_or_else(|| anyhow::anyhow!("unknown table {table_id}"))?;
            if !t.prep && (setup.header_row.is_some() || setup.changes_view()) {
                anyhow::bail!("This table is read live, so its columns cannot be changed here.");
            }
            // The links that lead to this table, by name: a formula of another table may write a
            // column of this one after the name of a link (V4-D28).
            let links: Vec<String> =
                s.ws.relationships
                    .iter()
                    .filter(|r| r.to.table == table_id)
                    .filter_map(|r| r.name.clone())
                    .collect();
            (
                t.clone(),
                s.ws.tables.get(table_id).cloned().unwrap_or_default(),
                links,
            )
        };
        // A name that two columns would answer to is refused on every save, not only when the
        // calculated columns changed (V4-D77). So is a rename the SQL of a column cannot follow.
        if let Some(why) = calc::label_problem(&table, &old, &setup) {
            anyhow::bail!(why);
        }
        if let Some(why) = calc::unpivot_sql_problem(&old, &setup) {
            anyhow::bail!(why);
        }
        // The main date is a date column of the table (V4-D5), or a calculated column it will have.
        if let Some(d) = setup.main_date.as_deref() {
            let will_have = setup.calc.iter().any(|c| c.name == d);
            match table.column(d) {
                Some(c) if !c.dtype.is_temporal() && !c.calc => {
                    anyhow::bail!("{d} is not a date column, so it cannot be the main date.");
                }
                Some(_) => {}
                None if will_have => {}
                None => anyhow::bail!("The column {d} is not in {}.", table.label),
            }
        }
        // The formulas follow the names that changed. The check below judges what was sent.
        let sent = setup.calc.clone();
        let renames = calc::renamed_columns(&table, &old, &setup);
        for c in setup.calc.iter_mut().filter(|c| !c.sql) {
            c.formula = calc::follow_renames(&c.formula, &table, &renames, true, &[]);
        }
        if setup.calc != old.calc {
            self.check_calc_setup(&table, &old, &setup, &sent)?;
        }
        let calc_changed = setup.calc != old.calc;
        // A tile that adds up this table reads its main date, so the tiles ask again.
        let main_changed = setup.main_date != old.main_date;
        // A table that is marked as a link table (or no longer) changes what a tile can reach.
        let link_changed = setup.link_table != old.link_table;
        let (reload, touched) = {
            let mut s = self.state.write().unwrap();
            // The other tables' formulas that name this table's column, and the measures.
            let mut touched = Vec::new();
            if !renames.is_empty() {
                for m in s.ws.measures.iter_mut() {
                    m.formula =
                        calc::follow_renames(&m.formula, &table, &renames, false, &link_names);
                }
                for (id, other) in
                    s.ws.tables
                        .iter_mut()
                        .filter(|(id, _)| id.as_str() != table_id)
                {
                    let before = other.calc.clone();
                    for c in other.calc.iter_mut().filter(|c| !c.sql) {
                        c.formula =
                            calc::follow_renames(&c.formula, &table, &renames, false, &link_names);
                    }
                    if other.calc != before {
                        touched.push(id.clone());
                    }
                }
            }
            let retyped = |x: &TableSetup| -> BTreeMap<String, _> {
                x.columns
                    .iter()
                    .filter_map(|(n, c)| Some((n.clone(), (c.dtype?, c.date_format))))
                    .collect()
            };
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
            (reload.then(|| table.source_id.clone()), touched)
        };
        let reloaded = reload.is_some();
        if let Some(id) = reload {
            // A link that is still fresh is read from its copy, not downloaded inside this request.
            self.load_with(&id, false);
        } else if calc_changed || !renames.is_empty() {
            // The view of the table, and of the tables that read it, are made again.
            let mut ids = touched;
            ids.push(table_id.to_string());
            self.rebuild_calc(calc::Roots::Tables(&ids));
            // The new tables that read it see its new columns (V4-D6).
            self.load_readers(std::slice::from_ref(&table.source_id), &mut Vec::new());
        }
        self.persist();
        Ok(reloaded || calc_changed || main_changed || link_changed || !renames.is_empty())
    }

    /// The check of a setup's calculated columns before it is saved (see [`Engine::set_table_setup`]).
    fn check_calc_setup(
        &self,
        table: &Table,
        old: &TableSetup,
        new: &TableSetup,
        sent: &[CalcColumn],
    ) -> anyhow::Result<()> {
        let (mut catalog, rels, health, mut setups, bases, memory) = {
            let s = self.state.read().unwrap();
            (
                s.catalog.clone(),
                s.ws.relationships.clone(),
                s.health.clone(),
                s.ws.tables.clone(),
                s.calc_base.clone(),
                s.calc_memory.clone(),
            )
        };
        if new.calc.len() > calc::MAX_COLUMNS {
            anyhow::bail!(
                "A table can have up to {} calculated columns. Remove some, or put a part of the work in a measure.",
                calc::MAX_COLUMNS
            );
        }
        // A column that a link uses as its key stays.
        for r in &rels {
            for f in [&r.from, &r.to].into_iter().filter(|f| f.table == table.id) {
                let was = old.calc.iter().any(|c| c.name == f.column);
                if was && !new.calc.iter().any(|c| c.name == f.column) {
                    anyhow::bail!(
                        "The column {} is the key of a link ({}). Remove the link in Model first.",
                        f.column,
                        links::link_label(r, &catalog)
                    );
                }
            }
        }
        if new.is_empty() {
            setups.remove(&table.id);
        } else {
            setups.insert(table.id.clone(), new.clone());
        }
        // The formulas are read with the names the table will show.
        if let Some(t) = catalog.tables.iter_mut().find(|t| t.id == table.id) {
            stamp_setup(t, Some(new));
        }
        let today = self.today();
        let world = calc::World {
            catalog: &catalog,
            rels: &rels,
            health: &health,
            setups: &setups,
            bases: &bases,
            memory: &memory,
            today,
            now: today.and_time(chrono::Local::now().time()),
        };
        let checked = {
            let db = self.db.lock().unwrap();
            calc::check_columns(&db, &world, &table.id)
        };
        for bad in checked.failing {
            let c = &new.calc[bad.index];
            // Saved before, as it is, and what it can read has not moved: not this save's doing.
            let as_sent = sent.get(bad.index).unwrap_or(c);
            let unchanged = old
                .calc
                .iter()
                .position(|o| o.name == c.name && o.formula == as_sent.formula && o.sql == c.sql)
                .is_some_and(|at| {
                    // Every column that was above it is still above it, or gone. A column that
                    // moved below it changes what the formula can read.
                    old.calc[..at].iter().all(|o| {
                        new.calc[..bad.index].iter().any(|n| n.name == o.name)
                            || !new.calc.iter().any(|n| n.name == o.name)
                    })
                });
            if !unchanged {
                // The place is in the text the engine checked, which a rename may have rewritten.
                // The user sees the text that was sent, so the place is moved onto it.
                let (at, len) =
                    calc::map_back(&as_sent.formula, &c.formula, bad.error.at, bad.error.len);
                return Err(CalcRefused {
                    index: bad.index,
                    name: c.name.clone(),
                    formula: as_sent.formula.clone(),
                    error: formula::FormulaError::new(bad.error.message, at, len),
                }
                .into());
            }
        }
        // A column that reads another table is not a key.
        for r in &rels {
            for f in [&r.from, &r.to].into_iter().filter(|f| f.table == table.id) {
                if checked.related.contains(&f.column) {
                    anyhow::bail!(
                        "The column {} reads another table, so it cannot be the key of a link ({}). Remove the link in Model first.",
                        f.column,
                        links::link_label(r, &catalog)
                    );
                }
            }
        }
        Ok(())
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

    /// The date column a time group of `table` uses (V4-D5): the one picked, else its only date column.
    pub fn main_date_of(&self, table: &str) -> Option<String> {
        let s = self.state.read().unwrap();
        measures::main_date(&s.catalog, &s.ws.tables, table).map(|c| c.name.clone())
    }

    pub fn catalog(&self) -> Catalog {
        self.state.read().unwrap().catalog.clone()
    }

    /// What a link is called: its name, or the one-side table with the many-side column.
    pub fn link_label(&self, id: &str) -> Option<String> {
        let s = self.state.read().unwrap();
        let r = s.ws.relationships.iter().find(|r| r.id == id)?;
        Some(links::link_label(r, &s.catalog))
    }

    /// Runs `f` and returns what it returns with the links its queries crossed that have
    /// something to tell the reader. Every query that `f` runs counts, so a tile's spark line
    /// and its side questions are included, not only its main one (V4-D45).
    pub fn collecting<R>(&self, f: impl FnOnce() -> R) -> (R, Vec<String>) {
        let before = CROSSED.replace(Some(Vec::new()));
        let result = f();
        let found = CROSSED.replace(before).unwrap_or_default();
        (result, found)
    }

    /// What the reader of a result in the app should know about those links: a key that repeats
    /// on the side joined into, so one row of it was used (V4-D26). Nothing is read from the
    /// data; the sentences come from the health made when the tables loaded. They name the keys
    /// and the files, and stay in the app.
    pub fn link_notes(&self, crossed: &[String]) -> Vec<String> {
        let s = self.state.read().unwrap();
        let sources = source_lookup(&s);
        notes_of(&s, crossed, false, |r, h| {
            links::tile_note(r, h, &s.catalog, &sources)
        })
    }

    /// The same notes with no key value and no file name, for what leaves the computer (V4-D52).
    pub fn public_link_notes(&self, crossed: &[String]) -> Vec<String> {
        let s = self.state.read().unwrap();
        notes_of(&s, crossed, true, |r, h| {
            links::public_note(r, h, &s.catalog)
        })
    }

    /// What is wrong with the calculated columns, one sentence each, in words that name the table
    /// and the column and nothing else: no formula, no file name, no value (V4-D52). For what
    /// writes to a log or leaves the computer, such as `tenun snapshot` and the agent (V4-D77).
    pub fn public_table_problems(&self) -> Vec<String> {
        let s = self.state.read().unwrap();
        let mut out = Vec::new();
        for t in &s.catalog.tables {
            for c in t.columns.iter().filter(|c| c.error.is_some()) {
                out.push(format!("{}: {} could not be computed.", t.label, c.name));
            }
            if t.notes.iter().any(|n| n.starts_with(calc::NOT_MADE)) {
                out.push(format!("{}: a calculated column was not made.", t.label));
            }
        }
        out
    }

    /// Checks a formula against the open workspace and runs a preview of it (the first 5 values
    /// of a column, the grand total of a measure). The answer holds no SQL.
    pub fn check_formula(
        &self,
        req: &formula::CheckRequest,
    ) -> anyhow::Result<formula::CheckReply> {
        self.check_calc(req, None, false)
    }

    /// Like [`Engine::check_formula`], for a calculated column that is being written. `before` is
    /// the name of the column being edited: the columns of the table from it on are not there for
    /// the formula, because a calculated column may use an earlier one, never itself or a later
    /// one (V4-D3). Not given: every column is there. With `sql` the formula is raw DuckDB SQL,
    /// checked by the rules of V4-D62.
    pub fn check_calc(
        &self,
        req: &formula::CheckRequest,
        before: Option<&str>,
        sql: bool,
    ) -> anyhow::Result<formula::CheckReply> {
        let (mut catalog, rels, health) = {
            let s = self.state.read().unwrap();
            (
                s.catalog.clone(),
                s.ws.relationships.clone(),
                s.health.clone(),
            )
        };
        let table_id = req
            .table
            .as_deref()
            .and_then(|t| formula::check::find_table(&catalog, t))
            .map(|t| t.id.clone());
        // The table is seen as it is above the column being written.
        if let (Some(id), Some(name)) = (&table_id, before)
            && let Some(t) = catalog.tables.iter_mut().find(|t| &t.id == id)
            && let Some(at) = t
                .columns
                .iter()
                .position(|c| c.calc && c.name == name.trim())
        {
            let keep = t.columns[..at].to_vec();
            t.columns = keep;
        }
        let today = self.today();
        let cx = formula::Context {
            catalog: &catalog,
            rels: &rels,
            health: &health,
            today,
            now: today.and_time(chrono::Local::now().time()),
            // A column formula is read the way a view reads it: one row per key (V4-D63).
            view: req.kind == formula::Kind::Column,
        };
        let conn = self.conn()?;
        if sql {
            return Ok(formula::answer_sql(req, &cx, &conn));
        }
        Ok(formula::answer(req, &cx, &conn))
    }
}

/// The calculated columns of each table that could not be computed, by table id, in words for
/// the app (the reason can quote the formula).
fn table_errors_of(s: &State) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for t in &s.catalog.tables {
        let bad: Vec<String> = t
            .columns
            .iter()
            .filter_map(|c| Some(format!("{}: {}", c.name, c.error.as_ref()?)))
            .chain(
                t.notes
                    .iter()
                    .filter(|n| n.starts_with(calc::NOT_MADE))
                    .cloned(),
            )
            .collect();
        if !bad.is_empty() {
            out.insert(t.id.clone(), bad.join(" "));
        }
    }
    out
}

/// One pass of the calculated columns (see [`Engine::rebuild_calc`]). It takes the handles the
/// engine holds instead of the engine, so the thread that checks live links can run it when an
/// answer changes (V4-D77).
fn rebuild_calc_in(
    db: &Mutex<Connection>,
    state: &RwLock<State>,
    today: NaiveDate,
    roots: calc::Roots<'_>,
) -> Vec<String> {
    for _ in 0..3 {
        let (catalog, rels, health, setups, bases, memory, generation) = {
            let s = state.read().unwrap();
            (
                s.catalog.clone(),
                s.ws.relationships.clone(),
                s.health.clone(),
                s.ws.tables.clone(),
                s.calc_base.clone(),
                s.calc_memory.clone(),
                s.tables_gen,
            )
        };
        let world = calc::World {
            catalog: &catalog,
            rels: &rels,
            health: &health,
            setups: &setups,
            bases: &bases,
            memory: &memory,
            today,
            now: today.and_time(chrono::Local::now().time()),
        };
        let out = {
            let db = db.lock().unwrap();
            calc::rebuild(&db, &world, &roots, true, true)
        };
        let mut s = state.write().unwrap();
        if s.tables_gen != generation {
            // A load put new tables in while this pass ran. Do it again on them.
            continue;
        }
        if !out.built.is_empty() {
            s.data_gen += 1;
        }
        for b in out.built {
            let id = b.table.id.clone();
            let Some(slot) = s.catalog.tables.iter_mut().find(|t| t.id == id) else {
                continue;
            };
            *slot = b.table;
            match b.base {
                Some(base) => s.calc_base.insert(id.clone(), base),
                None => s.calc_base.remove(&id),
            };
            s.calc_related.retain(|(t, _)| *t != id);
            for m in b.columns.iter().filter(|m| m.related && m.error.is_none()) {
                s.calc_related.insert((id.clone(), m.name.clone()));
            }
            if b.columns.iter().any(|m| m.uses_time) {
                s.calc_day.insert(id.clone(), today);
            } else {
                s.calc_day.remove(&id);
            }
            // What the next pass, and the next reload, need to know.
            let names: BTreeSet<&String> = b.columns.iter().map(|m| &m.name).collect();
            let types = s.calc_memory.types.entry(id.clone()).or_default();
            types.retain(|name, _| names.contains(name));
            for m in &b.columns {
                if let Some(d) = m.dtype {
                    types.insert(m.name.clone(), d);
                }
            }
            if types.is_empty() {
                s.calc_memory.types.remove(&id);
            }
            if b.reads.is_empty() {
                s.calc_memory.reads.remove(&id);
            } else {
                s.calc_memory.reads.insert(id, b.reads);
            }
        }
        return out.related_tables;
    }
    Vec::new()
}

/// The ids of the tables at the two ends of these links.
fn tables_of_links(state: &RwLock<State>, links: &[String]) -> Vec<String> {
    let s = state.read().unwrap();
    let mut out: Vec<String> = Vec::new();
    for r in s.ws.relationships.iter().filter(|r| links.contains(&r.id)) {
        for t in [&r.from.table, &r.to.table] {
            if !out.contains(t) {
                out.push(t.clone());
            }
        }
    }
    out
}

/// The name of a source and how many tables it holds, for the sentences about a link.
fn source_lookup(s: &State) -> impl Fn(&str) -> Option<(String, usize)> + '_ {
    move |id| {
        let src = s.ws.source(id)?;
        let n = s
            .catalog
            .tables
            .iter()
            .filter(|t| t.source_id == id)
            .count();
        Some((src.name.clone(), n))
    }
}

/// The health of every checked link, as the app shows it.
fn health_out(s: &State) -> BTreeMap<String, HealthOut> {
    let lookup = source_lookup(s);
    s.ws.relationships
        .iter()
        .filter_map(|r| {
            let h = s.health.get(&r.id)?;
            Some((r.id.clone(), links::health_out(r, h, &s.catalog, &lookup)))
        })
        .collect()
}

/// What named measures are read against: the measures, the main dates, the links and the clock.
fn measure_env<'a>(
    s: &'a State,
    today: NaiveDate,
    grains: &'a BTreeMap<(String, String), measures::Period>,
) -> measures::Env<'a> {
    measures::Env {
        measures: &s.ws.measures,
        tables: &s.ws.tables,
        rels: &s.ws.relationships,
        health: &s.health,
        now: today.and_time(chrono::Local::now().time()),
        grains,
    }
}

/// No main date is read as a period: what a caller that only asks about types and tables uses.
static NO_GRAINS: BTreeMap<(String, String), measures::Period> = BTreeMap::new();

/// The id a tile note uses for a calculated column that could not be computed.
const CALC_NOTE: &str = "calc:";

/// The id a tile note uses for a sentence that is its own words: the groups of a question that sit behind
/// a link table put a row under several of them (V4-D112). The sentence names table labels only.
const OVERLAP_NOTE: &str = "overlap:";

/// Tells the tile that its groups overlap, or that a filter behind a link table can keep a row twice over
/// (V4-D125), for a question that runs as a custom SELECT and has no `parts` to say it with.
fn note_overlap(sc: &query::Scope<'_>) {
    let notes: Vec<String> = sc
        .overlap_notes()
        .into_iter()
        .chain(sc.filter_notes())
        .map(|n| format!("{OVERLAP_NOTE}{n}"))
        .collect();
    if !notes.is_empty() {
        note_crossed(&notes);
    }
}

/// Tells the tile that a calculated column it reads is blank because it could not be computed.
fn note_blank_calc(s: &State, q: &SemanticQuery) {
    let mut fields: Vec<(&str, &str)> = Vec::new();
    for m in &q.measures {
        if let Some(c) = &m.column {
            fields.push((&m.table, c));
        }
        if let Some(p) = m
            .per
            .as_ref()
            .and_then(|p| p.column.as_ref().map(|c| (&p.table, c)))
        {
            fields.push((p.0, p.1));
        }
    }
    fields.extend(
        q.dimensions
            .iter()
            .map(|d| (d.table.as_str(), d.column.as_str())),
    );
    fields.extend(
        q.filters
            .iter()
            .map(|f| (f.table.as_str(), f.column.as_str())),
    );
    let ids: Vec<String> = fields
        .into_iter()
        .filter(|(t, c)| {
            s.catalog
                .table(t)
                .and_then(|t| t.column(c))
                .is_some_and(|c| c.error.is_some())
        })
        .map(|(t, c)| format!("{CALC_NOTE}{t}\u{1f}{c}"))
        .collect();
    if !ids.is_empty() {
        note_crossed(&ids);
    }
}

/// What the reader should know about a blank calculated column, in the app (`public` false: the
/// reason, in words) or anywhere else (`public` true: the table and the column only, because a
/// formula can hold a text the author typed).
fn blank_calc_note(s: &State, id: &str, public: bool) -> Option<String> {
    let (table, column) = id.strip_prefix(CALC_NOTE)?.split_once('\u{1f}')?;
    let t = s.catalog.table(table)?;
    let c = t.column(column)?;
    let reason = c.error.as_deref()?;
    Some(if public {
        format!("{}: {} could not be computed.", t.label, column)
    } else {
        format!(
            "{}: {} could not be computed, so its values are blank. {}",
            t.label, column, reason
        )
    })
}

/// The sentences of the links in `crossed` that have one, once each.
fn notes_of(
    s: &State,
    crossed: &[String],
    public: bool,
    say: impl Fn(&Relationship, &LinkHealth) -> Option<String>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in crossed {
        if let Some(n) = id.strip_prefix(OVERLAP_NOTE) {
            if !out.iter().any(|x| x == n) {
                out.push(n.to_string());
            }
            continue;
        }
        if id.starts_with(CALC_NOTE) {
            if let Some(n) = blank_calc_note(s, id, public)
                && !out.contains(&n)
            {
                out.push(n);
            }
            continue;
        }
        if let (Some(r), Some(h)) = (
            s.ws.relationships.iter().find(|r| &r.id == id),
            s.health.get(id),
        ) && let Some(n) = say(r, h)
            && !out.contains(&n)
        {
            out.push(n);
        }
    }
    out
}

/// Why a link's name no longer works: a table has that name now (its label or its id), or another
/// link has (V4-D46). A formula reads a table first, so the link would never be found.
fn link_clashes(s: &State) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for r in &s.ws.relationships {
        let Some(n) = r.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) else {
            continue;
        };
        let same = |a: &str| a.trim().to_lowercase() == n.to_lowercase();
        if s.catalog
            .tables
            .iter()
            .any(|t| same(&t.label) || same(&t.id))
        {
            out.insert(
                r.id.clone(),
                format!(
                    "A table is called {n} too, so a formula reads the table, not this link. Give the link another name."
                ),
            );
        } else if s
            .ws
            .relationships
            .iter()
            .any(|o| o.id != r.id && o.name.as_deref().is_some_and(same))
        {
            out.insert(
                r.id.clone(),
                format!("Another link is called {n}. Give one of them another name."),
            );
        }
    }
    out
}

/// The links that touch a source are unknown from now on: their record goes, and a check that
/// began before this writes nothing (V4-D51).
fn forget_links(s: &mut State, source: &str) {
    let prefix = format!("{source}/");
    let ids: Vec<String> =
        s.ws.relationships
            .iter()
            .filter(|r| r.from.table.starts_with(&prefix) || r.to.table.starts_with(&prefix))
            .map(|r| r.id.clone())
            .collect();
    for id in ids {
        s.health.remove(&id);
        *s.epoch.entry(id).or_insert(0) += 1;
    }
}

/// Keeps the answer for a link, and remembers what kind it was before the data changed it.
fn record(s: &mut State, id: &str, mut h: LinkHealth) {
    let kind = h.kind();
    let was = s.kinds.get(id).and_then(|(last, was)| {
        let origin = was.unwrap_or(*last);
        (kind != origin).then_some(origin)
    });
    h.was = was;
    s.kinds.insert(id.to_string(), (kind, was));
    s.health.insert(id.to_string(), h);
}

/// One link to check: the one side and the many side, each a table and a column, and how many
/// times the record of the link had been thrown away when the job was made. `sides` is `None`
/// while one of the two tables is not loaded.
struct LinkJob {
    id: String,
    epoch: u64,
    sides: Option<((Table, String), (Table, String))>,
}

/// Reads the data of each link and writes the answers (V4-D26, V4-D51). A link whose record was
/// thrown away since the job was made is left alone: a newer load owns it. A check that fails
/// leaves the link unknown, which reads one row per key.
fn run_checks(
    db: &Mutex<Connection>,
    state: &RwLock<State>,
    jobs: Vec<LinkJob>,
    live_rows: u64,
    delay_ms: u64,
) -> Vec<String> {
    let mut changed: Vec<String> = Vec::new();
    if delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
    let conn = db.lock().unwrap().try_clone().ok();
    // The same column in several links is read once.
    let mut memo = Memo::default();
    for job in jobs {
        let health = match (&job.sides, &conn) {
            (Some(((one, oc), (many, mc))), Some(conn)) => {
                measure_link(conn, (one, oc), (many, mc), live_rows, &mut memo).ok()
            }
            _ => None,
        };
        let mut s = state.write().unwrap();
        if !s.ws.relationships.iter().any(|r| r.id == job.id)
            || s.epoch.get(&job.id).copied().unwrap_or(0) != job.epoch
        {
            continue;
        }
        // What a path can do with the link: unknown, or which ways it may be crossed.
        let way = |s: &State| {
            s.health
                .get(&job.id)
                .map(|h| (h.forward_ok(), h.backward_ok()))
        };
        let before = way(&s);
        match health {
            Some(h) => record(&mut s, &job.id, h),
            None => {
                s.health.remove(&job.id);
            }
        }
        if way(&s) != before {
            changed.push(job.id.clone());
        }
    }
    changed
}

/// What a check read of one column, kept while the checks of one load run.
#[derive(Default)]
struct Memo {
    counts: HashMap<(String, String), Counts>,
    ones: HashMap<(String, String), OneSide>,
}

/// How many rows of a column have a value, how many repeat one, and whether only the first rows
/// were read.
#[derive(Clone, Default)]
struct Counts {
    rows: u64,
    dups: u64,
    /// Set when only the first rows were read: how many.
    limit: Option<u64>,
}

/// What the one side of a link says: its counts, how many keys repeat, and the most repeated few
/// with their row counts.
#[derive(Clone, Default)]
struct OneSide {
    counts: Counts,
    keys: u64,
    sample: Vec<serde_json::Value>,
    sample_rows: Vec<u64>,
}

/// A live database table is read up to `rows` rows when a link is checked; a local table whole.
fn live_limit(t: &Table, rows: u64) -> Option<u64> {
    (!t.prep).then_some(rows)
}

/// The keys of a column as a relation `(k)`: all of them, or the first `limit` rows.
fn keys_of(t: &Table, column: &str, limit: Option<u64>) -> String {
    let q = ident(column);
    match limit {
        Some(n) => format!("(SELECT {q} AS k FROM {} LIMIT {n})", t.sql_ref),
        None => format!("(SELECT {q} AS k FROM {})", t.sql_ref),
    }
}

/// `count(key)` and `count(key) - count(DISTINCT key)` of a column. Blank keys are not counted:
/// they never match a row.
fn key_counts(
    conn: &Connection,
    t: &Table,
    column: &str,
    limit: Option<u64>,
) -> anyhow::Result<Counts> {
    let rows = run(
        conn,
        &format!(
            "SELECT CAST(count(*) AS DOUBLE), CAST(count(k) AS DOUBLE), \
             CAST(count(k) - count(DISTINCT k) AS DOUBLE) FROM {}",
            keys_of(t, column, limit)
        ),
        &[],
    )?;
    let num = |i: usize| {
        rows.first()
            .and_then(|r| r.get(i))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u64
    };
    Ok(Counts {
        rows: num(1),
        dups: num(2),
        limit: limit.filter(|n| num(0) >= *n),
    })
}

fn measure_one_side(
    conn: &Connection,
    t: &Table,
    column: &str,
    limit: Option<u64>,
) -> anyhow::Result<OneSide> {
    let counts = key_counts(conn, t, column, limit)?;
    if counts.dups == 0 {
        return Ok(OneSide {
            counts,
            ..OneSide::default()
        });
    }
    let src = keys_of(t, column, limit);
    // The key as text, so a large whole number is not written as 1.2e18 and the answer reads the
    // same in JSON for any column type.
    let key = match t.column(column).map(|c| c.dtype) {
        Some(DType::Number) => "CASE WHEN CAST(k AS DOUBLE) = floor(CAST(k AS DOUBLE)) \
             AND abs(CAST(k AS DOUBLE)) < 1e15 THEN CAST(CAST(k AS BIGINT) AS VARCHAR) \
             ELSE CAST(k AS VARCHAR) END"
            .to_string(),
        _ => "CAST(k AS VARCHAR)".to_string(),
    };
    let found = run(
        conn,
        &format!(
            "SELECT {key}, CAST(count(*) AS DOUBLE) FROM {src} WHERE k IS NOT NULL \
             GROUP BY k HAVING count(*) > 1 ORDER BY 2 DESC, 1 LIMIT {}",
            links::SAMPLE_KEYS
        ),
        &[],
    )?;
    let count = run(
        conn,
        &format!(
            "SELECT CAST(count(*) AS DOUBLE) FROM (SELECT 1 FROM {src} WHERE k IS NOT NULL \
             GROUP BY k HAVING count(*) > 1)"
        ),
        &[],
    )?;
    let keys = count
        .first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64;
    Ok(OneSide {
        counts,
        keys,
        sample: found.iter().map(|r| r[0].clone()).collect(),
        sample_rows: found
            .iter()
            .map(|r| r[1].as_f64().unwrap_or(0.0) as u64)
            .collect(),
    })
}

/// How many rows of the many side have a key that repeats on the one side.
fn many_hits(
    conn: &Connection,
    one: (&Table, &str),
    many: (&Table, &str),
    rows: u64,
) -> anyhow::Result<u64> {
    let (ot, oc) = one;
    let (mt, mc) = many;
    let text = match (ot.column(oc), mt.column(mc)) {
        (Some(a), Some(b)) => a.dtype != b.dtype,
        _ => true,
    };
    let key = |x: &str| {
        if text {
            format!("CAST({x} AS VARCHAR)")
        } else {
            x.to_string()
        }
    };
    let found = run(
        conn,
        &format!(
            "SELECT CAST(count(*) AS DOUBLE) FROM {} m WHERE {} IN \
             (SELECT {} FROM {} WHERE k IS NOT NULL GROUP BY {} HAVING count(*) > 1)",
            keys_of(mt, mc, live_limit(mt, rows)),
            key("m.k"),
            key("k"),
            keys_of(ot, oc, live_limit(ot, rows)),
            key("k"),
        ),
        &[],
    )?;
    Ok(found
        .first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64)
}

/// What the data of the two columns of a link says.
fn measure_link(
    conn: &Connection,
    one: (&Table, &str),
    many: (&Table, &str),
    rows: u64,
    memo: &mut Memo,
) -> anyhow::Result<LinkHealth> {
    let key = |t: &Table, c: &str| (t.id.clone(), c.to_string());
    let many_counts = match memo.counts.get(&key(many.0, many.1)) {
        Some(c) => c.clone(),
        None => {
            let c = key_counts(conn, many.0, many.1, live_limit(many.0, rows))?;
            memo.counts.insert(key(many.0, many.1), c.clone());
            c
        }
    };
    let side = match memo.ones.get(&key(one.0, one.1)) {
        Some(o) => o.clone(),
        None => {
            let o = measure_one_side(conn, one.0, one.1, live_limit(one.0, rows))?;
            memo.ones.insert(key(one.0, one.1), o.clone());
            o
        }
    };
    let hits = if side.counts.dups > 0 && many_counts.rows > 0 {
        many_hits(conn, one, many, rows)?
    } else {
        0
    };
    Ok(LinkHealth {
        one_rows: side.counts.rows,
        one_dups: side.counts.dups,
        one_keys: side.keys,
        sample: side.sample,
        sample_rows: side.sample_rows,
        many_rows: many_counts.rows,
        many_dups: many_counts.dups,
        many_hits: hits,
        limit: many_counts.limit.or(side.counts.limit),
        was: None,
    })
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

/// The note file that goes with a link's cache: what the last fetch had to say about the table.
fn notes_file(cache: &Path) -> PathBuf {
    cache.with_extension("notes")
}

fn write_notes(cache: &Path, notes: &[String]) {
    let file = notes_file(cache);
    if notes.is_empty() {
        let _ = std::fs::remove_file(file);
    } else {
        let _ = std::fs::write(file, notes.join("\n"));
    }
}

fn read_notes(cache: &Path) -> Vec<String> {
    std::fs::read_to_string(notes_file(cache))
        .map(|t| {
            t.lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The secrets entry of a link's key, when the link has one.
fn link_secret(kind: &SourceKind) -> Option<String> {
    let SourceKind::Url {
        url, key: Some(k), ..
    } = kind
    else {
        return None;
    };
    let host = fetch::check(url).ok()?;
    Some(Secrets::link_key(&host, k.place, &k.name))
}

/// A setting typed by a person: trimmed, and nothing when empty.
fn clean(v: Option<String>) -> Option<String> {
    v.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
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
    calc::schema_of(id)
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
        // A calculated column carries its own description when the column menu has none.
        let own = c
            .calc
            .then(|| {
                setup.and_then(|s| {
                    s.calc
                        .iter()
                        .find(|x| x.name == c.name)
                        .and_then(|x| x.description.clone())
                })
            })
            .flatten();
        c.description = o
            .and_then(|o| o.description.clone())
            .or(own)
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
            // What the fetch had to say: more pages than were read, columns left out.
            if let Some(t) = tables.iter_mut().find(|t| t.name == name) {
                t.notes.extend(read_notes(cache));
            }
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
        // The engine makes a new table from the catalog (`load_derived`), never from here.
        SourceKind::Derived { .. } => anyhow::bail!("a new table is made from other tables"),
    }
}

/// Creates the table `name` as a view over a SELECT. When the table has changes (types, unpivot)
/// the SELECT sits under a second view that holds them.
/// Makes the view of a new table and reads it into the catalog (V4-D6). The setup of the table (types,
/// unpivot) applies as for a file. A merge gets its notes: the rows that found a match, and the keys the
/// right table repeats (V4-D126).
fn load_derived(
    db: &Connection,
    src: &Source,
    plan: &derived::Plan,
    setups: &BTreeMap<String, TableSetup>,
) -> anyhow::Result<Vec<Table>> {
    let schema = schema_of(&src.id);
    // A view of an older spec under another name goes: the schema holds this table only.
    db.execute_batch(&format!(
        "DROP SCHEMA IF EXISTS {q} CASCADE; CREATE SCHEMA {q};",
        q = ident(&schema)
    ))?;
    let outcome = make_table(db, &schema, &src.name, &plan.sql, setups)?;
    let mut tables = catalog_local(db, src, &schema)?;
    stamp_table_outcome(&mut tables, &src.name, outcome);
    if let Some(m) = &plan.merge
        && let Some(t) = tables.iter_mut().find(|t| t.name == src.name)
    {
        t.notes.extend(merge_notes(db, m));
    }
    Ok(tables)
}

/// The words under a merge, from its counts.
fn merge_notes(db: &Connection, m: &derived::MergeCounts) -> Vec<String> {
    let row = run(db, &m.matched, &[])
        .ok()
        .and_then(|r| r.into_iter().next())
        .unwrap_or_default();
    let n = |i: usize| row.get(i).and_then(serde_json::Value::as_i64).unwrap_or(0);
    let repeated = count_of(db, &m.repeated);
    m.notes(n(0), n(1), repeated)
}

/// The one number a `SELECT count(*) …` answers, or 0.
fn count_of(db: &Connection, sql: &str) -> i64 {
    run(db, sql, &[])
        .ok()
        .and_then(|r| r.into_iter().next())
        .and_then(|r| r.first().and_then(serde_json::Value::as_i64))
        .unwrap_or(0)
}

/// The columns a SELECT gives, with their types.
fn describe(db: &Connection, sql: &str) -> anyhow::Result<Vec<Column>> {
    let mut stmt = db.prepare(&format!("DESCRIBE {sql}"))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(name, ty)| Column::new(name, DType::from_duckdb(&ty), ty))
        .collect())
}

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
        if name.starts_with("__stage_") || name.starts_with("__raw_") || name.starts_with("__calc_")
        {
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
