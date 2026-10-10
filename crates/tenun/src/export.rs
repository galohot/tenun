//! Exported dashboards are assembled by the UI (it already knows how to query
//! every tile). The server supplies the viewer bundle and, for the desktop app,
//! writes the finished file where the user chose.

use serde::Deserialize;

const VIEWER_JS: &str = include_str!("../../../ui/dist-viewer/viewer.js");
const VIEWER_CSS: &str = include_str!("../../../ui/dist-viewer/viewer.css");

pub fn assets() -> serde_json::Value {
    serde_json::json!({ "js": VIEWER_JS, "css": VIEWER_CSS })
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
