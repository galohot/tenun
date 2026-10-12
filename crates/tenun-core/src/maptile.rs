//! The map tile (build 06, V4-D8, V4-D9): a filled map of countries, provinces or kabupaten/kota, or points
//! from a latitude and a longitude.
//!
//! **Filled.** The tile groups its value by its place field. Each group is read as a place (`geo::match_place`
//! for the bundled maps, the names and ISO codes of the downloaded file for another country). A row of the result
//! is one place: its code, then the values. Two names of one place ("Kab. Bandung" and "3204") are one row, worked
//! out again over the two names together, so an average or a ratio is right too. `geo.raw` keeps the names of
//! each place, which a click filters by. A name that is no place, or two, is in `geo.unmatched`.
//!
//! **Points.** The tile groups by latitude, longitude and an optional colour field; the value (or the number of
//! rows) sizes each point. Coordinates outside the earth are dropped and counted.

use crate::engine::{Engine, QueryResult};
use crate::geo::{self, Level, Matched};
use crate::query::{Dimension, Filter, FilterOp, Measure, SemanticQuery};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// The map of a tile (`tile.map`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MapSpec {
    pub level: Level,
    /// ISO 3166-1 alpha-2 of the country, for a province or kabupaten map. Not set: Indonesia.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<MapStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lat: Option<Dimension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lon: Option<Dimension>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MapStyle {
    Filled,
    Points,
}

impl MapSpec {
    /// The country of a province or kabupaten map, upper case. Indonesia when none is set.
    pub fn country(&self) -> String {
        self.country
            .as_deref()
            .filter(|c| !c.trim().is_empty())
            .unwrap_or("ID")
            .trim()
            .to_ascii_uppercase()
    }

    pub fn points(&self) -> bool {
        self.style == Some(MapStyle::Points)
    }
}

/// What a map result says about its places (`QueryResult.geo`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GeoInfo {
    pub level: Option<Level>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// The code of the place of each row, index-aligned with the rows (filled maps).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub codes: Vec<Option<String>>,
    /// The names in the data of each row's place, index-aligned with the rows: what a click filters by.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw: Vec<Vec<Value>>,
    /// The standard name of each row's place, index-aligned with the rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    /// Names that are no place, or two, with the hint for the second kind. At most `MAX_UNMATCHED`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unmatched: Vec<Unmatched>,
    /// How many names are not on the map, the ones not listed included.
    #[serde(default, rename = "unmatchedCount", skip_serializing_if = "is_zero")]
    pub unmatched_count: usize,
    /// Points: the rows whose latitude or longitude is not on the earth, or blank.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dropped: usize,
    /// Another country's boundaries are not on this computer yet: the app downloads them, then asks again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub download: bool,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Unmatched {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// The most names the unmatched list carries.
pub const MAX_UNMATCHED: usize = 50;
/// The most places a map reads.
const MAX_PLACES: u32 = 5000;

/// The words of a map that cannot be drawn yet.
fn needs(what: &str) -> anyhow::Error {
    anyhow::anyhow!("{what}")
}

/// The text of a place value. A column of codes may be read as numbers: a whole number loses its `.0` (`32.0` is
/// province 32), and a kabupaten or kota code, which always has two digits after the dot, gets its last zero back
/// (`32.10` was read as 32.1). Tenun's own list of places, saved as CSV and read again, is read this way.
fn place_text(raw: &Value, level: Level) -> String {
    match raw {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            Some(f)
                if level == Level::Regency && ((f * 100.0).round() - f * 100.0).abs() < 1e-6 =>
            {
                format!("{f:.2}")
            }
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

/// Runs a map tile. `filters` are the filters that apply to it (its own clicks taken out).
pub fn run(
    engine: &Engine,
    measures: &[Measure],
    dims: &[Dimension],
    spec: &MapSpec,
    filters: Vec<Filter>,
) -> anyhow::Result<QueryResult> {
    if spec.points() {
        return points(engine, measures, dims, spec, filters);
    }
    let place = dims
        .first()
        .ok_or_else(|| needs("A map needs a field that names the places."))?;
    let measure = measures
        .first()
        .ok_or_else(|| needs("A map needs a value to colour the places by."))?;
    let country = spec.country();
    // Another country's provinces: its names come from the downloaded file.
    let other = (spec.level != Level::Country && !geo::BUNDLED.contains(&country.as_str()))
        .then(|| geo::downloaded_places(&engine.geo_dir(), &country, spec.level));
    if let Some(None) = other {
        let mut res = QueryResult::plain(vec![], vec![]);
        res.geo = Some(GeoInfo {
            level: Some(spec.level),
            country: Some(country),
            download: true,
            ..GeoInfo::default()
        });
        return Ok(res);
    }
    let other = other.flatten();
    let read = |raw: &str| -> Matched {
        match &other {
            Some(idx) => idx.find(raw),
            None => geo::match_place(spec.level, &country, raw),
        }
    };

    let base = engine.query(&SemanticQuery {
        measures: vec![measure.clone()],
        dimensions: vec![place.clone()],
        filters: filters.clone(),
        sort: vec![],
        limit: Some(MAX_PLACES),
    })?;
    // Each place, with the names that mean it, in the order they first come.
    let mut order: Vec<String> = Vec::new();
    let mut names: HashMap<String, Vec<Value>> = HashMap::new();
    let mut values: HashMap<String, Value> = HashMap::new();
    let mut unmatched: Vec<Unmatched> = Vec::new();
    let mut unmatched_count = 0;
    for row in &base.rows {
        let raw = row.first().cloned().unwrap_or(Value::Null);
        let text = place_text(&raw, spec.level);
        let m = if text.trim().is_empty() {
            Matched::None
        } else {
            read(&text)
        };
        match m.code() {
            Some(code) => {
                let code = code.to_string();
                if !names.contains_key(&code) {
                    order.push(code.clone());
                    values.insert(code.clone(), row.get(1).cloned().unwrap_or(Value::Null));
                }
                names.entry(code).or_default().push(raw);
            }
            None => {
                // A blank place is not a name to fix, and is not drawn.
                if text.trim().is_empty() {
                    continue;
                }
                unmatched_count += 1;
                if unmatched.len() < MAX_UNMATCHED {
                    unmatched.push(Unmatched {
                        name: text,
                        hint: m.hint(),
                    });
                }
            }
        }
    }
    // A place written two ways is asked again over both names, so a value that does not add up is right too.
    for code in &order {
        let raw = &names[code];
        if raw.len() < 2 {
            continue;
        }
        let mut fs = filters.clone();
        fs.push(Filter {
            table: place.table.clone(),
            column: place.column.clone(),
            grain: place.grain,
            op: FilterOp::In,
            values: raw.clone(),
            via: place.via.clone(),
        });
        let one = engine.query(&SemanticQuery {
            measures: vec![measure.clone()],
            dimensions: vec![],
            filters: fs,
            sort: vec![],
            limit: None,
        })?;
        let v = one
            .rows
            .first()
            .and_then(|r| r.first())
            .cloned()
            .unwrap_or(Value::Null);
        values.insert(code.clone(), v);
    }
    // The biggest first, so the rank of a place is its place in the list.
    order.sort_by(|a, b| {
        let (x, y) = (values[a].as_f64(), values[b].as_f64());
        match (x, y) {
            (Some(x), Some(y)) => y.total_cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => a.cmp(b),
        }
    });
    let mut res = base;
    res.rows = order
        .iter()
        .map(|c| vec![Value::String(c.clone()), values[c].clone()])
        .collect();
    res.geo = Some(GeoInfo {
        level: Some(spec.level),
        country: (spec.level != Level::Country).then(|| country.clone()),
        codes: order.iter().map(|c| Some(c.clone())).collect(),
        raw: order.iter().map(|c| names[c].clone()).collect(),
        names: order
            .iter()
            .map(|c| match &other {
                Some(idx) => idx.name_of(c),
                None => geo::name_of(spec.level, &country, c),
            })
            .collect(),
        unmatched,
        unmatched_count,
        dropped: 0,
        download: false,
    });
    Ok(res)
}

/// Points: one row for each place in the data, `[latitude, longitude, colour group?, size]`.
fn points(
    engine: &Engine,
    measures: &[Measure],
    dims: &[Dimension],
    spec: &MapSpec,
    filters: Vec<Filter>,
) -> anyhow::Result<QueryResult> {
    let (Some(lat), Some(lon)) = (&spec.lat, &spec.lon) else {
        return Err(needs("A map of points needs a latitude and a longitude."));
    };
    let mut dimensions = vec![lat.clone(), lon.clone()];
    dimensions.extend(dims.first().cloned());
    // Without a value, each point is sized by its number of rows.
    let size = measures.first().cloned().unwrap_or(Measure {
        table: lat.table.clone(),
        column: None,
        agg: crate::query::Agg::Count,
        per: None,
        via: lat.via.clone(),
        named: None,
    });
    let mut res = engine.query(&SemanticQuery {
        measures: vec![size],
        dimensions,
        filters,
        sort: vec![],
        limit: Some(MAX_PLACES),
    })?;
    let on_earth = |v: Option<&Value>, max: f64| {
        v.and_then(Value::as_f64)
            .is_some_and(|x| x.is_finite() && (-max..=max).contains(&x))
    };
    let before = res.rows.len();
    res.rows
        .retain(|r| on_earth(r.first(), 90.0) && on_earth(r.get(1), 180.0));
    res.geo = Some(GeoInfo {
        level: Some(Level::Country),
        country: spec
            .country
            .as_deref()
            .filter(|c| !c.trim().is_empty())
            .map(|c| c.trim().to_ascii_uppercase()),
        dropped: before - res.rows.len(),
        ..GeoInfo::default()
    });
    Ok(res)
}
