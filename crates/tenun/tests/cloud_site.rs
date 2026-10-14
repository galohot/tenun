//! A site published to tenun.cloud (build 14): the app creates a site (not a dashboard), uploads every page in one
//! gzip payload with the number of pages, keeps the link in the site settings, and a save from the app never rolls the
//! link back. A site of more than 10 pages is refused in words before anything is sent. The timed updates cover the
//! site while it is switched on. Remove deletes the site there and forgets the settings.
//!
//! A small server on this computer stands in for tenun.cloud and records every request.

use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use tenun::cloud::{self, Every, SITE_ID, Schedule};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A stand-in for tenun.cloud: it answers the site API and records what it was sent.
fn server() -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let url = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or_default().to_string();
            let path = parts.next().unwrap_or_default().to_string();
            let mut headers = Vec::new();
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                let (k, v) = h.split_once(':').unwrap();
                headers.push((k.trim().to_lowercase(), v.trim().to_string()));
            }
            let len = headers
                .iter()
                .find(|(k, _)| k == "content-length")
                .map_or(0, |(_, v)| v.parse().unwrap());
            let mut body = vec![0; len];
            r.read_exact(&mut body).unwrap();
            let reply = match (method.as_str(), path.as_str()) {
                ("POST", "/api/v1/sites") => json!({ "id": "S1" }),
                ("PUT", "/api/v1/sites/S1/snapshot") => {
                    json!({ "ok": true, "url": format!("{url}/s/S1"), "version": 1 })
                }
                _ => json!({ "ok": true }),
            }
            .to_string();
            log.lock().unwrap().push(Seen {
                method,
                path,
                headers,
                body,
            });
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
        }
    });
    (base, seen)
}

fn engine(base: &str, name: &str) -> (Engine, Secrets) {
    let d = std::env::temp_dir().join(format!("tenun-cloudsite-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("sales.csv"), "store,sales\na,1\nb,2\n").unwrap();
    let secrets = Secrets::new(d.join("config").join("secrets.json"));
    cloud::save_connection(&secrets, base, "tok", "owner@example.com").unwrap();
    let e = Engine::new(Secrets::new(d.join("engine").join("secrets.json"))).unwrap();
    e.add_source(AddSource::File {
        path: d.join("sales.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    (e, secrets)
}

fn pages(t: &str, n: usize) -> Vec<Value> {
    (1..=n)
        .map(|i| {
            json!({ "id": format!("p{i}"), "title": format!("Page {i}"), "tiles": [
                { "id": "k", "type": "kpi", "title": "", "dims": [],
                  "measures": [{ "table": t, "column": "sales", "agg": "sum" }] }
            ]})
        })
        .collect()
}

fn publish_of(e: &Engine) -> Value {
    e.site().unwrap()["publish"].clone()
}

#[test]
fn a_site_is_published_as_one_site_and_keeps_its_link() {
    let (base, seen) = server();
    let (e, secrets) = engine(&base, "one");
    let t = e.snapshot().tables[0].id.clone();
    e.set_dashboards(pages(&t, 2));
    e.set_site(
        json!({ "on": true, "title": "Shops", "nav": "top", "home": "p1",
        "publish": { "access": "invited", "invites": ["a@example.com"], "every": "hour" } }),
    )
    .unwrap();

    let v = cloud::publish_site(&e, &secrets, &base).unwrap();
    assert_eq!(v["url"], json!(format!("{base}/s/S1")));
    let reqs = seen.lock().unwrap().clone();
    let calls: Vec<_> = reqs
        .iter()
        .map(|r| format!("{} {}", r.method, r.path))
        .collect();
    assert_eq!(
        calls,
        [
            "POST /api/v1/sites",
            "PATCH /api/v1/sites/S1",
            "PUT /api/v1/sites/S1/snapshot"
        ]
    );
    assert_eq!(reqs[0].header("authorization"), Some("Bearer tok"));
    let patch: Value = serde_json::from_slice(&reqs[1].body).unwrap();
    assert_eq!(
        patch,
        json!({ "title": "Shops", "access": "invited", "invites": ["a@example.com"], "embed": false })
    );
    let put = &reqs[2];
    assert_eq!(put.header("x-tenun-pages"), Some("2"));
    assert_eq!(put.header("x-tenun-every"), Some("60"));
    assert_eq!(put.header("x-tenun-title"), Some("Shops"));
    assert_eq!(put.header("content-type"), Some("application/gzip"));
    let mut text = String::new();
    GzDecoder::new(&put.body[..])
        .read_to_string(&mut text)
        .unwrap();
    let payload: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(payload["kind"], json!("site"));
    assert_eq!(payload["order"], json!(["p1", "p2"]));
    // Each page is there, with its number: 1 + 2 = 3.
    assert_eq!(
        payload["pages"]["p2"]["base"]["k"]["rows"][0][0],
        json!(3.0)
    );

    // The link is kept in the site settings.
    let p = publish_of(&e);
    assert_eq!(p["id"], json!("S1"));
    assert_eq!(p["url"], json!(format!("{base}/s/S1")));
    assert_eq!(p["error"], json!(""));
    assert!(!p["last"].as_str().unwrap().is_empty());

    // A save from the app (an older copy, without the link) changes the settings but not the link.
    e.set_site(json!({ "on": true, "title": "Shops", "nav": "side",
        "publish": { "access": "link", "invites": [], "every": "day" } }))
        .unwrap();
    let p = publish_of(&e);
    assert_eq!(
        (&p["id"], &p["url"], &p["access"], &p["every"]),
        (
            &json!("S1"),
            &json!(format!("{base}/s/S1")),
            &json!("link"),
            &json!("day")
        )
    );
    // A save that names no publish settings keeps them whole.
    e.set_site(json!({ "on": true, "title": "Shops", "nav": "top" }))
        .unwrap();
    assert_eq!(publish_of(&e)["id"], json!("S1"));

    // Published again: the same site, no new one.
    seen.lock().unwrap().clear();
    cloud::publish_site(&e, &secrets, &base).unwrap();
    let calls: Vec<_> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|r| format!("{} {}", r.method, r.path))
        .collect();
    assert_eq!(
        calls,
        ["PATCH /api/v1/sites/S1", "PUT /api/v1/sites/S1/snapshot"]
    );

    // Remove: deleted there, forgotten here.
    seen.lock().unwrap().clear();
    cloud::unpublish_site(&e, &secrets, &base).unwrap();
    let r = seen.lock().unwrap().clone();
    assert_eq!(
        (r.len(), r[0].method.as_str(), r[0].path.as_str()),
        (1, "DELETE", "/api/v1/sites/S1")
    );
    assert_eq!(publish_of(&e), Value::Null);
    assert_eq!(e.site().unwrap()["title"], json!("Shops"));
}

#[test]
fn a_site_of_more_than_ten_pages_is_refused_before_anything_is_sent() {
    let (base, seen) = server();
    let (e, secrets) = engine(&base, "big");
    let t = e.snapshot().tables[0].id.clone();
    e.set_dashboards(pages(&t, 11));
    e.set_site(json!({ "on": true, "title": "Big", "nav": "top" }))
        .unwrap();
    let err = cloud::publish_site(&e, &secrets, &base).unwrap_err();
    assert_eq!(
        err.to_string(),
        "A site on tenun.cloud holds up to 10 pages; this one has 11. Leave some pages out of the workspace, or export the site instead."
    );
    assert!(seen.lock().unwrap().is_empty());
    // The error is shown in the app.
    assert_eq!(publish_of(&e)["error"], json!(err.to_string()));
}

#[test]
fn the_timed_updates_cover_the_site_while_it_is_on() {
    let (base, _) = server();
    let (e, _) = engine(&base, "timed");
    let t = e.snapshot().tables[0].id.clone();
    e.set_dashboards(pages(&t, 2));
    let now = chrono::Utc::now();
    let mut s = Schedule::default();
    // Published an hour and a minute ago, every hour: due.
    let last = (now - chrono::Duration::minutes(61)).to_rfc3339();
    e.set_site(json!({ "on": true, "title": "Shops", "nav": "top" }))
        .unwrap();
    e.update_site(|site| {
        site["publish"] = json!({ "id": "S1", "every": "hour", "last": last });
    });
    assert_eq!(s.due(&e, now), [SITE_ID]);
    assert_eq!(cloud::item_name(SITE_ID), "the site");
    // Published 10 minutes ago: not yet.
    let recent = (now - chrono::Duration::minutes(10)).to_rfc3339();
    e.update_site(|site| site["publish"]["last"] = json!(recent));
    assert!(s.due(&e, now).is_empty());
    // On change: due once the data changed, at most hourly.
    e.update_site(|site| {
        site["publish"]["every"] = json!("change");
        site["publish"]["last"] = json!(last);
    });
    assert!(s.due(&e, now).is_empty());
    s.mark_changed(&e);
    assert_eq!(s.due(&e, now), [SITE_ID]);
    // Switched off as a site: no longer published, whatever its schedule.
    e.update_site(|site| site["on"] = json!(false));
    assert!(s.due(&e, now).is_empty());
    assert!(cloud::site_publishes(&e).is_none());
    assert_eq!(
        cloud::settings(&json!({ "publish": { "every": "day" } })).map(|p| p.every),
        Some(Every::Day)
    );
}
