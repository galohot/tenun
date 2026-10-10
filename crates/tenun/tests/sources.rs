//! Data prep II: a folder of files, a pasted table and a CSV link.

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};
use tenun_core::Engine;
use tenun_core::catalog::DType;
use tenun_core::engine::AddSource;
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::workspace::{ColumnSetup, Every, SourceKind, TableSetup, Workspace};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-src-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// An engine over an empty workspace saved as `<dir>/Books.tenun`.
fn engine(d: &Path) -> Engine {
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.replace(
        Workspace {
            name: "Books".into(),
            ..Workspace::default()
        },
        Some(d.join("Books.tenun")),
    );
    e.save(None).unwrap();
    e
}

fn ask(e: &Engine, v: Value) -> Vec<Vec<Value>> {
    let q: SemanticQuery = serde_json::from_value(v).unwrap();
    let mut rows = e.query(&q).unwrap().rows;
    rows.sort_by_key(|r| r[0].to_string());
    rows
}

fn folder(path: &Path) -> AddSource {
    AddSource::Folder {
        path: path.to_string_lossy().into_owned(),
        pattern: None,
    }
}

#[test]
fn files_of_one_shape_stack_and_a_missing_column_is_blank() {
    let d = dir("stack");
    let sales = d.join("sales");
    std::fs::create_dir_all(&sales).unwrap();
    std::fs::write(sales.join("jan.csv"), "region,amount\nNorth,10\nSouth,20\n").unwrap();
    std::fs::write(sales.join("feb.csv"), "region,amount\nNorth,5\n").unwrap();
    // The third file has one more column.
    std::fs::write(
        sales.join("mar.csv"),
        "region,amount,channel\nSouth,7,web\nNorth,3,shop\n",
    )
    .unwrap();
    std::fs::write(sales.join("notes.txt"), "not read").unwrap();
    let e = engine(&d);
    let id = e.add_source(folder(&sales)).unwrap();
    let snap = e.snapshot();
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    assert_eq!(snap.tables.len(), 1, "one table for the folder");
    let t = &snap.tables[0];
    assert_eq!(t.id, format!("{id}/sales"));
    let cols: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cols, ["file", "region", "amount", "channel"]);
    assert_eq!(t.rows, Some(5));
    // Sums by file match the files.
    let by_file = ask(
        &e,
        json!({
            "measures": [{ "table": t.id, "column": "amount", "agg": "sum" }],
            "dimensions": [{ "table": t.id, "column": "file" }]
        }),
    );
    assert_eq!(
        by_file,
        vec![
            vec![json!("feb.csv"), json!(5.0)],
            vec![json!("jan.csv"), json!(30.0)],
            vec![json!("mar.csv"), json!(10.0)],
        ]
    );
    // The channel is blank for the two files that do not have it.
    let blank = e
        .preview(&t.id, 100)
        .unwrap()
        .rows
        .iter()
        .filter(|r| r[3].is_null())
        .count();
    assert_eq!(blank, 3);
    // The path is stored relative to the workspace file.
    let text = std::fs::read_to_string(d.join("Books.tenun")).unwrap();
    assert!(text.contains("\"kind\": \"folder\""));
    assert!(text.contains("\"path\": \"sales\""));
    assert!(text.contains("*.csv;*.xlsx"));
}

#[test]
fn a_file_with_no_shared_column_is_skipped_and_named() {
    let d = dir("skip");
    let f = d.join("mixed");
    std::fs::create_dir_all(&f).unwrap();
    std::fs::write(f.join("a.csv"), "id,qty\n1,2\n").unwrap();
    std::fs::write(f.join("b.csv"), "id,qty\n2,3\n").unwrap();
    std::fs::write(f.join("c.csv"), "colour,size\nred,L\n").unwrap();
    let e = engine(&d);
    e.add_source(folder(&f)).unwrap();
    let t = e.snapshot().tables.remove(0);
    assert_eq!(t.rows, Some(2));
    assert!(
        t.notes
            .iter()
            .any(|n| n.contains("c.csv") && n.contains("no column in common")),
        "{:?}",
        t.notes
    );
}

#[test]
fn excel_files_stack_by_the_sheet_they_share() {
    let d = dir("xlsx");
    let f = d.join("books");
    std::fs::create_dir_all(&f).unwrap();
    for (name, rows) in [
        ("a.xlsx", [("x", 1.0), ("y", 2.0)]),
        ("b.xlsx", [("z", 4.0), ("x", 8.0)]),
    ] {
        let mut wb = rust_xlsxwriter::Workbook::new();
        let s = wb.add_worksheet().set_name("Data").unwrap();
        s.write_string(0, 0, "item").unwrap();
        s.write_string(0, 1, "qty").unwrap();
        for (i, (k, v)) in rows.iter().enumerate() {
            s.write_string(i as u32 + 1, 0, *k).unwrap();
            s.write_number(i as u32 + 1, 1, *v).unwrap();
        }
        wb.save(f.join(name)).unwrap();
    }
    // One CSV joins them too.
    std::fs::write(f.join("c.csv"), "item,qty\nx,16\n").unwrap();
    let e = engine(&d);
    let id = e.add_source(folder(&f)).unwrap();
    let t = format!("{id}/books");
    let rows = ask(
        &e,
        json!({
            "measures": [{ "table": t, "column": "qty", "agg": "sum" }],
            "dimensions": [{ "table": t, "column": "item" }]
        }),
    );
    assert_eq!(
        rows,
        vec![
            vec![json!("x"), json!(25.0)],
            vec![json!("y"), json!(2.0)],
            vec![json!("z"), json!(4.0)],
        ]
    );
}

#[test]
fn adding_a_fourth_file_triggers_a_reload() {
    let d = dir("watch");
    let f = d.join("inbox");
    std::fs::create_dir_all(&f).unwrap();
    for (i, n) in ["a", "b", "c"].iter().enumerate() {
        std::fs::write(f.join(format!("{n}.csv")), format!("k,v\n{n},{}\n", i + 1)).unwrap();
    }
    let e = engine(&d);
    let id = e.add_source(folder(&f)).unwrap();
    assert_eq!(e.catalog().tables[0].rows, Some(3));

    let (tx, rx) = std::sync::mpsc::channel();
    let mut w = tenun_core::watch::FileWatcher::new(move |paths| {
        let _ = tx.send(paths);
    })
    .unwrap();
    let folders: Vec<PathBuf> = e.watched_folders().into_iter().map(|(_, p)| p).collect();
    assert_eq!(folders.len(), 1);
    w.set(&[], &folders);
    std::thread::sleep(Duration::from_millis(200));

    std::fs::write(f.join("d.csv"), "k,v\nd,4\n").unwrap();
    let paths = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("a change was reported");
    let changed = e.sources_for(&paths);
    assert_eq!(changed, vec![id.clone()]);
    e.load(&id);
    assert_eq!(e.catalog().tables[0].rows, Some(4));

    // A removed file is noticed the same way.
    std::fs::remove_file(f.join("a.csv")).unwrap();
    let paths = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("a removal was reported");
    assert_eq!(e.sources_for(&paths), vec![id.clone()]);
    e.load(&id);
    assert_eq!(e.catalog().tables[0].rows, Some(3));

    // A file the pattern does not pick is not a reason to reload.
    std::fs::write(f.join("readme.txt"), "hello").unwrap();
    if let Ok(paths) = rx.recv_timeout(Duration::from_millis(800)) {
        assert!(e.sources_for(&paths).is_empty());
    }
}

/// A tiny web server on 127.0.0.1. `/ok` is a CSV that counts requests, `/page` is HTML,
/// `/huge` claims a body over 50 MB, anything else is a 404.
fn serve() -> (String, Arc<AtomicU32>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicU32::new(0));
    let h = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 2048];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            assert!(
                !req.to_lowercase().contains("cookie:"),
                "no cookies are sent: {req}"
            );
            let (status, ctype, len, body) = match path.as_str() {
                "/ok" => {
                    let n = h.fetch_add(1, Ordering::SeqCst) + 1;
                    let body = format!("city,sales\nJakarta,{}\nBandung,5\n", 10 * n);
                    ("200 OK", "text/csv", body.len(), body)
                }
                "/page" => {
                    let body = "<html><body>Sign in</body></html>".to_string();
                    ("200 OK", "text/html; charset=utf-8", body.len(), body)
                }
                "/huge" => ("200 OK", "text/csv", 60_000_000, "a,b\n".to_string()),
                _ => {
                    let body = "nothing here".to_string();
                    ("404 Not Found", "text/plain", body.len(), body)
                }
            };
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}"
            );
        }
    });
    (base, hits)
}

fn link(url: String, every: Every) -> AddSource {
    AddSource::Url {
        url,
        every,
        name: Some("Cities".into()),
    }
}

#[test]
fn a_csv_link_loads_and_a_404_is_a_source_error() {
    let d = dir("url");
    let (base, hits) = serve();
    let e = engine(&d);
    let id = e
        .add_source(link(format!("{base}/ok"), Every::Hourly))
        .unwrap();
    let snap = e.snapshot();
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    assert!(snap.loaded_at.contains_key(&id), "the fetch time is kept");
    let t = &snap.tables[0];
    assert_eq!(t.id, format!("{id}/Cities"));
    assert_eq!(t.rows, Some(2));
    // The downloaded copy sits in the data folder next to the workspace file.
    assert!(
        d.join("Books data")
            .join(format!("link-{id}.csv"))
            .is_file()
    );
    // The workspace file keeps the link and the interval, no secrets.
    let text = std::fs::read_to_string(d.join("Books.tenun")).unwrap();
    assert!(text.contains("\"kind\": \"url\"") && text.contains("\"every\": \"hourly\""));

    // A 404 on a new link is refused in words and leaves no source behind.
    let err = e
        .add_source(link(format!("{base}/missing"), Every::Daily))
        .unwrap_err()
        .to_string();
    assert!(err.contains("404"), "{err}");
    assert_eq!(e.snapshot().workspace.sources.len(), 1);
    // A web page and an oversize file are refused in words too.
    let err = e
        .add_source(link(format!("{base}/page"), Every::Daily))
        .unwrap_err()
        .to_string();
    assert!(err.contains("not CSV"), "{err}");
    let err = e
        .add_source(link(format!("{base}/huge"), Every::Daily))
        .unwrap_err()
        .to_string();
    assert!(err.contains("50 MB"), "{err}");
    // Only https links are accepted from other hosts.
    let err = e
        .add_source(link("http://example.com/a.csv".into(), Every::Daily))
        .unwrap_err()
        .to_string();
    assert!(err.contains("https"), "{err}");

    // A reload fetches again.
    let before = hits.load(Ordering::SeqCst);
    e.load(&id);
    assert_eq!(hits.load(Ordering::SeqCst), before + 1);
    let rows = ask(
        &e,
        json!({ "measures": [{ "table": format!("{id}/Cities"), "column": "sales", "agg": "sum" }] }),
    );
    assert_eq!(rows[0][0].as_f64(), Some(25.0));
}

#[test]
fn a_retype_on_a_fresh_link_does_not_download_again() {
    let d = dir("url-retype");
    let (base, hits) = serve();
    let e = engine(&d);
    let id = e
        .add_source(link(format!("{base}/ok"), Every::Hourly))
        .unwrap();
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let setup = TableSetup {
        columns: [(
            "sales".to_string(),
            ColumnSetup {
                dtype: Some(DType::Text),
                ..ColumnSetup::default()
            },
        )]
        .into(),
        ..TableSetup::default()
    };
    assert!(
        e.set_table_setup(&format!("{id}/Cities"), Some(setup))
            .unwrap()
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1, "read from the copy");
    let t = &e.snapshot().tables[0];
    assert_eq!(t.column("sales").unwrap().dtype, DType::Text);
}

#[test]
fn a_link_that_fails_later_keeps_its_last_copy_and_shows_the_error() {
    let d = dir("url-fail");
    let (base, _) = serve();
    let e = engine(&d);
    let id = e
        .add_source(link(format!("{base}/ok"), Every::Manual))
        .unwrap();
    // The link now points at a missing page.
    {
        let mut ws = e.snapshot().workspace;
        ws.sources[0].kind = SourceKind::Url {
            url: format!("{base}/gone"),
            every: Every::Manual,
        };
        e.replace(ws, Some(d.join("Books.tenun")));
    }
    // Opening reads the copy that is there and does not fetch (manual).
    assert!(e.snapshot().errors.is_empty());
    e.load(&id);
    let snap = e.snapshot();
    assert!(snap.errors[&id].contains("404"), "{:?}", snap.errors);
    assert_eq!(snap.tables.len(), 1, "the last copy is still read");
    assert_eq!(snap.tables[0].rows, Some(2));
}

#[test]
fn links_are_fetched_on_their_schedule() {
    use std::time::UNIX_EPOCH;
    let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
    let ago = |s: u64| Some(now - Duration::from_secs(s));
    assert!(tenun_core::engine::url_due(Every::Hourly, None, now));
    assert!(!tenun_core::engine::url_due(
        Every::Hourly,
        ago(59 * 60),
        now
    ));
    assert!(tenun_core::engine::url_due(
        Every::Hourly,
        ago(61 * 60),
        now
    ));
    assert!(tenun_core::engine::url_due(Every::Min15, ago(16 * 60), now));
    assert!(!tenun_core::engine::url_due(
        Every::Daily,
        ago(23 * 3600),
        now
    ));
    assert!(!tenun_core::engine::url_due(
        Every::Manual,
        ago(999_999),
        now
    ));
    let _ = SystemTime::now();

    // The engine's own check: a fresh link is not fetched again, a manual one never is.
    let d = dir("url-due");
    let (base, hits) = serve();
    let e = engine(&d);
    e.add_source(link(format!("{base}/ok"), Every::Hourly))
        .unwrap();
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert!(e.refresh_due_urls().is_empty());
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    // Opening the workspace again reads the fresh copy without a download.
    let ws = e.snapshot().workspace;
    e.replace(ws, Some(d.join("Books.tenun")));
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert!(e.snapshot().errors.is_empty());
}

#[test]
fn a_pasted_table_becomes_a_csv_source_in_the_data_folder() {
    let d = dir("paste");
    let e = engine(&d);
    let text = "Region\tAmount\nNorth\t10\nSouth\t20\n";
    let id = e.add_pasted("Targets", text, true).unwrap();
    let file = d.join("Books data").join("Targets.csv");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "Region,Amount\nNorth,10\nSouth,20\n"
    );
    let snap = e.snapshot();
    assert!(matches!(
        snap.workspace.sources[0].kind,
        SourceKind::Csv { .. }
    ));
    assert_eq!(
        snap.workspace.sources[0].kind.path(),
        Some("Books data/Targets.csv")
    );
    let t = format!("{id}/Targets");
    let rows = ask(
        &e,
        json!({ "measures": [{ "table": t, "column": "Amount", "agg": "sum" }] }),
    );
    assert_eq!(rows[0][0].as_f64(), Some(30.0));
    // It is live: a change to the file shows on the next load.
    std::fs::write(&file, "Region,Amount\nNorth,100\n").unwrap();
    let rows = ask(
        &e,
        json!({ "measures": [{ "table": t, "column": "Amount", "agg": "sum" }] }),
    );
    assert_eq!(rows[0][0].as_f64(), Some(100.0));
    // Without the header row the columns get plain names; the first line is data.
    let id2 = e.add_pasted("Targets", "a\t1\nb\t2\n", false).unwrap();
    let t2 = e
        .catalog()
        .table(&format!("{id2}/Targets 2"))
        .cloned()
        .unwrap();
    let names: Vec<_> = t2.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Column 1", "Column 2"]);
    assert_eq!(t2.rows, Some(2));
    // Something that is not a table is refused in words.
    assert!(e.add_pasted("x", "just a word", true).is_err());
}

#[test]
fn a_paste_needs_a_saved_workspace() {
    let e = Engine::new(Secrets::new(dir("paste-unsaved").join("secrets.json"))).unwrap();
    e.new_workspace();
    let err = e
        .add_pasted("T", "a\tb\n1\t2\n", true)
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("Save the workspace first"), "{err}");
}

#[test]
fn text_only_tables_keep_their_header_and_odd_files_still_stack() {
    let d = dir("odd");
    let e = engine(&d);
    // Every cell is text: the first line must still be the header.
    let id = e
        .add_pasted("People", "Name\tCity\nAnn\tJakarta\nBo\tBandung\n", true)
        .unwrap();
    let t = e.catalog().table(&format!("{id}/People")).cloned().unwrap();
    let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Name", "City"]);
    assert_eq!(t.rows, Some(2));

    // A file with its own "file" column, and a column that is a number in one file and words in
    // the next.
    let f = d.join("odd");
    std::fs::create_dir_all(&f).unwrap();
    std::fs::write(f.join("a.csv"), "file,code\nx,1\n").unwrap();
    std::fs::write(f.join("b.csv"), "file,code\ny,north\n").unwrap();
    // A row with fewer cells than the header is padded, not refused.
    std::fs::write(f.join("c.csv"), "file,code\nz\n").unwrap();
    let id = e.add_source(folder(&f)).unwrap();
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    let t = e.catalog().table(&format!("{id}/odd")).cloned().unwrap();
    let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["file", "file 2", "code"]);
    assert_eq!(t.rows, Some(3));
}
