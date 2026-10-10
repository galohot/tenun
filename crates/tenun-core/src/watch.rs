//! Watches data files and reports when one changes.
//!
//! Excel and LibreOffice save by writing a temp file and renaming it over the
//! original, which drops a watch on the file itself. So we watch each file's
//! folder and match events by path, then wait for 300 ms of quiet before
//! reporting, because one save produces several events.

use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    dirs: HashSet<PathBuf>,
    files: std::sync::Arc<std::sync::Mutex<HashSet<PathBuf>>>,
}

const QUIET: Duration = Duration::from_millis(300);

impl FileWatcher {
    /// `on_change` receives the set of watched files that changed.
    pub fn new(on_change: impl Fn(BTreeSet<PathBuf>) + Send + 'static) -> anyhow::Result<Self> {
        let files = std::sync::Arc::new(std::sync::Mutex::new(HashSet::<PathBuf>::new()));
        let (tx, rx) = mpsc::channel::<PathBuf>();
        let watched = files.clone();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                if ev.kind.is_access() {
                    return;
                }
                let set = watched.lock().unwrap();
                for p in ev.paths {
                    let p = canonical(&p);
                    if set.contains(&p) {
                        let _ = tx.send(p);
                    }
                }
            }
        })?;
        std::thread::spawn(move || {
            while let Ok(first) = rx.recv() {
                let mut batch = BTreeSet::from([first]);
                while let Ok(p) = rx.recv_timeout(QUIET) {
                    batch.insert(p);
                }
                on_change(batch);
            }
        });
        Ok(Self {
            watcher,
            dirs: HashSet::new(),
            files,
        })
    }

    /// Replaces the watched set.
    pub fn set(&mut self, paths: &[PathBuf]) {
        let wanted: HashSet<PathBuf> = paths.iter().map(|p| canonical(p)).collect();
        let dirs: HashSet<PathBuf> = wanted
            .iter()
            .filter_map(|p| p.parent().map(|d| d.to_path_buf()))
            .collect();
        for d in self.dirs.difference(&dirs) {
            let _ = self.watcher.unwatch(d);
        }
        for d in dirs.difference(&self.dirs) {
            if let Err(e) = self.watcher.watch(d, RecursiveMode::NonRecursive) {
                tracing::warn!("cannot watch {}: {e}", d.display());
            }
        }
        self.dirs = dirs;
        *self.files.lock().unwrap() = wanted;
    }
}

/// Canonical form of a path whose file may not exist at this instant (mid-rename).
pub fn canonical(p: &std::path::Path) -> PathBuf {
    if let Ok(c) = p.canonicalize() {
        return c;
    }
    match (
        p.parent().and_then(|d| d.canonicalize().ok()),
        p.file_name(),
    ) {
        (Some(d), Some(n)) => d.join(n),
        _ => p.to_path_buf(),
    }
}
