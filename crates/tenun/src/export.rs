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
