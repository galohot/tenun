//! Data prep II: a folder of files, a pasted table, a CSV link, and the web link that reads JSON
//! with pages and a key (build 15).

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tenun_core::Engine;
use tenun_core::catalog::DType;
use tenun_core::engine::{AddSource, KeyEdit, KeyIn, LinkPreview};
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::workspace::{
    ColumnSetup, Every, KeyPlace, Paging, SourceKind, TableSetup, Workspace,
};

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
        records: None,
        pages: None,
        key: None,
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
    assert!(err.contains("a web page, not data"), "{err}");
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
            records: None,
            pages: None,
            key: None,
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

// ---- the web link reads JSON (build 15) ----------------------------------------------------

/// The key the test API accepts, and one it does not.
const KEY: &str = "k-123";
const WRONG: &str = "wrong-VALUE-777";
/// A key that `/echo` accepts and then repeats in its answer.
const ECHO: &str = "echo-key-98765";

/// Every request the test API got: the whole request head, as text.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    /// The requests whose first line starts with `GET <prefix>`.
    fn hits(&self, prefix: &str) -> Vec<String> {
        let all = self.0.lock().unwrap();
        all.iter()
            .filter(|h| h.starts_with(&format!("GET {prefix}")))
            .cloned()
            .collect()
    }
}

fn param(query: &str, name: &str) -> Option<String> {
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix(&format!("{name}=")).map(str::to_string))
}

struct Resp {
    status: &'static str,
    ctype: &'static str,
    /// Extra header lines, each ending in a line break.
    extra: String,
    body: String,
}

fn ok(v: Value) -> Resp {
    Resp {
        status: "200 OK",
        ctype: "application/json",
        extra: String::new(),
        body: v.to_string(),
    }
}

fn items(ids: &[u32]) -> Value {
    Value::Array(
        ids.iter()
            .map(|i| json!({ "id": i, "name": format!("n{i}") }))
            .collect(),
    )
}

/// A JSON API on this computer. Routes: `/json` (nested records), `/nd` (one object per line),
/// `/link`, `/field`, `/cursor`, `/num`, `/numt` and `/endless` (pages in each way), `/loop` (a next
/// link that points at itself), `/secure` and `/q` (need `KEY`), `/hop` and `/hopq` (redirect to
/// `localhost`, another host name for the same server), `/hop-same`, `/xpage`, `/echo` (repeats its
/// key) and `/big` (a second page that says it is 60 MB).
fn api() -> (String, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let log = Log::default();
    let l = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]).to_string();
            l.0.lock().unwrap().push(head.clone());
            let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
            let (path, q) = target.split_once('?').unwrap_or((&target, ""));
            let r = route(path, q, &head.to_lowercase(), port);
            // `/big?page=2` says it is 60 MB long and sends nothing more: the size is read from the
            // head of the answer, before the body.
            let length = if target.starts_with("/big?page=2") {
                60_000_000
            } else {
                r.body.len()
            };
            let _ = write!(
                s,
                "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n{}",
                r.status, r.ctype, length, r.extra, r.body
            );
        }
    });
    (format!("http://127.0.0.1:{port}"), log)
}

fn route(path: &str, q: &str, head: &str, port: u16) -> Resp {
    let page: u32 = param(q, "page").and_then(|p| p.parse().ok()).unwrap_or(1);
    let two = |p: u32| items(&[2 * p - 1, 2 * p]);
    let nested = json!({
        "meta": { "total": 3 },
        "errors": [{ "e": "x" }],
        "data": { "items": [
            { "id": 1, "name": "Ann", "owner": { "city": "Jakarta" }, "tags": ["a", "b"] },
            { "id": 2, "name": "Bo", "owner": { "city": "Bandung" }, "tags": [] },
            { "id": 3, "name": "Cy", "owner": null, "extra": true }
        ] }
    });
    let has_key = head.contains(&format!("x-api-key: {KEY}\r\n"))
        || head.contains(&format!("x-token: {KEY}\r\n"))
        || head.contains(&format!("authorization: bearer {KEY}\r\n"));
    let denied = || Resp {
        status: "401 Unauthorized",
        ctype: "application/json",
        extra: String::new(),
        body: r#"{"error":"no key"}"#.into(),
    };
    let redirect = |to: String| Resp {
        status: "302 Found",
        ctype: "text/plain",
        extra: format!("Location: {to}\r\n"),
        body: String::new(),
    };
    match path {
        "/json" => ok(nested),
        "/nd" => Resp {
            status: "200 OK",
            ctype: "application/x-ndjson",
            extra: String::new(),
            body: "{\"id\":1,\"name\":\"a\"}\n{\"id\":2,\"name\":\"b\",\"x\":9}\n\n{\"id\":3}\n"
                .into(),
        },
        // Three pages of two records, the next one named in the Link header (relative).
        "/link" => {
            let mut r = ok(two(page));
            if page < 3 {
                r.extra = format!("Link: </link?page={}>; rel=\"next\"\r\n", page + 1);
            }
            r
        }
        "/field" => {
            let next = if page < 3 {
                json!(format!("/field?page={}", page + 1))
            } else {
                Value::Null
            };
            ok(json!({ "items": two(page), "paging": { "next": next } }))
        }
        "/cursor" => {
            let at: u32 = param(q, "cursor")
                .and_then(|c| c.trim_start_matches('t').parse().ok())
                .unwrap_or(0);
            let next = if at < 2 {
                json!(format!("t{}", at + 1))
            } else {
                Value::Null
            };
            ok(json!({ "rows": two(at + 1), "next_cursor": next }))
        }
        // Two pages, then an empty one.
        "/num" => ok(json!({ "items": if page <= 2 { two(page) } else { json!([]) } })),
        // The World Bank shape. Past the last page the server answers with an error message.
        "/numt" if page <= 2 => ok(json!([{ "page": page, "pages": 2, "per_page": 2 }, two(page)])),
        "/numt" => ok(json!([{ "message": [{ "id": "175", "key": "Invalid format" }] }])),
        "/endless" => ok(json!({
            "items": items(&[page]),
            "next": format!("/endless?page={}", page + 1)
        })),
        "/loop" => ok(json!({ "items": items(&[1]), "next": "/loop" })),
        "/secure" if has_key => ok(nested),
        "/q" if param(q, "api_key").as_deref() == Some(KEY) => ok(nested),
        "/secure" | "/q" => denied(),
        // An API that repeats the link it was called with, key and all, and names a list after it.
        "/echo" if head.contains(&format!("x-api-key: {}\r\n", ECHO.to_lowercase())) => {
            let rows: Vec<Value> = (1..=2)
                .map(|i| json!({ "id": i, "self": format!("https://h.example/echo?api_key={ECHO}&i={i}") }))
                .collect();
            let mut m = serde_json::Map::new();
            m.insert(ECHO.to_string(), Value::Array(rows));
            ok(Value::Object(m))
        }
        "/echo" => denied(),
        "/big" => ok(json!({ "items": items(&[1, 2]), "next": "/big?page=2" })),
        "/hop" => redirect(format!("http://localhost:{port}/secure")),
        "/hopq" => redirect(format!("http://localhost:{port}/q")),
        "/hop-same" => redirect("/secure".into()),
        "/xpage" => {
            ok(json!({ "items": items(&[1]), "next": format!("http://localhost:{port}/xpage2") }))
        }
        "/xpage2" => ok(json!({ "items": items(&[2]), "next": null })),
        "/html" => Resp {
            status: "200 OK",
            ctype: "text/html",
            extra: String::new(),
            body: "<html>Sign in</html>".into(),
        },
        _ => Resp {
            status: "404 Not Found",
            ctype: "text/plain",
            extra: String::new(),
            body: "nothing here".into(),
        },
    }
}

fn api_link(
    url: String,
    records: Option<&str>,
    pages: Option<Paging>,
    key: Option<KeyIn>,
) -> AddSource {
    AddSource::Url {
        url,
        every: Every::Manual,
        name: Some("Api".into()),
        records: records.map(str::to_string),
        pages,
        key,
    }
}

fn key_in(place: KeyPlace, name: &str, value: &str) -> Option<KeyIn> {
    Some(KeyIn {
        place,
        name: name.into(),
        value: value.into(),
    })
}

fn header_key(value: &str) -> Option<KeyIn> {
    key_in(KeyPlace::Header, "X-Api-Key", value)
}

fn sum_of(e: &Engine, id: &str, column: &str) -> f64 {
    ask(
        e,
        json!({ "measures": [{ "table": format!("{id}/Api"), "column": column, "agg": "sum" }] }),
    )[0][0]
        .as_f64()
        .unwrap()
}

/// Every file under `dir` that holds `needle`.
fn files_holding(dir: &Path, needle: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for entry in std::fs::read_dir(&d).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                todo.push(p);
            } else if std::fs::read(&p)
                .unwrap()
                .windows(needle.len())
                .any(|w| w == needle.as_bytes())
            {
                found.push(p.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    found
}

#[test]
fn a_json_link_loads_the_longest_list_or_the_named_one_and_so_does_ndjson() {
    let d = dir("json");
    let (base, _) = api();
    let e = engine(&d);
    // No path: the longest list of objects, data.items, not the small errors list.
    let id = e
        .add_source(api_link(format!("{base}/json"), None, None, None))
        .unwrap();
    let snap = e.snapshot();
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    let t = &snap.tables[0];
    assert_eq!(t.id, format!("{id}/Api"));
    let cols: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cols, ["id", "name", "owner.city", "tags", "extra"]);
    assert_eq!(t.rows, Some(3));
    assert_eq!(sum_of(&e, &id, "id"), 6.0);
    // The copy is a CSV file, like any link (V3-D80).
    assert!(
        d.join("Books data")
            .join(format!("link-{id}.csv"))
            .is_file()
    );
    // A link with no new setting is saved as before: no records, pages or key.
    let text = std::fs::read_to_string(d.join("Books.tenun")).unwrap();
    assert!(!text.contains("records") && !text.contains("pages") && !text.contains("\"key\""));

    // A named path picks that list, and is saved.
    let id2 = e
        .add_source(api_link(format!("{base}/json"), Some("errors"), None, None))
        .unwrap();
    assert_eq!(
        e.catalog().table(&format!("{id2}/Api")).unwrap().rows,
        Some(1)
    );
    assert!(
        std::fs::read_to_string(d.join("Books.tenun"))
            .unwrap()
            .contains("\"records\": \"errors\"")
    );
    // A path that is not there fails in words and names the lists there are.
    let err = e
        .add_source(api_link(
            format!("{base}/json"),
            Some("data.rows"),
            None,
            None,
        ))
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        "There is no list at data.rows. Lists in this answer: data.items (3), errors (1)."
    );
    assert_eq!(
        e.snapshot().workspace.sources.len(),
        2,
        "nothing was left behind"
    );

    // One object per line.
    let nd = e
        .add_source(api_link(format!("{base}/nd"), None, None, None))
        .unwrap();
    let t = e.catalog().table(&format!("{nd}/Api")).cloned().unwrap();
    let cols: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cols, ["id", "name", "x"]);
    assert_eq!(t.rows, Some(3));
    // A web page is refused with the new words.
    let err = e
        .add_source(api_link(format!("{base}/html"), None, None, None))
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        "That link gives a web page, not data. In Google Sheets, publish the sheet as CSV and use that link."
    );
}

#[test]
fn pages_follow_a_link_header_a_field_a_cursor_and_a_page_number() {
    let d = dir("pages");
    let (base, log) = api();
    let e = engine(&d);
    let add = |path: &str, pages: Paging| {
        let id = e
            .add_source(api_link(format!("{base}{path}"), None, Some(pages), None))
            .unwrap();
        assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
        id
    };
    // The Link header, with a relative address: 3 pages of 2 records.
    let id = add("/link", Paging::Link);
    assert_eq!(
        e.catalog().table(&format!("{id}/Api")).unwrap().rows,
        Some(6)
    );
    assert_eq!(sum_of(&e, &id, "id"), 21.0);
    assert_eq!(log.hits("/link").len(), 3);
    // A field with a relative next link, that is null on the last page.
    let id = add(
        "/field",
        Paging::Field {
            path: "paging.next".into(),
        },
    );
    assert_eq!(
        e.catalog().table(&format!("{id}/Api")).unwrap().rows,
        Some(6)
    );
    assert_eq!(log.hits("/field").len(), 3);
    // A cursor goes back as a link parameter.
    let id = add(
        "/cursor",
        Paging::Cursor {
            path: "next_cursor".into(),
            param: "cursor".into(),
        },
    );
    assert_eq!(
        e.catalog().table(&format!("{id}/Api")).unwrap().rows,
        Some(6)
    );
    assert_eq!(sum_of(&e, &id, "id"), 21.0);
    assert!(log.hits("/cursor?cursor=t2")[0].contains("/cursor?cursor=t2"));
    // A page number goes up until a page has no records.
    let id = add(
        "/num",
        Paging::Page {
            param: "page".into(),
        },
    );
    assert_eq!(
        e.catalog().table(&format!("{id}/Api")).unwrap().rows,
        Some(4)
    );
    assert_eq!(
        log.hits("/num").len(),
        3,
        "the empty page is the last request"
    );
    // A next link that names a link already read ends the walk.
    let id = add(
        "/loop",
        Paging::Field {
            path: "next".into(),
        },
    );
    assert_eq!(
        e.catalog().table(&format!("{id}/Api")).unwrap().rows,
        Some(1)
    );
    assert_eq!(log.hits("/loop").len(), 1);
}

#[test]
fn a_page_number_stops_at_the_last_page_the_answer_names_and_starts_at_the_link() {
    let d = dir("pages-total");
    let (base, log) = api();
    let e = engine(&d);
    // The World Bank shape: [meta, [records]]. The answer says there are 2 pages, so a third
    // request (which the server would answer with an error message) is never sent.
    let id = e
        .add_source(api_link(
            format!("{base}/numt"),
            None,
            Some(Paging::Page {
                param: "page".into(),
            }),
            None,
        ))
        .unwrap();
    assert_eq!(
        e.catalog().table(&format!("{id}/Api")).unwrap().rows,
        Some(4)
    );
    assert_eq!(log.hits("/numt").len(), 2);
    assert!(log.hits("/numt")[0].starts_with("GET /numt?page=1 "));
    // The number in the link is where it starts.
    let id = e
        .add_source(api_link(
            format!("{base}/numt?page=2"),
            None,
            Some(Paging::Page {
                param: "page".into(),
            }),
            None,
        ))
        .unwrap();
    assert_eq!(
        e.catalog().table(&format!("{id}/Api")).unwrap().rows,
        Some(2)
    );
}

#[test]
fn two_hundred_pages_are_kept_with_a_note_and_the_table_loads() {
    let d = dir("cap");
    let (base, log) = api();
    let e = engine(&d);
    let id = e
        .add_source(api_link(
            format!("{base}/endless"),
            None,
            Some(Paging::Field {
                path: "next".into(),
            }),
            None,
        ))
        .unwrap();
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    let t = e.catalog().table(&format!("{id}/Api")).cloned().unwrap();
    assert_eq!(t.rows, Some(200));
    assert_eq!(log.hits("/endless").len(), 200);
    assert_eq!(
        t.notes,
        ["Read the first 200 pages (200 rows). The link has more."]
    );
    // The note is kept with the copy: it is still there when the workspace is opened again.
    let ws = e.snapshot().workspace;
    e.replace(ws, Some(d.join("Books.tenun")));
    let t = e.catalog().table(&format!("{id}/Api")).cloned().unwrap();
    assert_eq!(t.notes.len(), 1, "{:?}", t.notes);
    assert_eq!(log.hits("/endless").len(), 200, "opening did not fetch");
    // Removing the source removes the note file with the copy.
    e.remove_source(&id);
    assert!(
        !d.join("Books data")
            .join(format!("link-{id}.notes"))
            .exists()
    );
}

#[test]
fn a_header_key_is_sent_and_kept_out_of_every_file() {
    let d = dir("hkey");
    let (base, log) = api();
    let e = engine(&d);
    // Without the key the server says 401, and the words say what to do.
    let err = e
        .add_source(api_link(format!("{base}/secure"), None, None, None))
        .unwrap_err()
        .to_string();
    assert!(err.contains("answered 401 Unauthorized"), "{err}");
    assert!(
        err.ends_with("The link needs a key, or the key is wrong."),
        "{err}"
    );
    // A wrong key fails the same way, shows nothing of the key, and leaves no entry behind.
    let err = e
        .add_source(api_link(
            format!("{base}/secure"),
            None,
            None,
            header_key(WRONG),
        ))
        .unwrap_err()
        .to_string();
    assert!(err.contains("401") && !err.contains(WRONG), "{err}");
    assert!(
        std::fs::read_to_string(d.join("secrets.json")).is_ok_and(|t| !t.contains(WRONG)),
        "a failed add must not keep its key"
    );
    // A connection that fails does not show the key either.
    let err = e
        .add_source(api_link(
            "http://127.0.0.1:1/secure?x=1".into(),
            None,
            None,
            header_key(WRONG),
        ))
        .unwrap_err()
        .to_string();
    assert!(!err.contains(WRONG), "{err}");

    // The right key loads the records.
    let id = e
        .add_source(api_link(
            format!("{base}/secure"),
            None,
            None,
            header_key(KEY),
        ))
        .unwrap();
    let snap = e.snapshot();
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    assert_eq!(snap.tables[0].rows, Some(3));
    assert!(
        log.hits("/secure")
            .last()
            .unwrap()
            .to_lowercase()
            .contains("x-api-key: k-123")
    );
    // The workspace file holds the place and the name, and not the value.
    let text = std::fs::read_to_string(d.join("Books.tenun")).unwrap();
    assert!(
        text.contains("\"place\": \"header\"") && text.contains("\"name\": \"X-Api-Key\""),
        "{text}"
    );
    // The value sits in the secrets file, under the host, the place and the name, and nowhere else:
    // not the workspace file, not the copy, not the answer of /api/workspace.
    let secrets = std::fs::read_to_string(d.join("secrets.json")).unwrap();
    assert!(secrets.contains("link://127.0.0.1/header/X-Api-Key") && secrets.contains(KEY));
    assert_eq!(files_holding(&d, KEY), ["secrets.json"]);
    let wire = serde_json::to_string(&snap).unwrap();
    assert!(!wire.contains(KEY), "{wire}");
    assert!(wire.contains("\"link_keys\":[\"") && wire.contains(&id));
    // The key stays out of a debug print of the request, too.
    assert!(!format!("{:?}", header_key(KEY).unwrap()).contains(KEY));
    // A reload sends it again; the Authorization header works as well.
    let before = log.hits("/secure").len();
    e.load(&id);
    assert!(e.snapshot().errors.is_empty());
    assert_eq!(log.hits("/secure").len(), before + 1);
    assert!(
        log.hits("/secure")
            .last()
            .unwrap()
            .to_lowercase()
            .contains("x-api-key: k-123")
    );
    let id2 = e
        .add_source(api_link(
            format!("{base}/secure?v=2"),
            None,
            None,
            key_in(KeyPlace::Header, "Authorization", &format!("Bearer {KEY}")),
        ))
        .unwrap();
    assert!(e.snapshot().errors.is_empty());
    assert_eq!(
        e.catalog().table(&format!("{id2}/Api")).unwrap().rows,
        Some(3)
    );
    // Opening the workspace on another computer: the file has no value, so the link needs one.
    let other = engine(&dir("hkey-other"));
    let mut ws = e.snapshot().workspace;
    ws.sources.truncate(1);
    other.replace(ws, Some(d.join("Books.tenun")));
    other.load(&id);
    let snap = other.snapshot();
    assert!(snap.errors[&id].contains("401"), "{:?}", snap.errors);
    assert!(snap.link_keys.is_empty(), "no value is stored there");
}

#[test]
fn a_query_key_is_set_in_the_link_and_never_saved_in_it() {
    let d = dir("qkey");
    let (base, log) = api();
    let e = engine(&d);
    // The link already has a parameter of that name: the key replaces it.
    let id = e
        .add_source(api_link(
            format!("{base}/q?api_key=old&x=1"),
            None,
            None,
            key_in(KeyPlace::Query, "api_key", KEY),
        ))
        .unwrap();
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    assert_eq!(
        e.catalog().table(&format!("{id}/Api")).unwrap().rows,
        Some(3)
    );
    assert!(log.hits("/q")[0].starts_with(&format!("GET /q?api_key={KEY}&x=1 ")));
    let text = std::fs::read_to_string(d.join("Books.tenun")).unwrap();
    assert!(text.contains(&format!("\"url\": \"{base}/q?api_key=old&x=1\"")));
    assert_eq!(files_holding(&d, KEY), ["secrets.json"]);
    assert!(
        std::fs::read_to_string(d.join("secrets.json"))
            .unwrap()
            .contains("link://127.0.0.1/query/api_key")
    );
}

#[test]
fn the_key_goes_only_to_the_host_of_the_saved_link() {
    let d = dir("otherhost");
    let (base, log) = api();
    let e = engine(&d);
    // The server answers /hop with a redirect to http://localhost:<port>/secure: the same server
    // under another host name. The second request must arrive without the key.
    let err = e
        .add_source(api_link(format!("{base}/hop"), None, None, header_key(KEY)))
        .unwrap_err()
        .to_string();
    assert!(err.contains("401"), "{err}");
    assert!(
        log.hits("/hop")[0]
            .to_lowercase()
            .contains("x-api-key: k-123")
    );
    let second = log.hits("/secure");
    assert_eq!(second.len(), 1);
    assert!(
        second[0].to_lowercase().contains("host: localhost:"),
        "{}",
        second[0]
    );
    assert!(
        !second[0].to_lowercase().contains("x-api-key"),
        "{}",
        second[0]
    );
    // The same for a key in the link.
    let err = e
        .add_source(api_link(
            format!("{base}/hopq"),
            None,
            None,
            key_in(KeyPlace::Query, "api_key", KEY),
        ))
        .unwrap_err()
        .to_string();
    assert!(err.contains("401") && !err.contains(KEY), "{err}");
    assert!(log.hits("/hopq")[0].contains(&format!("api_key={KEY}")));
    assert!(
        !log.hits("/q")[0].contains("api_key"),
        "{}",
        log.hits("/q")[0]
    );
    // A redirect inside the same host keeps the key.
    let id = e
        .add_source(api_link(
            format!("{base}/hop-same"),
            None,
            None,
            header_key(KEY),
        ))
        .unwrap();
    assert_eq!(
        e.catalog().table(&format!("{id}/Api")).unwrap().rows,
        Some(3)
    );
    // A page link to another host gets no key either, and the page is still read.
    let id = e
        .add_source(api_link(
            format!("{base}/xpage"),
            None,
            Some(Paging::Field {
                path: "next".into(),
            }),
            header_key(KEY),
        ))
        .unwrap();
    assert_eq!(
        e.catalog().table(&format!("{id}/Api")).unwrap().rows,
        Some(2)
    );
    assert!(
        log.hits("/xpage")[0]
            .to_lowercase()
            .contains("x-api-key: k-123")
    );
    let page2 = &log.hits("/xpage2")[0];
    assert!(
        page2.to_lowercase().contains("host: localhost:")
            && !page2.to_lowercase().contains("x-api-key")
    );
}

#[test]
fn link_settings_change_the_records_and_the_key_and_the_secret_follows() {
    let d = dir("settings");
    let (base, _) = api();
    let e = engine(&d);
    let id = e
        .add_source(api_link(
            format!("{base}/secure"),
            None,
            None,
            header_key(KEY),
        ))
        .unwrap();
    let rows = |e: &Engine| e.catalog().table(&format!("{id}/Api")).unwrap().rows;
    assert_eq!(rows(&e), Some(3));
    // A new records path fetches again.
    e.set_link(&id, Some("errors".into()), None, KeyEdit::Keep)
        .unwrap();
    assert_eq!(rows(&e), Some(1));
    // A key with no value keeps the stored value.
    e.set_link(
        &id,
        Some("data.items".into()),
        None,
        KeyEdit::Set(header_key("").unwrap()),
    )
    .unwrap();
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    assert_eq!(rows(&e), Some(3));
    // A new name with no value takes the stored value along. The old entry stays, because a link
    // in another workspace on this computer may still use it.
    e.set_link(
        &id,
        None,
        None,
        KeyEdit::Set(key_in(KeyPlace::Header, "X-Token", "").unwrap()),
    )
    .unwrap();
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    let secrets = std::fs::read_to_string(d.join("secrets.json")).unwrap();
    assert!(secrets.contains("link://127.0.0.1/header/X-Token") && secrets.contains("X-Api-Key"));
    assert_eq!(files_holding(&d, KEY), ["secrets.json"]);
    // `key: null` removes the key and deletes its secret. The fetch then fails with the words for
    // a missing key, and the last copy is still there.
    e.set_link(&id, None, None, KeyEdit::Remove).unwrap();
    let snap = e.snapshot();
    assert!(snap.errors[&id].contains("401"), "{:?}", snap.errors);
    assert_eq!(rows(&e), Some(3), "the last copy is still read");
    assert!(snap.link_keys.is_empty());
    let secrets = std::fs::read_to_string(d.join("secrets.json")).unwrap();
    assert!(!secrets.contains("X-Token"), "{secrets}");
    // Only the entry under the old name is left, and only in the secrets file.
    assert_eq!(files_holding(&d, KEY), ["secrets.json"]);
    let text = std::fs::read_to_string(d.join("Books.tenun")).unwrap();
    assert!(!text.contains("\"key\""), "{text}");
    // The settings of a source that is not a web link are refused.
    assert!(e.set_link("s99", None, None, KeyEdit::Keep).is_err());
}

#[test]
fn a_removed_link_keeps_its_key_for_other_workspaces() {
    let d = dir("remove-key");
    let (base, _) = api();
    let e = engine(&d);
    let a = e
        .add_source(api_link(
            format!("{base}/secure"),
            None,
            None,
            header_key(KEY),
        ))
        .unwrap();
    let b = e
        .add_source(api_link(
            format!("{base}/secure?v=2"),
            None,
            None,
            header_key(""),
        ))
        .unwrap();
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    // The second link had no value of its own: it used the stored one.
    e.remove_source(&a);
    assert!(
        std::fs::read_to_string(d.join("secrets.json"))
            .unwrap()
            .contains(KEY)
    );
    // The entry is shared by every workspace on this computer, so removing the last link in
    // this one keeps it. Only "Remove key" in Link settings deletes it.
    e.remove_source(&b);
    assert!(
        std::fs::read_to_string(d.join("secrets.json"))
            .unwrap()
            .contains(KEY)
    );
}

#[test]
fn a_preview_reads_only_the_first_page_and_saves_nothing() {
    let d = dir("preview");
    let (base, _) = api();
    let (csv_base, _) = serve();
    let e = engine(&d);
    let look = |url: String, records: Option<&str>, key: Option<KeyIn>| {
        e.link_preview(LinkPreview {
            url,
            records: records.map(str::to_string),
            key,
        })
    };
    let p = look(format!("{base}/json"), None, None).unwrap();
    assert_eq!(p.format, "json");
    let lists: Vec<_> = p.lists.iter().map(|l| (l.path.as_str(), l.rows)).collect();
    assert_eq!(lists, [("data.items", 3), ("errors", 1)]);
    assert_eq!(p.records, "data.items");
    assert_eq!(p.columns, ["id", "name", "owner.city", "tags", "extra"]);
    assert_eq!((p.total, p.rows.len()), (3, 3));
    assert_eq!(p.rows[0], ["1", "Ann", "Jakarta", "[\"a\",\"b\"]", ""]);
    assert_eq!(p.pages_hint, "");
    // Another list, by its path.
    assert_eq!(
        look(format!("{base}/json"), Some("errors"), None)
            .unwrap()
            .columns,
        ["e"]
    );
    // The way to the next page, in each of its forms.
    let hint = |path: &str| {
        look(format!("{base}{path}"), None, None)
            .unwrap()
            .pages_hint
    };
    assert_eq!(hint("/link"), "link");
    assert_eq!(hint("/field"), "field:paging.next");
    assert_eq!(hint("/cursor"), "cursor:next_cursor");
    assert_eq!(hint("/numt"), "page");
    assert_eq!(hint("/num?page=1"), "page");
    // A CSV link shows its first rows too.
    let p = look(format!("{csv_base}/ok"), None, None).unwrap();
    assert_eq!((p.format, p.pages_hint.as_str()), ("csv", ""));
    assert_eq!(p.columns, ["city", "sales"]);
    assert_eq!(p.rows, [["Jakarta", "10"], ["Bandung", "5"]]);
    assert!(look(format!("{base}/html"), None, None).is_err());
    // A key is used for the look and not kept: no copy, no entry.
    let p = look(format!("{base}/secure"), None, header_key(KEY)).unwrap();
    assert_eq!(p.total, 3);
    let err = look(format!("{base}/secure"), None, header_key(WRONG))
        .unwrap_err()
        .to_string();
    assert!(err.contains("401") && !err.contains(WRONG), "{err}");
    assert!(files_holding(&d, KEY).is_empty());
    assert!(!d.join("Books data").exists());
    assert!(e.snapshot().workspace.sources.is_empty());
    // After the link is added, a look with no value uses the stored one.
    e.add_source(api_link(
        format!("{base}/secure"),
        None,
        None,
        header_key(KEY),
    ))
    .unwrap();
    assert_eq!(
        look(format!("{base}/secure"), None, header_key(""))
            .unwrap()
            .total,
        3
    );
}

#[test]
fn a_key_that_the_answer_repeats_is_hidden_in_the_copy_and_in_the_errors() {
    let d = dir("echo");
    let (base, _) = api();
    let e = engine(&d);
    // A wrong path names the lists there are. One is named after the key: the words hide it.
    let err = e
        .add_source(api_link(
            format!("{base}/echo"),
            Some("nope"),
            None,
            key_in(KeyPlace::Header, "X-Api-Key", ECHO),
        ))
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        "There is no list at nope. Lists in this answer: ••• (2)."
    );
    // The records repeat the key in a link. The copy, the table and the preview hide it.
    let id = e
        .add_source(api_link(
            format!("{base}/echo"),
            None,
            None,
            key_in(KeyPlace::Header, "X-Api-Key", ECHO),
        ))
        .unwrap();
    let t = e.catalog().table(&format!("{id}/Api")).cloned().unwrap();
    assert_eq!(t.rows, Some(2));
    assert_eq!(t.notes, ["The key was in the data. Tenun hid it."]);
    assert_eq!(
        files_holding(&d, ECHO),
        ["secrets.json"],
        "the value is in the secrets file only"
    );
    let copy =
        std::fs::read_to_string(d.join("Books data").join(format!("link-{id}.csv"))).unwrap();
    assert!(
        copy.contains("https://h.example/echo?api_key=•••&i=1"),
        "{copy}"
    );
    let p = e
        .link_preview(LinkPreview {
            url: format!("{base}/echo"),
            records: None,
            key: key_in(KeyPlace::Header, "X-Api-Key", ECHO),
        })
        .unwrap();
    assert!(
        !serde_json::to_string(&p)
            .unwrap()
            .contains(&format!("api_key={ECHO}"))
    );
    assert_eq!(p.rows[0][1], "https://h.example/echo?api_key=•••&i=1");
}

#[test]
fn a_page_that_says_it_is_over_the_limit_ends_the_walk_with_a_note() {
    let d = dir("big");
    let (base, log) = api();
    let e = engine(&d);
    let id = e
        .add_source(api_link(
            format!("{base}/big"),
            None,
            Some(Paging::Field {
                path: "next".into(),
            }),
            None,
        ))
        .unwrap();
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    let t = e.catalog().table(&format!("{id}/Api")).cloned().unwrap();
    assert_eq!(t.rows, Some(2), "what was read is kept");
    assert_eq!(
        t.notes,
        ["Read the first page (2 rows) and stopped at 50 MB. The link has more."]
    );
    assert_eq!(log.hits("/big").len(), 2);
}

#[test]
fn a_failed_add_leaves_the_key_that_was_stored_before() {
    let d = dir("keep-stored");
    let (base, _) = api();
    // One workspace has a link with a key.
    let first = engine(&d);
    first
        .add_source(api_link(
            format!("{base}/secure"),
            None,
            None,
            header_key(KEY),
        ))
        .unwrap();
    // Another workspace on this computer uses the stored value for a link that then fails. The
    // first workspace still needs the key.
    let second = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    second.replace(
        Workspace {
            name: "Other".into(),
            ..Workspace::default()
        },
        Some(d.join("Other.tenun")),
    );
    let err = second
        .add_source(api_link(format!("{base}/gone"), None, None, header_key("")))
        .unwrap_err()
        .to_string();
    assert!(err.contains("404"), "{err}");
    let secrets = std::fs::read_to_string(d.join("secrets.json")).unwrap();
    assert!(
        secrets.contains("link://127.0.0.1/header/X-Api-Key") && secrets.contains(KEY),
        "{secrets}"
    );
}
