//! Drill-down inside a chart (v5 build 01, V5-D3): a date goes year › quarter › month › day, and a hierarchy
//! (Region › Store) goes down its fields. The path the reader took is not saved in the tile; it comes with the
//! question. A path turns into the next level's field plus one filter for each value of the path, and the tile
//! runs as any tile does.

use crate::engine::{Engine, QueryResult};
use crate::query::{Dimension, Filter, FilterOp, Grain};
use crate::snapshot::{DashFilter, Tile, run_tile, run_tile_public};
use crate::workspace::Hierarchy;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where the reader is: one step for each level they went down. A step with a value keeps only that value of its
/// level; a step without one ("Next level") shows the level below for every value of it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DrillState {
    #[serde(default)]
    pub path: Vec<Step>,
}

/// One step down. `value` absent: every value. `value: null`: the blank value.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Step {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<Value>,
}

/// A value that is there, even `null`, is `Some`: only a missing one means "every value".
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

/// What the result says about drilling: the names of the levels, the one shown, the path to it, and whether there is
/// a level below.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DrillInfo {
    pub levels: Vec<String>,
    /// The grain of each level that is a date cut into periods, so the app can write "Q3 2026".
    pub grains: Vec<Option<Grain>>,
    pub at: usize,
    pub path: Vec<Step>,
    #[serde(rename = "canDown")]
    pub can_down: bool,
}

/// The source a drill filter names: never the tile itself, which would leave it out (`own_excluded`).
pub const DRILL_SOURCE: &str = "\u{0}drill";

/// The tile types that drill (GRANDPLAN 01 step 4).
pub fn drills(kind: &str) -> bool {
    // A map is left out: a place stands for several names in the data, and a path holds one value (V5-D36).
    matches!(
        kind,
        "bar" | "line" | "area" | "table" | "pie" | "donut" | "pivot"
    )
}

/// One level of a drill: a field, with the grain of a date.
#[derive(Debug, Clone)]
struct Level {
    dim: Dimension,
    label: String,
}

const DATE_CHAIN: [Grain; 4] = [Grain::Year, Grain::Quarter, Grain::Month, Grain::Day];

fn grain_label(g: Grain) -> &'static str {
    match g {
        Grain::Year => "Year",
        Grain::Quarter => "Quarter",
        Grain::Month => "Month",
        Grain::Week => "Week",
        Grain::Day => "Day",
        _ => "Period",
    }
}

/// The levels a tile drills through, from its own first field down; `None` when it does not drill.
fn chain(engine: &Engine, tile: &Tile, hierarchies: &[Hierarchy]) -> Option<Vec<Level>> {
    let spec = tile.drill.as_ref()?;
    if !drills(&tile.kind) {
        return None;
    }
    let first = tile.dims.first()?;
    match &spec.hierarchy {
        None => {
            // A date cut into periods: its grain, then the smaller ones. A week goes to its days.
            let g = first.grain?;
            let grains: Vec<Grain> = match g {
                Grain::Week => vec![Grain::Week, Grain::Day],
                _ => DATE_CHAIN.iter().copied().skip_while(|x| *x != g).collect(),
            };
            (grains.len() >= 2).then(|| {
                grains
                    .into_iter()
                    .map(|g| Level {
                        dim: Dimension {
                            grain: Some(g),
                            ..first.clone()
                        },
                        label: grain_label(g).to_string(),
                    })
                    .collect()
            })
        }
        Some(id) => {
            let h = hierarchies.iter().find(|h| &h.id == id)?;
            let start = h
                .levels
                .iter()
                .position(|l| l.table == first.table && l.column == first.column)?;
            let catalog = engine.catalog();
            let levels: Vec<Level> = h.levels[start..]
                .iter()
                .enumerate()
                .map(|(i, l)| Level {
                    // The top level is the tile's own field, with its link (a named link stays named).
                    // A lower field of the same table keeps that link too, so the whole path reads one link.
                    dim: if i == 0 {
                        first.clone()
                    } else {
                        Dimension {
                            table: l.table.clone(),
                            column: l.column.clone(),
                            grain: None,
                            via: (l.table == first.table)
                                .then(|| first.via.clone())
                                .flatten(),
                        }
                    },
                    label: catalog
                        .table(&l.table)
                        .and_then(|t| t.columns.iter().find(|c| c.name == l.column))
                        .and_then(|c| c.label.clone())
                        .unwrap_or_else(|| l.column.clone()),
                })
                .collect();
            (levels.len() >= 2).then_some(levels)
        }
    }
}

/// The tile as it is drilled, the filters of its path, and what to tell the reader. `None`: the tile does not drill.
pub fn apply(
    engine: &Engine,
    tile: &Tile,
    state: &DrillState,
) -> Option<(Tile, Vec<DashFilter>, DrillInfo)> {
    let levels = chain(engine, tile, &engine.hierarchies())?;
    let deepest = levels.len() - 1;
    let path: Vec<Step> = state.path.iter().take(deepest).cloned().collect();
    let at = path.len();
    let mut t = tile.clone();
    t.dims[0] = levels[at].dim.clone();
    let filters = path
        .iter()
        .zip(&levels)
        .filter_map(|(step, l)| {
            let v = step.value.clone()?;
            Some(DashFilter {
                filter: Filter {
                    table: l.dim.table.clone(),
                    column: l.dim.column.clone(),
                    grain: l.dim.grain,
                    op: FilterOp::In,
                    values: crate::filters::with_empty_text(
                        engine,
                        &l.dim.table,
                        &l.dim.column,
                        vec![v],
                    ),
                    via: l.dim.via.clone(),
                },
                from_tile: DRILL_SOURCE.to_string(),
                to: None,
            })
        })
        .collect();
    let info = DrillInfo {
        levels: levels.iter().map(|l| l.label.clone()).collect(),
        grains: levels.iter().map(|l| l.dim.grain).collect(),
        at,
        path,
        can_down: at < deepest,
    };
    Some((t, filters, info))
}

/// Runs a tile at the drill state (none: its top level). A tile that does not drill runs as it is.
pub fn run(
    engine: &Engine,
    tile: &Tile,
    filters: &[DashFilter],
    state: &DrillState,
) -> anyhow::Result<QueryResult> {
    run_with(engine, tile, filters, state, false)
}

/// [`run`] for what leaves the computer (see [`run_tile_public`]).
pub fn run_public(
    engine: &Engine,
    tile: &Tile,
    filters: &[DashFilter],
    state: &DrillState,
) -> anyhow::Result<QueryResult> {
    run_with(engine, tile, filters, state, true)
}

fn run_with(
    engine: &Engine,
    tile: &Tile,
    filters: &[DashFilter],
    state: &DrillState,
    public: bool,
) -> anyhow::Result<QueryResult> {
    let go = |t: &Tile, fs: &[DashFilter]| {
        if public {
            run_tile_public(engine, t, fs)
        } else {
            run_tile(engine, t, fs)
        }
    };
    let Some((t, extra, info)) = apply(engine, tile, state) else {
        return go(tile, filters);
    };
    let mut fs = filters.to_vec();
    fs.extend(extra);
    let mut res = go(&t, &fs)?;
    res.drill = Some(info);
    Ok(res)
}
