//! An exported site (build 13, V4-D15, V4-D16): every page of the workspace's site in one payload, each as an
//! exported page is, and for each tile whose click opens another page (a drill-through), that page kept to each clicked
//! value, for the first [`DRILL_MAX`] values. The viewer draws the site's navigation around the pages and routes
//! between them by the address.

use std::collections::{BTreeMap, HashSet};

use serde_json::{Value, json};

use crate::engine::Engine;
use crate::filters;
use crate::query::{Filter, FilterOp};
use crate::shape;
use crate::snapshot::{self, DashFilter, js_key};

/// How many values of a clicked field an exported site opens a page for. Past it, the page opens unfiltered and says
/// why (V4-D16).
pub const DRILL_MAX: usize = 50;

/// The source a drill-through's filter names, so no tile of the opened page takes it as its own (`site.ts DRILL`).
pub const DRILL: &str = "\u{0}drill";

/// Every page of the site and the pages its drill-throughs open. `progress` hears (pages done, pages in all).
pub fn build_site(
    engine: &Engine,
    mut progress: impl FnMut(usize, usize),
) -> anyhow::Result<Value> {
    let site = engine
        .site()
        .filter(|s| s.get("on").and_then(Value::as_bool) == Some(true))
        .ok_or_else(|| {
            anyhow::anyhow!("This workspace is not a site. Switch it on under Site… first.")
        })?;
    let dashboards = engine.dashboards();
    let ids: Vec<String> = dashboards
        .iter()
        .filter_map(|d| d.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    let total = ids.len();
    let mut pages = serde_json::Map::new();
    let mut drill = serde_json::Map::new();
    for (n, id) in ids.iter().cloned().enumerate() {
        let page = snapshot::build(engine, &id, &[])?;
        for tile in page["tiles"].as_array().cloned().unwrap_or_default() {
            let Some(target) = tile
                .pointer("/action/page")
                .and_then(Value::as_str)
                .filter(|t| *t != id && ids.iter().any(|x| x == t))
            else {
                continue;
            };
            let tile_id = tile.get("id").and_then(Value::as_str).unwrap_or_default();
            let res = &page["base"][tile_id];
            let choices = drill_choices(engine, &tile, res);
            let more = choices.len() > DRILL_MAX;
            let mut values = serde_json::Map::new();
            for (key, filter) in choices.into_iter().take(DRILL_MAX) {
                let f = DashFilter {
                    filter,
                    from_tile: DRILL.into(),
                    to: None,
                };
                values.insert(key, snapshot::build_with(engine, target, &[f], false)?);
            }
            drill.insert(
                format!("{id}/{tile_id}"),
                json!({ "page": target, "values": values, "more": more }),
            );
        }
        pages.insert(id, page);
        progress(n + 1, total);
    }
    let every = every_page_values(engine, &ids, &pages)?;
    // Each page's name, for the navigation and for "From <page>" on a page a drill-through opened.
    let titles: serde_json::Map<String, Value> = dashboards
        .iter()
        .filter_map(|d| {
            Some((
                d.get("id")?.as_str()?.to_string(),
                d.get("title").cloned().unwrap_or(json!("")),
            ))
        })
        .collect();
    let title = site
        .get("title")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| engine.workspace_name());
    Ok(json!({
        "version": 2,
        "kind": "site",
        "title": title,
        "workspace": engine.workspace_name(),
        "locale": engine.locale(),
        "generated": snapshot::chrono_now(),
        "site": site,
        "order": ids,
        "titles": titles,
        // Pages left out of the navigation: still in `pages`, since a link or a click reaches them.
        "hidden": dashboards.iter().filter(|d| d.get("hidden").and_then(Value::as_bool) == Some(true)).filter_map(|d| d.get("id").cloned()).collect::<Vec<_>>(),
        "pages": pages,
        "drill": drill,
        "every": every,
    }))
}

/// How many values of an every-page filter an exported site holds the other pages for (V5-D5).
pub const EVERY_MAX: usize = 50;

/// Every-page filters offline (v5 build 02, V5-D37): for each list or dropdown filter tile marked "Apply to every
/// page", and each of its first [`EVERY_MAX`] values, the results of every other page's tiles with that value. By
/// `<page>/<tile>`: `{ page, tile, more, values: { <value key>: { <other page>: { <tile>: result } } } }`.
fn every_page_values(
    engine: &Engine,
    ids: &[String],
    pages: &serde_json::Map<String, Value>,
) -> anyhow::Result<Value> {
    let mut every = serde_json::Map::new();
    // (source, value key, other page, the filter with that value)
    let mut jobs: Vec<(String, String, String, DashFilter)> = Vec::new();
    for (pid, page) in pages {
        for tile in page["tiles"].as_array().into_iter().flatten() {
            let style = tile
                .pointer("/filter/style")
                .and_then(Value::as_str)
                .unwrap_or("list");
            if tile.pointer("/filter/everyPage").and_then(Value::as_bool) != Some(true)
                || !matches!(style, "list" | "dropdown")
            {
                continue;
            }
            let (Some(tid), Some(table), Some(column)) = (
                tile.get("id").and_then(Value::as_str),
                tile.pointer("/dims/0/table").and_then(Value::as_str),
                tile.pointer("/dims/0/column").and_then(Value::as_str),
            ) else {
                continue;
            };
            let via = tile
                .pointer("/dims/0/via")
                .and_then(Value::as_str)
                .map(str::to_string);
            let values: Vec<Value> = page["base"][tid]["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| r.get(0).cloned())
                .collect();
            let source = format!("{pid}/{tid}");
            for v in values.iter().take(EVERY_MAX) {
                let f = DashFilter {
                    filter: Filter {
                        table: table.to_string(),
                        column: column.to_string(),
                        grain: None,
                        op: FilterOp::In,
                        values: filters::with_empty_text(engine, table, column, vec![v.clone()]),
                        via: via.clone(),
                    },
                    from_tile: filters::every_id(pid, tid),
                    to: None,
                };
                for other in ids.iter().filter(|o| *o != pid) {
                    jobs.push((source.clone(), js_key(v), other.clone(), f.clone()));
                }
            }
            every.insert(
                source,
                json!({ "page": pid, "tile": tid, "more": values.len() > EVERY_MAX, "values": {} }),
            );
        }
    }
    let built = snapshot::in_parallel(engine, &jobs, |e, (_, _, page, f)| {
        snapshot::build_quiet(e, page, std::slice::from_ref(f), false).map(|p| p["base"].clone())
    })?;
    for ((source, key, page, _), base) in jobs.into_iter().zip(built) {
        let values = &mut every[&source]["values"];
        if values.get(&key).is_none() {
            values[&key] = json!({});
        }
        values[&key][&page] = base;
    }
    Ok(Value::Object(every))
}

/// What a click on each mark of a tile keeps only: a place of a map by every name it has, any other tile by its first
/// field, "Other" as every group that is not shown. The key is the one the viewer looks a click up by.
fn drill_choices(engine: &Engine, tile: &Value, res: &Value) -> Vec<(String, Filter)> {
    let Some(d) = tile.pointer("/dims/0") else {
        return vec![];
    };
    let (Some(table), Some(column)) = (
        d.get("table").and_then(Value::as_str),
        d.get("column").and_then(Value::as_str),
    ) else {
        return vec![];
    };
    let grain = d
        .get("grain")
        .and_then(|g| serde_json::from_value(g.clone()).ok());
    let via = d.get("via").and_then(Value::as_str).map(str::to_string);
    let filter = |op: FilterOp, values: Vec<Value>| Filter {
        table: table.to_string(),
        column: column.to_string(),
        grain,
        op,
        values,
        via: via.clone(),
    };
    let rows = res["rows"].as_array().cloned().unwrap_or_default();
    if tile.get("type").and_then(Value::as_str) == Some("map") {
        let raw = res
            .pointer("/geo/raw")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        return rows
            .iter()
            .zip(raw)
            .filter_map(|(r, names)| {
                let code = r.get(0)?.clone();
                let names = names.as_array()?.clone();
                Some((
                    js_key(&code),
                    filter(
                        FilterOp::In,
                        filters::with_empty_text(engine, table, column, names),
                    ),
                ))
            })
            .collect();
    }
    let other_of = res.get("otherOf").and_then(Value::as_array).cloned();
    let mut seen = HashSet::new();
    rows.iter()
        .filter_map(|r| r.get(0).cloned())
        .filter(|v| seen.insert(js_key(v)))
        .map(|v| {
            let key = js_key(&v);
            let f = match (&other_of, key == shape::OTHER_KEY) {
                (Some(keys), true) => filter(FilterOp::NotIn, keys.clone()),
                _ => filter(
                    FilterOp::In,
                    filters::with_empty_text(engine, table, column, vec![v]),
                ),
            };
            (key, f)
        })
        .collect()
}

/// The pages a site's address may name, with the file each is saved as in a folder (`siteview.ts fileNames`, the same
/// rule): the home page is `index.html`; any other a name from its title.
pub fn file_names(pages: &[(String, String)], home: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut used: HashSet<String> = HashSet::from(["index".to_string()]);
    for (id, title) in pages {
        if id == home {
            out.insert(id.clone(), "index.html".to_string());
            continue;
        }
        let mut base = String::new();
        for c in title.chars() {
            let c = fold(c);
            if c.is_ascii_alphanumeric() {
                base.push(c.to_ascii_lowercase());
            } else if !base.ends_with('-') {
                base.push('-');
            }
        }
        let base: String = base.trim_matches('-').chars().take(60).collect();
        let base = if base.is_empty() {
            "page".to_string()
        } else {
            base
        };
        let mut name = base.clone();
        let mut k = 2;
        while used.contains(&name) {
            name = format!("{base}-{k}");
            k += 1;
        }
        used.insert(name.clone());
        out.insert(id.clone(), format!("{name}.html"));
    }
    out
}

/// A letter with its accent taken off (é → e), for a file name.
fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ñ' | 'Ñ' => 'n',
        'ç' | 'Ç' => 'c',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::file_names;

    #[test]
    fn a_folder_names_the_home_page_index_and_the_others_by_their_titles() {
        let pages: Vec<(String, String)> = [
            ("d1", "Coffee overview"),
            ("d2", "Ringkasan Penjualan — Q3 (Café)"),
            ("d3", "Coffee  Overview!"),
            ("d4", ""),
            ("d5", "Index"),
            ("d6", "日本"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let names = file_names(&pages, "d1");
        let got: Vec<&str> = names.values().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "index.html",
                "ringkasan-penjualan-q3-cafe.html",
                "coffee-overview.html",
                "page.html",
                "index-2.html",
                "page-2.html"
            ]
        );
    }
}
