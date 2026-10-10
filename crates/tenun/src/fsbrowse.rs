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

/// The biggest picture an image tile keeps: 1 MB.
pub const MAX_IMAGE_BYTES: u64 = 1_048_576;

/// The type of a picture from its first bytes: PNG, JPEG or WebP. Never SVG.
pub fn image_type(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if head.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Reads a picture for an image tile. The error says what is wrong in words.
pub fn read_image(path: &str) -> anyhow::Result<(&'static str, Vec<u8>)> {
    let p = Path::new(path);
    let size = std::fs::metadata(p)
        .map_err(|_| anyhow::anyhow!("That file could not be read."))?
        .len();
    if size > MAX_IMAGE_BYTES {
        anyhow::bail!("That picture is too big. Pick one under 1 MB.");
    }
    let bytes = std::fs::read(p).map_err(|_| anyhow::anyhow!("That file could not be read."))?;
    let kind =
        image_type(&bytes).ok_or_else(|| anyhow::anyhow!("Pick a PNG, JPEG or WebP picture."))?;
    Ok((kind, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_are_known_by_their_first_bytes() {
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(image_type(&[0xff, 0xd8, 0xff, 0xe0, 0]), Some("image/jpeg"));
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(
            image_type(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            None
        );
        assert_eq!(image_type(b""), None);
    }

    #[test]
    fn a_picture_is_read_with_its_type_or_refused_in_words() {
        let dir = std::env::temp_dir().join(format!("tenun-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.png");
        std::fs::write(&png, b"\x89PNG\r\n\x1a\n0000").unwrap();
        let (kind, bytes) = read_image(png.to_str().unwrap()).unwrap();
        assert_eq!((kind, bytes.len()), ("image/png", 12));
        let svg = dir.join("a.svg");
        std::fs::write(&svg, b"<svg/>").unwrap();
        let err = read_image(svg.to_str().unwrap()).unwrap_err().to_string();
        assert!(err.contains("PNG, JPEG or WebP"), "{err}");
        let big = dir.join("big.png");
        std::fs::write(&big, vec![0u8; MAX_IMAGE_BYTES as usize + 1]).unwrap();
        let err = read_image(big.to_str().unwrap()).unwrap_err().to_string();
        assert!(err.contains("under 1 MB"), "{err}");
        assert!(read_image(dir.join("none.png").to_str().unwrap()).is_err());
    }
}
