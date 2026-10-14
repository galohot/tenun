//! Alerts (v5 build 12, V5-D15): one message when a rule starts to hold, one when it is back to normal (if asked),
//! none in between; a send that fails is tried three times in all, then the rule shows the error; each channel gets
//! its own shape; no address or token is written in the `.tenun` file, the state file or an error; the value is the
//! tile's as its page opens (saved filters, filter tiles, the default view), never anything else.
//!
//! The data, by hand: October 2026, days 1 to 10, store A sells 100 a day (1,000) and store B 50 a day (500).
//! September: A 30 days of 10 (300).
//!
//! A small server on this computer stands in for Slack, Teams, Discord, Telegram and a web address, and records
//! what it is sent.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tenun::alerts::{self, Address};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::workspace::{AlertRule, ChannelKind};

/// What the stand-in was sent: (path, body).
type Seen = Arc<Mutex<Vec<(String, Value)>>>;
/// The status the stand-in answers on a path (200 when not set).
type Answers = Arc<Mutex<HashMap<String, u16>>>;

fn server() -> (String, Seen, Answers) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let answers: Answers = Arc::new(Mutex::new(HashMap::new()));
    let (log, ans) = (seen.clone(), answers.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let path = line
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
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
            let status = ans.lock().unwrap().get(&path).copied().unwrap_or(200);
            log.lock()
                .unwrap()
                .push((path, serde_json::from_slice(&body).unwrap_or(Value::Null)));
            write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok"
            )
            .unwrap();
        }
    });
    (base, seen, answers)
}

fn orders(a_per_day: u32) -> String {
    let mut csv = String::from("date,store,amount\n");
    for day in 1..=30 {
        csv.push_str(&format!("2026-09-{day:02},A,10\n"));
    }
    for day in 1..=10 {
        csv.push_str(&format!(
            "2026-10-{day:02},A,{a_per_day}\n2026-10-{day:02},B,50\n"
        ));
    }
    csv
}

struct Lab {
    engine: Engine,
    secrets: Secrets,
    dir: PathBuf,
    table: String,
    source: String,
}

fn lab(tag: &str) -> Lab {
    let dir = std::env::temp_dir().join(format!("tenun-alerts-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("orders.csv"), orders(100)).unwrap();
    let secrets = Secrets::new(dir.join("config").join("secrets.json"));
    let engine = Engine::new(Secrets::new(dir.join("engine").join("secrets.json"))).unwrap();
    let source = engine
        .add_source(AddSource::File {
            path: dir.join("orders.csv").to_string_lossy().into_owned(),
        })
        .unwrap();
    let table = engine.snapshot().tables[0].id.clone();
    Lab {
        engine,
        secrets,
        dir,
        table,
        source,
    }
}

impl Lab {
    /// One dashboard with a number tile "Revenue" (October so far) and the extra `json` of the dashboard.
    fn page(&self, extra: Value) {
        let t = &self.table;
        let mut d = json!({ "id": "d1", "title": "Shops", "refresh": 0, "tiles": [
            { "id": "k", "type": "kpi", "title": "Revenue", "x": 0, "y": 0, "w": 3, "h": 2,
              "measures": [{ "table": t, "column": "amount", "agg": "sum" }, { "table": t, "column": null, "agg": "count" }],
              "dims": [{ "table": t, "column": "date", "grain": "month" }] }
        ] });
        for (k, v) in extra.as_object().unwrap() {
            if k == "tiles" {
                for x in v.as_array().unwrap() {
                    d["tiles"].as_array_mut().unwrap().push(x.clone());
                }
            } else {
                d[k] = v.clone();
            }
        }
        self.engine.set_dashboards(vec![d]);
    }

    fn rule(&self, id: &str, op: &str, value: f64, back: bool, kind: &str) -> AlertRule {
        serde_json::from_value(
            json!({ "id": id, "dashboard": "d1", "tile": "k", "op": op, "value": value,
            "channel": { "kind": kind, "secret": "" }, "on": true, "back": back }),
        )
        .unwrap()
    }

    fn check(&self, telegram: &str) -> alerts::Outcome {
        alerts::check_with(
            &self.engine,
            &self.secrets,
            &self.dir.join("alerts.json"),
            chrono::Utc::now(),
            telegram,
        )
    }

    fn states(&self) -> std::collections::BTreeMap<String, alerts::RuleState> {
        alerts::load_states(&self.dir.join("alerts.json"))
    }

    /// Store A sells `n` a day in October from now on.
    fn a_sells(&self, n: u32) {
        std::fs::write(self.dir.join("orders.csv"), orders(n)).unwrap();
        self.engine.load(&self.source);
    }
}

fn hook(base: &str, path: &str) -> Address {
    Address {
        url: format!("{base}{path}"),
        ..Address::default()
    }
}

fn path(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

#[test]
fn a_rule_tells_once_when_it_starts_and_once_when_it_is_back() {
    let (base, seen, _) = server();
    let l = lab("once");
    l.page(json!({}));
    // A's October so far is 1,500 with B: below 2,000 holds.
    l.engine
        .set_alerts(vec![
            l.rule("a1", "below", 2000.0, true, "webhook"),
            l.rule("a2", "above", 1400.0, false, "discord"),
        ])
        .unwrap();
    alerts::set_address(
        &l.secrets,
        "a1",
        ChannelKind::Webhook,
        &hook(&base, "/hook"),
    )
    .unwrap();
    alerts::set_address(
        &l.secrets,
        "a2",
        ChannelKind::Discord,
        &hook(&base, "/discord"),
    )
    .unwrap();

    let out = l.check("");
    assert_eq!(out.sent.len(), 2, "{out:?}");
    let sent = seen.lock().unwrap().clone();
    let hook_body = &sent.iter().find(|(p, _)| p == "/hook").unwrap().1;
    assert_eq!(hook_body["event"], "start");
    assert_eq!(hook_body["value"], 1500.0);
    assert_eq!(
        hook_body["text"],
        "Revenue (Oct 1–10, 2026, so far) is 1,500, below 2,000. — Shops"
    );
    assert_eq!(
        sent.iter().find(|(p, _)| p == "/discord").unwrap().1["content"],
        "Revenue (Oct 1–10, 2026, so far) is 1,500, above 1,400. — Shops"
    );
    assert!(l.states()["a1"].holding_since.is_some());

    // Still holding: nothing more.
    assert!(l.check("").sent.is_empty());
    assert_eq!(seen.lock().unwrap().len(), 2);

    // A sells 150 a day: 2,000, no longer below 2,000. The rule that asked says so; the other says nothing.
    l.a_sells(150);
    let out = l.check("");
    assert_eq!(out.sent.len(), 1, "{out:?}");
    let (p, body) = seen.lock().unwrap().last().cloned().unwrap();
    assert_eq!(p, "/hook");
    assert_eq!(body["event"], "back");
    assert_eq!(
        body["text"],
        "Revenue (Oct 1–10, 2026, so far) is 2,000, no longer below 2,000. — Shops"
    );
    assert!(l.check("").sent.is_empty());

    // Below again: a new start.
    l.a_sells(100);
    assert_eq!(l.check("").sent.len(), 1);
    // Switched off: forgotten; on again while it holds, it starts again.
    let mut rules = l.engine.alerts();
    rules[0].on = false;
    l.engine.set_alerts(rules.clone()).unwrap();
    assert!(l.check("").sent.is_empty());
    assert!(!l.states().contains_key("a1"));
    rules[0].on = true;
    l.engine.set_alerts(rules).unwrap();
    assert_eq!(l.check("").sent.len(), 1);
}

#[test]
fn a_failed_send_is_tried_three_times_then_the_rule_shows_the_error() {
    let (base, seen, answers) = server();
    answers.lock().unwrap().insert("/slack".into(), 500);
    let l = lab("fail");
    l.page(json!({}));
    l.engine
        .set_alerts(vec![l.rule("a1", "below", 2000.0, true, "slack")])
        .unwrap();
    alerts::set_address(&l.secrets, "a1", ChannelKind::Slack, &hook(&base, "/slack")).unwrap();
    for _ in 0..5 {
        l.check("");
    }
    assert_eq!(seen.lock().unwrap().len(), 3);
    let st = &l.states()["a1"];
    assert_eq!(
        st.error,
        "127.0.0.1 answered 500. Tried 3 times; Tenun stopped trying."
            .replace("127.0.0.1", base.trim_start_matches("http://"))
    );
    assert!(st.pending.is_none());
    // It works again: nothing is sent until the rule starts to hold again.
    answers.lock().unwrap().clear();
    l.check("");
    assert_eq!(seen.lock().unwrap().len(), 3);
    // Back to normal says nothing: the start never went out.
    l.a_sells(150);
    l.check("");
    assert_eq!(seen.lock().unwrap().len(), 3);
    // A failure that then works: two tries, one message, the error cleared.
    let (base2, seen2, answers2) = server();
    answers2.lock().unwrap().insert("/slack".into(), 503);
    let l2 = lab("fail2");
    l2.page(json!({}));
    l2.engine
        .set_alerts(vec![l2.rule("b1", "below", 2000.0, true, "slack")])
        .unwrap();
    alerts::set_address(
        &l2.secrets,
        "b1",
        ChannelKind::Slack,
        &hook(&base2, "/slack"),
    )
    .unwrap();
    assert_eq!(l2.check("").failed.len(), 1);
    answers2.lock().unwrap().clear();
    assert_eq!(l2.check("").sent.len(), 1);
    assert_eq!(seen2.lock().unwrap().len(), 2);
    assert!(l2.states()["b1"].error.is_empty());
    assert!(l2.states()["b1"].told);
}

#[test]
fn each_channel_gets_its_own_shape_and_no_secret_is_written_or_shown() {
    let (base, seen, answers) = server();
    let l = lab("shapes");
    l.page(json!({}));
    let token = "123456789:AAEexampleexampleexample_secret-1";
    l.engine
        .set_alerts(vec![
            l.rule("s", "below", 2000.0, false, "slack"),
            l.rule("t", "below", 2000.0, false, "teams"),
            l.rule("d", "below", 2000.0, false, "discord"),
            l.rule("g", "below", 2000.0, false, "telegram"),
            l.rule("w", "below", 2000.0, false, "webhook"),
        ])
        .unwrap();
    for (id, kind, p) in [
        ("s", ChannelKind::Slack, "/services/T000/B000/slacksecret"),
        ("t", ChannelKind::Teams, "/workflows/teamssecret"),
        ("d", ChannelKind::Discord, "/api/webhooks/1/discordsecret"),
        ("w", ChannelKind::Webhook, "/hook?key=hooksecret"),
    ] {
        alerts::set_address(&l.secrets, id, kind, &hook(&base, p)).unwrap();
    }
    let tg = Address {
        token: token.into(),
        chat: "-1001234567890".into(),
        ..Address::default()
    };
    alerts::set_address(&l.secrets, "g", ChannelKind::Telegram, &tg).unwrap();
    // A bad token or no chat is refused in words.
    let bad = Address {
        token: "not-a-token".into(),
        chat: "1".into(),
        ..Address::default()
    };
    assert!(alerts::set_address(&l.secrets, "g", ChannelKind::Telegram, &bad).is_err());
    assert!(alerts::set_address(&l.secrets, "s", ChannelKind::Slack, &Address::default()).is_err());

    let out = l.check(&base);
    assert_eq!(out.sent.len(), 5, "{out:?}");
    let sent: HashMap<String, Value> = seen.lock().unwrap().iter().cloned().collect();
    let text = "Revenue (Oct 1–10, 2026, so far) is 1,500, below 2,000. — Shops";
    assert_eq!(
        sent["/services/T000/B000/slacksecret"],
        json!({ "text": text })
    );
    // Teams Workflows posts the Adaptive Card of the message.
    let teams = &sent["/workflows/teamssecret"];
    assert_eq!(teams["type"], "message");
    assert_eq!(
        teams["attachments"][0]["contentType"],
        "application/vnd.microsoft.card.adaptive"
    );
    assert_eq!(teams["attachments"][0]["content"]["body"][0]["text"], text);
    assert_eq!(
        sent["/api/webhooks/1/discordsecret"],
        json!({ "content": text })
    );
    assert_eq!(
        sent[&format!("/bot{token}/sendMessage")],
        json!({ "chat_id": "-1001234567890", "text": text, "disable_web_page_preview": true })
    );
    let w = &sent["/hook?key=hooksecret"];
    for k in [
        "rule",
        "event",
        "name",
        "value",
        "op",
        "threshold",
        "period",
        "dashboard",
        "at",
        "text",
    ] {
        assert!(w.get(k).is_some(), "{k} in {w}");
    }
    assert_eq!(
        (w["rule"].clone(), w["op"].clone(), w["threshold"].clone()),
        (json!("w"), json!("below"), json!(2000.0))
    );

    // Telegram fails: the error names Telegram, never the token.
    answers
        .lock()
        .unwrap()
        .insert(format!("/bot{token}/sendMessage"), 401);
    l.a_sells(150);
    l.check(&base);
    l.a_sells(100);
    let out = l.check(&base);
    let err = out
        .failed
        .iter()
        .find(|(id, _)| id == "g")
        .map(|(_, e)| e.clone())
        .unwrap_or_default();
    assert_eq!(err, "Telegram answered 401.");

    // An address that cannot be reached names its host, not its path.
    let gone = Address {
        url: "http://127.0.0.1:9/hooks/gonesecret".into(),
        ..Address::default()
    };
    alerts::set_address(&l.secrets, "w", ChannelKind::Webhook, &gone).unwrap();
    l.a_sells(150);
    l.check(&base);
    l.a_sells(100);
    let out = l.check(&base);
    let err = out
        .failed
        .iter()
        .find(|(id, _)| id == "w")
        .map(|(_, e)| e.clone())
        .unwrap_or_default();
    assert_eq!(err, "127.0.0.1:9 could not be reached.");

    // No secret in the workspace file or the state file; all of them in this computer's secrets.
    let file = l.dir.join("Shops.tenun");
    l.engine.save(Some(file.clone())).unwrap();
    for f in [file.as_path(), l.dir.join("alerts.json").as_path()] {
        let text = path(f);
        assert!(!text.is_empty());
        for s in [
            "AAEexample",
            "slacksecret",
            "teamssecret",
            "discordsecret",
            "hooksecret",
            "gonesecret",
            "1001234567890",
        ] {
            assert!(!text.contains(s), "{s} in {}", f.display());
        }
    }
    let secrets = path(l.secrets.path());
    assert!(secrets.contains("AAEexample") && secrets.contains("slacksecret"));
}

#[test]
fn the_value_is_the_page_as_it_opens_and_nothing_else() {
    let (base, seen, _) = server();
    let l = lab("page");
    let t = l.table.clone();
    // A filter tile on the store, saved with A picked: 1,000.
    l.page(json!({ "tiles": [
        { "id": "f", "type": "filter", "title": "", "x": 3, "y": 0, "w": 3, "h": 3, "measures": [],
          "dims": [{ "table": t, "column": "store" }], "filter": { "style": "list", "values": ["A"] } }
    ] }));
    // The tile writes its second value with one decimal: so does the message.
    l.engine.update_dashboard("d1", |d| {
        d["tiles"][0]["formats"] = json!([null, { "decimals": 1 }])
    });
    let mut count = l.rule("c", "above", 0.0, false, "webhook");
    count.measure = Some(1);
    count.name = "Number of orders".into();
    l.engine
        .set_alerts(vec![l.rule("a", "above", 0.0, false, "webhook"), count])
        .unwrap();
    for id in ["a", "c"] {
        alerts::set_address(
            &l.secrets,
            id,
            ChannelKind::Webhook,
            &hook(&base, &format!("/{id}")),
        )
        .unwrap();
    }
    l.check("");
    let values = |seen: &Seen| -> HashMap<String, f64> {
        seen.lock()
            .unwrap()
            .iter()
            .map(|(p, b)| (p.clone(), b["value"].as_f64().unwrap()))
            .collect()
    };
    let v = values(&seen);
    assert_eq!(v["/a"], 1000.0);
    // The second value of the tile, with its own name: A's 10 orders in October.
    assert_eq!(v["/c"], 10.0);
    assert!(seen.lock().unwrap().iter().any(|(_, b)| b["text"]
        == "Number of orders (Oct 1–10, 2026, so far) is 10.0, above 0.0. — Shops"));

    // A default view that picks B wins over the tile's saved pick; a view that is not the default does nothing.
    let (base2, seen2, _) = server();
    let l2 = lab("view");
    let t2 = l2.table.clone();
    l2.page(json!({ "tiles": [
        { "id": "f", "type": "filter", "title": "", "x": 3, "y": 0, "w": 3, "h": 3, "measures": [],
          "dims": [{ "table": t2, "column": "store" }], "filter": { "style": "list", "values": ["A"] } }
    ], "views": [
        { "id": "v1", "name": "B", "filters": [], "selections": { "f": { "values": ["B"] } } },
        { "id": "v2", "name": "All", "filters": [], "selections": {} }
    ], "defaultView": "v1" }));
    l2.engine
        .set_alerts(vec![l2.rule("a", "above", 0.0, false, "webhook")])
        .unwrap();
    alerts::set_address(&l2.secrets, "a", ChannelKind::Webhook, &hook(&base2, "/a")).unwrap();
    l2.check("");
    assert_eq!(values(&seen2)["/a"], 500.0);

    // A saved "Keep only" filter of the dashboard applies too: B alone, 500.
    let (base3, seen3, _) = server();
    let l3 = lab("keep");
    let t3 = l3.table.clone();
    l3.page(json!({ "filters": [{ "table": t3, "column": "store", "op": "in", "values": ["B"], "fromTile": "keep" }] }));
    l3.engine
        .set_alerts(vec![l3.rule("a", "above", 0.0, false, "webhook")])
        .unwrap();
    alerts::set_address(&l3.secrets, "a", ChannelKind::Webhook, &hook(&base3, "/a")).unwrap();
    l3.check("");
    assert_eq!(values(&seen3)["/a"], 500.0);
}

#[test]
fn a_rule_is_checked_before_it_is_kept() {
    let l = lab("rules");
    l.page(json!({}));
    let ok = l.rule("a", "below", 10.0, false, "slack");
    let mut outside = l.rule("b", "outside", 10.0, false, "slack");
    outside.to = Some(5.0);
    assert!(
        l.engine
            .set_alerts(vec![ok.clone(), outside.clone()])
            .is_err()
    );
    outside.to = None;
    assert!(l.engine.set_alerts(vec![outside.clone()]).is_err());
    outside.to = Some(20.0);
    l.engine.set_alerts(vec![ok.clone(), outside]).unwrap();
    assert!(l.engine.set_alerts(vec![ok.clone(), ok.clone()]).is_err());
    let mut bad = ok.clone();
    bad.id = "a/../b".into();
    assert!(l.engine.set_alerts(vec![bad]).is_err());
    // The rule points at its secret by its id.
    assert_eq!(l.engine.alerts()[0].channel.secret, "alert:a");
}
