//! `tenun mcp` (v5 build 14, V5-D18, V5-D22): the user's own AI app (Claude Desktop, Claude Code, any MCP client)
//! reads the workspace and builds dashboards in it. Tenun calls no AI and sends nothing anywhere: the AI app sends
//! what it reads to its own company, and the docs say so.
//!
//! The protocol is MCP over stdio (JSON-RPC 2.0, one message a line). The server is dual-era: a client of the current
//! revision (2026-07-28) sends its version with each request and may ask `server/discover`; an older client opens with
//! `initialize`. Both get the same tools.
//!
//! One writer: when the app or `tenun serve` has the same workspace open, every call goes to that server
//! (`POST /api/mcp/call`), so one process writes the file and the app shows each change at once. Otherwise this process
//! opens the workspace itself. The tools are the same code in both: [`call`].
//!
//! Safety: no SQL, no paths, no publishing, no secrets. A column whose name says it holds a password or a key is
//! never read. Each change is recorded, and `undo` (or Undo in the app) takes the last one back.

use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tenun_core::Engine;
use tenun_core::formula::{CheckRequest, Kind};
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot;
use tenun_core::workspace::NamedMeasure;

/// The protocol revision without a handshake, and the ones before it, which open with `initialize`.
pub const MODERN: &[&str] = &["2026-07-28"];
pub const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
/// How long a client may keep the answer of `server/discover` and `tools/list`: the tools never change while the
/// server runs, and they are the same for every client (2026-07-28 caching: `ttlMs`, `cacheScope`; Claude Code
/// refuses a list without them).
const TTL_MS: u64 = 3_600_000;

/// A query answers at most this many rows; a dashboard's tile shows its first 50.
pub const MAX_ROWS: u32 = 500;
const TILE_ROWS: usize = 50;
/// A formula has at most this many characters; a name or a title this many.
pub const MAX_FORMULA: usize = 4000;
const MAX_NAME: usize = 120;

const TILE_TYPES: &[&str] = &[
    "kpi",
    "bar",
    "line",
    "area",
    "share",
    "donut",
    "pie",
    "table",
    "scatter",
    "histogram",
    "waterfall",
    "pivot",
    "text",
    "filter",
    "heatmap",
    "funnel",
    "box",
    "bullet",
    "compare",
    "sankey",
    "calendar",
    "cohort",
    "gantt",
    "map",
];

const INSTRUCTIONS: &str = "Tenun builds dashboards from the user's own files and databases. Start with list_tables \
and describe_table, ask with query, then build with add_dashboard and add_tile; each new tile answers with its result, \
so you can see what you made. Tiles use the same fields as query: measures (table, column or null for a count, agg) and \
dims (table, column, grain for a date). A forecast is a line over time with \"forecast\": {\"periods\": 3}. Every change \
can be taken back with undo, and the user sees each change in the Tenun app.";

// ---- what AI apps changed --------------------------------------------------------

/// The dashboards and the measures: all that the tools change.
type State = (Vec<Value>, Vec<NamedMeasure>);

struct Change {
    what: String,
    before: State,
    after: State,
}

/// The changes AI apps made, newest last, so the last can be taken back.
#[derive(Default)]
pub struct Journal(Mutex<Vec<Change>>);

const JOURNAL: usize = 50;

fn state_of(engine: &Engine) -> State {
    (engine.dashboards(), engine.measures())
}

impl Journal {
    fn record(&self, what: String, before: State, after: State) {
        let mut j = self.0.lock().unwrap();
        j.push(Change {
            what,
            before,
            after,
        });
        if j.len() > JOURNAL {
            j.remove(0);
        }
    }
}

// ---- the tools ----------------------------------------------------------------------

/// What a tool gives back, and what it changed in words (for the app's notice and undo).
pub struct Reply {
    pub value: Value,
    pub changed: Option<String>,
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

fn field_schema() -> Value {
    json!({ "type": "object", "description": "A field: its table id and column; for a date, a grain.",
        "properties": { "table": { "type": "string" }, "column": { "type": "string" },
          "grain": { "type": "string", "enum": ["day", "week", "month", "quarter", "year", "weekday", "monthofyear", "hour"] } },
        "required": ["table", "column"] })
}

fn measure_schema() -> Value {
    json!({ "type": "object", "description": "A value: a column added up (agg), a count of rows (column null), or a named measure ({\"named\": \"m1\", \"table\": \"\", \"column\": null, \"agg\": \"sum\"}).",
        "properties": { "table": { "type": "string" }, "column": { "type": ["string", "null"] },
          "agg": { "type": "string", "enum": ["sum", "avg", "min", "max", "count", "count_distinct", "median"] },
          "named": { "type": "string" } },
        "required": ["table", "column", "agg"] })
}

fn filter_schema() -> Value {
    json!({ "type": "object", "properties": { "table": { "type": "string" }, "column": { "type": "string" },
        "op": { "type": "string", "enum": ["in", "not_in", "range", "gte", "lte", "eq"] }, "values": { "type": "array" } },
        "required": ["table", "column", "op", "values"] })
}

/// The fields of a number format, as a tile's `formats` holds them.
fn format_properties() -> Value {
    json!({
        "decimals": { "type": "integer", "minimum": 0, "maximum": 6, "description": "Fixed decimals." },
        "sig": { "type": "integer", "minimum": 1, "maximum": 6, "description": "Significant digits; wins over decimals." },
        "currency": { "type": "string", "description": "An ISO 4217 code such as USD, IDR, EUR, GBP." },
        "iso": { "type": "boolean", "description": "Write the code instead of the symbol: USD 1,234." },
        "compact": { "type": "boolean", "description": "Short (1.2M) or every digit." },
        "scale": { "type": "integer", "enum": [3, 6, 9, 12], "description": "A fixed unit: thousands, millions, billions, trillions (1.2M), in both forms." },
        "percent": { "type": "boolean", "description": "Multiply by 100 and add %." },
        "negative": { "type": "string", "enum": ["minus", "parens", "red", "parens_red"], "description": "How a negative is written: −1,234 or (1,234); red marks it." },
        "prefix": { "type": "string", "maxLength": 16 },
        "suffix": { "type": "string", "maxLength": 16 },
        "duration": { "type": "string", "enum": ["s", "m", "h", "d"], "description": "The value counts seconds, minutes, hours or days: 26:03:04, 1d 2h." },
        "zero": { "type": "string", "description": "Written for an exact 0." },
        "blank": { "type": "string", "description": "Written for an empty cell." },
        "code": { "type": "string", "description": "An Excel number format code such as #,##0.00;[Red](#,##0.00). When set, the other fields are ignored." }
    })
}

/// The tools, in a fixed order.
pub fn tools() -> Vec<Value> {
    let tile = json!({ "type": "object", "description": "A tile: type, title, measures, dims; options as the app writes them (sort, top, forecast, unusual, target, formats). A tile has no filter of its own: filters belong to the page. To limit a page, add a filter tile: {\"type\": \"filter\", \"dims\": [a field], \"filter\": {\"style\": \"dates\", \"range\": [\"2026-10-01\", \"2026-10-31\"]}} (or style list with values); it filters every tile of the page.",
        "properties": { "type": { "type": "string", "enum": TILE_TYPES }, "title": { "type": "string" },
          "measures": { "type": "array", "items": measure_schema() }, "dims": { "type": "array", "items": field_schema() },
          "sort": { "type": "object", "description": "The order of the groups: by value (biggest first with desc) or by label (A to Z).",
            "properties": { "by": { "type": "string", "enum": ["value", "label"] }, "desc": { "type": "boolean" } }, "required": ["by"] },
          "top": { "type": "object", "description": "Only the first n groups, the rest as Other when other is true.",
            "properties": { "n": { "type": "integer", "minimum": 1 }, "other": { "type": "boolean" } }, "required": ["n"] },
          "forecast": { "type": "object", "description": "A line or area over time forecasts its next periods. A month needs 26 finished months of data, a day 16 days; with fewer the result says so.",
            "properties": { "periods": { "type": "integer", "minimum": 1, "maximum": 24 }, "band": { "type": "boolean" } }, "required": ["periods"] },
          "formats": { "type": "array", "description": "How each measure's value is written, in the order of measures (null: as its column or named measure says). Every field is optional.",
            "items": { "type": ["object", "null"], "properties": format_properties() } } },
        "required": ["type"] });
    let read = json!({ "readOnlyHint": true });
    let write = json!({ "readOnlyHint": false, "destructiveHint": false });
    vec![
        json!({ "name": "list_tables", "description": "The tables of the workspace: id, name, rows, number of columns.",
            "inputSchema": { "type": "object", "additionalProperties": false }, "annotations": read }),
        json!({ "name": "describe_table", "description": "A table's columns (name, label, type, share of blank cells) and its first 5 rows.",
            "inputSchema": obj(json!({ "table": { "type": "string", "description": "A table id from list_tables, or its name." } }), &["table"]), "annotations": read }),
        json!({ "name": "list_relationships", "description": "How the tables link: each link's two fields. A query may group by a field of a linked table.",
            "inputSchema": { "type": "object", "additionalProperties": false }, "annotations": read }),
        json!({ "name": "query", "description": "Ask the data a question, as a tile does: measures grouped by dimensions, with filters. No SQL. At most 500 rows.",
            "inputSchema": obj(json!({ "measures": { "type": "array", "items": measure_schema() }, "dimensions": { "type": "array", "items": field_schema() },
              "filters": { "type": "array", "items": filter_schema() }, "sort": { "type": "array", "items": { "type": "object" } },
              "limit": { "type": "integer", "minimum": 1, "maximum": MAX_ROWS } }), &["measures"]), "annotations": read }),
        json!({ "name": "check_formula", "description": "Check a formula (Excel-like, with [Table].[column] references) before you use it: its type and a preview, or the error in words.",
            "inputSchema": obj(json!({ "formula": { "type": "string" }, "kind": { "type": "string", "enum": ["measure", "column"] }, "table": { "type": "string" } }), &["formula", "kind"]), "annotations": read }),
        json!({ "name": "list_dashboards", "description": "The dashboards: id, title, number of tiles.",
            "inputSchema": { "type": "object", "additionalProperties": false }, "annotations": read }),
        json!({ "name": "read_dashboard", "description": "A dashboard's tiles and what each shows as the page opens (the first 50 rows of each).",
            "inputSchema": obj(json!({ "dashboard": { "type": "string" } }), &["dashboard"]), "annotations": read }),
        json!({ "name": "add_measure", "description": "Add a named measure (a formula of totals, e.g. SUM([Orders].[amount]) / COUNTROWS([Orders])) that tiles can use as {\"named\": id}.",
            "inputSchema": obj(json!({ "name": { "type": "string" }, "formula": { "type": "string" }, "description": { "type": "string" } }), &["name", "formula"]), "annotations": write }),
        json!({ "name": "add_dashboard", "description": "Add an empty dashboard. Answers its id.",
            "inputSchema": obj(json!({ "title": { "type": "string" } }), &["title"]), "annotations": write }),
        json!({ "name": "add_tile", "description": "Add a tile to a dashboard, placed under the others. Answers the tile and its result, so you can see what it shows.",
            "inputSchema": obj(json!({ "dashboard": { "type": "string" }, "tile": tile }), &["dashboard", "tile"]), "annotations": write }),
        json!({ "name": "update_tile", "description": "Change a tile: the fields given replace the tile's own (title, measures, dims, options). Answers the tile and its result.",
            "inputSchema": obj(json!({ "dashboard": { "type": "string" }, "tile": { "type": "string" }, "changes": { "type": "object" } }), &["dashboard", "tile", "changes"]), "annotations": write }),
        json!({ "name": "remove_tile", "description": "Remove a tile from a dashboard.",
            "inputSchema": obj(json!({ "dashboard": { "type": "string" }, "tile": { "type": "string" } }), &["dashboard", "tile"]),
            "annotations": { "readOnlyHint": false, "destructiveHint": true } }),
        json!({ "name": "undo", "description": "Take back the last change an AI app made, if nothing changed after it.",
            "inputSchema": { "type": "object", "additionalProperties": false }, "annotations": write }),
    ]
}

/// A column whose name says it holds a password or a key: never read.
fn secret_like(name: &str) -> bool {
    let n = name.to_ascii_lowercase().replace(['-', ' '], "_");
    [
        "password",
        "passwd",
        "pwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "private_key",
        "access_key",
    ]
    .iter()
    .any(|w| n.contains(w))
}

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Give {key}."))
}

fn short(s: &str, what: &str) -> Result<String, String> {
    let s = s.trim();
    if s.is_empty() || s.chars().count() > MAX_NAME {
        return Err(format!("A {what} has 1 to {MAX_NAME} characters."));
    }
    Ok(s.to_string())
}

/// A table by id, or by its name or label (case does not matter).
fn table_of(engine: &Engine, want: &str) -> Result<tenun_core::catalog::Table, String> {
    let snap = engine.snapshot();
    snap.tables
        .iter()
        .find(|t| t.id == want)
        .or_else(|| {
            snap.tables
                .iter()
                .find(|t| t.label.eq_ignore_ascii_case(want) || t.name.eq_ignore_ascii_case(want))
        })
        .cloned()
        .ok_or_else(|| format!("There is no table {want}. list_tables gives their ids."))
}

/// Refuses a question that names a column whose name says it holds a password or a key.
fn no_secrets(v: &Value) -> Result<(), String> {
    match v {
        Value::Object(o) => {
            if let Some(c) = o.get("column").and_then(Value::as_str)
                && secret_like(c)
            {
                return Err(format!(
                    "Tenun does not give an AI app the column {c}: its name says it holds a password or a key."
                ));
            }
            o.values().try_for_each(no_secrets)
        }
        Value::Array(a) => a.iter().try_for_each(no_secrets),
        _ => Ok(()),
    }
}

fn dashboard_of(engine: &Engine, id: &str) -> Result<Value, String> {
    engine
        .dashboard(id)
        .ok_or_else(|| format!("There is no dashboard {id}. list_dashboards gives their ids."))
}

/// A compact view of a tile's result for an AI app.
fn result_of(engine: &Engine, dashboard: &str, tile: &str) -> Value {
    match snapshot::tile_as_opened(engine, dashboard, tile) {
        Ok(r) => {
            if let Some(k) = &r.kpi {
                return json!({ "value": k.value, "period": k.period, "so_far": k.partial, "change": k.delta, "versus": k.versus });
            }
            let mut out = json!({
                "columns": r.columns.iter().map(|c| json!({ "name": c.name, "role": c.role })).collect::<Vec<_>>(),
                "rows": r.rows.iter().take(TILE_ROWS).collect::<Vec<_>>(),
            });
            if r.rows.len() > TILE_ROWS {
                out["more_rows"] = json!(r.rows.len() - TILE_ROWS);
            }
            if let Some(f) = &r.forecast {
                out["forecast"] = json!({ "rows": f.rows, "method": f.method, "note": f.note });
            }
            if !r.notes.is_empty() {
                out["notes"] = json!(r.notes);
            }
            out
        }
        Err(e) => json!({ "error": e.to_string() }),
    }
}

/// The next free id of a kind: `d<n>`, `t<n>`.
fn next_id(prefix: &str, taken: impl Iterator<Item = String>) -> String {
    let n = taken
        .filter_map(|id| id.strip_prefix(prefix)?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    format!("{prefix}{}", n + 1)
}

/// The first free place for a `w` × `h` tile on the 12-column grid, from the top left: beside the others when the
/// row has room, else under them.
fn first_free(tiles: &[Value], w: u64, h: u64) -> (u64, u64) {
    let rect = |t: &Value| {
        let n = |k: &str| t[k].as_u64().unwrap_or(0);
        (n("x"), n("y"), n("w").max(1), n("h").max(1))
    };
    let taken: Vec<(u64, u64, u64, u64)> = tiles.iter().map(rect).collect();
    let bottom = taken.iter().map(|r| r.1 + r.3).max().unwrap_or(0);
    for y in 0..=bottom {
        for x in 0..=(12 - w.min(12)) {
            let free = taken.iter().all(|&(tx, ty, tw, th)| {
                x + w <= tx || tx + tw <= x || y + h <= ty || ty + th <= y
            });
            if free {
                return (x, y);
            }
        }
    }
    (0, bottom)
}

/// A tile has no filter of its own (filters belong to the page): a `filters` key is refused in words, not ignored.
fn no_tile_filters(t: &Value) -> Result<(), String> {
    if t.get("filters").is_some() || t.get("where").is_some() {
        return Err("A tile has no filter of its own: filters belong to the page. Add a filter tile to the page \
            ({\"type\": \"filter\", \"dims\": [the field], \"filter\": {\"style\": \"dates\", \"range\": [first day, last day]}}), \
            or put the tile on a page of its own."
            .into());
    }
    Ok(())
}

/// Checks a tile the way the app draws it: a known type, fields that exist, a result that runs.
fn check_tile(engine: &Engine, dash_id: &str, tile_id: &str) -> Result<Value, String> {
    let r = result_of(engine, dash_id, tile_id);
    match r.get("error").and_then(Value::as_str) {
        Some(e) => Err(format!("That tile does not work: {e}")),
        None => Ok(r),
    }
}

/// Runs one tool. `Err` is a refusal in words, for the AI app to read and correct.
pub fn call(engine: &Engine, journal: &Journal, name: &str, args: &Value) -> Result<Reply, String> {
    let read = |value: Value| {
        Ok(Reply {
            value,
            changed: None,
        })
    };
    match name {
        "list_tables" => {
            let snap = engine.snapshot();
            read(json!({ "tables": snap.tables.iter().map(|t| json!({
                "id": t.id, "name": if t.label.is_empty() { &t.name } else { &t.label }, "rows": t.rows,
                "columns": t.columns.iter().filter(|c| !c.hidden).count(),
            })).collect::<Vec<_>>() }))
        }
        "describe_table" => {
            let t = table_of(engine, text(args, "table")?)?;
            let preview = engine.preview(&t.id, 5).map_err(|e| e.to_string())?;
            let hide: Vec<bool> = preview
                .columns
                .iter()
                .map(|c| secret_like(&c.name))
                .collect();
            let rows: Vec<Vec<Value>> = preview
                .rows
                .iter()
                .map(|r| {
                    r.iter()
                        .zip(&hide)
                        .map(|(v, h)| if *h { json!("(not shown)") } else { v.clone() })
                        .collect()
                })
                .collect();
            read(json!({
                "id": t.id, "name": if t.label.is_empty() { &t.name } else { &t.label }, "rows": t.rows,
                "columns": t.columns.iter().filter(|c| !c.hidden).map(|c| json!({
                    "name": c.name, "label": c.label, "type": c.dtype, "blank": c.blank, "description": c.description,
                    "calculated": c.calc, "not_shown": secret_like(&c.name),
                })).collect::<Vec<_>>(),
                "sample": { "columns": preview.columns.iter().map(|c| &c.name).collect::<Vec<_>>(), "rows": rows },
            }))
        }
        "list_relationships" => {
            let ws = engine.snapshot().workspace;
            read(
                json!({ "relationships": ws.relationships.iter().map(|r| json!({
                "id": r.id, "name": r.name, "from": { "table": r.from.table, "column": r.from.column },
                "to": { "table": r.to.table, "column": r.to.column },
            })).collect::<Vec<_>>() }),
            )
        }
        "query" => {
            no_secrets(args)?;
            let mut q: SemanticQuery = serde_json::from_value(json!({
                "measures": args.get("measures").cloned().unwrap_or(json!([])),
                "dimensions": args.get("dimensions").cloned().unwrap_or(json!([])),
                "filters": args.get("filters").cloned().unwrap_or(json!([])),
                "sort": args.get("sort").cloned().unwrap_or(json!([])),
            }))
            .map_err(|e| format!("That question is not well formed: {e}"))?;
            q.limit = Some(
                args.get("limit")
                    .and_then(Value::as_u64)
                    .map_or(100, |l| l.clamp(1, u64::from(MAX_ROWS)) as u32),
            );
            let r = engine.query(&q).map_err(|e| e.to_string())?;
            read(json!({
                "columns": r.columns.iter().map(|c| json!({ "name": c.name, "role": c.role })).collect::<Vec<_>>(),
                "rows": r.rows,
            }))
        }
        "check_formula" => {
            let formula = text(args, "formula")?;
            if formula.chars().count() > MAX_FORMULA {
                return Err(format!("A formula has at most {MAX_FORMULA} characters."));
            }
            let kind = match args.get("kind").and_then(Value::as_str) {
                Some("column") => Kind::Column,
                _ => Kind::Measure,
            };
            let req = CheckRequest {
                formula: formula.to_string(),
                kind,
                table: args
                    .get("table")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            };
            let r = engine
                .check_calc(&req, None, false)
                .map_err(|e| e.to_string())?;
            read(serde_json::to_value(r).unwrap_or(Value::Null))
        }
        "list_dashboards" => read(
            json!({ "dashboards": engine.dashboards().iter().map(|d| json!({
            "id": d["id"], "title": d["title"], "tiles": d["tiles"].as_array().map_or(0, Vec::len),
        })).collect::<Vec<_>>() }),
        ),
        "read_dashboard" => {
            let id = text(args, "dashboard")?;
            let d = dashboard_of(engine, id)?;
            let tiles: Vec<Value> = d["tiles"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| {
                    let mut v = json!({ "id": t["id"], "type": t["type"], "title": t["title"],
                        "measures": t["measures"], "dims": t["dims"] });
                    let tid = t["id"].as_str().unwrap_or_default();
                    if !snapshot::is_static(t["type"].as_str().unwrap_or_default()) {
                        v["result"] = result_of(engine, id, tid);
                    }
                    v
                })
                .collect();
            read(json!({ "id": d["id"], "title": d["title"], "tiles": tiles }))
        }
        "add_measure" => {
            let name = short(text(args, "name")?, "name")?;
            let formula = text(args, "formula")?;
            if formula.chars().count() > MAX_FORMULA {
                return Err(format!("A formula has at most {MAX_FORMULA} characters."));
            }
            let before = state_of(engine);
            let mut list = before.1.clone();
            list.push(NamedMeasure {
                id: String::new(),
                name: name.clone(),
                formula: formula.to_string(),
                description: args
                    .get("description")
                    .and_then(Value::as_str)
                    .map(|s| s.chars().take(500).collect()),
                ..NamedMeasure::default()
            });
            engine.set_measures(list, false).map_err(|e| {
                e.downcast_ref::<tenun_core::engine::MeasureRefused>()
                    .map(|r| r.error.message.clone())
                    .unwrap_or_else(|| e.to_string())
            })?;
            let after = state_of(engine);
            let id = after
                .1
                .iter()
                .find(|m| !before.1.iter().any(|b| b.id == m.id))
                .map(|m| m.id.clone())
                .unwrap_or_default();
            let what = format!("added the measure {name}");
            journal.record(what.clone(), before, after);
            Ok(Reply {
                value: json!({ "id": id, "name": name, "use_as": { "named": id, "table": "", "column": null, "agg": "sum" } }),
                changed: Some(what),
            })
        }
        "add_dashboard" => {
            let title = short(text(args, "title")?, "title")?;
            let before = state_of(engine);
            let id = next_id(
                "d",
                before
                    .0
                    .iter()
                    .filter_map(|d| d["id"].as_str().map(str::to_string)),
            );
            let mut list = before.0.clone();
            list.push(json!({ "id": id, "title": title, "refresh": 0, "tiles": [] }));
            engine.set_dashboards(list);
            let what = format!("added the dashboard {title}");
            journal.record(what.clone(), before, state_of(engine));
            Ok(Reply {
                value: json!({ "id": id, "title": title }),
                changed: Some(what),
            })
        }
        "add_tile" | "update_tile" => {
            let dash_id = text(args, "dashboard")?.to_string();
            let d = dashboard_of(engine, &dash_id)?;
            let before = state_of(engine);
            let tiles: Vec<Value> = d["tiles"].as_array().cloned().unwrap_or_default();
            let (tile, what) = if name == "add_tile" {
                let mut t = args
                    .get("tile")
                    .filter(|t| t.is_object())
                    .cloned()
                    .ok_or("Give the tile.")?;
                no_secrets(&t)?;
                no_tile_filters(&t)?;
                let ty = t["type"].as_str().unwrap_or_default().to_string();
                if !TILE_TYPES.contains(&ty.as_str()) {
                    return Err(format!(
                        "A tile's type is one of {}.",
                        TILE_TYPES.join(", ")
                    ));
                }
                let (w, h) = if ty == "kpi" { (3, 2) } else { (6, 4) };
                let (x, y) = first_free(&tiles, w, h);
                let id = next_id(
                    "t",
                    tiles
                        .iter()
                        .filter_map(|x| x["id"].as_str().map(str::to_string)),
                );
                t["id"] = json!(id);
                for (k, v) in [
                    ("x", json!(x)),
                    ("y", json!(y)),
                    ("w", json!(w)),
                    ("h", json!(h)),
                    ("title", json!("")),
                    ("measures", json!([])),
                    ("dims", json!([])),
                ] {
                    if t.get(k).is_none() {
                        t[k] = v;
                    }
                }
                let title = t["title"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&ty)
                    .to_string();
                (
                    t,
                    format!(
                        "added the tile {title} to {}",
                        d["title"].as_str().unwrap_or("a dashboard")
                    ),
                )
            } else {
                let tid = text(args, "tile")?;
                let mut t = tiles
                    .iter()
                    .find(|x| x["id"] == tid)
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "There is no tile {tid} on {dash_id}. read_dashboard gives their ids."
                        )
                    })?;
                let changes = args
                    .get("changes")
                    .and_then(Value::as_object)
                    .ok_or("Give the changes.")?;
                no_secrets(&Value::Object(changes.clone()))?;
                no_tile_filters(&Value::Object(changes.clone()))?;
                for (k, v) in changes {
                    if k != "id" {
                        t[k] = v.clone();
                    }
                }
                if !TILE_TYPES.contains(&t["type"].as_str().unwrap_or_default()) {
                    return Err(format!(
                        "A tile's type is one of {}.",
                        TILE_TYPES.join(", ")
                    ));
                }
                (
                    t,
                    format!(
                        "changed a tile of {}",
                        d["title"].as_str().unwrap_or("a dashboard")
                    ),
                )
            };
            if let Some(title) = tile["title"].as_str()
                && title.chars().count() > MAX_NAME
            {
                return Err(format!("A title has at most {MAX_NAME} characters."));
            }
            serde_json::from_value::<snapshot::Tile>(tile.clone())
                .map_err(|e| format!("That tile is not well formed: {e}"))?;
            let tid = tile["id"].as_str().unwrap_or_default().to_string();
            let mut list = before.0.clone();
            if let Some(dd) = list.iter_mut().find(|x| x["id"] == dash_id.as_str())
                && let Some(ts) = dd["tiles"].as_array_mut()
            {
                match ts.iter_mut().find(|x| x["id"] == tid.as_str()) {
                    Some(x) => *x = tile.clone(),
                    None => ts.push(tile.clone()),
                }
            }
            engine.set_dashboards(list);
            // A tile that does not run is not kept: the AI app reads why.
            let result = match check_tile(engine, &dash_id, &tid) {
                Ok(r) => r,
                Err(e) => {
                    engine.set_dashboards(before.0);
                    return Err(e);
                }
            };
            journal.record(what.clone(), before, state_of(engine));
            Ok(Reply {
                value: json!({ "tile": tile, "result": result }),
                changed: Some(what),
            })
        }
        "remove_tile" => {
            let dash_id = text(args, "dashboard")?;
            let tid = text(args, "tile")?;
            let d = dashboard_of(engine, dash_id)?;
            if !d["tiles"]
                .as_array()
                .is_some_and(|ts| ts.iter().any(|x| x["id"] == tid))
            {
                return Err(format!(
                    "There is no tile {tid} on {dash_id}. read_dashboard gives their ids."
                ));
            }
            let before = state_of(engine);
            let mut list = before.0.clone();
            if let Some(dd) = list.iter_mut().find(|x| x["id"] == dash_id)
                && let Some(ts) = dd["tiles"].as_array_mut()
            {
                ts.retain(|x| x["id"] != tid);
            }
            engine.set_dashboards(list);
            let what = format!(
                "removed a tile from {}",
                d["title"].as_str().unwrap_or("a dashboard")
            );
            journal.record(what.clone(), before, state_of(engine));
            Ok(Reply {
                value: json!({ "removed": tid }),
                changed: Some(what),
            })
        }
        "undo" => {
            let mut j = journal.0.lock().unwrap();
            let Some(last) = j.last() else {
                return Err("There is no change of an AI app to take back.".into());
            };
            if state_of(engine) != last.after {
                return Err("The workspace changed after that change, so it is not taken back here. Use Undo in the Tenun app.".into());
            }
            let last = j.pop().unwrap();
            drop(j);
            engine.set_dashboards(last.before.0);
            if engine.measures() != last.before.1 {
                engine
                    .set_measures(last.before.1, true)
                    .map_err(|e| e.to_string())?;
            }
            let what = format!("took back: {}", last.what);
            Ok(Reply {
                value: json!({ "undone": last.what }),
                changed: Some(what),
            })
        }
        _ => Err(format!("There is no tool {name}.")),
    }
}

// ---- where the calls go ---------------------------------------------------------------

/// The app's servers on this computer register in `<config dir>/tenun/running/<pid>.json`: `{ "url", "token" }`.
pub fn running_dir(secrets: &Secrets) -> PathBuf {
    secrets.path().with_file_name("running")
}

/// Registers a running app server, so `tenun mcp` can find it. The file goes when the server stops.
pub struct Registration(PathBuf);

impl Registration {
    pub fn register(secrets: &Secrets, url: &str, token: Option<&str>) -> Option<Self> {
        let dir = running_dir(secrets);
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join(format!("{}.json", std::process::id()));
        std::fs::write(
            &path,
            json!({ "url": url.trim_end_matches('/'), "token": token }).to_string(),
        )
        .ok()?;
        Some(Self(path))
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// A running app server that has `file` open: its URL and token.
fn server_for(secrets: &Secrets, file: &Path) -> Option<(String, Option<String>)> {
    let want = std::fs::canonicalize(file).ok()?;
    let probe: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(2)))
        .http_status_as_error(false)
        .build()
        .into();
    for e in std::fs::read_dir(running_dir(secrets)).ok()?.flatten() {
        let Ok(v) = std::fs::read_to_string(e.path())
            .map(|s| serde_json::from_str::<Value>(&s).unwrap_or_default())
        else {
            continue;
        };
        let (Some(url), token) = (v["url"].as_str(), v["token"].as_str().map(str::to_string))
        else {
            continue;
        };
        let q = token
            .as_ref()
            .map(|t| format!("?token={t}"))
            .unwrap_or_default();
        let Ok(mut r) = probe.get(format!("{url}/api/workspace{q}")).call() else {
            continue;
        };
        let ws: Value = serde_json::from_str(&r.body_mut().read_to_string().unwrap_or_default())
            .unwrap_or_default();
        if ws["path"]
            .as_str()
            .and_then(|p| std::fs::canonicalize(p).ok())
            .as_deref()
            == Some(want.as_path())
        {
            return Some((url.to_string(), token));
        }
    }
    None
}

/// Where the tools run: the app that has the workspace open, or this process.
struct Backend {
    file: PathBuf,
    secrets: Secrets,
    local: Option<(Engine, Journal)>,
}

impl Backend {
    fn call(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        // The app may open the workspace while this runs: from then on, it is the one writer.
        if let Some((url, token)) = server_for(&self.secrets, &self.file) {
            let q = token.map(|t| format!("?token={t}")).unwrap_or_default();
            let mut r = agent()
                .post(format!("{url}/api/mcp/call{q}"))
                .header("x-tenun", "1")
                .header("content-type", "application/json")
                .send(json!({ "name": name, "arguments": args }).to_string())
                .map_err(|_| "The Tenun app could not be reached.".to_string())?;
            let v: Value = serde_json::from_str(&r.body_mut().read_to_string().unwrap_or_default())
                .unwrap_or_default();
            return match v["ok"].as_bool() {
                Some(true) => Ok(v["value"].clone()),
                _ => Err(v["error"]
                    .as_str()
                    .unwrap_or("The Tenun app refused the call.")
                    .to_string()),
            };
        }
        if self.local.is_none() {
            let engine = Engine::new(self.secrets.clone()).map_err(|e| e.to_string())?;
            engine
                .open(&self.file)
                .map_err(|e| format!("The workspace could not be opened: {e}"))?;
            self.local = Some((engine, Journal::default()));
        }
        let (engine, journal) = self.local.as_ref().unwrap();
        call(engine, journal, name, args).map(|r| r.value)
    }
}

// ---- the protocol ------------------------------------------------------------------------

fn error(id: &Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut e = json!({ "code": code, "message": message });
    if let Some(d) = data {
        e["data"] = d;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": e })
}

fn ok(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn server_info() -> Value {
    json!({ "name": "tenun", "title": "Tenun", "version": crate::VERSION })
}

/// Answers one message; `None` for a notification.
fn handle(backend: &mut Backend, line: &str) -> Option<Value> {
    let Ok(msg) = serde_json::from_str::<Value>(line) else {
        return Some(error(&Value::Null, -32700, "Parse error", None));
    };
    let id = msg.get("id").cloned()?;
    let method = msg["method"].as_str().unwrap_or_default();
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    // A current client names its version on each request; an older one named it in `initialize`.
    if let Some(v) = params
        .pointer("/_meta")
        .and_then(|m| m.get(VERSION_KEY))
        .and_then(Value::as_str)
        && !MODERN.contains(&v)
        && !LEGACY.contains(&v)
    {
        let supported: Vec<&str> = MODERN.iter().chain(LEGACY).copied().collect();
        return Some(error(
            &id,
            -32022,
            "Unsupported protocol version",
            Some(json!({ "supported": supported, "requested": v })),
        ));
    }
    Some(match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or_default();
            let version = if LEGACY.contains(&asked) {
                asked
            } else {
                LEGACY[0]
            };
            ok(
                &id,
                json!({ "protocolVersion": version, "capabilities": { "tools": {} }, "serverInfo": server_info(), "instructions": INSTRUCTIONS }),
            )
        }
        "server/discover" => ok(
            &id,
            json!({
                "resultType": "complete",
                "supportedVersions": MODERN.iter().chain(LEGACY).collect::<Vec<_>>(),
                "capabilities": { "tools": {} },
                "_meta": { "io.modelcontextprotocol/serverInfo": server_info() },
                "instructions": INSTRUCTIONS,
                "ttlMs": TTL_MS,
                "cacheScope": "public",
            }),
        ),
        "ping" => ok(&id, json!({})),
        "tools/list" => ok(
            &id,
            json!({ "resultType": "complete", "tools": tools(), "ttlMs": TTL_MS, "cacheScope": "public" }),
        ),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            if !tools().iter().any(|t| t["name"] == name) {
                return Some(error(&id, -32602, &format!("Unknown tool: {name}"), None));
            }
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match backend.call(name, &args) {
                Ok(v) => ok(
                    &id,
                    json!({ "resultType": "complete",
                    "content": [{ "type": "text", "text": v.to_string() }], "structuredContent": v, "isError": false }),
                ),
                Err(e) => ok(
                    &id,
                    json!({ "resultType": "complete", "content": [{ "type": "text", "text": e }], "isError": true }),
                ),
            }
        }
        _ => error(&id, -32601, &format!("Method not found: {method}"), None),
    })
}

/// `tenun mcp <file>`: serves the tools on standard input and output until the AI app closes them.
pub fn serve(file: &Path) -> anyhow::Result<()> {
    if !file.exists() {
        anyhow::bail!("{} does not exist", file.display());
    }
    let mut backend = Backend {
        file: file.to_path_buf(),
        secrets: Secrets::new(Secrets::default_path()),
        local: None,
    };
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = handle(&mut backend, &line) {
            writeln!(out, "{reply}")?;
            out.flush()?;
        }
    }
    Ok(())
}
