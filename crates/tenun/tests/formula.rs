//! Formula checking through the engine and through `POST /api/formula/check`.
//!
//! Three small CSV files stand for an order sheet, the stores and their targets. The totals
//! are small enough to add up by hand.

use serde_json::{Value, json};
use std::path::PathBuf;
use tenun::{Options, Running};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::formula::{CheckReply, CheckRequest, Kind};
use tenun_core::secrets::Secrets;
use tenun_core::workspace::FieldRef;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-formula-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Orders: 100, 250, 40, 75.5, 10 = 475.5. Targets: 300 + 300 + 100 + 150 = 850.
fn write_files(d: &std::path::Path) {
    std::fs::write(
        d.join("orders.csv"),
        "id,date,store_id,amount\n1,2026-01-15,1,100\n2,2026-01-20,1,250\n3,2026-02-03,2,40\n\
         4,2026-03-31,3,75.5\n5,2026-03-05,2,10\n",
    )
    .unwrap();
    std::fs::write(
        d.join("stores.csv"),
        "id,name,rent\n1,Kopi A,500\n2,Kopi B,300\n3,Kopi C,200\n",
    )
    .unwrap();
    std::fs::write(
        d.join("targets.csv"),
        "store_id,month,target\n1,2026-01,300\n1,2026-02,300\n2,2026-01,100\n3,2026-01,150\n",
    )
    .unwrap();
}

fn load(e: &Engine, d: &std::path::Path) -> Vec<String> {
    let mut ids = Vec::new();
    for f in ["orders.csv", "stores.csv", "targets.csv"] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    let snap = e.snapshot();
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    for name in ["orders", "stores", "targets"] {
        ids.push(
            snap.tables
                .iter()
                .find(|t| t.name == name)
                .unwrap_or_else(|| panic!("no table {name}"))
                .id
                .clone(),
        );
    }
    let field = |t: &str, c: &str| FieldRef {
        table: t.to_string(),
        column: c.to_string(),
    };
    e.add_relationship(field(&ids[0], "store_id"), field(&ids[1], "id"))
        .unwrap();
    e.add_relationship(field(&ids[2], "store_id"), field(&ids[1], "id"))
        .unwrap();
    ids
}

fn engine(tag: &str) -> (Engine, Vec<String>) {
    let d = dir(tag);
    write_files(&d);
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    let ids = load(&e, &d);
    (e, ids)
}

fn check(e: &Engine, formula: &str, kind: Kind, table: Option<&str>) -> CheckReply {
    e.check_formula(&CheckRequest {
        formula: formula.into(),
        kind,
        table: table.map(String::from),
    })
    .unwrap()
}

#[test]
fn the_engine_previews_a_column() {
    let (e, ids) = engine("col");
    let r = check(&e, "[amount] * 2", Kind::Column, Some(&ids[0]));
    assert!(r.ok, "{r:?}");
    assert_eq!(
        r.preview.unwrap(),
        [
            json!(200.0),
            json!(500.0),
            json!(80.0),
            json!(151.0),
            json!(20.0)
        ]
    );
    assert_eq!(r.uses.unwrap(), [ids[0].clone()]);
}

#[test]
fn the_engine_reads_a_related_column_and_dates() {
    let (e, ids) = engine("related");
    let r = check(
        &e,
        "[stores].[rent] - [amount]",
        Kind::Column,
        Some("orders"),
    );
    assert!(r.ok, "{r:?}");
    assert_eq!(r.preview.unwrap()[0], json!(400.0));
    assert_eq!(r.uses.unwrap(), [ids[1].clone(), ids[0].clone()]);

    let d = check(&e, "EOMONTH([date], 0)", Kind::Column, Some("orders"));
    assert!(d.ok, "{d:?}");
    assert_eq!(d.dtype, Some(tenun_core::catalog::DType::Date));
    assert_eq!(d.preview.unwrap()[0], json!("2026-01-31"));
}

#[test]
fn the_engine_totals_a_measure_across_tables() {
    let (e, ids) = engine("measure");
    let r = check(
        &e,
        "SUM([orders].[amount]) / SUM([targets].[target])",
        Kind::Measure,
        None,
    );
    assert!(r.ok, "{r:?}");
    let got = r.preview.unwrap()[0].as_f64().unwrap();
    assert!((got - 475.5 / 850.0).abs() < 1e-9, "{got}");
    assert_eq!(r.uses.unwrap(), [ids[0].clone(), ids[2].clone()]);
}

#[test]
fn the_engine_explains_a_wrong_formula() {
    let (e, _) = engine("wrong");
    let r = check(&e, "[amout] * 2", Kind::Column, Some("orders"));
    assert!(!r.ok);
    let err = r.error.unwrap();
    assert_eq!(err.message, "No column [amout]. Did you mean [amount]?");
    assert_eq!((err.at, err.len), (0, 7));
    assert!(r.preview.is_none() && r.dtype.is_none() && r.uses.is_none());
}

#[test]
fn the_engine_rejects_a_total_in_a_column_and_a_bare_column_in_a_measure() {
    let (e, _) = engine("rules");
    let a = check(&e, "SUM([amount])", Kind::Column, Some("orders"));
    assert!(
        a.error
            .unwrap()
            .message
            .starts_with("SUM works over many rows")
    );
    let b = check(&e, "[orders].[amount] / 2", Kind::Measure, None);
    assert_eq!(
        b.error.unwrap().message,
        "[orders].[amount] needs a total here, like SUM([orders].[amount])."
    );
}

// ---- the route -------------------------------------------------------------------------

async fn server(tag: &str) -> (Running, Vec<String>) {
    let running = tenun::start(Options {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        workspace: None,
        picker: None,
    })
    .await
    .unwrap();
    let d = dir(tag);
    write_files(&d);
    let ids = load(&running.state.engine, &d);
    (running, ids)
}

/// POSTs JSON to the running server and returns the status and the JSON answer.
async fn post(running: &Running, path: &str, body: Value) -> (u16, Value) {
    let url = format!("http://{}{path}", running.addr);
    tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let mut res = agent
            .post(url)
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
async fn the_route_answers_a_good_column_with_a_type_and_a_preview() {
    let (s, ids) = server("route-ok").await;
    let (status, v) = post(
        &s,
        "/api/formula/check",
        json!({ "formula": "[amount] * 2", "kind": "column", "table": ids[0] }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(v["ok"], true);
    assert_eq!(v["dtype"], "number");
    assert_eq!(v["preview"], json!([200.0, 500.0, 80.0, 151.0, 20.0]));
    assert_eq!(v["uses"], json!([ids[0]]));
    assert!(v.get("error").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_route_answers_a_wrong_formula_with_a_message_and_offsets() {
    let (s, ids) = server("route-bad").await;
    let (status, v) = post(
        &s,
        "/api/formula/check",
        json!({ "formula": "IF([amount] > 5; 1; 2)", "kind": "column", "table": ids[0] }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(v["ok"], false);
    assert_eq!(
        v["error"],
        json!({ "message": "Use a comma between the parts of a function.", "at": 15, "len": 1 })
    );
    assert!(v.get("preview").is_none() && v.get("dtype").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_route_answers_a_measure_with_its_grand_total() {
    let (s, _) = server("route-measure").await;
    let (_, v) = post(
        &s,
        "/api/formula/check",
        json!({ "formula": "SUM([orders].[amount])", "kind": "measure" }),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["dtype"], "number");
    assert_eq!(v["preview"], json!([475.5]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_route_never_sends_sql() {
    let (s, ids) = server("route-nosql").await;
    for body in [
        json!({ "formula": "[amount] * 2", "kind": "column", "table": ids[0] }),
        json!({ "formula": "SUM([orders].[amount])", "kind": "measure" }),
        json!({ "formula": "[nope]", "kind": "column", "table": ids[0] }),
        json!({ "formula": "SUM([amount])", "kind": "column", "table": ids[0] }),
        json!({ "formula": "[date] + 1", "kind": "column", "table": ids[0] }),
    ] {
        let (_, v) = post(&s, "/api/formula/check", body.clone()).await;
        let text = v.to_string().to_uppercase();
        for word in [
            "SELECT ",
            "CAST(",
            "T0.",
            "LEFT JOIN",
            "NULLIF(",
            "AS DOUBLE",
        ] {
            assert!(!text.contains(word), "{body}: {text}");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_route_needs_a_table_for_a_column() {
    let (s, _) = server("route-table").await;
    let (status, v) = post(
        &s,
        "/api/formula/check",
        json!({ "formula": "1 + 1", "kind": "column" }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(v["ok"], false);
    assert_eq!(
        v["error"]["message"],
        "Pick the table this formula belongs to first."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_route_refuses_an_unknown_kind() {
    let (s, _) = server("route-kind").await;
    let (status, _) = post(
        &s,
        "/api/formula/check",
        json!({ "formula": "1", "kind": "row" }),
    )
    .await;
    assert!(status == 400 || status == 422, "{status}");
}

// ---- the review of build 01 -----------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_route_refuses_a_formula_nested_to_the_length_limit_and_lives_on() {
    let (s, ids) = server("route-deep").await;
    for formula in [
        format!("{}1{}", "(".repeat(3000), ")".repeat(3000)),
        format!("{}1", "-".repeat(9000)),
        format!("{}TRUE{}", "NOT(".repeat(1500), ")".repeat(1500)),
    ] {
        let (status, v) = post(
            &s,
            "/api/formula/check",
            json!({ "formula": formula, "kind": "column", "table": ids[0] }),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"]["message"], "This formula is nested too deeply.");
    }
    let (_, v) = post(
        &s,
        "/api/formula/check",
        json!({ "formula": "1".repeat(10_001), "kind": "column", "table": ids[0] }),
    )
    .await;
    assert_eq!(v["error"]["message"], "This formula is too long.");
    // The server is still there.
    let (_, v) = post(
        &s,
        "/api/formula/check",
        json!({ "formula": "[amount] * 2", "kind": "column", "table": ids[0] }),
    )
    .await;
    assert_eq!(v["ok"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_route_answers_a_chain_of_carets_at_once() {
    let (s, ids) = server("route-carets").await;
    let formula = format!("{}2", "2^".repeat(25));
    let start = std::time::Instant::now();
    let (_, v) = post(
        &s,
        "/api/formula/check",
        json!({ "formula": formula, "kind": "column", "table": ids[0] }),
    )
    .await;
    assert!(start.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["dtype"], "number");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_route_counts_offsets_in_utf16_units() {
    let (s, ids) = server("route-utf16").await;
    let (_, v) = post(
        &s,
        "/api/formula/check",
        json!({ "formula": "\"😀\" & [amout]", "kind": "column", "table": ids[0] }),
    )
    .await;
    // The emoji is two UTF-16 units, so [amout] starts at 7, not 6.
    assert_eq!(v["error"]["at"], 7);
    assert_eq!(v["error"]["len"], 7);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_route_ignores_one_leading_equals_sign() {
    let (s, ids) = server("route-equals").await;
    let (_, v) = post(
        &s,
        "/api/formula/check",
        json!({ "formula": "= [amount] * 2", "kind": "column", "table": ids[0] }),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["preview"][0], json!(200.0));
}

#[test]
fn a_sum_of_nothing_is_zero_through_the_engine() {
    let (e, _) = engine("zero");
    let r = check(
        &e,
        "SUM([orders].[amount]) - SUMIF([orders].[amount], [orders].[id] > 100)",
        Kind::Measure,
        None,
    );
    assert!(r.ok, "{r:?}");
    assert_eq!(r.preview.unwrap(), [json!(475.5)]);
    // An average of nothing stays blank.
    let r = check(
        &e,
        "AVERAGEIF([orders].[amount], [orders].[id] > 100)",
        Kind::Measure,
        None,
    );
    assert_eq!(r.preview.unwrap(), [Value::Null]);
}
