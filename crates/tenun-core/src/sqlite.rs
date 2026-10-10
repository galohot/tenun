//! Reads SQLite files with a bundled SQLite, so no DuckDB extension download is
//! needed and the file works offline. Each table is copied into memory, like an
//! Excel sheet, and copied again when the file changes.

use crate::excel::{Cell, Sheet, build_sheet};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

pub fn read_database(path: &Path) -> anyhow::Result<Vec<Sheet>> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type IN ('table', 'view') \
         AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for name in names {
        let quoted = format!("\"{}\"", name.replace('"', "\"\""));
        let mut q = conn.prepare(&format!("SELECT * FROM {quoted}"))?;
        let cols: Vec<(String, String)> = q
            .columns()
            .iter()
            .map(|c| {
                (
                    c.name().to_string(),
                    c.decl_type().unwrap_or("").to_ascii_uppercase(),
                )
            })
            .collect();
        let mut grid: Vec<Vec<Cell>> =
            vec![cols.iter().map(|(n, _)| Cell::Text(n.clone())).collect()];
        let mut rows = q.query([])?;
        while let Some(row) = rows.next()? {
            let mut r = Vec::with_capacity(cols.len());
            for (i, (_, decl)) in cols.iter().enumerate() {
                r.push(match row.get_ref(i)? {
                    ValueRef::Null => Cell::Null,
                    ValueRef::Integer(v) if decl.contains("BOOL") => Cell::Bool(v != 0),
                    ValueRef::Integer(v) => Cell::Int(v),
                    ValueRef::Real(v) => Cell::Float(v),
                    ValueRef::Text(t) => {
                        let s = String::from_utf8_lossy(t).trim().to_string();
                        if decl.contains("DATE") || decl.contains("TIME") {
                            parse_time(&s).map(Cell::Time).unwrap_or(Cell::Text(s))
                        } else if s.is_empty() {
                            Cell::Null
                        } else {
                            Cell::Text(s)
                        }
                    }
                    ValueRef::Blob(_) => Cell::Null,
                });
            }
            grid.push(r);
        }
        let mut sheet = match build_sheet(&name, grid) {
            Some(s) => s,
            None => continue,
        };
        // An empty table still has a header; keep it so its columns show.
        sheet.name = name;
        out.push(sheet);
    }
    Ok(out)
}

fn parse_time(s: &str) -> Option<chrono::NaiveDateTime> {
    use chrono::{NaiveDate, NaiveDateTime};
    for f in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(t) = NaiveDateTime::parse_from_str(s, f) {
            return Some(t);
        }
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tables_with_types() {
        let p = std::env::temp_dir().join(format!("tenun-test-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&p);
        {
            let c = Connection::open(&p).unwrap();
            c.execute_batch(
                "CREATE TABLE t (id INTEGER, name TEXT, joined DATE, vip BOOLEAN, score REAL);
                 INSERT INTO t VALUES (1, 'a', '2026-01-02', 1, 1.5), (2, 'b', NULL, 0, 2);",
            )
            .unwrap();
        }
        let sheets = read_database(&p).unwrap();
        let t = &sheets[0];
        use crate::excel::ColType::*;
        let types: Vec<_> = t.columns.iter().map(|c| c.1).collect();
        assert_eq!(types, vec![Int, Text, Date, Bool, Float]);
        assert_eq!(t.rows.len(), 2);
        std::fs::remove_file(&p).ok();
    }
}
