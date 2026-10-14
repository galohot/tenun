//! Email updates (v5 build 13, V5-D51): the app sends the numbers of a page's number tiles beside each snapshot, as
//! the page writes them when it opens; tenun.cloud puts them in the email and never parses the snapshot. The setting
//! goes with the other settings.
//!
//! The data, by hand: September 2026, store A sells 10 a day (days 1–30); October, days 1 to 10, A sells 100 a day and
//! B 50 a day. October so far is 1,500; the same days of September are 100.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use tenun::cloud;
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;

/// What the stand-in was sent: (method, path, the digest header, the body).
type Seen = Arc<Mutex<Vec<(String, String, Option<String>, Vec<u8>)>>>;

fn server() -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let (log, url) = (seen.clone(), base.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or_default().to_string();
            let path = parts.next().unwrap_or_default().to_string();
            let (mut len, mut digest) = (0, None);
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
                if k.eq_ignore_ascii_case("x-tenun-digest") {
                    digest = Some(v.trim().to_string());
                }
            }
            let mut body = vec![0; len];
            r.read_exact(&mut body).unwrap();
            let reply = match (method.as_str(), path.as_str()) {
                ("POST", "/api/v1/dashboards") => json!({ "id": "D1" }),
                ("PUT", _) => json!({ "ok": true, "url": format!("{url}/d/D1"), "version": 1 }),
                _ => json!({ "ok": true }),
            }
            .to_string();
            log.lock().unwrap().push((method, path, digest, body));
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

/// Decodes base64 with the URL alphabet and no padding.
fn unbase64(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'-' => 62,
        _ => 63,
    };
    let mut out = Vec::new();
    for chunk in s.as_bytes().chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, c)| n | (u32::from(val(*c)) << (18 - 6 * i)));
        for i in 0..chunk.len() - 1 {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    out
}

#[test]
fn each_publish_carries_the_numbers_of_the_page_as_it_opens() {
    let (base, seen) = server();
    let d = std::env::temp_dir().join(format!("tenun-updates-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let mut csv = String::from("date,store,amount\n");
    for day in 1..=30 {
        csv.push_str(&format!("2026-09-{day:02},A,10\n"));
    }
    for day in 1..=10 {
        csv.push_str(&format!("2026-10-{day:02},A,100\n2026-10-{day:02},B,50\n"));
    }
    std::fs::write(d.join("orders.csv"), csv).unwrap();
    let secrets = Secrets::new(d.join("config").join("secrets.json"));
    cloud::save_connection(&secrets, &base, "tok", "owner@example.com").unwrap();
    let e = Engine::new(Secrets::new(d.join("engine").join("secrets.json"))).unwrap();
    e.add_source(AddSource::File {
        path: d.join("orders.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let t = e.snapshot().tables[0].id.clone();
    e.set_dashboards(vec![json!({ "id": "d1", "title": "Shops", "tiles": [
        { "id": "k", "type": "kpi", "title": "Revenue", "x": 0, "y": 0, "w": 3, "h": 2,
          "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
          "dims": [{ "table": t, "column": "date", "grain": "month" }], "formats": [{ "currency": "USD" }] },
        { "id": "n", "type": "kpi", "title": "", "x": 3, "y": 0, "w": 3, "h": 2,
          "measures": [{ "table": t, "column": null, "agg": "count" }], "dims": [] },
        { "id": "b", "type": "bar", "title": "By store", "x": 0, "y": 2, "w": 6, "h": 4,
          "measures": [{ "table": t, "column": "amount", "agg": "sum" }], "dims": [{ "table": t, "column": "store" }] }
    ], "publish": { "access": "link", "updates": "daily" } })]);

    let digest = cloud::digest(&e, "d1");
    assert_eq!(
        digest,
        json!({ "title": "Shops", "items": [
            { "name": "Revenue", "value": "$1,500", "period": "Oct 1–10, 2026, so far", "change": "▲ 1400% vs Sep 1–10" },
            // No title: the name the app gives it. Over all the data: no period.
            { "name": "Number of orders", "value": "50" }
        ] })
    );

    // The names the app gives values (the same words as `measureLabel` in the app).
    let name = |m: Value| {
        tenun_core::labels::measure_label(
            &e.catalog(),
            &e.measures(),
            &serde_json::from_value(m).unwrap(),
        )
    };
    assert_eq!(
        name(json!({ "table": t, "column": "amount", "agg": "sum" })),
        "Amount"
    );
    assert_eq!(
        name(json!({ "table": t, "column": "amount", "agg": "avg" })),
        "Average amount"
    );
    assert_eq!(
        name(json!({ "table": t, "column": "store", "agg": "count_distinct" })),
        "Unique store"
    );
    assert_eq!(
        name(
            json!({ "table": t, "column": "amount", "agg": "sum", "per": { "table": t, "column": null, "agg": "count" } })
        ),
        "Amount per order"
    );

    cloud::publish(&e, &secrets, &base, "d1").unwrap();
    let reqs = seen.lock().unwrap().clone();
    let patch = reqs.iter().find(|r| r.0 == "PATCH").unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&patch.3).unwrap()["updates"],
        "daily"
    );
    let put = reqs.iter().find(|r| r.0 == "PUT").unwrap();
    let sent: Value = serde_json::from_slice(&unbase64(put.2.as_deref().unwrap())).unwrap();
    assert_eq!(sent, digest);
}
