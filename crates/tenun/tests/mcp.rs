//! `tenun mcp` (v5 build 14, V5-D18): a scripted MCP client talks to the real binary over stdio. It opens with the
//! current protocol (no handshake) and with an older one (`initialize`); reads the tables; asks a question; adds a
//! measure, a dashboard and tiles and reads them back; takes the last change back. With `tenun serve` running on the
//! same workspace, the calls go through it, so the app has the new tile without a reload. Hostile calls are refused
//! in words.
//!
//! The data, by hand: September 2026, store A sells 10 a day (300); October 1–10, A 100 a day (1,000) and B 50 a day
//! (500). A = 1,300, B = 500. A `password` column sits beside them and is never read.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;

fn lab(tag: &str) -> (PathBuf, PathBuf) {
    let d = std::env::temp_dir().join(format!("tenun-mcp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("config")).unwrap();
    let mut csv = String::from("date,store,amount,password\n");
    for day in 1..=30 {
        csv.push_str(&format!("2026-09-{day:02},A,10,hunter2\n"));
    }
    for day in 1..=10 {
        csv.push_str(&format!(
            "2026-10-{day:02},A,100,hunter2\n2026-10-{day:02},B,50,hunter2\n"
        ));
    }
    std::fs::write(d.join("orders.csv"), csv).unwrap();
    let e = Engine::new(Secrets::new(
        d.join("config").join("tenun").join("secrets.json"),
    ))
    .unwrap();
    e.add_source(AddSource::File {
        path: d.join("orders.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    e.set_dashboards(vec![
        json!({ "id": "d1", "title": "Overview", "refresh": 0, "tiles": [] }),
    ]);
    let file = d.join("Shop.tenun");
    e.save(Some(file.clone())).unwrap();
    (d, file)
}

/// The binary with its config folder (secrets, the list of running servers) inside the test folder.
fn tenun(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_tenun"));
    c.env("XDG_CONFIG_HOME", dir.join("config"))
        .env("HOME", dir)
        .env("APPDATA", dir.join("config"))
        .env_remove("TENUN_CLOUD_URL");
    c
}

struct Client {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
}

impl Client {
    fn start(dir: &Path, file: &Path) -> Self {
        let mut child = tenun(dir)
            .arg("mcp")
            .arg(file)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
            next: 0,
        }
    }

    /// One request of the current protocol, with its version in `_meta`; the whole answer.
    fn ask(&mut self, method: &str, mut params: Value, version: &str) -> Value {
        self.next += 1;
        params["_meta"] = json!({ "io.modelcontextprotocol/protocolVersion": version,
            "io.modelcontextprotocol/clientInfo": { "name": "test", "version": "1" },
            "io.modelcontextprotocol/clientCapabilities": {} });
        self.send(json!({ "jsonrpc": "2.0", "id": self.next, "method": method, "params": params }))
    }

    fn send(&mut self, msg: Value) -> Value {
        writeln!(self.input, "{msg}").unwrap();
        self.input.flush().unwrap();
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], msg["id"]);
        v
    }

    /// A tool's answer: Ok(structured value) or Err(the words of a refusal).
    fn tool(&mut self, name: &str, args: Value) -> Result<Value, String> {
        let r = self.ask(
            "tools/call",
            json!({ "name": name, "arguments": args }),
            "2026-07-28",
        );
        let res = &r["result"];
        assert_eq!(res["resultType"], "complete", "{r}");
        if res["isError"] == true {
            return Err(res["content"][0]["text"].as_str().unwrap().to_string());
        }
        assert_eq!(
            serde_json::from_str::<Value>(res["content"][0]["text"].as_str().unwrap()).unwrap(),
            res["structuredContent"]
        );
        Ok(res["structuredContent"].clone())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn an_ai_app_reads_builds_and_takes_its_change_back() {
    let (dir, file) = lab("build");
    let mut c = Client::start(&dir, &file);

    // An older client opens with `initialize`; a current one may ask what the server speaks.
    let init = c.send(json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "old", "version": "1" } } }));
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "tenun");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    let d = c.ask("server/discover", json!({}), "2026-07-28");
    assert!(
        d["result"]["supportedVersions"]
            .as_array()
            .unwrap()
            .contains(&json!("2026-07-28"))
    );
    let bad = c.ask("tools/list", json!({}), "1999-01-01");
    assert_eq!(bad["error"]["code"], -32022);
    assert_eq!(bad["error"]["data"]["requested"], "1999-01-01");
    let list = c.ask("tools/list", json!({}), "2026-07-28");
    // A current client keeps the list as long as it says, for every client alike (Claude Code requires both).
    assert_eq!(list["result"]["ttlMs"], 3_600_000);
    assert_eq!(list["result"]["cacheScope"], "public");
    assert_eq!(d["result"]["cacheScope"], "public");
    let names: Vec<String> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "list_tables",
            "describe_table",
            "list_relationships",
            "query",
            "check_formula",
            "list_dashboards",
            "read_dashboard",
            "add_measure",
            "add_dashboard",
            "add_tile",
            "update_tile",
            "remove_tile",
            "undo"
        ]
    );

    // Read: the table, a question.
    let tables = c.tool("list_tables", json!({})).unwrap();
    let t = tables["tables"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(tables["tables"][0]["rows"], 50);
    let desc = c
        .tool("describe_table", json!({ "table": "orders" }))
        .unwrap();
    assert_eq!(desc["sample"]["rows"].as_array().unwrap().len(), 5);
    let amount = json!({ "table": t, "column": "amount", "agg": "sum" });
    let store = json!({ "table": t, "column": "store" });
    let q = c
        .tool(
            "query",
            json!({ "measures": [amount], "dimensions": [store], "sort": [{ "by": 0 }] }),
        )
        .unwrap();
    assert_eq!(q["rows"], json!([["A", 1300.0], ["B", 500.0]]));

    // Build: a measure, a dashboard, two tiles; each tile answers with its result.
    let m = c
        .tool("add_measure", json!({ "name": "Average order", "formula": "SUM([orders].[amount]) / COUNTROWS([orders])" }))
        .unwrap();
    assert_eq!(m["name"], "Average order");
    let dash = c
        .tool("add_dashboard", json!({ "title": "Stores" }))
        .unwrap();
    let dash = dash["id"].as_str().unwrap().to_string();
    let bar = c
        .tool(
            "add_tile",
            json!({ "dashboard": dash, "tile": { "type": "bar", "title": "Amount by store",
            "measures": [amount], "dims": [store] } }),
        )
        .unwrap();
    assert_eq!(bar["result"]["rows"].as_array().unwrap().len(), 2);
    // Placed in the first free place of the grid: the chart at the top left, the number beside it.
    assert_eq!(
        (bar["tile"]["x"].clone(), bar["tile"]["y"].clone()),
        (json!(0), json!(0))
    );
    let avg = c
        .tool(
            "add_tile",
            json!({ "dashboard": dash, "tile": { "type": "kpi", "measures": [m["use_as"]] } }),
        )
        .unwrap();
    // 1,800 over 50 orders.
    assert_eq!(avg["result"]["value"], 36.0);
    assert_eq!(
        (avg["tile"]["x"].clone(), avg["tile"]["y"].clone()),
        (json!(6), json!(0))
    );
    let line = c
        .tool("add_tile", json!({ "dashboard": dash, "tile": { "type": "line", "title": "By month",
            "measures": [amount], "dims": [{ "table": t, "column": "date", "grain": "month" }], "forecast": { "periods": 3 } } }))
        .unwrap();
    // Two months are too few to forecast, and the answer says so in words.
    assert!(
        line["result"]["forecast"]["note"]
            .as_str()
            .unwrap()
            .contains("needs at least")
    );
    let read = c
        .tool("read_dashboard", json!({ "dashboard": dash }))
        .unwrap();
    assert_eq!(read["tiles"].as_array().unwrap().len(), 3);
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(saved.contains("Amount by store") && saved.contains("Average order"));

    // Take back the last change: the line goes, the rest stays.
    assert_eq!(
        c.tool("undo", json!({})).unwrap()["undone"],
        "added the tile By month to Stores"
    );
    let read = c
        .tool("read_dashboard", json!({ "dashboard": dash }))
        .unwrap();
    assert_eq!(read["tiles"].as_array().unwrap().len(), 2);
    assert!(!std::fs::read_to_string(&file).unwrap().contains("By month"));
}

#[test]
fn hostile_calls_are_refused_in_words() {
    let (dir, file) = lab("hostile");
    let mut c = Client::start(&dir, &file);
    assert_eq!(
        c.tool("describe_table", json!({ "table": "/etc/passwd" }))
            .unwrap_err(),
        "There is no table /etc/passwd. list_tables gives their ids."
    );
    let long = format!("SUM([orders].[amount]) + LEN(\"{}\")", "x".repeat(10_001));
    assert_eq!(
        c.tool("add_measure", json!({ "name": "Long", "formula": long }))
            .unwrap_err(),
        "A formula has at most 4000 characters."
    );
    assert_eq!(
        c.tool(
            "update_tile",
            json!({ "dashboard": "d1", "tile": "t99", "changes": { "title": "x" } })
        )
        .unwrap_err(),
        "There is no tile t99 on d1. read_dashboard gives their ids."
    );
    let t = c.tool("list_tables", json!({})).unwrap()["tables"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let err = c
        .tool(
            "query",
            json!({ "measures": [{ "table": t, "column": "password", "agg": "count_distinct" }] }),
        )
        .unwrap_err();
    assert!(
        err.contains("does not give an AI app the column password"),
        "{err}"
    );
    // Its values are not shown either.
    let desc = c.tool("describe_table", json!({ "table": t })).unwrap();
    assert!(!desc.to_string().contains("hunter2"));
    assert!(
        c.tool(
            "add_tile",
            json!({ "dashboard": "d1", "tile": { "type": "script" } })
        )
        .unwrap_err()
        .starts_with("A tile's type is one of")
    );
    // A tile has no filter of its own: said, not ignored.
    assert!(
        c.tool(
            "add_tile",
            json!({ "dashboard": "d1", "tile": { "type": "kpi",
            "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
            "filters": [{ "table": t, "column": "store", "op": "in", "values": ["A"] }] } })
        )
        .unwrap_err()
        .starts_with("A tile has no filter of its own")
    );
    // A tile that does not run is not kept.
    assert!(
        c.tool(
            "add_tile",
            json!({ "dashboard": "d1", "tile": { "type": "bar",
            "measures": [{ "table": t, "column": "nope", "agg": "sum" }], "dims": [] } })
        )
        .unwrap_err()
        .starts_with("That tile does not work")
    );
    assert_eq!(
        c.tool("read_dashboard", json!({ "dashboard": "d1" }))
            .unwrap()["tiles"],
        json!([])
    );
    assert_eq!(
        c.tool("undo", json!({})).unwrap_err(),
        "There is no change of an AI app to take back."
    );
    let unknown = c.ask(
        "tools/call",
        json!({ "name": "run_sql", "arguments": {} }),
        "2026-07-28",
    );
    assert_eq!(unknown["error"]["code"], -32602);
}

#[test]
fn with_the_app_open_on_the_workspace_the_calls_go_through_it() {
    let (dir, file) = lab("app");
    let mut serve = tenun(&dir)
        .args(["serve", "--port", "0", "--no-open"])
        .arg(&file)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(serve.stdout.take().unwrap());
    let mut line = String::new();
    while !line.contains("is running at") {
        line.clear();
        out.read_line(&mut line).unwrap();
    }
    let url = line
        .split("is running at ")
        .nth(1)
        .unwrap()
        .trim()
        .trim_end_matches('/')
        .to_string();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();

    let mut c = Client::start(&dir, &file);
    let t = c.tool("list_tables", json!({})).unwrap()["tables"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    c.tool(
        "add_tile",
        json!({ "dashboard": "d1", "tile": { "type": "kpi", "title": "Amount",
        "measures": [{ "table": t, "column": "amount", "agg": "sum" }] } }),
    )
    .unwrap();
    // The app has it, with no reload: it made the change.
    let ws: Value = serde_json::from_str(
        &agent
            .get(format!("{url}/api/workspace"))
            .call()
            .unwrap()
            .body_mut()
            .read_to_string()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        ws["workspace"]["dashboards"][0]["tiles"][0]["title"],
        "Amount"
    );
    // Undo in the app (the notice's button) takes it back.
    let undo: Value = serde_json::from_str(
        &agent
            .post(format!("{url}/api/mcp/call"))
            .header("x-tenun", "1")
            .header("content-type", "application/json")
            .send(json!({ "name": "undo", "arguments": {} }).to_string())
            .unwrap()
            .body_mut()
            .read_to_string()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(undo["ok"], true, "{undo}");
    assert_eq!(
        c.tool("read_dashboard", json!({ "dashboard": "d1" }))
            .unwrap()["tiles"],
        json!([])
    );
    // A change the user made after the AI app's is never undone with it: undo refuses, in words.
    c.tool("add_dashboard", json!({ "title": "From the AI" }))
        .unwrap();
    let now: Value = serde_json::from_str(
        &agent
            .get(format!("{url}/api/workspace"))
            .call()
            .unwrap()
            .body_mut()
            .read_to_string()
            .unwrap(),
    )
    .unwrap();
    let mut dashboards = now["workspace"]["dashboards"].clone();
    dashboards[0]["title"] = json!("Renamed by the user");
    let put = agent
        .put(format!("{url}/api/dashboards"))
        .header("x-tenun", "1")
        .header("content-type", "application/json")
        .send(dashboards.to_string())
        .unwrap();
    assert_eq!(put.status(), 200);
    assert!(
        c.tool("undo", json!({}))
            .unwrap_err()
            .starts_with("The workspace changed after that change")
    );
    let _ = serve.kill();
    let _ = serve.wait();
}
