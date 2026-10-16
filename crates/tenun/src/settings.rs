//! Per-computer settings, kept next to the saved database passwords:
//! `<config dir>/tenun/settings.json`. They follow the person, not the
//! workspace file, so a shared `.tenun` file never carries them.
//!
//! A missing or broken file reads as the defaults. It is never an error.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tenun_core::secrets::Secrets;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Show the editing controls for Advanced features (V3-D1). Off by default.
    pub advanced: bool,
    /// The language of the app's screens, `en` or `id` (0.7 build 02). None: the computer's language.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Look for a new version once a day (0.7 build 01). None: on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updates: Option<bool>,
}

impl Settings {
    /// `<config dir>/tenun/settings.json`, beside `secrets.json`.
    pub fn default_path() -> PathBuf {
        Secrets::default_path().with_file_name("settings.json")
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Writes through a temporary file, so a crash cannot leave half a file.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tenun-settings-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn missing_file_reads_as_defaults() {
        let p = scratch("missing").join("settings.json");
        assert_eq!(Settings::load(&p), Settings::default());
        assert!(!Settings::load(&p).advanced);
    }

    #[test]
    fn broken_file_reads_as_defaults() {
        let dir = scratch("broken");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("settings.json");
        std::fs::write(&p, "{ not json").unwrap();
        assert_eq!(Settings::load(&p), Settings::default());
        std::fs::write(&p, r#"{"advanced":"yes"}"#).unwrap();
        assert_eq!(Settings::load(&p), Settings::default());
    }

    #[test]
    fn saved_settings_come_back() {
        let p = scratch("round").join("nested").join("settings.json");
        Settings {
            advanced: true,
            ..Default::default()
        }
        .save(&p)
        .unwrap();
        assert!(Settings::load(&p).advanced);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let dir = scratch("unknown");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("settings.json");
        std::fs::write(&p, r#"{"advanced":true,"later":1}"#).unwrap();
        assert!(Settings::load(&p).advanced);
    }
}
