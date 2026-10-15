//! Tiles and snapshots: what each tile asks, how a number tile compares
//! periods fairly, how a time chart marks an unfinished last period, and the
//! complete snapshot of a dashboard (every tile, plus the results needed to
//! cross-filter offline). One implementation for live tiles, exported files,
//! published dashboards and the headless agent.

use crate::advanced;
use crate::engine::{Engine, QueryResult};
use crate::filters::{self, TileFilter};
use crate::query::{Agg, Dimension, Filter, FilterOp, Grain, Measure, SemanticQuery, Sort};
use crate::shape;
use crate::workspace::Locale;
use chrono::{Datelike, Duration, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

/// A dashboard tile, as the UI stores it. Unknown fields are kept by callers
/// that need them (the payload carries the original JSON).
#[derive(Debug, Clone, Deserialize)]
pub struct Tile {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub measures: Vec<Measure>,
    #[serde(default)]
    pub dims: Vec<Dimension>,
    #[serde(default)]
    pub sort: Option<TileSort>,
    #[serde(default)]
    pub top: Option<TopN>,
    #[serde(default)]
    pub bins: Option<TileBins>,
    /// Tables: add a total row. Pivots have totals unless this is `false`.
    #[serde(default)]
    pub totals: Option<bool>,
    /// Pivot: the field whose values become the columns.
    #[serde(default)]
    pub cols: Option<Dimension>,
    /// How each value is shown (share, running total, change, rank…), index-aligned with
    /// `measures`.
    #[serde(default, rename = "showAs")]
    pub show_as: Option<Vec<Option<shape::ShowAs>>>,
    /// A time chart also draws the same question one year or one period earlier.
    #[serde(default)]
    pub compare: Option<shape::CompareTo>,
    /// Small multiples (Advanced): one chart for each value of this field.
    #[serde(default)]
    pub repeat: Option<Dimension>,
    /// Several values on one chart: together (default), in separate panels or indexed.
    #[serde(default)]
    pub layout: Option<shape::Layout>,
    /// Compare chart with one value: the two periods (values of the second field) to compare.
    /// Not set: the latest two.
    #[serde(default)]
    pub periods: Option<Vec<Value>>,
    /// Table: add a trend column. The field gives the periods, one line for each row.
    #[serde(default)]
    pub spark: Option<Dimension>,
    /// Pivot: groups of the row fields open and close, with a subtotal for each group.
    #[serde(default)]
    pub expand: Option<bool>,
    /// Filter tile: its style and what it has selected.
    #[serde(default)]
    pub filter: Option<TileFilter>,
    /// Map tile (build 06): the level, the country, filled or points.
    #[serde(default)]
    pub map: Option<crate::maptile::MapSpec>,
    /// Text tile: its words, in the Markdown subset. Live numbers in it (`{{measure:<id>}}`) make it ask the engine.
    #[serde(default)]
    pub text: Option<String>,
    /// Text tile: the period its live numbers cover (the dashboard's grain). Not set: by month.
    #[serde(default)]
    pub grain: Option<Grain>,
    /// The tile drills down (v5 build 01): a hierarchy, or (none named) the date chain of its first field.
    #[serde(default)]
    pub drill: Option<TileDrill>,
    /// The tiles its clicks and selections filter (v5 build 02): `"none"`, or a list. Not set: every tile.
    #[serde(default)]
    pub reach: Option<Reach>,
    /// Line and area over time (v5 build 08, Advanced): forecast the periods ahead.
    #[serde(default)]
    pub forecast: Option<crate::analytics::TileForecast>,
    /// Line, area and bar over time (v5 build 09, Advanced): mark the values far from the periods around them.
    #[serde(default)]
    pub unusual: Option<crate::analytics::TileUnusual>,
    /// Charts (v5 build 10, Advanced): up to 3 more values and a small trend in the tooltip of each group.
    #[serde(default)]
    pub tooltip: Option<crate::advanced::TileTooltip>,
}

/// Which tiles a tile's clicks and selections filter (V5-D4, V5-D37).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Reach {
    Tiles(Vec<String>),
    /// `"none"`: no tile.
    Word(String),
}

/// The tiles a filter made by `tile` reaches: `Some([])` for none, `Some(list)`, or `None` for every tile.
pub fn reach_of(tile: &Tile) -> Option<Vec<String>> {
    match tile.reach.as_ref()? {
        Reach::Tiles(list) => Some(list.clone()),
        Reach::Word(_) => Some(Vec::new()),
    }
}

/// How a tile drills (V5-D3).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TileDrill {
    /// A `Hierarchy` id. Not set: the date of the first field goes year › quarter › month › day.
    #[serde(default)]
    pub hierarchy: Option<String>,
}

/// The fields a card style gives and a card may set itself (build 08), as `card.ts CARD_FIELDS`.
const CARD_FIELDS: [&str; 5] = ["parts", "align", "size", "ground", "rules"];

/// A number card that names a style carries the style's fields under its own, and no style: an exported file and a
/// viewer need no styles (build 08, V4-D10). The same merge as `card.ts withoutStyle`. A style that is gone leaves the
/// card's own fields.
pub fn resolve_card(tile: &mut Value, styles: &[Value]) {
    let Some(card) = tile.get_mut("card").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(id) = card.remove("style") else {
        return;
    };
    let style = id
        .as_str()
        .and_then(|id| {
            styles
                .iter()
                .find(|s| s.get("id").and_then(Value::as_str) == Some(id))
        })
        .and_then(|s| s.get("card"))
        .and_then(Value::as_object);
    for f in CARD_FIELDS {
        if card.get(f).is_none_or(Value::is_null)
            && let Some(v) = style.and_then(|s| s.get(f))
        {
            card.insert(f.to_string(), v.clone());
        }
    }
}

/// Tiles that show something the user typed or picked, not a query: they have no result.
/// (A filter tile has one: the values it offers.)
pub fn is_static(kind: &str) -> bool {
    matches!(kind, "text" | "image" | "param")
}

/// Like [`is_static`], for one tile: a text tile with live numbers asks the engine for them (build 10), so it has a
/// result, follows the dashboard's filters and is worked out for every click of an exported file.
pub fn is_static_tile(t: &Tile) -> bool {
    is_static(&t.kind)
        && !(t.kind == "text" && !live_numbers(t.text.as_deref().unwrap_or("")).is_empty())
}

/// The named measures a text names as live numbers (`{{measure:<id>}}`, `|compare`, `|period`), each once, in order.
/// A token after a backslash is text (`markdown.ts`), and so is one with an id of other characters.
pub fn live_numbers(text: &str) -> Vec<String> {
    const OPEN: &str = "{{measure:";
    let mut out: Vec<String> = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find(OPEN).map(|i| from + i) {
        from = at + OPEN.len();
        let slashes = text[..at].chars().rev().take_while(|c| *c == '\\').count();
        if slashes % 2 == 1 {
            continue;
        }
        let Some(end) = text[from..].find("}}").map(|i| from + i) else {
            break;
        };
        let body = &text[from..end];
        let (id, part) = body.split_once('|').unwrap_or((body, ""));
        let ok_id = !id.is_empty()
            && id.len() <= 64
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if ok_id && matches!(part, "" | "compare" | "period") && !out.iter().any(|x| x == id) {
            out.push(id.to_string());
        }
    }
    out
}

/// Histogram options. No width means the engine picks one.
#[derive(Debug, Clone, Deserialize)]
pub struct TileBins {
    #[serde(default)]
    pub width: Option<f64>,
}

/// How a tile orders its groups. `manual` is applied by the engine after the top N
/// ([`advanced::manual_order`]).
#[derive(Debug, Clone, Deserialize)]
pub struct TileSort {
    pub by: SortBy,
    /// Manual order (Advanced): the groups in the order the user dragged them to.
    #[serde(default)]
    pub order: Option<Vec<String>>,
    /// Biggest first for `value`, A to Z for `label`.
    #[serde(default)]
    pub desc: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    Value,
    Label,
    Manual,
}

/// Keep the biggest `n` groups and, unless `other` is false, add the rest as one "Other".
#[derive(Debug, Clone, Deserialize)]
pub struct TopN {
    pub n: u32,
    #[serde(default)]
    pub other: Option<bool>,
    /// A number parameter that gives `n` (v5 build 05). Gone, or not a number: `n` as it is.
    #[serde(default)]
    pub param: Option<String>,
}

/// A cross-filter from a click on a tile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashFilter {
    #[serde(flatten)]
    pub filter: Filter,
    /// The tile the filter came from, which it does not filter. A filter the dashboard saves
    /// ("Keep only", "Exclude") names no tile, so it filters them all.
    #[serde(rename = "fromTile")]
    pub from_tile: String,
    /// The tiles it filters (v5 build 02, V5-D37): the reach of the tile it came from. `[]`: none. Not set: every tile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec<String>>,
}

impl DashFilter {
    /// Whether this filter applies to the tile `id`: not one of its own, and in its reach.
    pub fn reaches(&self, id: &str) -> bool {
        self.from_tile != id && self.to.as_ref().is_none_or(|to| to.iter().any(|t| t == id))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KpiInfo {
    pub value: Option<f64>,
    pub period: String,
    pub versus: Option<String>,
    pub prev: Option<f64>,
    pub delta: Option<f64>,
    pub partial: bool,
    pub bars: Vec<Bar>,
    /// The share of the period that has data, 0 to 1: days up to the last day with data, over
    /// the days of the period. 1 for a finished period. Absent for "All data".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed: Option<f64>,
    /// The tile's other values (its measures after the first) for the same rows and period as `value`: a card shows
    /// them (build 08). Empty, and not written, for a tile with one value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extras: Vec<Option<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bar {
    pub label: String,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Partial {
    pub key: String,
    pub until: String,
}

const MAX_CROSS_VALUES: usize = 30;

/// Offline drill-down holds the level below the top for this many values of a tile (V5-D5, as V4-D16).
pub const MAX_DRILL_VALUES: usize = 50;
/// The key of "Next level" (every value one level down) in an export's `down`.
pub const DRILL_ALL_KEY: &str = "\u{0}all";

/// The most forks of the engine that run at once. DuckDB also runs each query on several threads.
const FORKS: usize = 4;

/// Runs `job` for each item on forks of the engine, in parallel, and gives the answers in the order of
/// the items. With fewer than two items, or when a fork cannot be made, it runs on `engine` itself.
/// The first error stops the rest.
pub fn in_parallel<J: Sync, R: Send>(
    engine: &Engine,
    items: &[J],
    job: impl Fn(&Engine, &J) -> anyhow::Result<R> + Sync,
) -> anyhow::Result<Vec<R>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(FORKS)
        .min(items.len());
    let forks: Vec<Engine> = if threads >= 2 {
        (0..threads).map_while(|_| engine.fork().ok()).collect()
    } else {
        Vec::new()
    };
    if forks.len() < 2 {
        return items.iter().map(|i| job(engine, i)).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let failed = std::sync::atomic::AtomicBool::new(false);
    // The parameters' values of this call (a saved view's) go with the work to each thread (v5 build 05).
    let values = crate::params::overrides();
    let mut answers: Vec<(usize, anyhow::Result<R>)> = std::thread::scope(|s| {
        let workers: Vec<_> = forks
            .iter()
            .map(|e| {
                let (next, failed, job, values) = (&next, &failed, &job, &values);
                s.spawn(move || {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if i >= items.len() || failed.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        let r = crate::params::with(values, || job(e, &items[i]));
                        if r.is_err() {
                            failed.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        out.push((i, r));
                    }
                    out
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("a fork of the engine panicked"))
            .collect()
    });
    answers.sort_by_key(|(i, _)| *i);
    answers.into_iter().map(|(_, r)| r).collect()
}

/// A pivot reads up to this many rows, and its row and column totals up to the same.
pub(crate) const PIVOT_LIMIT: u32 = 5000;
/// A pivot draws at most this many columns. Its cells are read for these columns only.
pub(crate) const PIVOT_MAX_COLS: usize = 30;
const PIVOT_MAX_ROWS: usize = 3;
const PIVOT_MAX_MEASURES: usize = 4;
/// The key of a pivot column click in the offline cross-filter map of an export.
const COL_PREFIX: &str = "\u{1}col:";
/// The key of a click on a Sankey node in the offline cross-filter map: the stage, then the value.
pub const STAGE_PREFIX: &str = "\u{2}stage:";

fn own_excluded(tile: &Tile, filters: &[DashFilter]) -> Vec<Filter> {
    filters
        .iter()
        .filter(|f| f.reaches(&tile.id))
        .map(|f| f.filter.clone())
        .collect()
}

pub(crate) fn count_rows(table: &str) -> Measure {
    Measure {
        table: table.to_string(),
        column: None,
        agg: Agg::Count,
        per: None,
        via: None,
        named: None,
    }
}

/// The question a tile asks; `None` when it has nothing to ask yet.
pub fn tile_query(tile: &Tile, filters: &[DashFilter]) -> Option<SemanticQuery> {
    let kind = tile.kind.as_str();
    if is_static(kind) || (tile.measures.is_empty() && tile.dims.is_empty()) {
        return None;
    }
    let one = matches!(
        kind,
        "kpi" | "share" | "donut" | "pie" | "funnel" | "box" | "filter"
    );
    // A compare chart with one value reads two periods of its second field.
    let periods = kind == "compare" && tile.measures.len() < 2;
    // Small multiples: the group, then the field that makes one chart for each of its values.
    let repeating = advanced::repeats(tile);
    // Bar, line and area take up to four values side by side. Then there is no split.
    let many = matches!(kind, "bar" | "line" | "area") && tile.measures.len() > 1 && !repeating;
    let pivotish = matches!(kind, "pivot" | "heatmap");
    let dims: Vec<Dimension> = if one || matches!(kind, "waterfall" | "bullet" | "gantt" | "cohort")
    {
        tile.dims.iter().take(1).cloned().collect()
    } else if kind == "calendar" {
        // One cell for each day, whatever grain the field was saved with.
        tile.dims
            .iter()
            .take(1)
            .cloned()
            .map(|mut d| {
                d.grain = Some(Grain::Day);
                d
            })
            .collect()
    } else if kind == "sankey" {
        tile.dims
            .iter()
            .take(advanced::SANKEY_STAGES)
            .cloned()
            .collect()
    } else if kind == "compare" {
        tile.dims
            .iter()
            .take(if periods { 2 } else { 1 })
            .cloned()
            .collect()
    } else if kind == "histogram" {
        vec![]
    } else if kind == "table" {
        tile.dims.clone()
    } else if pivotish {
        // Rows first, then the column field. A heat map has one row field.
        tile.dims
            .iter()
            .take(if kind == "heatmap" { 1 } else { PIVOT_MAX_ROWS })
            .chain(tile.cols.iter())
            .cloned()
            .collect()
    } else if repeating {
        tile.dims
            .iter()
            .take(1)
            .chain(tile.repeat.iter())
            .cloned()
            .collect()
    } else if many {
        tile.dims.iter().take(1).cloned().collect()
    } else {
        tile.dims.iter().take(2).cloned().collect()
    };
    // A chart with a group but no value counts rows of the grouped table.
    let measures = if !tile.measures.is_empty() {
        if kind == "table" {
            tile.measures.clone()
        } else if kind == "pivot" {
            tile.measures
                .iter()
                .take(PIVOT_MAX_MEASURES)
                .cloned()
                .collect()
        } else if many {
            tile.measures
                .iter()
                .take(MAX_CHART_MEASURES)
                .cloned()
                .collect()
        } else if kind == "scatter" {
            // Across and up, and a third value for the size of the dot.
            tile.measures.iter().take(3).cloned().collect()
        } else if matches!(kind, "bullet" | "gantt" | "cohort") || (kind == "compare" && !periods) {
            // Bullet: the value, then its target. Gantt: start, then end. Cohort: the entity, then a
            // value to add up. Compare: the two values.
            tile.measures.iter().take(2).cloned().collect()
        } else {
            vec![tile.measures[0].clone()]
        }
    } else if kind == "table" {
        vec![]
    } else {
        vec![count_rows(&dims.first().unwrap_or(&tile.dims[0]).table)]
    };
    // Output columns are the dimensions, then the measures.
    let mut sort = vec![];
    // A pivot keeps the engine's order: its rows and columns are not one list to sort.
    // A funnel without a chosen order puts the biggest stage first.
    let by = tile
        .sort
        .as_ref()
        .map(|s| (s.by, s.desc))
        .or_else(|| (kind == "funnel").then_some((SortBy::Value, None)))
        // A Gantt chart starts with the task that starts first.
        .or_else(|| (kind == "gantt").then_some((SortBy::Value, Some(false))));
    // A compare chart is ordered by the engine afterwards. A calendar keeps its days in time order.
    if let Some((by, desc)) = by
        && !dims.is_empty()
        && !pivotish
        && !matches!(kind, "compare" | "calendar" | "sankey")
    {
        match by {
            SortBy::Value if !measures.is_empty() => sort.push(Sort {
                by: dims.len(),
                desc: desc.unwrap_or(true),
            }),
            SortBy::Label => sort.push(Sort {
                by: 0,
                desc: desc.unwrap_or(false),
            }),
            _ => {}
        }
    }
    // A calendar reads the latest days first and cuts them to the last 12 months afterwards.
    if kind == "calendar" {
        sort = vec![Sort { by: 0, desc: true }];
    }
    Some(SemanticQuery {
        measures,
        dimensions: dims,
        filters: own_excluded(tile, filters),
        sort,
        limit: Some(match kind {
            "table" => 500,
            "pivot" | "heatmap" => PIVOT_LIMIT,
            "kpi" | "calendar" => 400,
            "gantt" => advanced::MAX_GANTT_ROWS,
            _ => 2000,
        }),
    })
}

const MAX_CHART_MEASURES: usize = 4;

/// Runs one tile, with number-tile and unfinished-period extras, and a note for every link its
/// queries cross whose joined side repeats a key (V4-D26). Every query the tile runs counts,
/// its spark line and its side questions too (V4-D45), not only its main one.
///
/// The result carries two lists: `notes`, which name the keys and the files and stay in the app,
/// and `public_notes`, which have no value and no file name, for pictures and exports (V4-D52).
pub fn run_tile(
    engine: &Engine,
    tile: &Tile,
    filters: &[DashFilter],
) -> anyhow::Result<QueryResult> {
    // A Top N that takes a parameter takes its value of this call (v5 build 05).
    let top = top_param(engine, tile);
    let tile = top.as_ref().map_or(tile, |(t, _)| t);
    // Period names, year to date and the number tile's periods follow the workspace's calendar (v5 build 04). The
    // parameters its queries read are noted, for the steps of an export (build 05).
    let ((res, crossed), used) = crate::params::collecting(|| {
        crate::calendar::with(engine.calendar(), || {
            engine.collecting(|| run_tile_inner(engine, tile, filters))
        })
    });
    let mut res = res?;
    res.params = used.into_iter().collect();
    if let Some((_, id)) = &top
        && !res.params.contains(id)
    {
        res.params.push(id.clone());
    }
    res.notes = engine.link_notes(&crossed);
    res.public_notes = engine.public_link_notes(&crossed);
    // What a named measure could not split or filter by names tables and columns only, so the
    // same words go where the other notes go (V4-D85b).
    let mut words: Vec<&str> = Vec::new();
    for p in &res.parts {
        if !words.contains(&p.note.as_str()) {
            words.push(&p.note);
        }
    }
    let mut words: Vec<String> = words.into_iter().map(str::to_string).collect();
    // A stock with a group that has no row on its period's last day: it counts as nothing, and the tile says how
    // often (v5 build 07, V5-D9).
    if let Some(note) = stock_gaps(engine, tile, &res) {
        words.push(note);
    }
    for w in &words {
        if !res.notes.contains(w) {
            res.notes.push(w.clone());
        }
        if !res.public_notes.contains(w) {
            res.public_notes.push(w.clone());
        }
    }
    Ok(res)
}

/// The tile with its Top N set from its parameter, and the parameter's id; `None` when it has no parameter that is
/// a number now.
fn top_param(engine: &Engine, tile: &Tile) -> Option<(Tile, String)> {
    let id = tile.top.as_ref()?.param.as_ref()?;
    let n = engine
        .params_now()
        .into_iter()
        .find(|p| &p.id == id && p.is_number())?
        .number()?;
    let mut t = tile.clone();
    if let Some(top) = t.top.as_mut() {
        top.n = n.round().clamp(1.0, 1000.0) as u32;
    }
    Some((t, id.clone()))
}

/// The note of a tile with a closing (opening) value grouped by a time and by something else, such as a store: for each
/// period, how many of the groups that have rows in it have none on its last (first) day. `None` when every group has.
fn stock_gaps(engine: &Engine, tile: &Tile, res: &QueryResult) -> Option<String> {
    let time = tile
        .dims
        .iter()
        .position(|d| d.grain.is_some_and(|g| !g.is_part()))?;
    let grain = tile.dims[time].grain?;
    // The lowest group that is not the time: the entity whose rows may be missing.
    let entity = (0..tile.dims.len()).rev().find(|&i| i != time)?;
    let nd = res
        .columns
        .iter()
        .take_while(|c| c.role == "dimension")
        .count();
    let (at, closing) = tile.measures.iter().enumerate().find_map(|(j, m)| {
        let f = engine.measure_facts(m).ok()?;
        f.stock.map(|c| (nd + j, c))
    })?;
    let mut periods: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for r in &res.rows {
        let (Some(k), Some(v)) = (r.get(time).and_then(Value::as_str), r.get(at)) else {
            continue;
        };
        let e = periods.entry(k.to_string()).or_default();
        e.0 += 1;
        if v.is_null() {
            e.1 += 1;
        }
    }
    let loc = engine.locale();
    let gaps: Vec<String> = periods
        .iter()
        .filter(|(_, (_, missing))| *missing > 0)
        .map(|(k, (all, missing))| {
            let name =
                parse_day(k).map_or_else(|| k.clone(), |d| period_name(d, grain, false, loc));
            format!("{name}: {missing} of {all}")
        })
        .collect();
    if gaps.is_empty() {
        return None;
    }
    let d = &tile.dims[entity];
    let field = engine
        .catalog()
        .table(&d.table)
        .and_then(|t| t.columns.iter().find(|c| c.name == d.column))
        .map_or_else(
            || d.column.clone(),
            |c| c.label.clone().unwrap_or_else(|| c.name.clone()),
        );
    let shown = gaps.len().min(6);
    let more = match gaps.len() - shown {
        0 => String::new(),
        n => format!(", and {n} more"),
    };
    Some(format!(
        "Some values of {field} have no row on the {} day with data of a period, so they count as nothing there: {}{more}.",
        if closing { "last" } else { "first" },
        gaps[..shown].join(", ")
    ))
}

/// Like [`run_tile`], for what leaves the computer: a snapshot, an exported page, a published
/// page, an Excel file. The notes in the result are the ones with no value and no file name.
pub fn run_tile_public(
    engine: &Engine,
    tile: &Tile,
    filters: &[DashFilter],
) -> anyhow::Result<QueryResult> {
    let mut res = run_tile(engine, tile, filters).map_err(|e| public_error(tile, e))?;
    res.notes = std::mem::take(&mut res.public_notes);
    Ok(res)
}

/// The error of a tile that leaves the computer (a snapshot, an exported or published page, an agent log, a
/// publish error saved in the file): the words of the app may quote a formula or a text typed in it
/// (`"web secret" is text`), so a tile with a named measure says only which measure and what happened
/// (V4-D52, V4-D54, V4-D104).
fn public_error(tile: &Tile, e: anyhow::Error) -> anyhow::Error {
    use crate::query::QueryError;
    match e.downcast_ref::<QueryError>() {
        Some(QueryError::MeasureBroken(name, _)) => {
            anyhow::anyhow!("The measure {name} cannot be worked out.")
        }
        // The words of the other query errors name tables and columns only.
        Some(_) => e,
        // A message of the database may quote a value.
        None if tile.measures.iter().any(|m| m.named.is_some()) => {
            anyhow::anyhow!("A tile that uses a measure could not be worked out.")
        }
        None => e,
    }
}

fn run_tile_inner(
    engine: &Engine,
    tile: &Tile,
    filters: &[DashFilter],
) -> anyhow::Result<QueryResult> {
    if tile.kind == "kpi" {
        return run_kpi(engine, tile, filters);
    }
    if tile.kind == "text" {
        return run_text(engine, tile, filters);
    }
    if tile.kind == "histogram" {
        return shape::histogram(engine, tile, &own_excluded(tile, filters));
    }
    if tile.kind == "box" {
        return advanced::boxplot(engine, tile, &own_excluded(tile, filters));
    }
    if tile.kind == "sankey" {
        return advanced::sankey(engine, tile, &own_excluded(tile, filters));
    }
    if tile.kind == "cohort" {
        return advanced::cohort(engine, tile, &own_excluded(tile, filters));
    }
    if tile.kind == "filter" {
        return filters::options(engine, tile, &own_excluded(tile, filters));
    }
    if tile.kind == "map" {
        let spec = tile.map.clone().unwrap_or(crate::maptile::MapSpec {
            level: crate::geo::Level::Province,
            country: None,
            style: None,
            lat: None,
            lon: None,
        });
        return crate::maptile::run(
            engine,
            &tile.measures,
            &tile.dims,
            &spec,
            own_excluded(tile, filters),
        );
    }
    let mut q =
        tile_query(tile, filters).ok_or_else(|| anyhow::anyhow!("the tile has no fields yet"))?;
    let wide = q.filters.clone();
    // Small multiples draw at most a dozen charts; the question is cut to those.
    let repeat = advanced::limit_repeat(engine, tile, &mut q, &wide)?;
    if matches!(tile.kind.as_str(), "pivot" | "heatmap")
        && let (Some(c), Some(m)) = (&tile.cols, q.measures.first())
    {
        // With many column values the cells are read for the first ones only, in the order of
        // the column totals. Otherwise the row limit could cut the data in the middle of a column.
        let all = engine.query(&SemanticQuery {
            measures: vec![m.clone()],
            dimensions: vec![c.clone()],
            filters: wide.clone(),
            sort: vec![],
            limit: Some(PIVOT_LIMIT),
        })?;
        if all.rows.len() > PIVOT_MAX_COLS {
            q.filters.push(Filter {
                table: c.table.clone(),
                column: c.column.clone(),
                grain: c.grain,
                op: FilterOp::In,
                values: all
                    .rows
                    .iter()
                    .take(PIVOT_MAX_COLS)
                    .filter_map(|r| r.first().cloned())
                    .collect(),
                via: c.via.clone(),
            });
        }
    }
    let pair = if tile.kind == "compare" {
        advanced::compare_periods(engine, tile, &mut q, &wide)?
    } else {
        None
    };
    let mut res = engine.query(&q)?;
    res.repeat = repeat;
    if tile.kind == "compare" {
        res.partial = advanced::compare_partial(engine, &q, &wide, pair.as_ref());
        advanced::compare_pairs(tile, &q, &mut res, pair);
    }
    if tile.kind == "calendar" {
        advanced::calendar_window(&mut res);
    }
    // Fill gaps and cut to the top N first, so the unfinished-period check below
    // still sees the real last period.
    shape::apply(engine, tile, &q, &wide, &mut res)?;
    advanced::sparks(engine, tile, &q, &mut res)?;
    // More in the tooltip (v5 build 10): the groups on show, after the top N.
    advanced::tips(engine, tile, &q, &mut res)?;
    let d = match tile.dims.first() {
        Some(d)
            if d.grain.is_some_and(|g| !g.is_part())
                && matches!(tile.kind.as_str(), "bar" | "line" | "area" | "waterfall") =>
        {
            d
        }
        _ => return Ok(res),
    };
    // A time chart also asks where the data ends, so an unfinished last
    // period is drawn as unfinished instead of as a drop.
    let latest = engine.query(&SemanticQuery {
        measures: vec![Measure {
            table: d.table.clone(),
            column: Some(d.column.clone()),
            agg: Agg::Max,
            per: None,
            via: d.via.clone(),
            named: None,
        }],
        dimensions: vec![],
        filters: q.filters.clone(),
        sort: vec![],
        limit: None,
    })?;
    let grain = d.grain.unwrap();
    let last = res
        .rows
        .iter()
        .filter_map(|r| r.first().and_then(|v| v.as_str()))
        .max()
        .map(str::to_string);
    if let (Some(max_raw), Some(last)) = (first_str(&latest), last)
        && grain != Grain::Day
        && let (Some(start), Some(max_day)) = (parse_day(&last), parse_day(&max_raw))
    {
        let end = add_grain(start, grain, 1) - Duration::days(1);
        if max_day < end {
            res.partial = Some(Partial {
                key: last,
                until: max_raw[..10].to_string(),
            });
        }
    }
    // A forecast (v5 build 08) after the unfinished period is known: it is left out of the fit.
    if matches!(tile.kind.as_str(), "line" | "area") {
        crate::analytics::attach(tile, grain, &mut res);
    }
    // Unusual values (v5 build 09): never the unfinished period.
    if matches!(tile.kind.as_str(), "line" | "area" | "bar") {
        crate::analytics::attach_unusual(tile, &mut res);
    }
    Ok(res)
}

fn first_str(r: &QueryResult) -> Option<String> {
    r.rows.first()?.first()?.as_str().map(str::to_string)
}

fn first_num(r: &QueryResult) -> Option<f64> {
    let i = r.columns.iter().position(|c| c.role == "measure")?;
    r.rows.first()?.get(i)?.as_f64()
}

/// A text tile's live numbers (build 10): each named measure it names, worked out as a number tile would show it, over
/// the main date of the measure's first table (by month, or the dashboard's grain), so "|compare" and "|period" read as
/// on a number tile. A measure without a date there is the total of every row. A measure that is gone, or that cannot
/// be worked out, has no value: the tile shows a dash.
fn run_text(engine: &Engine, tile: &Tile, filters: &[DashFilter]) -> anyhow::Result<QueryResult> {
    let infos = engine.measure_info();
    let mut values = BTreeMap::new();
    for id in live_numbers(tile.text.as_deref().unwrap_or("")) {
        let Some(home) = infos.get(&id).and_then(|i| i.tables.first().cloned()) else {
            continue;
        };
        let dims: Vec<Value> = engine
            .main_date_of(&home)
            .map(|c| json!({ "table": home, "column": c, "grain": tile.grain.unwrap_or(Grain::Month) }))
            .into_iter()
            .collect();
        let number: Tile = serde_json::from_value(json!({
            "id": tile.id, "type": "kpi",
            "measures": [{ "table": home, "column": null, "agg": "count", "named": id }],
            "dims": dims
        }))?;
        let Ok(Some(k)) = run_kpi(engine, &number, filters).map(|r| r.kpi) else {
            continue;
        };
        values.insert(id.clone(), json!(k.value));
        values.insert(format!("{id}|compare"), json!(k.delta));
        values.insert(format!("{id}|period"), json!(k.period));
    }
    Ok(QueryResult {
        values: Some(values),
        ..QueryResult::default()
    })
}

fn run_kpi(engine: &Engine, tile: &Tile, filters: &[DashFilter]) -> anyhow::Result<QueryResult> {
    let dim = tile.dims.first();
    // A number tile with only a date counts rows, like a chart with no value.
    let m = match (tile.measures.first(), dim) {
        (Some(m), _) => m.clone(),
        (None, Some(d)) => count_rows(&d.table),
        (None, None) => anyhow::bail!("the tile has no fields yet"),
    };
    let fs = own_excluded(tile, filters);
    let loc = engine.locale();
    let ask = |dims: Vec<Dimension>, filters: Vec<Filter>, limit: Option<u32>| {
        engine.query(&SemanticQuery {
            measures: vec![m.clone()],
            dimensions: dims,
            filters,
            sort: vec![],
            limit,
        })
    };
    let total = ask(vec![], fs.clone(), None)?;
    let mut res = total.clone();
    let plain = |value, period: &str| KpiInfo {
        value,
        period: period.into(),
        versus: None,
        prev: None,
        delta: None,
        partial: false,
        bars: vec![],
        elapsed: None,
        extras: vec![],
    };

    // A number over time needs a date grouped by a grain; anything else is a plain total.
    let Some((d, g)) = dim.and_then(|d| d.grain.filter(|g| !g.is_part()).map(|g| (d, g))) else {
        res.kpi = Some(KpiInfo {
            extras: extra_values(engine, tile, &fs)?,
            ..plain(first_num(&total), "All data")
        });
        return Ok(res);
    };
    let series = ask(vec![d.clone()], fs.clone(), Some(5000))?;
    let latest = engine.query(&SemanticQuery {
        measures: vec![Measure {
            table: d.table.clone(),
            column: Some(d.column.clone()),
            agg: Agg::Max,
            per: None,
            via: d.via.clone(),
            named: None,
        }],
        dimensions: vec![],
        filters: fs.clone(),
        sort: vec![],
        limit: None,
    })?;
    let mi = series
        .columns
        .iter()
        .position(|c| c.role == "measure")
        .unwrap_or(1);
    let rows: Vec<(String, Option<f64>)> = series
        .rows
        .iter()
        .filter_map(|r| {
            r.first()
                .and_then(|k| k.as_str())
                .map(|k| (k.to_string(), r.get(mi).and_then(|v| v.as_f64())))
        })
        .collect();
    let Some((last_key, value)) = rows.last().cloned() else {
        res.kpi = Some(plain(None, "No data"));
        return Ok(res);
    };
    let start = parse_day(&last_key).ok_or_else(|| anyhow::anyhow!("bad period {last_key}"))?;
    let end = add_grain(start, g, 1) - Duration::days(1);
    let max_raw = first_str(&latest);
    let max_day = max_raw.as_deref().and_then(parse_day).unwrap_or(end);
    let partial = max_day < end && g != Grain::Day;
    let prev_start = add_grain(start, g, -1);
    // Days with data over days in the period. The data runs up to its last day, so the days
    // between are counted even when no row falls on them (a quiet Sunday is not a missing day).
    let elapsed = if partial {
        let days = (end - start).num_days() + 1;
        (((max_day - start).num_days() + 1) as f64 / days as f64).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let datetime = series.columns.first().map(|c| c.dtype) == Some(crate::catalog::DType::DateTime)
        || max_raw.as_deref().is_some_and(|r| r.contains('T'));
    // The rows from one day to another, both whole, on top of the tile's filters.
    let window = |lo: NaiveDate, hi: NaiveDate| {
        let mut wfs = fs.clone();
        wfs.push(Filter {
            table: d.table.clone(),
            column: d.column.clone(),
            grain: None,
            op: FilterOp::Between,
            values: vec![
                json!(iso(lo)),
                json!(format!(
                    "{}{}",
                    iso(hi),
                    if datetime { "T23:59:59" } else { "" }
                )),
            ],
            via: d.via.clone(),
        });
        wfs
    };
    // A card's other values cover the period of the value: so far, when it is not over.
    let extras = extra_values(
        engine,
        tile,
        &window(start, if partial { max_day } else { end }),
    )?;

    // A stock (v5 build 07) is a value on a day, not over days: "On Oct 14" against "On Sep 14".
    let stock = engine.measure_facts(&m).is_ok_and(|f| f.stock.is_some());
    let (prev, period, versus) = if partial {
        // Compare the same number of days: Oct 1–14 against Sep 1–14.
        let days = (max_day - start).num_days();
        let prev_last = start - Duration::days(1);
        let prev_end = (prev_start + Duration::days(days)).min(prev_last);
        let wfs = window(prev_start, prev_end);
        let prev = first_num(&ask(vec![], wfs, None)?);
        if stock {
            (prev, on_day(max_day, loc), one_day(prev_end, loc))
        } else {
            (
                prev,
                span(start, max_day, true, loc),
                span(prev_start, prev_end, false, loc),
            )
        }
    } else {
        let prev = rows
            .iter()
            .find(|(k, _)| k.get(..10) == Some(iso(prev_start).as_str()))
            .and_then(|(_, v)| *v);
        if stock {
            // A finished period's stock is read on its last day with data: that day, and the one of the period before.
            let prev_day = engine
                .query(&SemanticQuery {
                    measures: vec![Measure {
                        table: d.table.clone(),
                        column: Some(d.column.clone()),
                        agg: Agg::Max,
                        per: None,
                        via: d.via.clone(),
                        named: None,
                    }],
                    dimensions: vec![],
                    filters: window(prev_start, start - Duration::days(1)),
                    sort: vec![],
                    limit: None,
                })
                .ok()
                .and_then(|r| first_str(&r))
                .and_then(|s| parse_day(&s));
            (
                prev,
                on_day(max_day.min(end), loc),
                prev_day.map_or_else(
                    || period_name(prev_start, g, false, loc),
                    |p| one_day(p, loc),
                ),
            )
        } else {
            (
                prev,
                period_name(start, g, false, loc),
                period_name(prev_start, g, false, loc),
            )
        }
    };
    let bars = rows[rows.len().saturating_sub(12)..]
        .iter()
        .map(|(k, v)| Bar {
            label: parse_day(k)
                .map(|d| period_name(d, g, false, loc))
                .unwrap_or_else(|| k.clone()),
            value: *v,
        })
        .collect();
    let delta = match (value, prev) {
        (Some(v), Some(p)) if p != 0.0 => Some((v - p) / p.abs()),
        _ => None,
    };
    res.kpi = Some(KpiInfo {
        value,
        period,
        versus: Some(versus),
        prev,
        delta,
        partial,
        bars,
        elapsed: Some(elapsed),
        extras,
    });
    Ok(res)
}

/// The values of a number tile after its first, over the given rows (build 08): a card shows them beside the value.
fn extra_values(
    engine: &Engine,
    tile: &Tile,
    filters: &[Filter],
) -> anyhow::Result<Vec<Option<f64>>> {
    tile.measures
        .iter()
        .skip(1)
        .map(|m| {
            Ok(first_num(&engine.query(&SemanticQuery {
                measures: vec![m.clone()],
                dimensions: vec![],
                filters: filters.to_vec(),
                sort: vec![],
                limit: None,
            })?))
        })
        .collect()
}

// ---- dates -------------------------------------------------------------------

pub(crate) fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

pub(crate) fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn add_grain(d: NaiveDate, g: Grain, n: i32) -> NaiveDate {
    let months = |k: i32| {
        if k >= 0 {
            d.checked_add_months(Months::new(k as u32))
        } else {
            d.checked_sub_months(Months::new((-k) as u32))
        }
        .unwrap_or(d)
    };
    match g {
        Grain::Day => d + Duration::days(n as i64),
        Grain::Week => d + Duration::days(7 * n as i64),
        Grain::Month => months(n),
        Grain::Quarter => months(3 * n),
        Grain::Year => months(12 * n),
        // A date part has no next period.
        Grain::MonthOfYear | Grain::Weekday | Grain::Hour => d,
    }
}

const MONTHS: [[&str; 12]; 2] = [
    [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    [
        "Januari",
        "Februari",
        "Maret",
        "April",
        "Mei",
        "Juni",
        "Juli",
        "Agustus",
        "September",
        "Oktober",
        "November",
        "Desember",
    ],
];

const SHORT_MONTHS: [[&str; 12]; 2] = [
    [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    [
        "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des",
    ],
];

fn table(loc: Locale) -> usize {
    match loc {
        Locale::En => 0,
        Locale::Id => 1,
    }
}

fn short(m: u32, loc: Locale) -> &'static str {
    SHORT_MONTHS[table(loc)][(m - 1) as usize]
}

/// A day and month in the order the locale reads them: "Oct 14" or "14 Okt".
fn day_month(m: u32, day: u32, loc: Locale) -> String {
    match loc {
        Locale::En => format!("{} {day}", short(m, loc)),
        Locale::Id => format!("{day} {}", short(m, loc)),
    }
}

/// English: "September 2026", "Q3 2026", "2026", "Week of Sep 7, 2026", "Sep 7, 2026".
/// Indonesian: "September 2026", "Q3 2026", "2026", "Minggu 7 Sep 2026", "7 Sep 2026".
pub fn period_name(d: NaiveDate, g: Grain, short_form: bool, loc: Locale) -> String {
    // A fiscal year or quarter has its own name: "FY 2027", "Q1 FY 2027" (v5 build 04).
    if let Some(name) = crate::calendar::current().fiscal_name(d, g, loc) {
        return name;
    }
    let (y, m, day) = (d.year(), d.month(), d.day());
    let full_date = match loc {
        Locale::En => format!("{} {day}, {y}", short(m, loc)),
        Locale::Id => format!("{day} {} {y}", short(m, loc)),
    };
    match g {
        Grain::Year => y.to_string(),
        Grain::Quarter => format!("Q{} {y}", (m - 1) / 3 + 1),
        Grain::Month if short_form => short(m, loc).to_string(),
        Grain::Month => format!("{} {y}", MONTHS[table(loc)][(m - 1) as usize]),
        Grain::Week if short_form => day_month(m, day, loc),
        Grain::Week => match loc {
            Locale::En => format!("Week of {full_date}"),
            Locale::Id => format!("Minggu {full_date}"),
        },
        Grain::Day => full_date,
        Grain::MonthOfYear | Grain::Weekday | Grain::Hour => short(m, loc).to_string(),
    }
}

/// English "Oct 1–14" (same month) or "Sep 28 – Oct 4"; Indonesian "1–14 Okt" or "28 Sep – 4 Okt".
/// "On Oct 14, 2026" (Indonesian "Per 14 Okt 2026"): the day a stock is read on (v5 build 07).
fn on_day(d: NaiveDate, loc: Locale) -> String {
    match loc {
        Locale::En => format!("On {}", one_day(d, loc)),
        Locale::Id => format!("Per {}", one_day(d, loc)),
    }
}

/// "Oct 14, 2026", "14 Okt 2026": the day the number before it was read on, after "vs".
fn one_day(d: NaiveDate, loc: Locale) -> String {
    match loc {
        Locale::En => format!("{}, {}", day_month(d.month(), d.day(), loc), d.year()),
        Locale::Id => format!("{} {}", day_month(d.month(), d.day(), loc), d.year()),
    }
}

fn span(a: NaiveDate, b: NaiveDate, with_year: bool, loc: Locale) -> String {
    let same = a.month() == b.month();
    let sep = if same { "–" } else { " – " };
    let year = match (with_year, loc) {
        (false, _) => String::new(),
        (true, Locale::En) => format!(", {}", b.year()),
        (true, Locale::Id) => format!(" {}", b.year()),
    };
    match loc {
        Locale::En => {
            let left = day_month(a.month(), a.day(), loc);
            let right = if same {
                b.day().to_string()
            } else {
                day_month(b.month(), b.day(), loc)
            };
            format!("{left}{sep}{right}{year}")
        }
        Locale::Id => {
            let left = if same {
                a.day().to_string()
            } else {
                day_month(a.month(), a.day(), loc)
            };
            format!("{left}{sep}{}{year}", day_month(b.month(), b.day(), loc))
        }
    }
}

// ---- snapshots ---------------------------------------------------------------

/// The key a category value has in the UI (`String(v)` in JavaScript).
pub fn js_key(v: &Value) -> String {
    match v {
        Value::Null => "\u{0}null".into(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{}", f as i64),
            Some(f) => format!("{f}"),
            None => n.to_string(),
        },
        other => other.to_string(),
    }
}

/// Builds the complete snapshot of a dashboard with the given cross-filters (the marks picked
/// by clicking). The dashboard's own filters join them: the ones it saves, the selection of each
/// filter tile, and its one grain for every time tile.
pub fn build(engine: &Engine, dashboard_id: &str, filters: &[DashFilter]) -> anyhow::Result<Value> {
    build_with(engine, dashboard_id, filters, true)
}

/// Like [`build`]; without `cross`, no click of the page is worked out ahead: a page a drill-through opens in an
/// exported site (build 13), which would otherwise hold every click of every opened value.
pub fn build_with(
    engine: &Engine,
    dashboard_id: &str,
    filters: &[DashFilter],
    cross_too: bool,
) -> anyhow::Result<Value> {
    // A database changes without a load: the links of live tables are read again, so a page that
    // is exported or published says what the data says now (V4-D51).
    engine.recheck_live_links();
    // A calculated column with `TODAY()` is as of today in what leaves the computer (V4-D34).
    engine.refresh_dates();
    build_quiet(engine, dashboard_id, filters, cross_too)
}

/// [`build_with`] without reading live links and dates again: for pages built many times in one export, on forks of
/// the engine, after one [`build_with`] did that (an exported site's every-page values, v5 build 02).
pub fn build_quiet(
    engine: &Engine,
    dashboard_id: &str,
    filters: &[DashFilter],
    cross_too: bool,
) -> anyhow::Result<Value> {
    let dash = engine
        .dashboard(dashboard_id)
        .ok_or_else(|| anyhow::anyhow!("no dashboard {dashboard_id}"))?;
    // The page as it opens holds each step of its parameters (v5 build 05, V5-D5); a saved view holds its own values.
    let mut payload = build_dash(
        engine,
        &dash,
        filters,
        cross_too,
        &BTreeMap::new(),
        cross_too,
    )?;
    // Saved views (v5 build 03, V5-D5): each one in full, at most `MAX_VIEWS`, and the one the dashboard opens on.
    let views: Vec<Value> = dash
        .get("views")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if cross_too && !views.is_empty() {
        let mut out = Vec::new();
        for v in views.iter().take(MAX_VIEWS) {
            let (Some(id), Some(name)) = (
                v.get("id").and_then(Value::as_str),
                v.get("name").and_then(Value::as_str),
            ) else {
                continue;
            };
            let (view_dash, clicks, drills) = with_view(&dash, v);
            let mut fs: Vec<DashFilter> = filters.to_vec();
            fs.extend(clicks);
            // The view's parameter values stand in for the saved ones while it is built (v5 build 05).
            let values: BTreeMap<String, Value> = v
                .get("params")
                .and_then(Value::as_object)
                .map(|o| o.iter().map(|(k, x)| (k.clone(), x.clone())).collect())
                .unwrap_or_default();
            let p = crate::params::with(&values, || {
                build_dash(engine, &view_dash, &fs, true, &drills, false)
            })?;
            out.push(json!({ "id": id, "name": name, "payload": p }));
        }
        let default = dash.get("defaultView").and_then(Value::as_str);
        if let Some(d) = default.filter(|d| out.iter().any(|v| v["id"] == *d)) {
            payload["defaultView"] = json!(d);
        }
        payload["views"] = json!(out);
    }
    Ok(payload)
}

/// One tile as its page opens (v5 build 12): with the dashboard's saved filters, its filter tiles' selections, its one
/// grain and the every-page filters, under its default view when it has one. Never a reader's clicks. An alert reads
/// its value here.
pub fn tile_as_opened(
    engine: &Engine,
    dashboard_id: &str,
    tile_id: &str,
) -> anyhow::Result<QueryResult> {
    let dash = engine
        .dashboard(dashboard_id)
        .ok_or_else(|| anyhow::anyhow!("no dashboard {dashboard_id}"))?;
    let view = dash
        .get("defaultView")
        .and_then(Value::as_str)
        .and_then(|d| {
            dash.get("views")?
                .as_array()?
                .iter()
                .find(|v| v.get("id").and_then(Value::as_str) == Some(d))
                .cloned()
        });
    let (dash, clicks, drills) = match &view {
        Some(v) => with_view(&dash, v),
        None => (dash, Vec::new(), BTreeMap::new()),
    };
    let values: BTreeMap<String, Value> = view
        .as_ref()
        .and_then(|v| v.get("params"))
        .and_then(Value::as_object)
        .map(|o| o.iter().map(|(k, x)| (k.clone(), x.clone())).collect())
        .unwrap_or_default();
    crate::params::with(&values, || {
        let grain = filters::dashboard_grain(&dash);
        let styles = engine.card_styles();
        let mut tiles = Vec::new();
        for mut raw in dash
            .get("tiles")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            if let Some(g) = grain {
                filters::apply_grain(&mut raw, g);
            }
            resolve_card(&mut raw, &styles);
            tiles.push(serde_json::from_value::<Tile>(raw)?);
        }
        let mut fs = filters::dashboard_filters(engine, &dash, &tiles);
        fs.extend(filters::every_page(engine, dashboard_id));
        let own: HashSet<String> = fs.iter().map(|f| f.from_tile.clone()).collect();
        fs.extend(clicks.into_iter().filter(|f| !own.contains(&f.from_tile)));
        let tile = tiles
            .iter()
            .find(|t| t.id == tile_id)
            .ok_or_else(|| anyhow::anyhow!("no tile {tile_id}"))?;
        crate::drill::run_public(
            engine,
            tile,
            &fs,
            &drills.get(tile_id).cloned().unwrap_or_default(),
        )
    })
}

/// A dashboard keeps at most this many views; an export holds each one in full (V5-D5).
pub const MAX_VIEWS: usize = 10;

/// A dashboard as a saved view reads it (v5 build 03): each filter tile with the view's selection (none when the view
/// has none for it), the view's grain, its clicks that still name a tile, and the drill path of each tile that drills.
pub fn with_view(
    dash: &Value,
    view: &Value,
) -> (
    Value,
    Vec<DashFilter>,
    BTreeMap<String, crate::drill::DrillState>,
) {
    let mut d = dash.clone();
    let ids: HashSet<String> = dash
        .get("tiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|t| t.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    if let Some(tiles) = d.get_mut("tiles").and_then(Value::as_array_mut) {
        for t in tiles {
            if t.get("type").and_then(Value::as_str) != Some("filter") {
                continue;
            }
            let id = t
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let sel = view.pointer(&format!(
                "/selections/{}",
                id.replace('~', "~0").replace('/', "~1")
            ));
            if let Some(f) = t.get_mut("filter").and_then(Value::as_object_mut) {
                for k in ["values", "range", "preset"] {
                    f.remove(k);
                }
                if let Some(sel) = sel.and_then(Value::as_object) {
                    for k in ["values", "range", "preset"] {
                        if let Some(v) = sel.get(k) {
                            f.insert(k.to_string(), v.clone());
                        }
                    }
                }
            }
        }
    }
    d["grain"] = view.get("grain").cloned().unwrap_or(Value::Null);
    let clicks: Vec<DashFilter> = view
        .get("filters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| serde_json::from_value::<DashFilter>(f.clone()).ok())
        .filter(|f| ids.contains(&f.from_tile))
        .collect();
    let drills = view
        .get("drill")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(id, _)| ids.contains(*id))
        .filter_map(|(id, path)| {
            let path = serde_json::from_value(path.clone()).ok()?;
            Some((id.clone(), crate::drill::DrillState { path }))
        })
        .collect();
    (d, clicks, drills)
}

/// One page from its dashboard's JSON: the tiles at their drill states (none: the top level).
fn build_dash(
    engine: &Engine,
    dash: &Value,
    filters: &[DashFilter],
    cross_too: bool,
    drills: &BTreeMap<String, crate::drill::DrillState>,
    steps_too: bool,
) -> anyhow::Result<Value> {
    let dashboard_id = dash.get("id").and_then(Value::as_str).unwrap_or_default();
    let dash = dash.clone();
    let grain = filters::dashboard_grain(&dash);
    let raw_tiles: Vec<Value> = dash
        .get("tiles")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    let mut all_tiles = Vec::new();
    let mut all_raw = Vec::new();
    let styles = engine.card_styles();
    for mut raw in raw_tiles {
        if let Some(g) = grain {
            filters::apply_grain(&mut raw, g);
        }
        resolve_card(&mut raw, &styles);
        all_tiles.push(serde_json::from_value::<Tile>(raw.clone())?);
        all_raw.push(raw);
    }
    // The dashboard's own filters come from its saved JSON, so a cross-filter the caller sends for
    // the same source is dropped: it would count twice.
    let mut own = filters::dashboard_filters(engine, &dash, &all_tiles);
    // The filters of other pages' filter tiles marked "Apply to every page" (v5 build 02). One the caller sends for
    // the same source (another value, in an exported site) takes its place.
    let every: Vec<DashFilter> = filters::every_page(engine, dashboard_id)
        .into_iter()
        .filter(|e| !filters.iter().any(|f| f.from_tile == e.from_tile))
        .collect();
    own.extend(every.iter().cloned());
    let own_sources: HashSet<&str> = own.iter().map(|f| f.from_tile.as_str()).collect();
    let clicked: Vec<DashFilter> = filters
        .iter()
        .filter(|f| !own_sources.contains(f.from_tile.as_str()))
        .cloned()
        .collect();
    // The saved filters as they are written in the dashboard, with the words a chip says ("Other").
    let saved = filters::saved_filters(&dash);
    let filters: Vec<DashFilter> = own.into_iter().chain(clicked.iter().cloned()).collect();
    let filters = filters.as_slice();

    let mut tiles = Vec::new();
    let mut raw_kept = Vec::new();
    for (t, raw) in all_tiles.into_iter().zip(all_raw) {
        // A text or image tile has no result, but the export and the viewer draw it. So does a
        // filter tile, which offers the values of its field.
        if is_static(&t.kind) || t.kind == "filter" || tile_query(&t, filters).is_some() {
            tiles.push(t);
            raw_kept.push(raw);
        }
    }

    let mut base: BTreeMap<String, QueryResult> = BTreeMap::new();
    for t in tiles.iter().filter(|t| !is_static_tile(t)) {
        // A tile that drills says so in its result: the viewer offers the drill switch (v5 build 01). A saved view
        // holds the level its reader drilled to (build 03).
        let state = drills.get(&t.id).cloned().unwrap_or_default();
        base.insert(
            t.id.clone(),
            crate::drill::run_public(engine, t, filters, &state)?,
        );
    }

    // Offline cross-filtering: each clickable tile, each of its values, every other tile.
    let mut cross: BTreeMap<String, BTreeMap<String, BTreeMap<String, QueryResult>>> =
        BTreeMap::new();
    // (clicked tile, value key, the filters with the click), in the order they were found.
    let mut clicks: Vec<(String, String, Vec<DashFilter>)> = Vec::new();
    for src in tiles.iter().filter(|_| cross_too) {
        // A cohort row is not a value of any field, so a click on it has nothing to filter by.
        if matches!(src.kind.as_str(), "kpi" | "cohort") {
            continue;
        }
        let Some(res) = base.get(&src.id) else {
            continue;
        };
        // Only a list of values can be clicked offline; a range or a span of dates cannot.
        if src.kind == "filter"
            && !matches!(
                src.filter.as_ref().map(|f| f.style.as_str()),
                None | Some("list") | Some("dropdown")
            )
        {
            continue;
        }
        // What a click on each mark filters by: a group of the first dimension, or for a
        // histogram the range of the bin.
        let mut groups: Vec<Vec<(Value, Filter)>> = vec![];
        let choices: Vec<(Value, Filter)> = if src.kind == "histogram" {
            let (Some(m), Some(bins)) = (
                src.measures.first().filter(|m| m.column.is_some()),
                res.bins,
            ) else {
                continue;
            };
            res.rows
                .iter()
                .filter(|r| r.get(1).and_then(Value::as_f64).is_some_and(|n| n > 0.0))
                .filter_map(|r| {
                    let lo = r.first()?.as_f64()?;
                    Some((
                        r[0].clone(),
                        Filter {
                            table: m.table.clone(),
                            column: m.column.clone()?,
                            grain: None,
                            op: FilterOp::Range,
                            values: vec![r[0].clone(), shape::round_edge(lo + bins.width).into()],
                            via: m.via.clone(),
                        },
                    ))
                })
                .collect()
        } else if src.kind == "map" {
            // A place filters by every name of it in the data (build 06). The key is its code. Points
            // are not clicked.
            let (Some(d), Some(geo)) = (src.dims.first(), res.geo.as_ref()) else {
                continue;
            };
            res.rows
                .iter()
                .zip(&geo.raw)
                .filter_map(|(r, raw)| {
                    let code = r.first()?.clone();
                    Some((
                        code,
                        Filter {
                            table: d.table.clone(),
                            column: d.column.clone(),
                            grain: d.grain,
                            op: FilterOp::In,
                            values: filters::with_empty_text(
                                engine,
                                &d.table,
                                &d.column,
                                raw.clone(),
                            ),
                            via: d.via.clone(),
                        },
                    ))
                })
                .collect()
        } else if src.kind == "sankey" {
            // Every node is a value of the field of its stage. The key says which stage.
            let mut seen = HashSet::new();
            let mut out = Vec::new();
            for r in &res.rows {
                let (Some(stage), Some(from), Some(to)) = (
                    r.first().and_then(Value::as_u64),
                    r.get(1).cloned(),
                    r.get(2).cloned(),
                ) else {
                    continue;
                };
                for (at, v) in [(stage as usize, from), (stage as usize + 1, to)] {
                    let Some(d) = src.dims.get(at) else { continue };
                    if js_key(&v) == shape::OTHER_KEY {
                        continue;
                    }
                    let key = format!("{STAGE_PREFIX}{at}:{}", js_key(&v));
                    if seen.insert(key.clone()) {
                        out.push((
                            Value::String(key),
                            Filter {
                                table: d.table.clone(),
                                column: d.column.clone(),
                                grain: d.grain,
                                op: FilterOp::In,
                                values: vec![v],
                                via: d.via.clone(),
                            },
                        ));
                    }
                }
            }
            out
        } else {
            let Some(d) = src.dims.first() else { continue };
            // A calendar is always read by day.
            let grain = if src.kind == "calendar" {
                Some(Grain::Day)
            } else {
                d.grain
            };
            let mut seen = HashSet::new();
            res.rows
                .iter()
                .filter_map(|r| r.first().cloned())
                .filter(|v| seen.insert(js_key(v)))
                .map(|v| {
                    // The "Other" bar stands for every group that is not in the top N.
                    let (op, values) = match (&res.other_of, js_key(&v) == shape::OTHER_KEY) {
                        (Some(keys), true) => (FilterOp::NotIn, keys.clone()),
                        _ => (FilterOp::In, vec![v.clone()]),
                    };
                    // The blank value stands for empty text too, as in the filter tile.
                    let values = filters::with_empty_text(engine, &d.table, &d.column, values);
                    (
                        v,
                        Filter {
                            table: d.table.clone(),
                            column: d.column.clone(),
                            grain,
                            op,
                            values,
                            via: d.via.clone(),
                        },
                    )
                })
                .collect()
        };
        groups.push(choices);
        // A click on a pivot column header filters by the column field. Its key starts with
        // `COL_PREFIX`, so it can never collide with a value of the first row field.
        if let (true, Some(c)) = (matches!(src.kind.as_str(), "pivot" | "heatmap"), &src.cols) {
            let at = if src.kind == "heatmap" {
                src.dims.len().min(1)
            } else {
                src.dims.len().min(PIVOT_MAX_ROWS)
            };
            let mut seen = HashSet::new();
            groups.push(
                res.rows
                    .iter()
                    .filter_map(|r| r.get(at).cloned())
                    .filter(|v| seen.insert(js_key(v)))
                    .map(|v| {
                        (
                            Value::String(format!("{COL_PREFIX}{}", js_key(&v))),
                            Filter {
                                table: c.table.clone(),
                                column: c.column.clone(),
                                grain: c.grain,
                                op: FilterOp::In,
                                values: vec![v],
                                via: c.via.clone(),
                            },
                        )
                    })
                    .collect(),
            );
        }
        let cap = if src.kind == "histogram" {
            shape::AUTO_MAX_BINS
        } else if src.kind == "sankey" {
            advanced::SANKEY_STAGES * advanced::MAX_NODES
        } else {
            MAX_CROSS_VALUES
        };
        for choices in groups {
            if choices.is_empty() || choices.len() > cap {
                continue;
            }
            for (v, filter) in choices {
                let mut fs: Vec<DashFilter> = filters
                    .iter()
                    .filter(|f| f.from_tile != src.id)
                    .cloned()
                    .collect();
                fs.push(DashFilter {
                    filter,
                    from_tile: src.id.clone(),
                    to: reach_of(src),
                });
                clicks.push((src.id.clone(), js_key(&v), fs));
            }
        }
    }
    // Every click runs every other tile: hundreds of small queries, run on forks of the engine in
    // parallel (V5-D35). The order of the results does not depend on the threads.
    let answers = in_parallel(engine, &clicks, |e, (src, _, fs)| {
        let mut per_tile = BTreeMap::new();
        // A tile the click does not reach keeps its own result; the viewer knows the reach (v5 build 02).
        let reach = fs.last().and_then(|f| f.to.clone());
        for t in tiles.iter().filter(|t| {
            t.id != *src
                && !is_static_tile(t)
                && t.kind != "filter"
                && reach.as_ref().is_none_or(|r| r.contains(&t.id))
        }) {
            per_tile.insert(t.id.clone(), run_tile_public(e, t, fs)?);
        }
        Ok(per_tile)
    })?;
    for ((src, key, _), per_tile) in clicks.into_iter().zip(answers) {
        cross.entry(src).or_default().insert(key, per_tile);
    }

    // Offline drill-down (V5-D5): the level below the top for the first values of each tile that drills, and
    // "Next level" for all of them. Deeper clicks say that the file holds one level.
    let mut downs: Vec<(String, String, crate::drill::DrillState)> = Vec::new();
    for t in tiles.iter().filter(|_| cross_too) {
        let Some((res, info)) = base
            .get(&t.id)
            .and_then(|r| r.drill.as_ref().map(|d| (r, d)))
        else {
            continue;
        };
        if !info.can_down {
            continue;
        }
        let mut seen = HashSet::new();
        for v in res
            .rows
            .iter()
            .filter_map(|r| r.first().cloned())
            .filter(|v| js_key(v) != shape::OTHER_KEY && seen.insert(js_key(v)))
            .take(MAX_DRILL_VALUES)
        {
            let key = js_key(&v);
            // One level below the level shown: a saved view may have drilled already (build 03).
            let mut path = info.path.clone();
            path.push(crate::drill::Step { value: Some(v) });
            downs.push((t.id.clone(), key, crate::drill::DrillState { path }));
        }
        let mut path = info.path.clone();
        path.push(crate::drill::Step { value: None });
        downs.push((
            t.id.clone(),
            DRILL_ALL_KEY.to_string(),
            crate::drill::DrillState { path },
        ));
    }
    let drilled = in_parallel(engine, &downs, |e, (id, _, state)| {
        let t = tiles
            .iter()
            .find(|t| &t.id == id)
            .expect("a tile of this page");
        crate::drill::run_public(e, t, filters, state)
    })?;
    let mut down: BTreeMap<String, BTreeMap<String, QueryResult>> = BTreeMap::new();
    for ((id, key, _), res) in downs.into_iter().zip(drilled) {
        down.entry(id).or_default().insert(key, res);
    }

    // Parameters offline (v5 build 05, V5-D5, V5-D40): for a parameter with at most 21 values, the tiles that read it
    // at each value but the one shown. One parameter moves at a time, with no click and no drill.
    let params_now = engine.params_now();
    let mut jobs: Vec<(String, String, Value, String)> = Vec::new();
    for p in params_now.iter().filter(|_| steps_too) {
        let Some(values) = p.steps() else { continue };
        let readers: Vec<&String> = tiles
            .iter()
            .filter(|t| base.get(&t.id).is_some_and(|r| r.params.contains(&p.id)))
            .map(|t| &t.id)
            .collect();
        for v in values.iter().filter(|v| js_key(v) != js_key(&p.value)) {
            for id in &readers {
                jobs.push((p.id.clone(), js_key(v), v.clone(), (*id).clone()));
            }
        }
    }
    let stepped = in_parallel(engine, &jobs, |e, (param, _, v, id)| {
        let t = tiles
            .iter()
            .find(|t| &t.id == id)
            .expect("a tile of this page");
        let values = BTreeMap::from([(param.clone(), v.clone())]);
        crate::params::with(&values, || {
            crate::drill::run_public(e, t, filters, &Default::default())
        })
    })?;
    let mut steps: BTreeMap<String, BTreeMap<String, BTreeMap<String, QueryResult>>> =
        BTreeMap::new();
    for ((param, key, _, id), res) in jobs.into_iter().zip(stepped) {
        steps
            .entry(param)
            .or_default()
            .entry(key)
            .or_default()
            .insert(id, res);
    }

    // The measures the tiles name: what the viewer needs to write their names and to know whether
    // their values add up (V4-D85d). The formula stays at home.
    let mut named: Vec<String> = Vec::new();
    for m in tiles.iter().flat_map(|t| t.measures.iter()) {
        if let Some(id) = &m.named
            && !named.contains(id)
        {
            named.push(id.clone());
        }
    }
    // So do a text tile's live numbers (build 10): the viewer writes them with the measure's name and format.
    for t in tiles.iter().filter(|t| t.kind == "text") {
        for id in live_numbers(t.text.as_deref().unwrap_or("")) {
            if !named.contains(&id) {
                named.push(id);
            }
        }
    }
    let info = engine.measure_info();
    let mut used: HashSet<String> = HashSet::new();
    for t in &tiles {
        for m in &t.measures {
            used.insert(m.table.clone());
            if let Some(p) = &m.per {
                used.insert(p.table.clone());
            }
            if let Some(i) = m.named.as_ref().and_then(|id| info.get(id)) {
                used.extend(i.tables.iter().cloned());
            }
        }
        used.extend(t.dims.iter().map(|d| d.table.clone()));
        used.extend(t.spark.iter().map(|d| d.table.clone()));
    }
    // A filter's chip names its field, so the file carries the table of every filter too: a page a drill-through
    // opened says "Store: Senja Menteng", not the column's bare name (build 13).
    used.extend(filters.iter().map(|f| f.filter.table.clone()));
    let measures: Vec<Value> = engine
        .measures()
        .into_iter()
        .filter(|m| named.contains(&m.id))
        .map(|m| {
            let i = info.get(&m.id);
            json!({
                "id": m.id,
                "name": m.name,
                "format": m.format,
                "dtype": i.and_then(|i| i.dtype),
                "additive": i.is_some_and(|i| i.additive),
            })
        })
        .collect();
    let tables: Vec<Value> = engine
        .catalog()
        .tables
        .iter()
        .filter(|t| used.contains(t.id.as_str()))
        .map(|t| {
            // The viewer needs the names, types and the column overrides, not the data checks. The
            // reason a calculated column is blank can quote its formula, so it stays at home (V4-D52).
            let mut columns = json!(t.columns);
            for c in columns.as_array_mut().into_iter().flatten() {
                for k in ["blank", "mixed", "failed", "error"] {
                    c.as_object_mut().map(|o| o.remove(k));
                }
            }
            json!({ "id": t.id, "name": t.name, "label": t.label, "columns": columns })
        })
        .collect();
    // The links the tiles name, so a viewer can write "Shipped from › region" in a title.
    let mut links: BTreeMap<String, String> = BTreeMap::new();
    for raw in &raw_kept {
        fn vias(v: &Value, out: &mut Vec<String>) {
            match v {
                Value::Object(m) => {
                    if let Some(Value::String(id)) = m.get("via") {
                        out.push(id.clone());
                    }
                    m.values().for_each(|x| vias(x, out));
                }
                Value::Array(a) => a.iter().for_each(|x| vias(x, out)),
                _ => {}
            }
        }
        let mut ids = Vec::new();
        vias(raw, &mut ids);
        for id in ids {
            if let Some(label) = engine.link_label(&id) {
                links.insert(id, label);
            }
        }
    }
    let mut payload = json!({
        "version": 2,
        "title": dash.get("title").cloned().unwrap_or(json!("Dashboard")),
        "workspace": engine.workspace_name(),
        "locale": engine.locale(),
        "generated": chrono_now(),
        "tables": tables,
        "tiles": raw_kept,
        "base": base,
        "cross": cross,
        "filters": clicked,
        "saved": saved,
        "every": every,
        "grain": grain,
    });
    // The viewer names periods in the workspace's calendar ("FY 2027"), as the app does (v5 build 04).
    let cal = engine.calendar();
    if cal != crate::calendar::Calendar::default() {
        payload["calendar"] = json!(cal);
    }
    // The digit style and the formats of the columns of the tables it carries: the viewer writes numbers and dates
    // as the app does.
    if let Some(d) = engine.digits() {
        payload["digits"] = json!(d);
    }
    let column_formats = engine.column_formats(|t| used.contains(t));
    if !column_formats.is_empty() {
        payload["columnFormats"] = Value::Object(column_formats);
    }
    // The parameters the page uses (its tiles name them, or its measures read them), with the values it shows, without
    // their descriptions (v5 build 05); the steps it holds.
    let named = crate::params::ids_named_by(std::slice::from_ref(&dash));
    let list: Vec<crate::params::Param> = params_now
        .into_iter()
        .filter(|p| named.contains(&p.id) || base.values().any(|r| r.params.contains(&p.id)))
        .map(|p| crate::params::Param {
            description: None,
            ..p
        })
        .collect();
    if !list.is_empty() {
        payload["params"] = json!(list);
    }
    if !steps.is_empty() {
        payload["steps"] = json!(steps);
    }
    if !down.is_empty() {
        payload["down"] = json!(down);
        // The hierarchies the drilled tiles use: the viewer names the field of the level it shows.
        let used: HashSet<&str> = tiles
            .iter()
            .filter_map(|t| t.drill.as_ref()?.hierarchy.as_deref())
            .collect();
        let hierarchies: Vec<crate::workspace::Hierarchy> = engine
            .hierarchies()
            .into_iter()
            .filter(|h| used.contains(h.id.as_str()))
            .collect();
        if !hierarchies.is_empty() {
            payload["hierarchies"] = json!(hierarchies);
        }
    }
    if !links.is_empty() {
        payload["links"] = json!(links);
    }
    if !measures.is_empty() {
        payload["measures"] = json!(measures);
    }
    // A story draws as a column in its own order (build 11); a grid page says nothing, as before.
    if dash.get("layout").and_then(Value::as_str) == Some("flow") {
        payload["layout"] = json!("flow");
        if let Some(order) = dash.get("order").filter(|o| o.is_array()) {
            payload["order"] = order.clone();
        }
    }
    Ok(payload)
}

pub(crate) fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    chrono::DateTime::from_timestamp(now.as_secs() as i64, now.subsec_nanos())
        .map(|t| t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: Locale = Locale::En;
    const ID: Locale = Locale::Id;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn names_and_spans() {
        assert_eq!(
            period_name(d("2026-09-01"), Grain::Month, false, EN),
            "September 2026"
        );
        assert_eq!(
            period_name(d("2026-07-01"), Grain::Quarter, false, EN),
            "Q3 2026"
        );
        assert_eq!(
            period_name(d("2026-09-07"), Grain::Week, false, EN),
            "Week of Sep 7, 2026"
        );
        assert_eq!(
            span(d("2026-10-01"), d("2026-10-14"), true, EN),
            "Oct 1–14, 2026"
        );
        assert_eq!(
            span(d("2026-09-28"), d("2026-10-04"), false, EN),
            "Sep 28 – Oct 4"
        );
    }

    #[test]
    fn indonesian_names_and_spans() {
        assert_eq!(
            period_name(d("2026-09-01"), Grain::Month, false, ID),
            "September 2026"
        );
        assert_eq!(
            period_name(d("2026-10-01"), Grain::Month, false, ID),
            "Oktober 2026"
        );
        assert_eq!(period_name(d("2026-08-01"), Grain::Month, true, ID), "Agu");
        assert_eq!(
            period_name(d("2026-07-01"), Grain::Quarter, false, ID),
            "Q3 2026"
        );
        assert_eq!(
            period_name(d("2026-09-07"), Grain::Week, false, ID),
            "Minggu 7 Sep 2026"
        );
        assert_eq!(
            period_name(d("2026-10-14"), Grain::Day, false, ID),
            "14 Okt 2026"
        );
        assert_eq!(
            span(d("2026-10-01"), d("2026-10-14"), true, ID),
            "1–14 Okt 2026"
        );
        assert_eq!(
            span(d("2026-09-28"), d("2026-10-04"), false, ID),
            "28 Sep – 4 Okt"
        );
        assert_eq!(
            span(d("2026-09-28"), d("2026-10-04"), true, ID),
            "28 Sep – 4 Okt 2026"
        );
    }

    #[test]
    fn grain_arithmetic() {
        assert_eq!(
            add_grain(d("2026-10-01"), Grain::Month, -1),
            d("2026-09-01")
        );
        assert_eq!(
            add_grain(d("2026-01-01"), Grain::Quarter, -1),
            d("2025-10-01")
        );
        assert_eq!(add_grain(d("2026-09-07"), Grain::Week, 1), d("2026-09-14"));
    }

    #[test]
    fn keys_match_javascript() {
        assert_eq!(js_key(&json!(1.0)), "1");
        assert_eq!(js_key(&json!(1.5)), "1.5");
        assert_eq!(js_key(&json!("a")), "a");
        assert_eq!(js_key(&Value::Null), "\u{0}null");
        assert_eq!(js_key(&json!(true)), "true");
    }
}
