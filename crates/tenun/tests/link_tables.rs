//! Link tables (build 03b): many to many, with honest totals.
//!
//! A programme can have several funders and a funder several programmes; the table
//! `programme_funders` says which. Spending is by programme. A query may cross the link table once it
//! is marked, and then a spending row is under each of its funders, and in the total once.
//!
//! Programmes: 1 A (budget 100), 2 B (200), 3 C (300), 4 D (400). Funders: 1 Ford, 2 Gates, 3 EU,
//! 4 Wellcome. Pairs: A has Ford and Gates; B has Ford and Wellcome; C has Gates and EU (the pair C-EU is
//! written twice); D has no funder.
//!
//! Spending rows (id, programme, date, amount): 1 A Jan 10, 2 A Feb 20, 3 B Jan 30, 4 C Jan 40, 5 C Feb 50,
//! 6 D Feb 60, 7 A Feb 5. In all 215. By programme: A 35, B 30, C 90, D 60.
//!
//! Every expected number below is worked out from those rows by hand, not by the engine:
//! - by funder: Ford 35 + 30 = 65, Gates 35 + 90 = 125, EU 90, Wellcome 30, no funder 60. The five
//!   bars add up to 370, and the total is 215.
//! - by kind of funder (Ford, Gates and Wellcome are Private, EU is Public): Private 35 + 30 + 90 = 155
//!   (A has two private funders and counts once), Public 90, no funder 60.

use serde_json::{Value, json};
use std::path::PathBuf;
use tenun_core::Engine;
use tenun_core::engine::{AddSource, QueryResult};
use tenun_core::filters;
use tenun_core::query::{Filter, QueryError, SemanticQuery};
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile, run_tile_public};
use tenun_core::workspace::{FieldRef, NamedMeasure, TableSetup};

const OTHER: &str = "\u{0}other";
/// The key of a blank group in a result.
const BLANK: &str = "\u{0}null";

const PROGRAMMES: &str = "id,name,budget\n1,A,100\n2,B,200\n3,C,300\n4,D,400\n";
const FUNDERS: &str =
    "id,name,kind\n1,Ford,Private\n2,Gates,Private\n3,EU,Public\n4,Wellcome,Private\n";
const PAIRS: &str =
    "programme_id,funder_id,share\n1,1,0.5\n1,2,0.5\n2,1,0.6\n2,4,0.4\n3,2,0.5\n3,3,0.5\n3,3,0.5\n";
const SPENDING: &str = "id,programme_id,date,amount\n1,1,2026-01-10,10\n2,1,2026-02-10,20\n3,2,2026-01-15,30\n\
    4,3,2026-01-20,40\n5,3,2026-02-25,50\n6,4,2026-02-05,60\n7,1,2026-02-11,5\n";

fn field(t: &str, c: &str) -> FieldRef {
    FieldRef {
        table: t.into(),
        column: c.into(),
    }
}

struct World {
    e: Engine,
    d: PathBuf,
    spending: String,
    programmes: String,
    funders: String,
    pairs: String,
}

fn world_of(tag: &str, programmes: &str, funders: &str, pairs: &str, spending: &str) -> World {
    let d = std::env::temp_dir().join(format!("tenun-link-tables-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    for (f, text) in [
        ("programmes.csv", programmes),
        ("funders.csv", funders),
        ("programme_funders.csv", pairs),
        ("spending.csv", spending),
    ] {
        std::fs::write(d.join(f), text).unwrap();
    }
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    for f in [
        "programmes.csv",
        "funders.csv",
        "programme_funders.csv",
        "spending.csv",
    ] {
        e.add_source(AddSource::File {
            path: d.join(f).to_string_lossy().into_owned(),
        })
        .unwrap();
    }
    let snap = e.snapshot();
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    let id = |name: &str| {
        snap.tables
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("no table {name}"))
            .id
            .clone()
    };
    let w = World {
        e,
        d,
        spending: id("spending"),
        programmes: id("programmes"),
        funders: id("funders"),
        pairs: id("programme_funders"),
    };
    w.e.add_relationship(
        field(&w.spending, "programme_id"),
        field(&w.programmes, "id"),
    )
    .unwrap();
    w.e.add_relationship(field(&w.pairs, "programme_id"), field(&w.programmes, "id"))
        .unwrap();
    w.e.add_relationship(field(&w.pairs, "funder_id"), field(&w.funders, "id"))
        .unwrap();
    w
}

/// The world with the link table marked.
fn world(tag: &str) -> World {
    let w = world_of(tag, PROGRAMMES, FUNDERS, PAIRS, SPENDING);
    w.mark(true);
    w
}

impl World {
    fn mark(&self, on: bool) {
        let setup = on.then(|| TableSetup {
            link_table: true,
            ..Default::default()
        });
        self.e.set_table_setup(&self.pairs, setup).unwrap();
    }

    fn q(&self, v: Value) -> QueryResult {
        let q: SemanticQuery = serde_json::from_value(v).unwrap();
        self.e.query(&q).unwrap()
    }

    fn err(&self, v: Value) -> QueryError {
        let q: SemanticQuery = serde_json::from_value(v).unwrap();
        let e = self.e.query(&q).unwrap_err();
        e.downcast::<QueryError>().unwrap()
    }

    fn amount(&self) -> Value {
        json!({ "table": self.spending, "column": "amount", "agg": "sum" })
    }

    fn funder(&self, column: &str) -> Value {
        json!({ "table": self.funders, "column": column })
    }

    fn month(&self) -> Value {
        json!({ "table": self.spending, "column": "date", "grain": "month" })
    }

    fn filter(&self, column: &str, op: &str, values: Value) -> Value {
        json!({ "table": self.funders, "column": column, "op": op, "values": values })
    }

    fn tile(&self, ty: &str, measures: Value, dims: Value, extra: Value) -> Tile {
        let mut t = json!({ "id": "t", "type": ty, "measures": measures, "dims": dims });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            t[k] = v;
        }
        serde_json::from_value(t).unwrap()
    }

    /// Spending by a field of the funders, with these filters.
    fn by(&self, column: &str, filters: Value) -> Vec<(String, f64)> {
        pairs(&self.q(json!({
            "measures": [self.amount()], "dimensions": [self.funder(column)], "filters": filters
        })))
    }

    /// The total spending under these filters, and the number of rows it adds up.
    fn total(&self, filters: Value) -> f64 {
        let r = self.q(json!({ "measures": [self.amount()], "filters": filters }));
        r.rows[0][0].as_f64().unwrap_or(0.0)
    }

    fn rows(&self, filters: Value) -> u64 {
        let r = self.q(json!({
            "measures": [{ "table": self.spending, "column": null, "agg": "count" }], "filters": filters
        }));
        r.rows[0][0].as_f64().unwrap() as u64
    }
}

/// `(group, value)` of every row of a one-group result, sorted by group.
fn pairs(r: &QueryResult) -> Vec<(String, f64)> {
    let mut v: Vec<(String, f64)> = r
        .rows
        .iter()
        .map(|row| {
            (
                snapshot::js_key(&row[0]),
                row.last().and_then(Value::as_f64).unwrap_or(f64::NAN),
            )
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} is not {b}");
}

fn same(got: Vec<(String, f64)>, want: &[(&str, f64)]) {
    let mut want: Vec<(String, f64)> = want.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    want.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(got.len(), want.len(), "{got:?} against {want:?}");
    for (g, w) in got.iter().zip(&want) {
        assert_eq!(g.0, w.0, "{got:?} against {want:?}");
        near(g.1, w.1);
    }
}

// ---- the groups ----------------------------------------------------------------------------

#[test]
fn spending_by_funder_is_each_row_under_each_of_its_funders() {
    let w = world("by-funder");
    let r = w.q(json!({ "measures": [w.amount()], "dimensions": [w.funder("name")] }));
    // The pair C-EU is written twice, and EU still has 90, not 180. Programme D has no funder.
    same(
        pairs(&r),
        &[
            ("Ford", 65.0),
            ("Gates", 125.0),
            ("EU", 90.0),
            ("Wellcome", 30.0),
            (BLANK, 60.0),
        ],
    );
    let bars: f64 = r.rows.iter().map(|x| x[1].as_f64().unwrap()).sum();
    near(bars, 370.0);
    // The total counts each row once.
    near(w.total(json!([])), 215.0);
    // The result says the groups overlap, once for the value, in words that name tables only.
    assert_eq!(r.parts.len(), 1);
    assert!(r.parts[0].overlap && !r.parts[0].repeats);
    assert_eq!(r.parts[0].measure, 0);
    assert_eq!(
        r.parts[0].note,
        "A row of spending with two funders is under both. The total counts it once."
    );
}

#[test]
fn a_row_counts_once_in_a_group_however_many_of_its_funders_are_in_it() {
    let w = world("by-kind");
    // Programme A has two private funders: its 35 is in Private once.
    same(
        w.by("kind", json!([])),
        &[("Private", 155.0), ("Public", 90.0), (BLANK, 60.0)],
    );
    // Two groups behind the link table: a row is under each distinct pair of them.
    let r = w.q(json!({
        "measures": [w.amount()],
        "dimensions": [w.funder("kind"), w.funder("name")]
    }));
    let cell = |kind: &str, name: &str| {
        r.rows
            .iter()
            .find(|x| snapshot::js_key(&x[0]) == kind && snapshot::js_key(&x[1]) == name)
            .map(|x| x[2].as_f64().unwrap())
    };
    assert_eq!(cell("Private", "Ford"), Some(65.0));
    assert_eq!(cell("Private", "Gates"), Some(125.0));
    assert_eq!(cell("Public", "EU"), Some(90.0));
    assert_eq!(cell("Public", "Gates"), None);
    assert_eq!(r.rows.len(), 5);
}

#[test]
fn a_group_in_front_of_the_link_table_and_one_behind_it_work_together() {
    let w = world("both");
    let month = w.month();
    let r = w.q(json!({
        "measures": [w.amount()],
        "dimensions": [month, w.funder("name")]
    }));
    // January: A 10 (Ford, Gates), B 30 (Ford, Wellcome), C 40 (Gates, EU). February: A 25 (Ford, Gates),
    // C 50 (Gates, EU), D 60 (no funder).
    let cell = |m: &str, f: &str| {
        r.rows
            .iter()
            .find(|x| snapshot::js_key(&x[0]) == m && snapshot::js_key(&x[1]) == f)
            .map(|x| x[2].as_f64().unwrap())
    };
    assert_eq!(cell("2026-01-01", "Ford"), Some(40.0));
    assert_eq!(cell("2026-01-01", "Gates"), Some(50.0));
    assert_eq!(cell("2026-01-01", "EU"), Some(40.0));
    assert_eq!(cell("2026-01-01", "Wellcome"), Some(30.0));
    assert_eq!(cell("2026-02-01", "Ford"), Some(25.0));
    assert_eq!(cell("2026-02-01", "Gates"), Some(75.0));
    assert_eq!(cell("2026-02-01", "EU"), Some(50.0));
    assert_eq!(cell("2026-02-01", BLANK), Some(60.0));
    assert_eq!(cell("2026-02-01", "Wellcome"), None);
}

#[test]
fn counts_distinct_and_averages_across_the_link_table() {
    let w = world("countd");
    let r = w.q(json!({
        "measures": [{ "table": w.spending, "column": "programme_id", "agg": "count_distinct" }],
        "dimensions": [w.funder("name")]
    }));
    // Ford has programmes A and B, Gates A and C, EU C, Wellcome B, and D has no funder.
    same(
        pairs(&r),
        &[
            ("Ford", 2.0),
            ("Gates", 2.0),
            ("EU", 1.0),
            ("Wellcome", 1.0),
            (BLANK, 1.0),
        ],
    );
    let r = w.q(json!({
        "measures": [{ "table": w.spending, "column": "amount", "agg": "avg" }],
        "dimensions": [w.funder("name")]
    }));
    // Ford: rows 1, 2, 7 of A and row 3 of B, (10 + 20 + 5 + 30) / 4. Gates: A and C, (35 + 90) / 5.
    // EU and the pair written twice: rows 4 and 5, (40 + 50) / 2.
    same(
        pairs(&r),
        &[
            ("Ford", 16.25),
            ("Gates", 25.0),
            ("EU", 45.0),
            ("Wellcome", 30.0),
            (BLANK, 60.0),
        ],
    );
    // An average is not added up on this tile, and the engine says so for the first value.
    assert!(r.parts.iter().any(|p| p.overlap && p.measure == 0));
}

// ---- totals ----------------------------------------------------------------------------------

#[test]
fn a_number_tile_a_table_and_a_pivot_total_come_without_the_link_table() {
    let w = world("totals");
    // A number tile has no group.
    let kpi = run_tile(
        &w.e,
        &w.tile("kpi", json!([w.amount()]), json!([]), json!({})),
        &[],
    )
    .unwrap();
    near(kpi.kpi.unwrap().value.unwrap(), 215.0);
    assert!(kpi.parts.is_empty());
    // A table by funder, with a total row: the rows add up to 370, the total is 215.
    let t = w.tile(
        "table",
        json!([w.amount()]),
        json!([w.funder("name")]),
        json!({ "totals": true }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    near(r.totals.as_ref().unwrap().grand[0].as_f64().unwrap(), 215.0);
    let rows: f64 = r.rows.iter().map(|x| x[1].as_f64().unwrap()).sum();
    near(rows, 370.0);
    assert!(
        r.notes
            .iter()
            .any(|n| n.starts_with("A row of spending with two funders is under both."))
    );
    // A pivot of funders by month: a column total is the month's spending, each row once.
    let p = w.tile(
        "pivot",
        json!([w.amount()]),
        json!([w.funder("name")]),
        json!({ "cols": w.month() }),
    );
    let r = run_tile(&w.e, &p, &[]).unwrap();
    let totals = r.totals.as_ref().unwrap();
    near(totals.grand[0].as_f64().unwrap(), 215.0);
    let cols = totals.cols.as_ref().unwrap();
    let col = |m: &str| {
        cols.iter()
            .find(|c| snapshot::js_key(&c[0]) == m)
            .and_then(|c| c[1].as_f64())
            .unwrap()
    };
    near(col("2026-01-01"), 80.0);
    near(col("2026-02-01"), 135.0);
    // The row totals are the bars.
    let rows = totals.rows.as_ref().unwrap();
    let row = |f: &str| {
        rows.iter()
            .find(|c| snapshot::js_key(&c[0]) == f)
            .and_then(|c| c[1].as_f64())
            .unwrap()
    };
    near(row("Gates"), 125.0);
    near(row("Wellcome"), 30.0);
}

#[test]
fn every_picture_note_is_public_and_the_words_name_tables_only() {
    let w = world("public");
    let t = w.tile(
        "bar",
        json!([w.amount()]),
        json!([w.funder("name")]),
        json!({}),
    );
    let r = run_tile_public(&w.e, &t, &[]).unwrap();
    assert_eq!(
        r.notes,
        vec!["A row of spending with two funders is under both. The total counts it once."]
    );
    assert!(
        !r.notes
            .iter()
            .any(|n| n.contains("Ford") || n.contains(".csv"))
    );
    // The app and a picture say the same, once.
    let app = run_tile(&w.e, &t, &[]).unwrap();
    assert_eq!(app.notes, r.notes);
    assert_eq!(app.public_notes, r.notes);
}

// ---- filters ---------------------------------------------------------------------------------

#[test]
fn a_filter_behind_the_link_table_keeps_each_row_once() {
    let w = world("filters");
    // Ford funds A and B: rows 1, 2, 7 and 3.
    near(
        w.total(json!([w.filter("name", "in", json!(["Ford"]))])),
        65.0,
    );
    assert_eq!(w.rows(json!([w.filter("name", "in", json!(["Ford"]))])), 4);
    // Ford or Gates: A has both and is kept once, so 6 rows and 155, not 10 rows and 220.
    let both = json!([w.filter("name", "in", json!(["Ford", "Gates"]))]);
    near(w.total(both.clone()), 155.0);
    assert_eq!(w.rows(both), 6);
    // B has Ford and Wellcome: once.
    near(
        w.total(json!([w.filter("name", "in", json!(["Ford", "Wellcome"]))])),
        65.0,
    );
    // The pair written twice: EU is 90.
    near(
        w.total(json!([w.filter("name", "in", json!(["EU"]))])),
        90.0,
    );
    // Not Ford and not Wellcome: a row passes when one of its funders does. A has Gates, C has Gates and
    // EU, D has none; B has only Ford and Wellcome.
    near(
        w.total(json!([w.filter(
            "name",
            "not_in",
            json!(["Ford", "Wellcome"])
        )])),
        185.0,
    );
    // The kind of funder: Public is programme C.
    near(
        w.total(json!([w.filter("kind", "in", json!(["Public"]))])),
        90.0,
    );
    // Blank is the rows with no funder.
    near(
        w.total(json!([w.filter("name", "in", json!([null]))])),
        60.0,
    );
    // Two filters on the same funder: a funder that is Gates and Public does not exist.
    near(
        w.total(json!([
            w.filter("name", "in", json!(["Gates"])),
            w.filter("kind", "in", json!(["Public"]))
        ])),
        0.0,
    );
    // EU and Public is one funder: programme C.
    near(
        w.total(json!([
            w.filter("name", "in", json!(["EU"])),
            w.filter("kind", "in", json!(["Public"]))
        ])),
        90.0,
    );
}

#[test]
fn a_filter_on_the_field_a_chart_groups_by_leaves_only_what_it_keeps() {
    let w = world("keep-only");
    // Keep only Ford: the chart by funder has one bar.
    same(
        w.by("name", json!([w.filter("name", "in", json!(["Ford"]))])),
        &[("Ford", 65.0)],
    );
    // Exclude Ford: its bar is gone, and the others are as they were.
    same(
        w.by("name", json!([w.filter("name", "not_in", json!(["Ford"]))])),
        &[
            ("Gates", 125.0),
            ("EU", 90.0),
            ("Wellcome", 30.0),
            (BLANK, 60.0),
        ],
    );
    // A filter on another field of the same table reads the same pairs: public funders only.
    same(
        w.by("name", json!([w.filter("kind", "in", json!(["Public"]))])),
        &[("EU", 90.0)],
    );
    // Private funders, by kind: A is in once.
    same(
        w.by("kind", json!([w.filter("kind", "in", json!(["Private"]))])),
        &[("Private", 155.0)],
    );
}

#[test]
fn a_click_on_a_funder_filters_a_monthly_line() {
    let w = world("cross");
    let line = w.tile(
        "line",
        json!([w.amount()]),
        json!([w.month()]),
        json!({ "id": "line" }),
    );
    let click = |name: &str| DashFilter {
        filter: serde_json::from_value::<Filter>(w.filter("name", "in", json!([name]))).unwrap(),
        from_tile: "bars".into(),
    };
    // Gates funds A and C: January 10 + 40, February 20 + 5 + 50.
    let r = run_tile(&w.e, &line, &[click("Gates")]).unwrap();
    let rows: Vec<(String, f64)> = pairs(&r);
    same(rows, &[("2026-01-01", 50.0), ("2026-02-01", 75.0)]);
    // Without the click: January 80, February 135.
    let all = run_tile(&w.e, &line, &[]).unwrap();
    same(pairs(&all), &[("2026-01-01", 80.0), ("2026-02-01", 135.0)]);
    // The rows behind the Gates bar are five spending rows of 125 in all.
    let bars = w.tile(
        "bar",
        json!([w.amount()]),
        json!([w.funder("name")]),
        json!({}),
    );
    let rows = filters::rows_behind(
        &w.e,
        &bars,
        &[],
        Some(serde_json::from_value(w.filter("name", "in", json!(["Gates"]))).unwrap()),
    )
    .unwrap();
    assert_eq!(rows.total, 5);
    let amount = rows
        .columns
        .iter()
        .position(|c| c.label == "amount")
        .unwrap();
    let sum: f64 = rows.rows.iter().map(|r| r[amount].as_f64().unwrap()).sum();
    near(sum, 125.0);
}

#[test]
fn the_snapshot_of_a_dashboard_carries_the_engine_numbers_for_every_click() {
    let w = world("snapshot");
    let tiles = json!([
        { "id": "bars", "type": "bar", "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
          "measures": [w.amount()], "dims": [w.funder("name")] },
        { "id": "line", "type": "line", "title": "", "x": 6, "y": 0, "w": 6, "h": 4,
          "measures": [w.amount()], "dims": [w.month()] },
        { "id": "total", "type": "kpi", "title": "", "x": 0, "y": 4, "w": 6, "h": 3,
          "measures": [w.amount()], "dims": [] }
    ]);
    w.e.set_dashboards(vec![
        json!({ "id": "d1", "title": "Main", "refresh": 0, "tiles": tiles }),
    ]);
    let snap = snapshot::build(&w.e, "d1", &[]).unwrap();
    let rows = |v: &Value| -> Vec<(String, f64)> {
        let mut o: Vec<(String, f64)> = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| (snapshot::js_key(&r[0]), r[1].as_f64().unwrap()))
            .collect();
        o.sort_by(|a, b| a.0.cmp(&b.0));
        o
    };
    // The bars are the engine's, with the note and the sign that the groups overlap.
    same(
        rows(&snap["base"]["bars"]),
        &[
            ("Ford", 65.0),
            ("Gates", 125.0),
            ("EU", 90.0),
            ("Wellcome", 30.0),
            (BLANK, 60.0),
        ],
    );
    assert_eq!(snap["base"]["bars"]["parts"][0]["overlap"], json!(true));
    assert_eq!(
        snap["base"]["bars"]["notes"][0],
        "A row of spending with two funders is under both. The total counts it once."
    );
    // The total tile is the plain total, with no note.
    near(snap["base"]["total"]["rows"][0][0].as_f64().unwrap(), 215.0);
    assert!(snap["base"]["total"].get("notes").is_none());
    // A click on the Gates bar filters the line: January 50, February 75.
    same(
        rows(&snap["cross"]["bars"]["Gates"]["line"]),
        &[("2026-01-01", 50.0), ("2026-02-01", 75.0)],
    );
    // And on the EU bar: programme C only.
    same(
        rows(&snap["cross"]["bars"]["EU"]["line"]),
        &[("2026-01-01", 40.0), ("2026-02-01", 50.0)],
    );
    // The total tile under a click is the rows of that funder, once.
    near(
        snap["cross"]["bars"]["Ford"]["total"]["rows"][0][0]
            .as_f64()
            .unwrap(),
        65.0,
    );
    // No file name in what leaves the computer.
    assert!(!snap.to_string().contains(".csv"));
}

// ---- top N and other -------------------------------------------------------------------------

#[test]
fn other_is_the_rows_with_a_funder_outside_the_top_n_each_counted_once() {
    let w = world("other");
    let t = w.tile(
        "bar",
        json!([w.amount()]),
        json!([w.funder("name")]),
        json!({ "top": { "n": 2 } }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    assert_eq!(r.other, Some(true));
    let got = pairs(&r);
    // Gates 125 and EU 90 are the top two. Outside them: Ford (A and B), Wellcome (B) and the rows with
    // no funder (D). A is 35, B is 30 (once, though it has two outside funders), D is 60: 125. The two
    // bars of Ford and Wellcome would have given 65 + 30 + 60 = 155.
    same(got, &[("Gates", 125.0), ("EU", 90.0), (OTHER, 125.0)]);
    // A click on the Other bar filters the other tiles by the funders that are not in the top.
    let keys = r.other_of.clone().unwrap();
    let filter = DashFilter {
        filter: serde_json::from_value::<Filter>(
            json!({ "table": w.funders, "column": "name", "op": "not_in", "values": keys }),
        )
        .unwrap(),
        from_tile: "bars".into(),
    };
    let line = w.tile("line", json!([w.amount()]), json!([w.month()]), json!({}));
    let l = run_tile(&w.e, &line, &[filter]).unwrap();
    // A: 10 in January, 25 in February. B: 30 in January. D: 60 in February.
    same(pairs(&l), &[("2026-01-01", 40.0), ("2026-02-01", 85.0)]);
}

// ---- named measures --------------------------------------------------------------------------

fn add_measure(w: &World, name: &str, formula: &str) -> String {
    let mut list = w.e.measures();
    list.push(NamedMeasure {
        name: name.into(),
        formula: formula.into(),
        ..Default::default()
    });
    w.e.set_measures(list, false).unwrap();
    w.e.measures().last().unwrap().id.clone()
}

#[test]
fn a_named_measure_over_two_tables_is_right_for_each_funder() {
    let w = world("named");
    let id = add_measure(
        &w,
        "Spent of budget",
        "SUM([spending].[amount]) / SUM([programmes].[budget])",
    );
    let r = w.q(json!({
        "measures": [{ "named": id, "table": "", "column": null, "agg": "sum" }],
        "dimensions": [w.funder("name")]
    }));
    // Spending over the budgets of the programmes of each funder. Ford: 65 over A 100 and B 200.
    // Gates: 125 over A 100 and C 300. EU: 90 over C 300. Wellcome: 30 over B 200. No funder: 60 over D 400.
    same(
        pairs(&r),
        &[
            ("Ford", 65.0 / 300.0),
            ("Gates", 125.0 / 400.0),
            ("EU", 90.0 / 300.0),
            ("Wellcome", 30.0 / 200.0),
            (BLANK, 60.0 / 400.0),
        ],
    );
    // The whole: 215 over 1000, not the mean of the funders.
    let all =
        w.q(json!({ "measures": [{ "named": id, "table": "", "column": null, "agg": "sum" }] }));
    near(all.rows[0][0].as_f64().unwrap(), 0.215);
    // Every part reaches the funders, so no table is "not split", and the ratio does not add up.
    assert!(
        r.parts.iter().all(|p| p.overlap && !p.repeats),
        "{:?}",
        r.parts
    );
    assert!(r.parts.iter().any(|p| p.overlap));
    // Filtered by a funder, one part of the sum is filtered the way the other is: Ford's programmes.
    let f = w.q(json!({
        "measures": [{ "named": id, "table": "", "column": null, "agg": "sum" }],
        "filters": [w.filter("name", "in", json!(["Ford"]))]
    }));
    near(f.rows[0][0].as_f64().unwrap(), 65.0 / 300.0);
    // A sum of one table is one total in the fast path, and the groups still overlap.
    let sum = add_measure(&w, "Spent", "SUM([spending].[amount])");
    let s = w.q(json!({
        "measures": [{ "named": sum, "table": "", "column": null, "agg": "sum" }],
        "dimensions": [w.funder("kind")]
    }));
    same(
        pairs(&s),
        &[("Private", 155.0), ("Public", 90.0), (BLANK, 60.0)],
    );
    assert!(s.parts.iter().any(|p| p.overlap));
}

#[test]
fn a_table_that_cannot_reach_the_funders_is_not_split_by_them_and_the_words_say_so() {
    let w = world("not-split");
    // Targets for the whole company, no link: each month has one, and 700 in all.
    std::fs::write(
        w.d.join("targets.csv"),
        "month,target\n2026-01-01,300\n2026-02-01,400\n",
    )
    .unwrap();
    w.e.add_source(AddSource::File {
        path: w.d.join("targets.csv").to_string_lossy().into_owned(),
    })
    .unwrap();
    let id = add_measure(
        &w,
        "Achievement",
        "SUM([spending].[amount]) / SUM([targets].[target])",
    );
    let r = w.q(json!({
        "measures": [{ "named": id, "table": "", "column": null, "agg": "sum" }],
        "dimensions": [w.funder("name")]
    }));
    // Every funder stands against the whole 700: Ford 65 / 700, Gates 125 / 700, and so on.
    same(
        pairs(&r),
        &[
            ("Ford", 65.0 / 700.0),
            ("Gates", 125.0 / 700.0),
            ("EU", 90.0 / 700.0),
            ("Wellcome", 30.0 / 700.0),
            (BLANK, 60.0 / 700.0),
        ],
    );
    let notes: Vec<&str> = r.parts.iter().map(|p| p.note.as_str()).collect();
    assert_eq!(
        notes,
        [
            "targets is not split by name.",
            "A row of spending with two funders is under both. The total counts it once."
        ]
    );
    assert!(r.parts[0].repeats && !r.parts[0].overlap);
    assert!(r.parts[1].overlap && !r.parts[1].repeats);
    // By month, the targets follow the months and the funders follow the pairs.
    let by_month = w.q(json!({
        "measures": [{ "named": id, "table": "", "column": null, "agg": "sum" }],
        "dimensions": [w.month(), w.funder("name")]
    }));
    let cell = |m: &str, f: &str| {
        by_month
            .rows
            .iter()
            .find(|x| snapshot::js_key(&x[0]) == m && snapshot::js_key(&x[1]) == f)
            .map(|x| x[2].as_f64().unwrap())
    };
    // Gates in January: A 10 and C 40 over the 300 of January.
    near(cell("2026-01-01", "Gates").unwrap(), 50.0 / 300.0);
    near(cell("2026-02-01", "Gates").unwrap(), 75.0 / 400.0);
}

// ---- no share of a whole ---------------------------------------------------------------------

#[test]
fn a_share_of_the_whole_is_refused_with_words_and_the_value_is_shown() {
    let w = world("shares");
    let t = w.tile(
        "bar",
        json!([w.amount()]),
        json!([w.funder("name")]),
        json!({ "showAs": ["pct_total"] }),
    );
    let r = run_tile(&w.e, &t, &[]).unwrap();
    // The values are the amounts, not shares that would add up to 172 %.
    same(
        pairs(&r),
        &[
            ("Ford", 65.0),
            ("Gates", 125.0),
            ("EU", 90.0),
            ("Wellcome", 30.0),
            (BLANK, 60.0),
        ],
    );
    assert!(r.columns[1].unit.is_none());
    assert!(
        r.parts.iter().any(|p| !p.overlap
            && p.note.contains(
                "is not shown as % of total: one row can be in two groups, so its values do not add up"
            )),
        "{:?}",
        r.parts
    );
    // A running total and a change work: they do not claim to add up to a whole.
    let c = w.tile(
        "line",
        json!([w.amount()]),
        json!([w.month(), w.funder("kind")]),
        json!({ "showAs": ["change"] }),
    );
    assert!(run_tile(&w.e, &c, &[]).is_ok());
    // The same tile on the month, which is not behind a link table, shares as it always did.
    let by_month = w.tile(
        "bar",
        json!([w.amount()]),
        json!([w.month()]),
        json!({ "showAs": ["pct_total"] }),
    );
    let m = run_tile(&w.e, &by_month, &[]).unwrap();
    assert_eq!(m.columns[1].unit, Some(tenun_core::query::Unit::Percent));
    assert!(m.parts.is_empty());
}

#[test]
fn a_sankey_across_a_link_table_says_why_it_cannot_follow_a_row() {
    let w = world("sankey");
    let t = w.tile(
        "sankey",
        json!([w.amount()]),
        json!([w.funder("name"), w.funder("kind")]),
        json!({}),
    );
    let e = run_tile(&w.e, &t, &[]).unwrap_err().to_string();
    assert!(e.contains("behind a link table"), "{e}");
}

// ---- what is crossed and what is not -----------------------------------------------------------

#[test]
fn an_unmarked_table_is_not_crossed_and_marking_it_is_all_it_takes() {
    let w = world_of("unmarked", PROGRAMMES, FUNDERS, PAIRS, SPENDING);
    let q = json!({ "measures": [w.amount()], "dimensions": [w.funder("name")] });
    match w.err(q.clone()) {
        QueryError::Unreachable(a, b) => {
            assert_eq!((a.as_str(), b.as_str()), ("spending", "funders"))
        }
        other => panic!("{other:?}"),
    }
    w.mark(true);
    assert_eq!(w.q(q.clone()).rows.len(), 5);
    // Unmarked again: the same words, and the filters that need the crossing are skipped and told.
    w.mark(false);
    assert!(matches!(w.err(q), QueryError::Unreachable(..)));
    let r = w.q(json!({
        "measures": [w.amount()],
        "filters": [w.filter("name", "in", json!(["Ford"]))]
    }));
    assert_eq!(r.ignored_filters, vec![0]);
    near(r.rows[0][0].as_f64().unwrap(), 215.0);
}

#[test]
fn a_value_cannot_be_read_from_behind_a_link_table() {
    let w = world("value");
    // Counting funders for each month would count a funder once for each row of spending it funds.
    match w.err(json!({
        "measures": [w.amount(), { "table": w.funders, "column": null, "agg": "count" }],
        "dimensions": [w.month()]
    })) {
        QueryError::BehindLinkTable(t, link) => {
            assert_eq!(
                (t.as_str(), link.as_str()),
                ("funders", "programme_funders")
            );
        }
        other => panic!("{other:?}"),
    }
    // The funders as the rows of the tile, with a group of the programmes behind the link table,
    // is the same crossing the other way, and is fine.
    let r = w.q(json!({
        "measures": [{ "table": w.funders, "column": null, "agg": "count" }],
        "dimensions": [{ "table": w.programmes, "column": "name" }]
    }));
    // A has Ford and Gates, B has Ford and Wellcome, C has Gates and EU. D has no funder, so no funders.
    same(pairs(&r), &[("A", 2.0), ("B", 2.0), ("C", 2.0)]);
}

#[test]
fn a_group_of_the_link_table_itself_is_a_group_behind_it() {
    let w = world("on-the-link");
    let r = w.q(json!({
        "measures": [w.amount()],
        "dimensions": [{ "table": w.pairs, "column": "share" }]
    }));
    // A: 0.5 and 0.5, B: 0.6 and 0.4, C: 0.5 and 0.5 (twice). Share 0.5 holds A, C; 0.6 holds B; 0.4 holds B.
    same(
        pairs(&r),
        &[("0.5", 125.0), ("0.6", 30.0), ("0.4", 30.0), (BLANK, 60.0)],
    );
}

// ---- the one-row-per-key rule on both sides --------------------------------------------------------

#[test]
fn a_repeated_key_on_either_one_side_never_doubles_a_row() {
    // Programme 2 is listed twice, and funder 3 is too, after the links were made.
    let w = world("repeats");
    for (file, text) in [
        (
            "programmes.csv",
            "id,name,budget\n1,A,100\n2,B,200\n2,B again,999\n3,C,300\n4,D,400\n",
        ),
        (
            "funders.csv",
            "id,name,kind\n1,Ford,Private\n2,Gates,Private\n3,EU,Public\n3,EU old,Public\n4,Wellcome,Private\n",
        ),
    ] {
        std::fs::write(w.d.join(file), text).unwrap();
        let table = if file == "programmes.csv" {
            &w.programmes
        } else {
            &w.funders
        };
        let source =
            w.e.snapshot()
                .tables
                .iter()
                .find(|t| &t.id == table)
                .unwrap()
                .source_id
                .clone();
        w.e.load(&source);
    }
    assert!(w.e.snapshot().errors.is_empty());
    // The first row of each key is used: EU is EU, and B is B. Nothing is doubled.
    same(
        w.by("name", json!([])),
        &[
            ("Ford", 65.0),
            ("Gates", 125.0),
            ("EU", 90.0),
            ("Wellcome", 30.0),
            (BLANK, 60.0),
        ],
    );
    near(w.total(json!([])), 215.0);
    near(
        w.total(json!([w.filter("name", "in", json!(["EU"]))])),
        90.0,
    );
    // The tile says what the data did, with the keys in the app and without them outside.
    let t = w.tile(
        "bar",
        json!([w.amount()]),
        json!([w.funder("name")]),
        json!({}),
    );
    let app = run_tile(&w.e, &t, &[]).unwrap();
    assert!(
        app.notes
            .iter()
            .any(|n| n.contains("repeats id") || n.contains("rows with id")),
        "{:?}",
        app.notes
    );
    let public = run_tile_public(&w.e, &t, &[]).unwrap();
    assert!(
        public
            .notes
            .iter()
            .any(|n| n.contains("The funders table repeats"))
    );
    assert!(
        !public
            .notes
            .iter()
            .any(|n| n.contains(".csv") || n.contains("EU old"))
    );
}

// ---- links and the file ------------------------------------------------------------------------

#[test]
fn the_mark_is_in_the_file_only_when_it_is_set() {
    let w = world_of("file", PROGRAMMES, FUNDERS, PAIRS, SPENDING);
    let path = w.d.join("w.tenun");
    w.e.save(Some(path.clone())).unwrap();
    let plain = std::fs::read_to_string(&path).unwrap();
    assert!(!plain.contains("linkTable"), "{plain}");
    w.mark(true);
    w.e.save(Some(path.clone())).unwrap();
    let marked = std::fs::read_to_string(&path).unwrap();
    assert!(marked.contains("\"linkTable\": true"), "{marked}");
    w.mark(false);
    w.e.save(Some(path.clone())).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), plain);
}

#[test]
fn a_changed_mark_tells_the_tiles_to_ask_again() {
    let w = world_of("changed", PROGRAMMES, FUNDERS, PAIRS, SPENDING);
    let on = TableSetup {
        link_table: true,
        ..Default::default()
    };
    assert!(w.e.set_table_setup(&w.pairs, Some(on.clone())).unwrap());
    // The same setup again changes nothing.
    assert!(!w.e.set_table_setup(&w.pairs, Some(on)).unwrap());
    assert!(w.e.set_table_setup(&w.pairs, None).unwrap());
}
