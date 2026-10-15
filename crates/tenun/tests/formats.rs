//! Number formats around the formatter: a column's format under a tile's own (formats §2.3), the workspace's digit
//! style set over HTTP, and a column's format and date pattern kept through the table setup route.

use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use tenun::alerts;
use tenun_core::Engine;
use tenun_core::numtext::{self, NumFormat};
use tenun_core::secrets::Secrets;
use tenun_core::workspace::{Digits, Locale, NamedMeasure};

fn sample_dir() -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tenun-formats-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sample");
    for f in ["coffee.xlsx", "stores.csv", "customers.sqlite"] {
        std::fs::copy(src.join(f), dir.join(f)).unwrap();
    }
    dir
}

fn engine() -> Engine {
    let dir = sample_dir();
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    e.replace(tenun::sample::workspace(), Some(dir.join("Coffee.tenun")));
    e
}

fn set_column_format(e: &Engine, column: &str, f: Value) {
    let mut setup = e
        .snapshot()
        .workspace
        .tables
        .get("s1/Orders")
        .cloned()
        .unwrap_or_default();
    setup.columns.entry(column.into()).or_default().format = Some(f);
    e.set_table_setup("s1/Orders", Some(setup)).unwrap();
}

#[test]
fn a_plain_total_starts_from_its_column_format_and_the_tile_wins_field_by_field() {
    let e = engine();
    set_column_format(
        &e,
        "amount",
        json!({ "currency": "IDR", "decimals": 0, "negative": "parens" }),
    );
    let tile = |m: Value, own: Value| json!({ "measures": [m], "formats": [own] });
    let amount = |agg: &str| json!({ "table": "s1/Orders", "column": "amount", "agg": agg });

    // Sum, average, min, max, median and p90 of the column take its format; the tile's own fields go over it.
    for agg in ["sum", "avg", "min", "max", "median", "p90"] {
        let f = alerts::format_of(&e, &tile(amount(agg), json!({ "decimals": 2 })), 0);
        assert_eq!(f.currency.as_deref(), Some("IDR"), "{agg}");
        assert_eq!(f.decimals, Some(2), "{agg}");
        assert_eq!(f.negative, Some(numtext::Negative::Parens), "{agg}");
    }
    // A measure without `agg` sums.
    let f = alerts::format_of(
        &e,
        &tile(
            json!({ "table": "s1/Orders", "column": "amount" }),
            Value::Null,
        ),
        0,
    );
    assert_eq!(f.currency.as_deref(), Some("IDR"));
    // A count, a distinct count and a ratio are not the column's numbers.
    for m in [
        amount("count"),
        amount("count_distinct"),
        json!({ "table": "s1/Orders", "column": "amount", "agg": "sum",
                "per": { "table": "s1/Orders", "agg": "count" } }),
    ] {
        assert_eq!(
            alerts::format_of(&e, &tile(m.clone(), Value::Null), 0),
            NumFormat::default(),
            "{m}"
        );
    }
    // A named measure keeps its own format, not a column's.
    e.set_measures(
        vec![NamedMeasure {
            id: String::new(),
            name: "Revenue".into(),
            formula: "SUM([Orders].[amount])".into(),
            format: Some(json!({ "currency": "USD" })),
            description: None,
        }],
        false,
    )
    .unwrap();
    let id = e.measures()[0].id.clone();
    let f = alerts::format_of(
        &e,
        &tile(json!({ "named": id }), json!({ "decimals": 1 })),
        0,
    );
    assert_eq!((f.currency.as_deref(), f.decimals), (Some("USD"), Some(1)));
    // The message writes the value with the workspace's digit style.
    e.set_digits(Some(Digits::SpaceComma));
    let f = alerts::format_of(&e, &tile(amount("sum"), Value::Null), 0);
    assert_eq!(
        numtext::short_in(
            1_234_567.0,
            Locale::En,
            e.digits(),
            &NumFormat {
                compact: Some(false),
                ..f
            }
        ),
        "Rp 1\u{202F}234\u{202F}567"
    );
}

async fn send(running: &tenun::Running, path: &str, body: Value) -> (u16, Value) {
    let url = format!("http://{}{path}", running.addr);
    tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let mut res = agent
            .put(url)
            .header("content-type", "application/json")
            .header("x-tenun", "1")
            .send(body.to_string())
            .unwrap();
        let status = res.status().as_u16();
        let text = res.body_mut().read_to_string().unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_digit_style_and_column_formats_are_set_over_http() {
    let dir = sample_dir();
    let path = dir.join("Coffee.tenun");
    std::fs::write(
        &path,
        serde_json::to_string(&tenun::sample::workspace()).unwrap(),
    )
    .unwrap();
    let running = tenun::start(tenun::Options {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        workspace: Some(path.clone()),
        picker: None,
    })
    .await
    .unwrap();
    let saved =
        || -> Value { serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap() };

    // Set: the snapshot comes back with it, and the file keeps it.
    let (status, snap) = send(
        &running,
        "/api/workspace/digits",
        json!({ "digits": "indian" }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(snap["workspace"]["digits"], "indian");
    assert_eq!(running.state.engine.digits(), Some(Digits::Indian));
    assert_eq!(saved()["digits"], "indian");
    // A style this version does not know clears it, as null does.
    let (_, snap) = send(
        &running,
        "/api/workspace/digits",
        json!({ "digits": "roman" }),
    )
    .await;
    assert!(snap["workspace"].get("digits").is_none());
    send(
        &running,
        "/api/workspace/digits",
        json!({ "digits": "apos_dot" }),
    )
    .await;
    let (_, snap) = send(&running, "/api/workspace/digits", json!({ "digits": null })).await;
    assert!(snap["workspace"].get("digits").is_none());
    assert!(saved().get("digits").is_none());

    // A column's format and date pattern go through the table setup route as they are.
    let format = json!({ "currency": "EUR", "scale": 6, "code": "#,##0.0,,\"M\"", "later": true });
    let (status, snap) = send(
        &running,
        "/api/tables/setup",
        json!({ "table": "s1/Orders", "setup": { "columns": {
            "amount": { "format": format },
            "date": { "date_pattern": "EEEE d MMMM yyyy" } } } }),
    )
    .await;
    assert_eq!(status, 200, "{snap}");
    let cols = &snap["workspace"]["tables"]["s1/Orders"]["columns"];
    assert_eq!(cols["amount"]["format"], format);
    assert_eq!(cols["date"]["date_pattern"], "EEEE d MMMM yyyy");
    assert_eq!(
        saved()["tables"]["s1/Orders"]["columns"]["amount"]["format"],
        format
    );
    // An empty format or pattern leaves no trace.
    let (_, snap) = send(
        &running,
        "/api/tables/setup",
        json!({ "table": "s1/Orders", "setup": { "columns": {
            "amount": { "format": {} }, "date": { "date_pattern": " " } } } }),
    )
    .await;
    assert!(
        snap["workspace"]["tables"]
            .get("s1/Orders")
            .is_none_or(|t| t["columns"].get("amount").is_none())
    );
}
