//! A small file browser for picking data files when there is no native dialog
//! (the browser mode). It lists one folder at a time and hides dot-files.

use serde::Serialize;
use std::path::{Path, PathBuf};
use tenun_core::engine::FILE_EXTENSIONS;

#[derive(Serialize)]
pub struct Listing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<Entry>,
    pub places: Vec<Place>,
}

#[derive(Serialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub dir: bool,
    pub size: u64,
    /// True for files Tenun can open.
    pub supported: bool,
}

#[derive(Serialize)]
pub struct Place {
    pub name: String,
    pub path: String,
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn list(path: Option<&str>) -> anyhow::Result<Listing> {
    let dir = match path {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => home(),
    };
    let dir = dir.canonicalize()?;
    let mut entries = Vec::new();
    for e in std::fs::read_dir(&dir)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        let ext = Path::new(&name)
            .extension()
            .map(|x| x.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        entries.push(Entry {
            path: e.path().to_string_lossy().into_owned(),
            dir: meta.is_dir(),
            size: meta.len(),
            supported: meta.is_file() && FILE_EXTENSIONS.contains(&ext.as_str()),
            name,
        });
    }
    entries.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let h = home();
    let places = ["Desktop", "Documents", "Downloads"]
        .iter()
        .map(|n| h.join(n))
        .filter(|p| p.is_dir())
        .map(|p| Place {
            name: p.file_name().unwrap().to_string_lossy().into_owned(),
            path: p.to_string_lossy().into_owned(),
        })
        .chain(std::iter::once(Place {
            name: "Home".into(),
            path: h.to_string_lossy().into_owned(),
        }))
        .collect();
    Ok(Listing {
        parent: dir.parent().map(|p| p.to_string_lossy().into_owned()),
        path: dir.to_string_lossy().into_owned(),
        entries,
        places,
    })
}
