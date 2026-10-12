//! Exported dashboards are assembled by the UI (it already knows how to query
//! every tile). The server supplies the viewer bundle and, for the desktop app,
//! writes the finished file where the user chose.

use serde::Deserialize;

const VIEWER_JS: &str = include_str!("../../../ui/dist-viewer/viewer.js");
const VIEWER_CSS: &str = include_str!("../../../ui/dist-viewer/viewer.css");

pub fn assets() -> serde_json::Value {
    serde_json::json!({ "js": VIEWER_JS, "css": VIEWER_CSS })
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// One self-contained HTML file: the viewer bundle and the snapshot.
pub fn html(payload: &serde_json::Value) -> String {
    let title = payload
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("Tenun dashboard");
    // A "</script" inside the data or the bundle would end the script element early.
    let data = payload.to_string().replace('<', "\\u003c");
    let js = VIEWER_JS.replace("</script", "<\\/script");
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"color-scheme\" content=\"light dark\">\n<meta name=\"generator\" content=\"Tenun\">\n\
         <title>{}</title>\n<style>{VIEWER_CSS}</style>\n</head>\n<body>\n<div id=\"app\"></div>\n\
         <script>window.TENUN = {data};</script>\n<script type=\"module\">{js}</script>\n</body>\n</html>\n",
        escape(title)
    )
}

/// The map shapes a snapshot's map tiles draw, put in `payload.geo` so an exported file works offline
/// (build 06): Tenun's own TopoJSON files by name (`world`, `ID-1`, `ID-2`), another country's downloaded
/// GeoJSON by `<country>-<level>`. Only the shapes its tiles use: a file with no map carries none.
pub fn with_maps(payload: &mut serde_json::Value, geo_dir: &std::path::Path) {
    use tenun_core::geo::{self, Level};
    let mut bundled: Vec<&str> = Vec::new();
    let mut downloaded: Vec<(String, Level)> = Vec::new();
    let tiles = payload
        .get("tiles")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    for t in tiles.iter().filter(|t| t["type"] == "map") {
        let spec: Option<tenun_core::maptile::MapSpec> =
            serde_json::from_value(t["map"].clone()).ok();
        let level = spec.as_ref().map_or(Level::Province, |s| s.level);
        let country = spec
            .as_ref()
            .map_or_else(|| "ID".to_string(), |s| s.country());
        let points = spec.as_ref().is_some_and(|s| s.points());
        let want: &[&str] = match (points, level, country.as_str()) {
            // Points lie on Indonesia's provinces, or on the world.
            (true, _, "ID") => &["ID-1"],
            (true, _, _) | (false, Level::Country, _) => &["world"],
            (false, Level::Province, "ID") => &["ID-1"],
            (false, Level::Regency, "ID") => &["ID-2", "ID-1"],
            (false, _, _) => {
                downloaded.push((country.clone(), level));
                &[]
            }
        };
        for w in want {
            if !bundled.contains(w) {
                bundled.push(w);
            }
        }
    }
    if bundled.is_empty() && downloaded.is_empty() {
        return;
    }
    let mut geo = serde_json::Map::new();
    for name in bundled {
        if let Some(bytes) = crate::api::bundled(&format!("geo/{name}.topo.json"))
            && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            geo.insert(name.to_string(), v);
        }
    }
    for (country, level) in downloaded {
        let Some(iso3) = geo::iso3(&country) else {
            continue;
        };
        let file = geo_dir.join(format!("{iso3}-{}.geojson", level.adm()));
        if let Some(v) = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        {
            let word = match level {
                Level::Province => "province",
                Level::Regency => "regency",
                Level::Country => "country",
            };
            geo.insert(format!("{country}-{word}"), v);
        }
    }
    payload["geo"] = serde_json::Value::Object(geo);
}

#[derive(Deserialize)]
pub struct WriteFile {
    pub path: String,
    pub html: String,
}

pub fn write(req: WriteFile) -> anyhow::Result<()> {
    let mut p = std::path::PathBuf::from(&req.path);
    if p.extension().is_none_or(|e| e != "html") {
        p.set_extension("html");
    }
    std::fs::write(&p, req.html)?;
    Ok(())
}
