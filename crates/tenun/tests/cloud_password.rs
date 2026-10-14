//! Password links and embedding (v5 build 11, V5-D13, V5-D14), on the app's side: a password is kept in this
//! computer's secrets and never in the `.tenun` file; a publish sends it once, and again only when tenun.cloud cannot
//! hold it any more (a new cloud dashboard, or a password set again after another access). Embedding is never sent for
//! invited people.
//!
//! A small server on this computer stands in for tenun.cloud and records every request.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tenun::cloud;
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;

const PASSWORD: &str = "Q7#mango-river";

/// The settings changes the stand-in was sent: the path and the body of each.
type Seen = Arc<Mutex<Vec<(String, Value)>>>;

/// A stand-in for tenun.cloud. It records each settings change (PATCH) it is sent. When `gone` is set, the first
/// dashboard has been deleted on the website.
fn server(gone: Arc<AtomicBool>) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let url = base.clone();
    std::thread::spawn(move || {
        let mut made = 0;
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or_default().to_string();
            let path = parts.next().unwrap_or_default().to_string();
            let mut len = 0;
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                let (k, v) = h.split_once(':').unwrap();
                if k.eq_ignore_ascii_case("content-length") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            r.read_exact(&mut body).unwrap();
            let lost = gone.load(Ordering::SeqCst) && path.starts_with("/api/v1/dashboards/D1");
            let (status, reply) = match (method.as_str(), path.as_str()) {
                _ if lost => (404, json!({ "error": "not_found" })),
                ("POST", "/api/v1/dashboards") => {
                    made += 1;
                    (200, json!({ "id": format!("D{made}") }))
                }
                ("PUT", p) if p.ends_with("/snapshot") => {
                    let id = p.split('/').nth(4).unwrap_or_default();
                    (
                        200,
                        json!({ "ok": true, "url": format!("{url}/d/{id}"), "version": 1 }),
                    )
                }
                _ => (200, json!({ "ok": true })),
            };
            if method == "PATCH" && !lost {
                log.lock()
                    .unwrap()
                    .push((path, serde_json::from_slice(&body).unwrap()));
            }
            let reply = reply.to_string();
            write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
        }
    });
    (base, seen)
}

/// A saved workspace with one dashboard set to publish, and this computer's secrets.
fn lab(base: &str, name: &str) -> (Engine, Secrets, std::path::PathBuf) {
    let d = std::env::temp_dir().join(format!("tenun-cloudpw-{name}-{}", std::process::id()));
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
    let t = e.snapshot().tables[0].id.clone();
    e.set_dashboards(vec![json!({ "id": "d1", "title": "Shops", "tiles": [
        { "id": "k", "type": "kpi", "title": "", "dims": [],
          "measures": [{ "table": t, "column": "sales", "agg": "sum" }] }
    ], "publish": { "access": "link" } })]);
    let file = d.join("Shops.tenun");
    e.save(Some(file.clone())).unwrap();
    (e, secrets, file)
}

fn settings(e: &Engine) -> cloud::PublishSettings {
    cloud::settings(&e.dashboard("d1").unwrap()).unwrap()
}

fn set(e: &Engine, f: impl FnOnce(&mut Value)) {
    e.update_dashboard("d1", |d| f(&mut d["publish"]));
}

/// The settings sent by the last publish.
fn last(seen: &Seen) -> (String, Value) {
    seen.lock().unwrap().last().cloned().unwrap()
}

#[test]
fn a_password_stays_out_of_the_file_and_goes_to_tenun_cloud_once() {
    let gone = Arc::new(AtomicBool::new(false));
    let (base, seen) = server(gone.clone());
    let (e, secrets, file) = lab(&base, "once");
    // Too short, refused in words; nothing is kept.
    let err = cloud::set_item_password(&e, &secrets, "d1", "abc").unwrap_err();
    assert_eq!(err.to_string(), "A password has 4 to 200 characters.");
    assert_eq!(settings(&e).access, "link");

    cloud::set_item_password(&e, &secrets, "d1", PASSWORD).unwrap();
    let ps = settings(&e);
    assert_eq!(ps.access, "password");
    assert!(!ps.pw_sent);
    // The file holds a random key for the secret, and no part of the password: not even 4 characters of it in a row.
    let holds_none = |file: &std::path::Path| {
        let text = std::fs::read_to_string(file).unwrap();
        let chars: Vec<char> = PASSWORD.chars().collect();
        chars
            .windows(4)
            .map(|w| w.iter().collect::<String>())
            .all(|part| !text.contains(&part))
    };
    assert!(holds_none(&file));
    assert!(
        std::fs::read_to_string(secrets.path())
            .unwrap()
            .contains(PASSWORD)
    );

    // The first publish sends it; the next ones do not.
    cloud::publish(&e, &secrets, &base, "d1").unwrap();
    let (path, body) = last(&seen);
    assert_eq!(path, "/api/v1/dashboards/D1");
    assert_eq!(body["access"], "password");
    assert_eq!(body["password"], PASSWORD);
    assert!(settings(&e).pw_sent);
    e.save(None).unwrap();
    assert!(holds_none(&file));
    cloud::publish(&e, &secrets, &base, "d1").unwrap();
    let (_, body) = last(&seen);
    assert_eq!(body["access"], "password");
    assert!(body.get("password").is_none(), "{body}");

    // A new password goes once too.
    cloud::set_item_password(&e, &secrets, "d1", "a new one").unwrap();
    cloud::publish(&e, &secrets, &base, "d1").unwrap();
    assert_eq!(last(&seen).1["password"], "a new one");
    cloud::publish(&e, &secrets, &base, "d1").unwrap();
    assert!(last(&seen).1.get("password").is_none());

    // Anyone with the link drops the password there; choosing the password again sends the kept one again.
    set(&e, |p| p["access"] = json!("link"));
    cloud::publish(&e, &secrets, &base, "d1").unwrap();
    assert!(last(&seen).1.get("password").is_none());
    assert!(!settings(&e).pw_sent);
    set(&e, |p| p["access"] = json!("password"));
    cloud::publish(&e, &secrets, &base, "d1").unwrap();
    assert_eq!(last(&seen).1["password"], "a new one");

    // Deleted on the website: the new cloud dashboard gets the password with its first settings.
    gone.store(true, Ordering::SeqCst);
    cloud::publish(&e, &secrets, &base, "d1").unwrap();
    let (path, body) = last(&seen);
    assert_eq!(path, "/api/v1/dashboards/D2");
    assert_eq!(body["password"], "a new one");
    assert_eq!(settings(&e).id, "D2");
}

#[test]
fn embedding_is_sent_for_a_link_or_a_password_and_never_for_invited_people() {
    let (base, seen) = server(Arc::new(AtomicBool::new(false)));
    let (e, secrets, _) = lab(&base, "embed");
    set(&e, |p| p["embed"] = json!(true));
    cloud::publish(&e, &secrets, &base, "d1").unwrap();
    assert_eq!(last(&seen).1["embed"], true);
    set(&e, |p| {
        p["access"] = json!("invited");
        p["invites"] = json!(["a@example.com"]);
    });
    cloud::publish(&e, &secrets, &base, "d1").unwrap();
    let (_, body) = last(&seen);
    assert_eq!(body["access"], "invited");
    assert_eq!(body["embed"], false);
    // The setting stays on the dashboard for when the access changes back.
    assert!(settings(&e).embed);
}
