//! The real-file test set: loads every file of `tests/corpus/manifest.json` from a folder that
//! `scripts/corpus-fetch.py` filled, runs two queries on each table, and checks what Tenun read against
//! what a person checked by hand. A panic in one file is recorded and the run goes on.
//!
//! ```sh
//! python3 scripts/corpus-fetch.py ~/tenun-corpus
//! cargo run --release --example corpus -- ~/tenun-corpus [--json report.json]
//! ```

use serde_json::{Value, json};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tenun_core::Engine;
use tenun_core::catalog::{DType, Table};
use tenun_core::engine::AddSource;
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(folder) = args.first().map(PathBuf::from) else {
        eprintln!("usage: corpus <folder> [--json report.json]");
        std::process::exit(2);
    };
    let json_out = args
        .iter()
        .position(|a| a == "--json")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    let manifest_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/manifest.json");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    let files = manifest["files"].as_array().unwrap();

    // A panic is recorded by the run; the default hook would print a backtrace in the middle of the table.
    std::panic::set_hook(Box::new(|_| {}));

    let mut report = Vec::new();
    let (mut loaded, mut refused, mut panicked, mut missing, mut wrong) = (0, 0, 0, 0, 0);
    for f in files {
        let name = f["file"].as_str().unwrap();
        let path = folder.join(name);
        if !path.exists() {
            missing += 1;
            println!("{:<7} {name}", "missing");
            continue;
        }
        let started = Instant::now();
        let outcome = catch_unwind(AssertUnwindSafe(|| read_one(&path, name)));
        let secs = started.elapsed().as_secs_f64();
        let mut entry = match outcome {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => json!({ "file": name, "error": e }),
            Err(p) => {
                let msg = p
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "panic".into());
                json!({ "file": name, "panic": msg })
            }
        };
        entry["secs"] = json!((secs * 1000.0).round() / 1000.0);
        let problems = check(&entry, &f["expect"]);
        let status = if entry.get("panic").is_some() {
            panicked += 1;
            "PANIC"
        } else if entry.get("error").is_some() {
            refused += 1;
            "error"
        } else {
            loaded += 1;
            "ok"
        };
        if !problems.is_empty() {
            wrong += 1;
        }
        let what = match status {
            "PANIC" => entry["panic"].as_str().unwrap_or_default().to_string(),
            "error" => entry["error"].as_str().unwrap_or_default().to_string(),
            _ => summary(&entry),
        };
        println!(
            "{status:<7} {name:<60} {secs:>6.2}s  {}",
            one_line(&what, 110)
        );
        for p in &problems {
            println!("        ✗ {p}");
        }
        entry["problems"] = json!(problems);
        report.push(entry);
    }
    println!(
        "\n{} files: {loaded} loaded, {refused} refused with words, {panicked} panicked, {missing} missing; {wrong} not as expected.",
        files.len()
    );
    if let Some(out) = json_out {
        std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        println!("Report: {}", out.display());
    }
    if panicked + wrong > 0 {
        std::process::exit(1);
    }
}

/// Loads one file into a fresh engine and describes every table it gave, with two queries each.
fn read_one(path: &Path, name: &str) -> Result<Value, String> {
    let dir = std::env::temp_dir().join(format!("tenun-corpus-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).map_err(|e| e.to_string())?;
    let id = e
        .add_source(AddSource::File {
            path: path.to_string_lossy().into_owned(),
        })
        .map_err(|e| e.to_string())?;
    let snap = e.snapshot();
    if let Some(err) = snap.errors.get(&id) {
        return Err(err.clone());
    }
    let mut tables = Vec::new();
    for t in snap.tables.iter().filter(|t| t.source_id == id) {
        tables.push(describe(&e, t));
    }
    Ok(json!({ "file": name, "tables": tables }))
}

fn describe(e: &Engine, t: &Table) -> Value {
    let cols: Vec<Value> = t
        .columns
        .iter()
        .map(|c| json!({ "name": c.name, "type": format!("{:?}", c.dtype).to_lowercase(), "blank": c.blank, "mixed": c.mixed }))
        .collect();
    let count = timed(
        e,
        json!({ "measures": [{ "table": t.id, "column": null, "agg": "count" }] }),
    );
    let num = t.columns.iter().find(|c| matches!(c.dtype, DType::Number));
    let text = t.columns.iter().find(|c| matches!(c.dtype, DType::Text));
    let grouped = match (num, text) {
        (Some(n), Some(x)) => Some(timed(
            e,
            json!({ "measures": [{ "table": t.id, "column": n.name, "agg": "sum" }],
                    "dimensions": [{ "table": t.id, "column": x.name }],
                    "sort": [{ "by": 1, "desc": true }], "limit": 10 }),
        )),
        _ => None,
    };
    json!({
        "table": t.name, "rows": t.rows, "header_row": t.header_row, "notes": t.notes,
        "columns": cols, "count": count, "grouped": grouped,
    })
}

/// Runs a query and keeps its rows (at most 3), its time, or its error.
fn timed(e: &Engine, q: Value) -> Value {
    let q: SemanticQuery = serde_json::from_value(q).unwrap();
    let started = Instant::now();
    let out = e.query(&q);
    let ms = started.elapsed().as_millis() as u64;
    match out {
        Ok(r) => json!({ "ms": ms, "rows": r.rows.into_iter().take(3).collect::<Vec<_>>() }),
        Err(err) => json!({ "ms": ms, "error": err.to_string() }),
    }
}

/// What the manifest says a person saw in the file, compared with what Tenun read.
fn check(entry: &Value, expect: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if expect.is_null() {
        return out;
    }
    if let Some(want) = expect.get("error") {
        match entry["error"].as_str() {
            None => out.push("expected an error in words, but it loaded".into()),
            Some(got) if !got.contains(want.as_str().unwrap_or_default()) => {
                out.push(format!("the error says {got:?}, expected it to say {want}"))
            }
            Some(_) => {}
        }
        return out;
    }
    let Some(tables) = entry["tables"].as_array() else {
        out.push("expected it to load".into());
        return out;
    };
    for q in tables {
        for key in ["count", "grouped"] {
            if let Some(err) = q[key].get("error") {
                out.push(format!("{}: the {key} query failed: {err}", q["table"]));
            }
        }
    }
    if let Some(n) = expect["tables"].as_u64()
        && tables.len() as u64 != n
    {
        out.push(format!("{} tables, expected {n}", tables.len()));
    }
    let table = match expect["table"].as_str() {
        Some(name) => tables.iter().find(|t| t["table"] == name),
        None => tables.first(),
    };
    let Some(t) = table else {
        out.push(format!("no table {}", expect["table"]));
        return out;
    };
    if let Some(h) = expect.get("header_row")
        && !h.is_null()
        && t["header_row"] != *h
    {
        out.push(format!("header row {}, expected {h}", t["header_row"]));
    }
    if let Some(n) = expect["rows"].as_i64()
        && t["rows"].as_i64() != Some(n)
    {
        out.push(format!("{} rows, expected {n}", t["rows"]));
    }
    // Files that their publisher updates (World Bank, Our World in Data) only grow.
    if let Some(n) = expect["min_rows"].as_i64()
        && t["rows"].as_i64().is_none_or(|r| r < n)
    {
        out.push(format!("{} rows, expected at least {n}", t["rows"]));
    }
    if let Some(cols) = expect["columns"].as_array() {
        let got: Vec<&str> = t["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap_or_default())
            .collect();
        for (i, want) in cols.iter().enumerate() {
            let want = want.as_str().unwrap();
            if got.get(i) != Some(&want) {
                out.push(format!(
                    "column {} is {:?}, expected {want:?}",
                    i + 1,
                    got.get(i)
                ));
            }
        }
    }
    if let Some(types) = expect["types"].as_object() {
        for (col, want) in types {
            let got = t["columns"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == col.as_str())
                .map(|c| c["type"].clone());
            if got.as_ref() != Some(want) {
                out.push(format!("{col} is {got:?}, expected {want}"));
            }
        }
    }
    out
}

fn summary(entry: &Value) -> String {
    let tables = entry["tables"].as_array().unwrap();
    let first = tables.first();
    format!(
        "{} table(s); first: {} rows, {} columns, header row {}",
        tables.len(),
        first.map_or(json!(0), |t| t["rows"].clone()),
        first.map_or(0, |t| t["columns"].as_array().unwrap().len()),
        first.map_or(json!(null), |t| t["header_row"].clone()),
    )
}

fn one_line(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s
    }
}
