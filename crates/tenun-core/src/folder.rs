//! Folder sources: which files of a folder a source reads, and whether a changed path belongs
//! to it.

use std::path::{Path, PathBuf};

/// Extensions of the files a folder can stack: CSV and Excel.
const READABLE: &[&str] = &["csv", "tsv", "txt", "xlsx", "xlsm", "xlsb", "xls", "ods"];

/// True when the file name is one of the patterns. A pattern list is separated by `;` or `,`.
/// `*` stands for any text and `?` for one character. Case does not matter.
pub fn matches(name: &str, patterns: &str) -> bool {
    // Hidden files and the lock files that Excel leaves next to an open workbook.
    if name.starts_with('.') || name.starts_with("~$") {
        return false;
    }
    let name = name.to_lowercase();
    patterns
        .split([';', ','])
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .any(|p| glob(p.as_bytes(), name.as_bytes()))
}

fn glob(p: &[u8], s: &[u8]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some((b'*', rest)) => (0..=s.len()).any(|i| glob(rest, &s[i..])),
        Some((b'?', rest)) => !s.is_empty() && glob(rest, &s[1..]),
        Some((c, rest)) => s.first() == Some(c) && glob(rest, &s[1..]),
    }
}

pub fn is_readable(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| READABLE.contains(&e.as_str()))
}

/// The files of the folder that match the patterns, by name. Not the sub-folders.
pub fn files(dir: &Path, patterns: &str) -> std::io::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|e| matches(&e.file_name().to_string_lossy(), patterns))
        .map(|e| e.path())
        .collect();
    out.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    Ok(out)
}

/// True when a changed path is the folder itself or a file in it that a pattern picks.
pub fn is_touched_by(changed: &Path, folder: &Path, patterns: &str) -> bool {
    changed == folder
        || (changed.parent() == Some(folder)
            && changed
                .file_name()
                .is_some_and(|n| matches(&n.to_string_lossy(), patterns)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        assert!(matches("Sales.CSV", "*.csv;*.xlsx"));
        assert!(matches("a.xlsx", "*.csv; *.xlsx"));
        assert!(matches("sales_01.csv", "sales_??.csv"));
        assert!(!matches("sales_1.csv", "sales_??.csv"));
        assert!(!matches("a.txt", "*.csv;*.xlsx"));
        assert!(!matches("~$a.xlsx", "*.xlsx"));
        assert!(!matches(".hidden.csv", "*.csv"));
        assert!(matches("anything.csv", "*"));
        assert!(!matches("a.csv", ""));
    }

    #[test]
    fn a_change_belongs_to_the_folder() {
        let dir = Path::new("/d/sales");
        assert!(is_touched_by(Path::new("/d/sales/a.csv"), dir, "*.csv"));
        assert!(!is_touched_by(Path::new("/d/sales/a.txt"), dir, "*.csv"));
        assert!(!is_touched_by(
            Path::new("/d/sales/sub/a.csv"),
            dir,
            "*.csv"
        ));
        assert!(!is_touched_by(Path::new("/d/other/a.csv"), dir, "*.csv"));
        assert!(is_touched_by(dir, dir, "*.csv"));
    }
}
