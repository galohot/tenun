//! The workspace calendar (v5 build 04, V5-D6): a year that starts in April, a week that starts on Sunday, and the
//! formulas FISCALYEAR and FISCALQUARTER. With the default calendar nothing changes.
//!
//! The data, by hand: Mar 31 2026 A 10 · Apr 1 2026 A 5 · Jun 15 2026 B 7 · Jul 1 2026 B 4 · Mar 31 2027 C 1 ·
//! Apr 2 2027 C 2. By calendar year: 2026 = 26, 2027 = 3. With an April start: FY 2026 = 10, FY 2027 = 17,
//! FY 2028 = 2; Q4 FY 2026 (Jan–Mar 2026) = 10, Q1 FY 2027 = 12, Q2 FY 2027 = 4, Q4 FY 2027 = 1, Q1 FY 2028 = 2.

use chrono::NaiveDate;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tenun_core::Engine;
use tenun_core::calendar::Calendar;
use tenun_core::engine::AddSource;
use tenun_core::query::SemanticQuery;
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, Tile};
use tenun_core::workspace::{TableSetup, Workspace};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tenun-cal-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn data(d: &Path) -> (Engine, String) {
    std::fs::write(
        d.join("orders.csv"),
        "date,store,amount\n2026-03-31,A,10\n2026-04-01,A,5\n2026-06-15,B,7\n2026-07-01,B,4\n2027-03-31,C,1\n2027-04-02,C,2\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    e.replace(
        Workspace {
            name: "Calendar".into(),
            ..Workspace::default()
        },
        Some(d.join("Calendar.tenun")),
    );
    let id = e
        .add_source(AddSource::File {
            path: d.join("orders.csv").to_string_lossy().into_owned(),
        })
        .unwrap();
    let t = e
        .snapshot()
        .tables
        .iter()
        .find(|t| t.source_id == id)
        .unwrap()
        .id
        .clone();
    (e, t)
}

fn by(e: &Engine, t: &str, grain: &str, filters: Value) -> Vec<(Value, f64)> {
    let q: SemanticQuery = serde_json::from_value(json!({
        "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
        "dimensions": [{ "table": t, "column": "date", "grain": grain }],
        "filters": filters, "sort": [{ "by": 0 }]
    }))
    .unwrap();
    e.query(&q)
        .unwrap()
        .rows
        .into_iter()
        .map(|r| (r[0].clone(), r[1].as_f64().unwrap()))
        .collect()
}

const APRIL: Calendar = Calendar {
    year_start: 4,
    week_start: 1,
};

#[test]
fn a_year_that_starts_in_april_groups_and_names_its_years_and_quarters() {
    let (e, t) = data(&dir("april"));
    assert_eq!(
        by(&e, &t, "year", json!([])),
        [(json!("2026-01-01"), 26.0), (json!("2027-01-01"), 3.0)]
    );
    e.set_calendar(APRIL).unwrap();
    assert_eq!(
        by(&e, &t, "year", json!([])),
        [
            (json!("2025-04-01"), 10.0),
            (json!("2026-04-01"), 17.0),
            (json!("2027-04-01"), 2.0)
        ]
    );
    assert_eq!(
        by(&e, &t, "quarter", json!([])),
        [
            (json!("2026-01-01"), 10.0),
            (json!("2026-04-01"), 12.0),
            (json!("2026-07-01"), 4.0),
            (json!("2027-01-01"), 1.0),
            (json!("2027-04-01"), 2.0)
        ]
    );
    // A number tile by quarter names its period in the workspace's calendar.
    let k: Tile = serde_json::from_value(json!({ "id": "k", "type": "kpi",
        "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
        "dims": [{ "table": t, "column": "date", "grain": "quarter" }] }))
    .unwrap();
    // Up to March 2027, so the last quarter, Jan to Mar 2027, is finished: Q4 FY 2027, after Q3 FY 2027.
    let upto: Vec<snapshot::DashFilter> =
        serde_json::from_value(json!([{ "table": t, "column": "date",
        "op": "range", "values": ["2026-01-01", "2027-04-01"], "fromTile": "f" }]))
        .unwrap();
    let kpi = snapshot::run_tile(&e, &k, &upto).unwrap().kpi.unwrap();
    assert_eq!(kpi.period, "Q4 FY 2027");
    assert_eq!(kpi.versus.as_deref(), Some("Q3 FY 2027"));
    // With the data to April 2, the quarter is not over: the number says which days, as always.
    let kpi = snapshot::run_tile(&e, &k, &[]).unwrap().kpi.unwrap();
    assert_eq!(kpi.period, "Apr 1–2, 2027");
    // The calendar is saved with the workspace and comes back.
    e.save(None).unwrap();
    let back = Workspace::load(&e.workspace_path().unwrap()).unwrap();
    assert_eq!(back.calendar, Some(APRIL));
    // An export carries it, so the viewer names periods the same way.
    e.set_dashboards(vec![
        json!({ "id": "d1", "title": "C", "refresh": 0, "tiles": [] }),
    ]);
    assert_eq!(
        snapshot::build(&e, "d1", &[]).unwrap()["calendar"],
        json!({ "yearStart": 4, "weekStart": 1 })
    );
}

#[test]
fn year_to_date_and_this_quarter_start_with_the_year() {
    let (e, t) = data(&dir("ytd"));
    e.set_today(Some(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()));
    let ytd = json!([{ "table": t, "column": "date", "op": "relative", "values": ["ytd"] }]);
    let total = |e: &Engine| {
        by(e, &t, "year", ytd.clone())
            .iter()
            .map(|r| r.1)
            .sum::<f64>()
    };
    assert_eq!(total(&e), 22.0, "Jan 1 to Jun 15, 2026");
    e.set_calendar(APRIL).unwrap();
    assert_eq!(total(&e), 12.0, "Apr 1 to Jun 15, 2026");
    let q = json!([{ "table": t, "column": "date", "op": "relative", "values": ["last_quarter"] }]);
    assert_eq!(
        by(&e, &t, "day", q),
        [(json!("2026-03-31"), 10.0)],
        "Jan to Mar 2026"
    );
}

#[test]
fn a_week_that_starts_on_sunday() {
    let d = dir("week");
    // Saturday Oct 10, Sunday Oct 11, Monday Oct 12, 2026.
    std::fs::write(
        d.join("days.csv"),
        "date,amount\n2026-10-10,1\n2026-10-11,2\n2026-10-12,4\n",
    )
    .unwrap();
    let e = Engine::new(Secrets::new(d.join("secrets.json"))).unwrap();
    let id = e
        .add_source(AddSource::File {
            path: d.join("days.csv").to_string_lossy().into_owned(),
        })
        .unwrap();
    let t = e
        .snapshot()
        .tables
        .iter()
        .find(|t| t.source_id == id)
        .unwrap()
        .id
        .clone();
    assert_eq!(
        by(&e, &t, "week", json!([])),
        [(json!("2026-10-05"), 3.0), (json!("2026-10-12"), 4.0)]
    );
    e.set_calendar(Calendar {
        year_start: 1,
        week_start: 7,
    })
    .unwrap();
    assert_eq!(
        by(&e, &t, "week", json!([])),
        [(json!("2026-10-04"), 1.0), (json!("2026-10-11"), 6.0)]
    );
    e.set_calendar(Calendar {
        year_start: 1,
        week_start: 6,
    })
    .unwrap();
    assert_eq!(by(&e, &t, "week", json!([])), [(json!("2026-10-10"), 7.0)]);
}

#[test]
fn fiscalyear_and_fiscalquarter_follow_the_calendar_without_a_reload() {
    let (e, t) = data(&dir("formula"));
    let setup: TableSetup = serde_json::from_value(json!({ "calc": [
        { "name": "fy", "formula": "FISCALYEAR([date])" },
        { "name": "fq", "formula": "FISCALQUARTER([date])" }
    ] }))
    .unwrap();
    e.set_table_setup(&t, Some(setup)).unwrap();
    let group = |e: &Engine, col: &str| -> Vec<(Value, f64)> {
        let q: SemanticQuery = serde_json::from_value(json!({
            "measures": [{ "table": t, "column": "amount", "agg": "sum" }],
            "dimensions": [{ "table": t, "column": col }], "sort": [{ "by": 0 }]
        }))
        .unwrap();
        e.query(&q)
            .unwrap()
            .rows
            .into_iter()
            .map(|r| (r[0].clone(), r[1].as_f64().unwrap()))
            .collect()
    };
    assert_eq!(
        group(&e, "fy"),
        [(json!(2026.0), 26.0), (json!(2027.0), 3.0)]
    );
    e.set_calendar(APRIL).unwrap();
    assert_eq!(
        group(&e, "fy"),
        [
            (json!(2026.0), 10.0),
            (json!(2027.0), 17.0),
            (json!(2028.0), 2.0)
        ]
    );
    assert_eq!(
        group(&e, "fq"),
        [(json!(1.0), 14.0), (json!(2.0), 4.0), (json!(4.0), 11.0)]
    );
}
