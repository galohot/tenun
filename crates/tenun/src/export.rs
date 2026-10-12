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

// ---- a whole site (build 13, V4-D15) ----------------------------------------------------------------------------------

/// The map shapes of every page of a site, once, at the top (`site.geo`): the pages and the pages a drill-through
/// opens share them.
pub fn site_with_maps(site: &mut serde_json::Value, geo_dir: &std::path::Path) {
    let mut geo = serde_json::Map::new();
    let mut take = |p: &mut serde_json::Value| {
        with_maps(p, geo_dir);
        if let Some(serde_json::Value::Object(g)) = p.as_object_mut().and_then(|o| o.remove("geo"))
        {
            for (k, v) in g {
                geo.entry(k).or_insert(v);
            }
        }
    };
    if let Some(pages) = site["pages"].as_object_mut() {
        for p in pages.values_mut() {
            take(p);
        }
    }
    if let Some(drill) = site["drill"].as_object_mut() {
        for d in drill.values_mut() {
            if let Some(values) = d["values"].as_object_mut() {
                for p in values.values_mut() {
                    take(p);
                }
            }
        }
    }
    if !geo.is_empty() {
        site["geo"] = serde_json::Value::Object(geo);
    }
}

/// A site saved as a folder: `index.html` (the home page) and one file for each other page, each with the site's
/// navigation, its own page, the pages its drill-throughs open, and the maps they draw, so any file works alone.
pub fn site_files(site: &serde_json::Value) -> Vec<(String, String)> {
    let order: Vec<String> = site["order"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let titles: Vec<(String, String)> = order
        .iter()
        .map(|id| {
            (
                id.clone(),
                site["titles"][id].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    let home = site
        .pointer("/site/home")
        .and_then(|h| h.as_str())
        .filter(|h| order.iter().any(|x| x == h))
        .map(str::to_string)
        .or_else(|| order.first().cloned())
        .unwrap_or_default();
    let names = tenun_core::site::file_names(&titles, &home);
    let geo = site.get("geo").cloned();
    order
        .iter()
        .map(|id| {
            let mut one = site.clone();
            let page = site["pages"][id].clone();
            one["pages"] = serde_json::json!({ id: page });
            // Only the drill-throughs of this page's tiles.
            let prefix = format!("{id}/");
            let drill: serde_json::Map<String, serde_json::Value> = site["drill"]
                .as_object()
                .map(|d| {
                    d.iter()
                        .filter(|(k, _)| k.starts_with(&prefix))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .unwrap_or_default();
            one["drill"] = serde_json::Value::Object(drill);
            one["files"] = serde_json::json!(names);
            one["page"] = serde_json::json!(id);
            if let Some(g) = &geo {
                one["geo"] = g.clone();
            }
            (names[id].clone(), html(&one))
        })
        .collect()
}

/// A ZIP archive of files, stored as they are (no compression): small code, and every unzip tool reads it.
pub fn zip(files: &[(String, String)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let le16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
    let le32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    for (name, body) in files {
        let data = body.as_bytes();
        let crc = crc32(data);
        let offset = out.len() as u32;
        // Local file header: version 2.0, UTF-8 names (bit 11), stored, a fixed time (2026-01-01 00:00).
        le32(&mut out, 0x0403_4b50);
        for x in [20u16, 0x0800, 0, 0, 0x5C21] {
            le16(&mut out, x);
        }
        le32(&mut out, crc);
        le32(&mut out, data.len() as u32);
        le32(&mut out, data.len() as u32);
        le16(&mut out, name.len() as u16);
        le16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        // Its entry in the central directory.
        le32(&mut central, 0x0201_4b50);
        for x in [20u16, 20, 0x0800, 0, 0, 0x5C21] {
            le16(&mut central, x);
        }
        le32(&mut central, crc);
        le32(&mut central, data.len() as u32);
        le32(&mut central, data.len() as u32);
        for x in [name.len() as u16, 0, 0, 0, 0] {
            le16(&mut central, x);
        }
        le32(&mut central, 0);
        le32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let at = out.len() as u32;
    out.extend_from_slice(&central);
    le32(&mut out, 0x0605_4b50);
    for x in [0u16, 0, files.len() as u16, files.len() as u16] {
        le16(&mut out, x);
    }
    le32(&mut out, central.len() as u32);
    le32(&mut out, at);
    le16(&mut out, 0);
    out
}

/// CRC-32 (IEEE), as ZIP needs it.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_and_a_stored_zip_read_back() {
        // The check value of CRC-32/ISO-HDLC.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        let z = zip(&[
            ("index.html".into(), "<p>hi</p>".into()),
            ("a.html".into(), "x".into()),
        ]);
        // Two local headers, two central entries, and the end record that names 2 entries.
        assert_eq!(&z[..4], &[0x50, 0x4b, 0x03, 0x04]);
        let end = z.len() - 22;
        assert_eq!(&z[end..end + 4], &[0x50, 0x4b, 0x05, 0x06]);
        assert_eq!(u16::from_le_bytes([z[end + 10], z[end + 11]]), 2);
        // The first file's bytes sit right after its 30-byte header and its name.
        assert_eq!(&z[30 + 10..30 + 10 + 9], b"<p>hi</p>");
    }
}
