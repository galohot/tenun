//! Starter dashboards (0.7 build 04): each starter on its own sample guesses every role, and the
//! numbers of its dashboard match a count made here, by hand, from the CSV text. A required role
//! without a column, or a column of the wrong kind, is refused in words and adds nothing.
//!
//! `real_files_are_guessed` runs the guess on files of the real-file test set. It needs the folder
//! that `scripts/corpus-fetch.py` fills: `TENUN_CORPUS=<folder> cargo test --test starters -- --ignored`.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::engine::AddSource;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{Tile, run_tile};
use tenun_core::starter;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-starter-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A fresh engine with one file added; returns it and the file's (first) table id.
fn load(d: &Path, path: &Path) -> (Engine, String) {
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    let sid = e
        .add_source(AddSource::File {
            path: path.to_string_lossy().into_owned(),
        })
        .unwrap();
    let t = e
        .catalog()
        .tables
        .iter()
        .find(|t| t.source_id == sid)
        .unwrap()
        .id
        .clone();
    (e, t)
}

fn with_sample(id: &str) -> (Engine, String, PathBuf) {
    let d = dir(id);
    let s = starter::get(id).unwrap();
    let p = d.join(&s.sample);
    std::fs::write(&p, starter::sample(id).unwrap()).unwrap();
    let (e, t) = load(&d, &p);
    (e, t, d)
}

/// The sample as rows of fields, by column name. The samples have no quoted fields.
fn rows(id: &str) -> Vec<BTreeMap<String, String>> {
    let text = starter::sample(id).unwrap();
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().unwrap().split(',').collect();
    lines
        .map(|l| {
            head.iter()
                .zip(l.split(','))
                .map(|(h, v)| (h.to_string(), v.to_string()))
                .collect()
        })
        .collect()
}

fn num(r: &BTreeMap<String, String>, c: &str) -> f64 {
    r[c].parse().unwrap()
}

/// Guesses, checks every guess against `expect`, applies, and gives back the dashboard.
fn guess_and_apply(
    e: &Engine,
    starter_id: &str,
    table: &str,
    expect: &[(&str, Option<&str>)],
) -> Value {
    let guesses = e.starter_guess(starter_id, table).unwrap();
    let got: Vec<(String, Option<String>)> = guesses
        .iter()
        .map(|g| (g.role.clone(), g.column.clone()))
        .collect();
    let want: Vec<(String, Option<String>)> = expect
        .iter()
        .map(|(r, c)| (r.to_string(), c.map(str::to_string)))
        .collect();
    assert_eq!(got, want, "{starter_id}");
    let mapping: BTreeMap<String, String> = guesses
        .into_iter()
        .filter_map(|g| g.column.map(|c| (g.role, c)))
        .collect();
    let id = e.apply_starter(starter_id, table, &mapping, None).unwrap();
    e.dashboard(&id).unwrap()
}

/// Runs every tile of a dashboard; none may fail. Gives the results by tile id.
fn run_all(e: &Engine, dash: &Value) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for t in dash["tiles"].as_array().unwrap() {
        let tile: Tile = serde_json::from_value(t.clone()).unwrap();
        let res = run_tile(e, &tile, &[]).unwrap_or_else(|err| panic!("{}: {err:#}", tile.id));
        out.insert(tile.id.clone(), serde_json::to_value(res).unwrap());
    }
    out
}

/// The one value of a number tile.
fn kpi(res: &BTreeMap<String, Value>, id: &str) -> f64 {
    let rows = res[id]["rows"].as_array().unwrap();
    rows[0]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .as_f64()
        .unwrap()
}

/// A chart's rows as label → its values.
fn groups(res: &BTreeMap<String, Value>, id: &str) -> BTreeMap<String, Vec<f64>> {
    res[id]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let r = r.as_array().unwrap();
            let label = match &r[0] {
                Value::String(s) => s.clone(),
                v => v.to_string(),
            };
            (
                label,
                r[1..]
                    .iter()
                    .map(|v| v.as_f64().unwrap_or(f64::NAN))
                    .collect(),
            )
        })
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

#[test]
fn sales_on_its_sample() {
    let (e, t, _d) = with_sample("sales");
    let dash = guess_and_apply(
        &e,
        "sales",
        &t,
        &[
            ("date", Some("date")),
            ("amount", Some("amount")),
            ("product", Some("product")),
            ("quantity", Some("qty")),
            ("region", Some("city")),
        ],
    );
    assert_eq!(dash["title"], json!("Sales"));
    assert_eq!(dash["tiles"].as_array().unwrap().len(), 8);
    let res = run_all(&e, &dash);
    let data = rows("sales");
    let total: f64 = data.iter().map(|r| num(r, "amount")).sum();
    let qty: f64 = data.iter().map(|r| num(r, "qty")).sum();
    assert_eq!(data.len(), 420);
    assert!(close(kpi(&res, "t1"), total));
    assert_eq!(kpi(&res, "t2"), 420.0);
    assert!(close(kpi(&res, "t3"), total / 420.0));
    assert!(close(kpi(&res, "t4"), qty));
    // Six months on the line, and they add up to the total.
    let by_month = groups(&res, "t5");
    assert_eq!(by_month.len(), 6);
    assert!(close(by_month.values().map(|v| v[0]).sum(), total));
    // The cities, each to the rupiah.
    let mut city: BTreeMap<String, f64> = BTreeMap::new();
    for r in &data {
        *city.entry(r["city"].clone()).or_default() += num(r, "amount");
    }
    let got = groups(&res, "t6");
    assert_eq!(got.len(), city.len());
    for (c, v) in &city {
        assert!(close(got[c][0], *v), "{c}");
    }
    // The best product comes first.
    let mut product: BTreeMap<String, f64> = BTreeMap::new();
    for r in &data {
        *product.entry(r["product"].clone()).or_default() += num(r, "amount");
    }
    let best = product.iter().max_by(|a, b| a.1.total_cmp(b.1)).unwrap();
    let first = &res["t7"]["rows"][0];
    assert_eq!(first[0], json!(best.0));
    assert!(close(first[1].as_f64().unwrap(), *best.1));
}

#[test]
fn budget_on_its_sample() {
    let (e, t, _d) = with_sample("budget");
    let dash = guess_and_apply(
        &e,
        "budget",
        &t,
        &[
            ("account", Some("account")),
            ("budget", Some("budget")),
            ("actual", Some("actual")),
            ("date", Some("month")),
            ("unit", Some("unit")),
        ],
    );
    let res = run_all(&e, &dash);
    let data = rows("budget");
    let budget: f64 = data.iter().map(|r| num(r, "budget")).sum();
    let actual: f64 = data.iter().map(|r| num(r, "actual")).sum();
    assert!(close(kpi(&res, "t1"), budget));
    assert!(close(kpi(&res, "t2"), actual));
    assert!(close(kpi(&res, "t3"), actual / budget));
    // Budget and actual of each account.
    let mut acc: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for r in &data {
        let x = acc.entry(r["account"].clone()).or_default();
        x.0 += num(r, "budget");
        x.1 += num(r, "actual");
    }
    let got = groups(&res, "t4");
    assert_eq!(got.len(), 6);
    for (a, (b, x)) in &acc {
        assert!(close(got[a][0], *b) && close(got[a][1], *x), "{a}");
    }
    assert_eq!(groups(&res, "t5").len(), 12);
    assert_eq!(groups(&res, "t6").len(), 5);
}

#[test]
fn stock_on_its_sample() {
    let (e, t, _d) = with_sample("stock");
    let dash = guess_and_apply(
        &e,
        "stock",
        &t,
        &[
            ("item", Some("item")),
            ("quantity", Some("stock")),
            ("warehouse", Some("warehouse")),
            ("category", Some("category")),
            ("value", Some("value")),
        ],
    );
    let res = run_all(&e, &dash);
    let data = rows("stock");
    let qty: f64 = data.iter().map(|r| num(r, "stock")).sum();
    let value: f64 = data.iter().map(|r| num(r, "value")).sum();
    let items: std::collections::BTreeSet<&str> = data.iter().map(|r| r["item"].as_str()).collect();
    assert!(close(kpi(&res, "t1"), qty));
    assert_eq!(kpi(&res, "t2"), items.len() as f64);
    assert!(close(kpi(&res, "t3"), value));
    let mut wh: BTreeMap<String, f64> = BTreeMap::new();
    for r in &data {
        *wh.entry(r["warehouse"].clone()).or_default() += num(r, "stock");
    }
    let got = groups(&res, "t5");
    for (w, v) in &wh {
        assert!(close(got[w][0], *v), "{w}");
    }
    // The table starts with the item that has the least in store.
    let mut item: BTreeMap<String, f64> = BTreeMap::new();
    for r in &data {
        *item.entry(r["item"].clone()).or_default() += num(r, "stock");
    }
    let low = item.iter().min_by(|a, b| a.1.total_cmp(b.1)).unwrap();
    assert_eq!(res["t6"]["rows"][0][0], json!(low.0));
}

#[test]
fn attendance_on_its_sample() {
    let (e, t, _d) = with_sample("attendance");
    let dash = guess_and_apply(
        &e,
        "attendance",
        &t,
        &[
            ("date", Some("date")),
            ("person", Some("name")),
            ("status", Some("status")),
            ("unit", Some("department")),
        ],
    );
    let res = run_all(&e, &dash);
    let data = rows("attendance");
    let days: std::collections::BTreeSet<&str> = data.iter().map(|r| r["date"].as_str()).collect();
    assert_eq!(kpi(&res, "t1"), data.len() as f64);
    assert_eq!(kpi(&res, "t2"), 18.0);
    assert_eq!(kpi(&res, "t3"), days.len() as f64);
    let mut status: BTreeMap<String, f64> = BTreeMap::new();
    for r in &data {
        *status.entry(r["status"].clone()).or_default() += 1.0;
    }
    // A donut folds its smallest slices into one "other" slice; what it folds adds up.
    let got = groups(&res, "t4");
    let mut other = 0.0;
    for (s, n) in &status {
        match got.get(s) {
            Some(v) => assert_eq!(v[0], *n, "{s}"),
            None => other += n,
        }
    }
    assert_eq!(got.get("\0other").map_or(0.0, |v| v[0]), other);
}

#[test]
fn survey_on_its_sample() {
    let (e, t, _d) = with_sample("survey");
    let dash = guess_and_apply(
        &e,
        "survey",
        &t,
        &[
            ("question", Some("question")),
            ("score", Some("score")),
            ("group", Some("outlet")),
            ("respondent", Some("respondent")),
            ("date", Some("date")),
        ],
    );
    let res = run_all(&e, &dash);
    let data = rows("survey");
    let total: f64 = data.iter().map(|r| num(r, "score")).sum();
    assert!(close(kpi(&res, "t1"), total / data.len() as f64));
    assert_eq!(kpi(&res, "t2"), 360.0);
    assert_eq!(kpi(&res, "t3"), 60.0);
    let mut q: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for r in &data {
        let x = q.entry(r["question"].clone()).or_default();
        x.0 += num(r, "score");
        x.1 += 1.0;
    }
    let got = groups(&res, "t4");
    for (k, (s, n)) in &q {
        assert!(close(got[k][0], s / n), "{k}");
    }
}

#[test]
fn a_required_role_without_a_column_adds_nothing() {
    let (e, t, _d) = with_sample("sales");
    let mut mapping: BTreeMap<String, String> = [
        ("date", "date"),
        ("product", "product"),
        ("quantity", "qty"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    let err = e
        .apply_starter("sales", &t, &mapping, None)
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        "Choose a column for Amount. The Sales starter needs it."
    );
    assert!(e.dashboards().is_empty());
    // A column of text for a role of numbers.
    mapping.insert("amount".into(), "city".into());
    let err = e
        .apply_starter("sales", &t, &mapping, None)
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        "Amount needs a column of numbers, and city holds text."
    );
    // A column that is not there.
    mapping.insert("amount".into(), "price".into());
    assert!(e.apply_starter("sales", &t, &mapping, None).is_err());
    assert!(e.dashboards().is_empty());
}

#[test]
fn an_optional_role_left_out_drops_only_its_tiles() {
    let (e, t, _d) = with_sample("sales");
    let mapping: BTreeMap<String, String> = [
        ("date", "date"),
        ("product", "product"),
        ("amount", "amount"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    let id = e
        .apply_starter("sales", &t, &mapping, Some("Penjualan"))
        .unwrap();
    let dash = e.dashboard(&id).unwrap();
    assert_eq!(dash["title"], json!("Penjualan"));
    let ids: Vec<&str> = dash["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    // Quantity (t4) and region (t6, t8) have no column.
    assert_eq!(ids, ["t1", "t2", "t3", "t5", "t7"]);
    run_all(&e, &dash);
    // A second one gets its own id and a numbered title.
    let id2 = e
        .apply_starter("sales", &t, &mapping, Some("Penjualan"))
        .unwrap();
    assert_ne!(id, id2);
    assert_eq!(e.dashboard(&id2).unwrap()["title"], json!("Penjualan 2"));
}

#[test]
fn other_column_names_are_guessed() {
    // Indonesian headers, in another order, with an extra column.
    let d = dir("names");
    let p = d.join("penjualan.csv");
    std::fs::write(
        &p,
        "No,Tanggal,Nama Barang,Kota,Jumlah,Nilai Penjualan (Rp)\n\
         1,2026-01-02,Kopi,Bandung,2,50000\n2,2026-01-03,Teh,Medan,1,20000\n3,2026-01-04,Kopi,Medan,3,75000\n",
    )
    .unwrap();
    let (e, t) = load(&d, &p);
    let g = e.starter_guess("sales", &t).unwrap();
    let got: Vec<Option<&str>> = g.iter().map(|g| g.column.as_deref()).collect();
    assert_eq!(
        got,
        [
            Some("Tanggal"),
            Some("Nilai Penjualan (Rp)"),
            Some("Nama Barang"),
            Some("Jumlah"),
            Some("Kota")
        ]
    );
}

/// Files of the real-file test set where a starter fits, and what a person picked for each role.
#[test]
#[ignore = "needs the real-file test set: TENUN_CORPUS=<folder>"]
fn real_files_are_guessed() {
    let Some(root) = std::env::var_os("TENUN_CORPUS").map(PathBuf::from) else {
        panic!("set TENUN_CORPUS to the folder scripts/corpus-fetch.py filled");
    };
    type Case = (
        &'static str,
        &'static str,
        &'static [(&'static str, Option<&'static str>)],
    );
    let cases: &[Case] = &[
        // BPS key indicators: a value per indicator and period. `indicator` is nearly one value per
        // row, so it groups nothing; the seven data flows do.
        (
            "hdx-04-indonesia-key-indicators.csv",
            "sales",
            &[
                ("date", Some("start_date")),
                ("amount", Some("value")),
                ("product", Some("dataflow")),
                ("quantity", None),
                ("region", None),
            ],
        ),
        // Internal displacements: a figure per event and date. The id is never the amount. Product and
        // region are weak guesses (`role`, `locations_accuracy`) that a person changes to `category`
        // and `locations_name` on the role screen; the test after the loop does that.
        (
            "hdx-09-indonesia-internal-displacements-updates.csv",
            "sales",
            &[
                ("date", Some("displacement_date")),
                ("amount", Some("figure")),
                ("product", Some("role")),
                ("quantity", None),
                ("region", Some("locations_accuracy")),
            ],
        ),
        // Demographic and health surveys: one value per indicator and place.
        (
            "hdx-01-indonesia-subnational-demographic-and-he.csv",
            "survey",
            &[
                ("question", Some("Indicator")),
                ("score", Some("Value")),
                ("group", Some("Location")),
                ("respondent", None),
                ("date", None),
            ],
        ),
        (
            "hdx-07-indonesia-national-demographic-and-healt.csv",
            "survey",
            &[
                ("question", Some("Indicator")),
                ("score", Some("Value")),
                ("group", Some("CharacteristicLabel")),
                ("respondent", None),
                ("date", None),
            ],
        ),
    ];
    for (file, starter_id, expect) in cases {
        let path = root.join(file);
        assert!(path.exists(), "{} is missing", path.display());
        let d = dir(&format!("real-{starter_id}"));
        let (e, t) = load(&d, &path);
        let guesses = e.starter_guess(starter_id, &t).unwrap();
        let got: Vec<(&str, Option<&str>)> = guesses
            .iter()
            .map(|g| (g.role.as_str(), g.column.as_deref()))
            .collect();
        assert_eq!(&got, expect, "{file}");
        let mapping: BTreeMap<String, String> = guesses
            .into_iter()
            .filter_map(|g| g.column.map(|c| (g.role, c)))
            .collect();
        let id = e.apply_starter(starter_id, &t, &mapping, None).unwrap();
        run_all(&e, &e.dashboard(&id).unwrap());
    }
    // The fix a person makes on the role screen: their columns replace the guesses.
    let d = dir("real-fixed");
    let (e, t) = load(
        &d,
        &root.join("hdx-09-indonesia-internal-displacements-updates.csv"),
    );
    let mapping: BTreeMap<String, String> = [
        ("date", "displacement_date"),
        ("amount", "figure"),
        ("product", "category"),
        ("region", "locations_name"),
    ]
    .into_iter()
    .map(|(r, c)| (r.to_string(), c.to_string()))
    .collect();
    let id = e.apply_starter("sales", &t, &mapping, None).unwrap();
    let res = run_all(&e, &e.dashboard(&id).unwrap());
    assert!(!res.is_empty());
}

#[test]
fn each_indonesian_sample_is_guessed_like_the_english_one() {
    for s in starter::all() {
        let place = |lang: &str| {
            let (name, csv) = starter::sample_in(&s.id, lang).unwrap();
            let d = dir(&format!("{}-{lang}", s.id));
            let p = d.join(name);
            std::fs::write(&p, &csv).unwrap();
            let (e, t) = load(&d, &p);
            let head: Vec<String> = csv
                .lines()
                .next()
                .unwrap()
                .split(',')
                .map(str::to_string)
                .collect();
            // Each role's column, as its place in the header.
            let at: Vec<Option<usize>> = e
                .starter_guess(&s.id, &t)
                .unwrap()
                .into_iter()
                .map(|g| g.column.map(|c| head.iter().position(|h| *h == c).unwrap()))
                .collect();
            at
        };
        assert_eq!(place("id"), place("en"), "{}", s.id);
    }
}
