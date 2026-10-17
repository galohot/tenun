//! New versions of the desktop app (0.7 build 01). The shell hands the server an [`Updater`]; the
//! server decides when to look and tells the windows what it found. `tenun serve` has no updater.
//!
//! The app looks at most once a day, and only while the setting is on. It installs only when the
//! user says so. The look is one request for `latest.json` on the GitHub release: it carries nothing
//! but what any download carries.

use crate::AppState;
use crate::settings::Settings;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// What a desktop shell can do about new versions.
pub trait Updater: Send + Sync {
    /// Looks for a newer version. None: this one is the newest.
    fn check(&self) -> anyhow::Result<Option<UpdateInfo>>;
    /// Downloads the version `check` found, checks its signature, installs it and restarts the app.
    fn install(&self) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    /// The release notes, as written in the release.
    pub notes: String,
}

/// What the windows are told (`GET /api/update`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateStatus {
    /// This is the desktop app, so it can update itself.
    pub supported: bool,
    /// The newer version found by the last look.
    pub available: Option<UpdateInfo>,
    /// When the last look ended (RFC 3339), from this run or an earlier one.
    pub checked: Option<String>,
    /// Why the last look or install failed, in words.
    pub error: Option<String>,
}

const DAY: chrono::TimeDelta = chrono::TimeDelta::hours(24);

/// `<config dir>/tenun/update-checked`, beside the settings: the time of the last look.
pub fn stamp_path(settings_path: &Path) -> PathBuf {
    settings_path.with_file_name("update-checked")
}

fn last_check(settings_path: &Path) -> Option<chrono::DateTime<chrono::Utc>> {
    let s = std::fs::read_to_string(stamp_path(settings_path)).ok()?;
    chrono::DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|d| d.to_utc())
}

/// Whether the app should look now: the setting is on and the last look is a day old.
pub fn due(
    settings: &Settings,
    last: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    settings.updates != Some(false) && last.is_none_or(|l| now - l >= DAY || l > now)
}

/// Looks for a new version now and tells the windows. One look at a time.
pub fn check_now(state: &Arc<AppState>) {
    let Some(updater) = state.updater.lock().unwrap().clone() else {
        return;
    };
    if state.update_busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let result = updater.check();
    let now = chrono::Utc::now().to_rfc3339();
    let _ = std::fs::write(stamp_path(&state.settings_path), &now);
    {
        let mut st = state.update.lock().unwrap();
        st.checked = Some(now);
        match result {
            Ok(found) => {
                st.available = found;
                st.error = None;
            }
            Err(e) => st.error = Some(format!("{e:#}")),
        }
    }
    state.update_busy.store(false, Ordering::SeqCst);
    emit(state);
}

/// Called once a minute: looks when a look is due.
pub fn tick(state: &Arc<AppState>) {
    if state.updater.lock().unwrap().is_none() {
        return;
    }
    let settings = Settings::load(&state.settings_path);
    if due(
        &settings,
        last_check(&state.settings_path),
        chrono::Utc::now(),
    ) {
        let st = state.clone();
        std::thread::spawn(move || check_now(&st));
    }
}

/// Installs the version found. On success the app restarts and this never returns.
pub fn install(state: &Arc<AppState>) -> anyhow::Result<()> {
    let updater = state
        .updater
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Only the desktop app can update itself."))?;
    if let Err(e) = updater.install() {
        state.update.lock().unwrap().error = Some(format!("{e:#}"));
        emit(state);
        return Err(e);
    }
    Ok(())
}

pub fn status(state: &AppState) -> UpdateStatus {
    let mut st = state.update.lock().unwrap().clone();
    st.supported = state.updater.lock().unwrap().is_some();
    if st.checked.is_none() {
        st.checked = last_check(&state.settings_path).map(|d| d.to_rfc3339());
    }
    st
}

fn emit(state: &AppState) {
    if let Ok(v) = serde_json::to_value(status(state)) {
        state.emit(serde_json::json!({ "type": "update", "status": v }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2026, 10, 16, h, 0, 0).unwrap()
    }

    #[test]
    fn looks_once_a_day_while_on() {
        let on = Settings::default();
        assert!(due(&on, None, at(9)));
        assert!(!due(&on, Some(at(9)), at(20)));
        assert!(due(&on, Some(at(9)), at(9) + DAY));
        // A clock set back does not stop the looks for good.
        assert!(due(&on, Some(at(20)), at(9)));
    }

    #[test]
    fn never_looks_when_off() {
        let off = Settings {
            updates: Some(false),
            ..Default::default()
        };
        assert!(!due(&off, None, at(9)));
        assert!(!due(&off, Some(at(0)), at(0) + DAY * 30));
    }
}
