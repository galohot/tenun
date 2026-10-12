//! Boundaries of other countries, downloaded once (build 05, V4-D9, V4-D19). A small server on 127.0.0.1 plays
//! geoBoundaries: its API names the file, the file is GeoJSON.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tenun_core::geo::{self, Level};

/// `/KEN/ADM1/` answers with the link of `/ken1.geojson`; `/MOZ/ADM1/` with the link of a web page; anything else
/// is a 404. Every request counts.
fn serve() -> (String, Arc<AtomicU32>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicU32::new(0));
    let h = hits.clone();
    let me = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 2048];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            h.fetch_add(1, Ordering::SeqCst);
            let (status, body) = match path.as_str() {
                "/KEN/ADM1/" => (
                    "200 OK",
                    format!(r#"{{"boundaryISO":"KEN","simplifiedGeometryGeoJSON":"{me}/ken1.geojson"}}"#),
                ),
                "/ken1.geojson" => (
                    "200 OK",
                    r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"shapeName":"Nairobi","shapeISO":"KE-30"},"geometry":{"type":"Polygon","coordinates":[[[36.7,-1.3],[36.9,-1.3],[36.9,-1.2],[36.7,-1.3]]]}}]}"#.to_string(),
                ),
                "/MOZ/ADM1/" => (
                    "200 OK",
                    format!(r#"{{"simplifiedGeometryGeoJSON":"{me}/page"}}"#),
                ),
                "/page" => ("200 OK", "<html><body>Rate limited</body></html>".to_string()),
                _ => ("404 Not Found", "nothing here".to_string()),
            };
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (base, hits)
}

fn dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-geo-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn a_country_that_is_not_bundled_is_downloaded_once_and_read_from_the_copy() {
    let (api, hits) = serve();
    let d = dir("once");
    let file = geo::boundaries("KE", Level::Province, &d, &api).unwrap();
    assert_eq!(file.file_name().unwrap(), "KEN-ADM1.geojson");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("Nairobi"));
    assert_eq!(hits.load(Ordering::SeqCst), 2, "the API, then the file");
    // The second time it is the copy: no request.
    let again = geo::boundaries("ke", Level::Province, &d, &api).unwrap();
    assert_eq!(again, file);
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[test]
fn what_is_not_a_map_is_not_kept_and_the_words_say_what_happened() {
    let (api, _) = serve();
    let d = dir("words");
    // The link gives a web page.
    let e = geo::boundaries("MZ", Level::Province, &d, &api).unwrap_err();
    assert_eq!(e.to_string(), "The map of MZ did not download as a map.");
    assert!(!d.join("MOZ-ADM1.geojson").exists());
    // geoBoundaries has nothing at that level.
    let e = geo::boundaries("KE", Level::Regency, &d, &api).unwrap_err();
    assert_eq!(
        e.to_string(),
        "geoBoundaries has no second-level map of KE."
    );
    // A code nobody knows, and a whole country (countries ship with Tenun).
    assert_eq!(
        geo::boundaries("ZZ", Level::Province, &d, &api)
            .unwrap_err()
            .to_string(),
        "ZZ is not a country code Tenun knows."
    );
    assert!(geo::boundaries("KE", Level::Country, &d, &api).is_err());
    // No server at all: the words say so, and nothing is kept.
    let e =
        geo::boundaries("KE", Level::Province, &dir("offline"), "http://127.0.0.1:9").unwrap_err();
    assert!(
        e.to_string().starts_with("Could not reach geoBoundaries"),
        "{e}"
    );
}
