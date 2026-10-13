//! Filter ops, filter tiles, saved filters, the dashboard grain and "Show rows". The expected
//! numbers are worked out by hand from the rows below. "Today" is fixed with `Engine::set_today`.

use chrono::NaiveDate;
use serde_json::{Value, json};
use std::path::PathBuf;
use tenun_core::Engine;
use tenun_core::filters::{self, rows_behind};
use tenun_core::query::{Filter, FilterOp, SemanticQuery};
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile};
use tenun_core::workspace::{FieldRef, Relationship, Source, SourceKind, Workspace};

/// `region` is empty on two rows. 2026-03-15 is a Sunday.
const SALES: &str = "date,ts,product,region,store,qty,amount
2026-01-05,2026-01-05 09:30:00,Arabica,North,1,1,100
2026-01-20,2026-01-20 09:10:00,Robusta,South,2,2,50
2026-03-10,2026-03-10 14:00:00,Arabica 50%,North,1,3,200
2026-03-15,2026-03-15 14:45:00,Liberica_X,,2,4,30
2026-04-01,2026-04-01 23:59:00,Excelsa,South,1,5,10
2026-10-14,2026-10-14 23:30:00,Geisha,North,1,6,20
2026-10-14,2026-10-14 08:00:00,Kopi,\"\",2,7,15
";

const STORES: &str = "id,city\n1,Jakarta\n2,Bandung\n";

fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn tile_json(id: &str, ty: &str, measures: Value, dims: Value, extra: Value) -> Value {
    let mut t = json!({ "id": id, "type": ty, "title": "", "x": 0, "y": 0, "w": 6, "h": 4,
                        "measures": measures, "dims": dims });
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        t[k] = v;
    }
    t
}

fn sum_amount() -> Value {
    json!([{ "table": "s1/sales", "column": "amount", "agg": "sum" }])
}

fn dim(col: &str) -> Value {
    json!({ "table": "s1/sales", "column": col })
}

fn engine(tag: &str, dashboard: Value) -> Engine {
    let dir: PathBuf = std::env::temp_dir().join(format!("tenun-fl-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sales.csv"), SALES).unwrap();
    std::fs::write(dir.join("stores.csv"), STORES).unwrap();
    let mut big = String::from("n,v\n");
    for i in 0..600 {
        big.push_str(&format!("{i},{}\n", i % 7));
    }
    std::fs::write(dir.join("big.csv"), big).unwrap();
    let source = |id: &str, file: &str| Source {
        id: id.into(),
        name: file.into(),
        kind: SourceKind::Csv { path: file.into() },
    };
    let ws = Workspace {
        name: "Filters".into(),
        sources: vec![
            source("s1", "sales.csv"),
            source("s2", "stores.csv"),
            source("s3", "big.csv"),
        ],
        relationships: vec![Relationship {
            id: "r1".into(),
            from: FieldRef {
                table: "s1/sales".into(),
                column: "store".into(),
            },
            to: FieldRef {
                table: "s2/stores".into(),
                column: "id".into(),
            },
            name: None,
        }],
        dashboards: vec![dashboard],
        ..Workspace::default()
    };
    let e = Engine::new(Secrets::new(dir.join("secrets.json"))).unwrap();
    e.replace(ws, Some(dir.join("F.tenun")));
    assert!(e.snapshot().errors.is_empty(), "{:?}", e.snapshot().errors);
    e
}

fn plain(tag: &str) -> Engine {
    engine(
        tag,
        json!({ "id": "d1", "title": "T", "refresh": 0, "tiles": [] }),
    )
}

fn filter(column: &str, op: FilterOp, values: Value) -> Filter {
    Filter {
        table: "s1/sales".into(),
        column: column.into(),
        grain: None,
        op,
        values: values.as_array().cloned().unwrap_or_default(),
        via: None,
    }
}

/// The total amount of the rows that pass these filters.
fn amount(e: &Engine, filters: Vec<Filter>) -> f64 {
    let r = e
        .query(&SemanticQuery {
            measures: serde_json::from_value(sum_amount()).unwrap(),
            dimensions: vec![],
            filters,
            sort: vec![],
            limit: None,
        })
        .unwrap();
    r.rows[0][0].as_f64().unwrap_or(0.0)
}

fn count(e: &Engine, filters: Vec<Filter>) -> f64 {
    let r = e
        .query(&SemanticQuery {
            measures: serde_json::from_value(
                json!([{ "table": "s1/sales", "column": null, "agg": "count" }]),
            )
            .unwrap(),
            dimensions: vec![],
            filters,
            sort: vec![],
            limit: None,
        })
        .unwrap();
    r.rows[0][0].as_f64().unwrap()
}

// ---- the new ops ---------------------------------------------------------------------------

#[test]
fn contains_and_starts_with_ignore_case() {
    let e = plain("like");
    // Arabica, Arabica 50%, and no other product has "ARAB" in it.
    assert_eq!(
        amount(
            &e,
            vec![filter("product", FilterOp::Contains, json!(["ARAB"]))]
        ),
        300.0
    );
    assert_eq!(
        amount(
            &e,
            vec![filter("product", FilterOp::StartsWith, json!(["arab"]))]
        ),
        300.0
    );
    // "ica" is inside Arabica, Arabica 50% and Liberica_X, but only Liberica_X ends with it.
    assert_eq!(
        amount(
            &e,
            vec![filter("product", FilterOp::Contains, json!(["ica"]))]
        ),
        330.0
    );
    assert_eq!(
        amount(
            &e,
            vec![filter("product", FilterOp::StartsWith, json!(["ica"]))]
        ),
        0.0
    );
    // A number column can be searched as text.
    assert_eq!(
        count(&e, vec![filter("qty", FilterOp::Contains, json!([1]))]),
        1.0
    );
}

#[test]
fn percent_and_underscore_are_plain_characters() {
    let e = plain("wild");
    // "%" alone would match every row as a wildcard; as text it matches the one product with "%".
    assert_eq!(
        amount(
            &e,
            vec![filter("product", FilterOp::Contains, json!(["%"]))]
        ),
        200.0
    );
    // "_" would match any character; as text it matches only "Liberica_X".
    assert_eq!(
        amount(
            &e,
            vec![filter("product", FilterOp::Contains, json!(["_"]))]
        ),
        30.0
    );
    assert_eq!(
        amount(
            &e,
            vec![filter(
                "product",
                FilterOp::StartsWith,
                json!(["Arabica 5_%"])
            )]
        ),
        0.0
    );
    assert_eq!(
        amount(
            &e,
            vec![filter(
                "product",
                FilterOp::StartsWith,
                json!(["Arabica 50%"])
            )]
        ),
        200.0
    );
    // A backslash is a plain character too.
    assert_eq!(
        amount(
            &e,
            vec![filter("product", FilterOp::Contains, json!(["\\"]))]
        ),
        0.0
    );
}

#[test]
fn a_hostile_value_is_only_text() {
    let e = plain("hostile");
    for bad in [
        "'; DROP TABLE sales; --",
        "%' OR '1'='1",
        "\" OR 1=1 --",
        "x') UNION SELECT 1 --",
        "\\' OR ''='",
    ] {
        for op in [FilterOp::Contains, FilterOp::StartsWith] {
            assert_eq!(
                amount(&e, vec![filter("product", op, json!([bad]))]),
                0.0,
                "{bad}"
            );
        }
    }
    // The table is still there and nothing leaked.
    assert_eq!(count(&e, vec![]), 7.0);
}

#[test]
fn blank_text_is_null_or_empty() {
    let e = plain("blank");
    // Two rows have no region (the CSV reader makes both NULL).
    assert_eq!(
        count(&e, vec![filter("region", FilterOp::Blank, json!([]))]),
        2.0
    );
    assert_eq!(
        count(&e, vec![filter("region", FilterOp::NotBlank, json!([]))]),
        5.0
    );
    // A number column is blank only when NULL: none of the quantities are.
    assert_eq!(
        count(&e, vec![filter("qty", FilterOp::Blank, json!([]))]),
        0.0
    );
    assert_eq!(
        count(&e, vec![filter("qty", FilterOp::NotBlank, json!([]))]),
        7.0
    );
}

#[test]
fn blank_on_a_date_is_null_only() {
    let e = plain("blankdate");
    assert_eq!(
        count(&e, vec![filter("date", FilterOp::Blank, json!([]))]),
        0.0
    );
    assert_eq!(
        count(&e, vec![filter("date", FilterOp::NotBlank, json!([]))]),
        7.0
    );
}

#[test]
fn relative_presets_count_from_the_fixed_day() {
    use tenun_core::query::relative_range;
    let e = plain("rel");
    // Thursday 2026-03-19: the 15th (30) and the 10th (200) are inside some windows.
    e.set_today(Some(day("2026-03-19")));
    let expect = [
        ("last_7_days", 30.0),   // Mar 13 to Mar 19
        ("last_30_days", 230.0), // Feb 18 to Mar 19
        ("this_month", 230.0),   // Mar 1 to Mar 19
        ("last_month", 0.0),     // February
        ("this_quarter", 380.0), // Jan 1 to Mar 19
        ("last_quarter", 0.0),   // Oct to Dec 2025
        ("ytd", 380.0),
        ("last_12_months", 380.0), // Mar 20 2025 to Mar 19 2026
    ];
    for (preset, want) in expect {
        for column in ["date", "ts"] {
            let got = amount(
                &e,
                vec![filter(column, FilterOp::Relative, json!([preset]))],
            );
            assert_eq!(got, want, "{preset} on {column}");
        }
    }
    // The ranges themselves are half open and end the day after today.
    assert_eq!(
        relative_range("last_7_days", day("2026-03-19")),
        Some((day("2026-03-13"), day("2026-03-20")))
    );
}

#[test]
fn a_date_time_late_on_the_last_day_is_inside() {
    let e = plain("endofday");
    // 2026-10-14 23:30 is the last moment of its day. Today is that day: both rows count.
    e.set_today(Some(day("2026-10-14")));
    assert_eq!(
        amount(
            &e,
            vec![filter("ts", FilterOp::Relative, json!(["last_7_days"]))]
        ),
        35.0
    );
    assert_eq!(
        amount(
            &e,
            vec![filter("ts", FilterOp::Relative, json!(["this_month"]))]
        ),
        35.0
    );
    // The week still holds the day until its seventh day is over.
    e.set_today(Some(day("2026-10-15")));
    assert_eq!(
        amount(
            &e,
            vec![filter("ts", FilterOp::Relative, json!(["last_7_days"]))]
        ),
        35.0
    );
    e.set_today(Some(day("2026-10-20")));
    assert_eq!(
        amount(
            &e,
            vec![filter("ts", FilterOp::Relative, json!(["last_7_days"]))]
        ),
        35.0
    );
    e.set_today(Some(day("2026-10-21")));
    assert_eq!(
        amount(
            &e,
            vec![filter("ts", FilterOp::Relative, json!(["last_7_days"]))]
        ),
        0.0
    );
}

#[test]
fn relative_needs_a_date_column_and_a_known_preset() {
    let e = plain("relbad");
    let err = |f: Filter| {
        e.query(&SemanticQuery {
            measures: serde_json::from_value(sum_amount()).unwrap(),
            dimensions: vec![],
            filters: vec![f],
            sort: vec![],
            limit: None,
        })
        .unwrap_err()
        .to_string()
    };
    assert!(err(filter("product", FilterOp::Relative, json!(["ytd"]))).contains("date column"));
    assert!(
        err(filter("date", FilterOp::Relative, json!(["next_week"]))).contains("does not match")
    );
}

// ---- filter tiles, saved filters and the grain in build() -----------------------------------

fn dashboard(extra: Value, tiles: Vec<Value>) -> Value {
    let mut d = json!({ "id": "d1", "title": "T", "refresh": 0, "tiles": tiles });
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        d[k] = v;
    }
    d
}

fn by_product() -> Value {
    tile_json(
        "bp",
        "bar",
        sum_amount(),
        json!([dim("product")]),
        json!({}),
    )
}

fn region_tile(selection: Value) -> Value {
    tile_json(
        "fr",
        "filter",
        json!([]),
        json!([dim("region")]),
        json!({ "filter": selection }),
    )
}

fn total_of(payload: &Value, tile: &str) -> f64 {
    payload["base"][tile]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r[1].as_f64().unwrap())
        .sum()
}

#[test]
fn a_list_filter_tile_filters_every_other_tile() {
    let d = dashboard(
        json!({}),
        vec![
            by_product(),
            tile_json("k", "kpi", sum_amount(), json!([]), json!({})),
            region_tile(json!({ "style": "list", "values": ["North"] })),
        ],
    );
    let e = engine("tilelist", d);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    // North: 100 + 200 + 20.
    assert_eq!(total_of(&p, "bp"), 320.0);
    assert_eq!(p["base"]["k"]["rows"][0][0].as_f64(), Some(320.0));
    // The filter tile itself is not filtered by its own selection: it offers every region.
    let opts = p["base"]["fr"]["rows"].as_array().unwrap();
    let count_of = |v: Value| {
        opts.iter()
            .find(|r| r[0] == v)
            .map(|r| r[1].as_f64().unwrap())
    };
    assert_eq!(count_of(json!("North")), Some(3.0));
    assert_eq!(count_of(json!("South")), Some(2.0));
    // NULL and empty text are one "blank" row.
    assert_eq!(count_of(Value::Null), Some(2.0));
    assert_eq!(opts[0][1].as_f64(), Some(3.0));
    // The tile stays in the file for the viewer to draw.
    assert_eq!(p["tiles"].as_array().unwrap().len(), 3);
    // A click on a value offline: every other tile has a result with that region picked.
    let south = &p["cross"]["fr"]["South"];
    assert_eq!(
        south["bp"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r[1].as_f64().unwrap())
            .sum::<f64>(),
        60.0
    );
    assert!(south.get("fr").is_none());
    // "(blank)" matches NULL and empty text.
    let blank = &p["cross"]["fr"]["\u{0}null"];
    assert_eq!(
        blank["bp"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r[1].as_f64().unwrap())
            .sum::<f64>(),
        45.0
    );
}

#[test]
fn a_filter_tile_with_nothing_selected_keeps_everything() {
    let d = dashboard(
        json!({}),
        vec![by_product(), region_tile(json!({ "style": "list" }))],
    );
    let e = engine("tilenone", d);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    assert_eq!(total_of(&p, "bp"), 425.0);
}

#[test]
fn range_and_date_filter_tiles() {
    let qty = json!({ "table": "s1/sales", "column": "qty" });
    let d = dashboard(
        json!({}),
        vec![
            by_product(),
            tile_json(
                "fq",
                "filter",
                json!([]),
                json!([qty]),
                json!({ "filter": { "style": "range", "range": [3, 5] } }),
            ),
        ],
    );
    let e = engine("tilerange", d);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    // qty 3, 4, 5 are inclusive: 200 + 30 + 10.
    assert_eq!(total_of(&p, "bp"), 240.0);
    // The slider's histogram: 24 bins from 1 to 7, counts add up to the 7 rows.
    let bins = &p["base"]["fq"];
    assert_eq!(bins["rows"].as_array().unwrap().len(), 24);
    assert_eq!(bins["bins"]["start"].as_f64(), Some(1.0));
    assert_eq!(bins["bins"]["width"].as_f64(), Some(0.25));
    let total: f64 = bins["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r[1].as_f64().unwrap())
        .sum();
    assert_eq!(total, 7.0);

    // Dates: a preset, then a custom span whose last day is kept in full.
    let ts = json!({ "table": "s1/sales", "column": "ts" });
    for (sel, want) in [
        (json!({ "style": "dates", "preset": "this_month" }), 35.0),
        (
            json!({ "style": "dates", "range": ["2026-03-10", "2026-03-15"] }),
            230.0,
        ),
        (
            json!({ "style": "dates", "range": ["2026-10-14", "2026-10-14"] }),
            35.0,
        ),
        (
            json!({ "style": "dates", "range": ["2026-04-01", null] }),
            45.0,
        ),
        (
            json!({ "style": "dates", "range": [null, "2026-01-20"] }),
            150.0,
        ),
    ] {
        let d = dashboard(
            json!({}),
            vec![
                by_product(),
                tile_json(
                    "fd",
                    "filter",
                    json!([]),
                    json!([ts]),
                    json!({ "filter": sel }),
                ),
            ],
        );
        let e = engine("tiledates", d);
        e.set_today(Some(day("2026-10-14")));
        let p = snapshot::build(&e, "d1", &[]).unwrap();
        assert_eq!(total_of(&p, "bp"), want, "{sel}");
        // The tile offers the first and last day of the data.
        let r = &p["base"]["fd"]["rows"][0];
        assert_eq!(r[0], json!("2026-01-05T09:30:00"));
        assert_eq!(r[1], json!("2026-10-14T23:30:00"));
    }
}

#[test]
fn saved_filters_apply_to_every_tile_including_the_source() {
    let saved = json!({ "fromTile": "pin1", "table": "s1/sales", "column": "product", "op": "in", "values": ["Arabica"], "label": null });
    let d = dashboard(json!({ "filters": [saved] }), vec![by_product()]);
    let e = engine("saved", d);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    // The bar chart that the filter came from still only shows Arabica.
    let rows = p["base"]["bp"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][1].as_f64(), Some(100.0));
    assert_eq!(p["saved"].as_array().unwrap().len(), 1);
    assert_eq!(p["filters"].as_array().unwrap().len(), 0);
    // A cross-filter from a click still works beside it, and the saved one still holds.
    let click = DashFilter {
        filter: filter("region", FilterOp::In, json!(["South"])),
        from_tile: "other".into(),
        to: None,
    };
    let p = snapshot::build(&e, "d1", &[click]).unwrap();
    assert_eq!(p["base"]["bp"]["rows"].as_array().unwrap().len(), 0);
    assert_eq!(p["filters"].as_array().unwrap().len(), 1);
    // Exclude: the same filter as `not_in`.
    let out = json!({ "fromTile": "pin2", "table": "s1/sales", "column": "product", "op": "not_in", "values": ["Arabica", "Robusta"] });
    let e = engine(
        "saved2",
        dashboard(json!({ "filters": [out] }), vec![by_product()]),
    );
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    assert_eq!(total_of(&p, "bp"), 275.0);
}

#[test]
fn one_grain_for_every_time_tile() {
    let monthly = |id: &str| {
        tile_json(
            id,
            "line",
            sum_amount(),
            json!([{ "table": "s1/sales", "column": "date", "grain": "month" }]),
            json!({}),
        )
    };
    let part = tile_json(
        "wd",
        "bar",
        sum_amount(),
        json!([{ "table": "s1/sales", "column": "date", "grain": "weekday" }]),
        json!({}),
    );
    let by_text = by_product();
    let d = dashboard(
        json!({ "grain": "year" }),
        vec![monthly("m1"), part, by_text],
    );
    let e = engine("grain", d);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    // The month line is a year line now: one point for 2026.
    let rows = p["base"]["m1"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], json!("2026-01-01"));
    assert_eq!(rows[0][1].as_f64(), Some(425.0));
    // The file says the same, so the viewer names the periods right.
    assert_eq!(p["tiles"][0]["dims"][0]["grain"], json!("year"));
    // A date part and a text group keep what they had.
    assert_eq!(p["tiles"][1]["dims"][0]["grain"], json!("weekday"));
    assert!(p["tiles"][2]["dims"][0].get("grain").is_none());
    assert_eq!(p["grain"], json!("year"));
    // "Auto" (no grain) leaves the tile as it is.
    let e = engine("grain0", dashboard(json!({}), vec![monthly("m1")]));
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    assert!(p["base"]["m1"]["rows"].as_array().unwrap().len() > 1);
    assert_eq!(p["tiles"][0]["dims"][0]["grain"], json!("month"));
    // A date part is not a grain a dashboard can ask for.
    let d = dashboard(json!({ "grain": "weekday" }), vec![monthly("m1")]);
    let e = engine("grain1", d);
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    assert_eq!(p["tiles"][0]["dims"][0]["grain"], json!("month"));
}

#[test]
fn filter_tiles_and_the_grain_help_with_the_comparison_year() {
    // A relative preset on the date moves back with the comparison like any other date filter.
    let line = tile_json(
        "ln",
        "line",
        sum_amount(),
        json!([{ "table": "s1/sales", "column": "date", "grain": "month" }]),
        json!({ "compare": "prev_year" }),
    );
    let fd = tile_json(
        "fd",
        "filter",
        json!([]),
        json!([dim("date")]),
        json!({ "filter": { "style": "dates", "preset": "ytd" } }),
    );
    let e = engine("cmp", dashboard(json!({}), vec![line, fd]));
    e.set_today(Some(day("2026-03-31")));
    let p = snapshot::build(&e, "d1", &[]).unwrap();
    // January and March of 2026 are on show.
    let keys: Vec<&str> = p["base"]["ln"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r[0].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["2026-01-01", "2026-02-01", "2026-03-01"]);
    // The comparison year has no data, but it ran with the moved window and did not fail.
    assert!(
        p["base"]["ln"]["compare"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r[1].is_null() || r[1].as_f64() == Some(0.0))
    );
}

// ---- options, run_tile and the rows behind a mark -----------------------------------------

#[test]
fn list_options_are_biggest_first_with_counts() {
    let t: Tile = serde_json::from_value(region_tile(json!({ "style": "dropdown" }))).unwrap();
    let e = plain("opts");
    let r = run_tile(&e, &t, &[]).unwrap();
    assert_eq!(r.rows[0][0], json!("North"));
    assert_eq!(r.rows[0][1].as_f64(), Some(3.0));
    // Another filter narrows the counts; the tile's own selection does not.
    let own = DashFilter {
        filter: filter("region", FilterOp::In, json!(["South"])),
        from_tile: "fr".into(),
        to: None,
    };
    let other = DashFilter {
        filter: filter("date", FilterOp::Gte, json!(["2026-03-01"])),
        from_tile: "k".into(),
        to: None,
    };
    let r = run_tile(&e, &t, &[own, other]).unwrap();
    let total: f64 = r.rows.iter().map(|r| r[1].as_f64().unwrap()).sum();
    assert_eq!(total, 5.0);
    assert!(filters::options(&e, &t, &[]).unwrap().rows.len() <= 200);
}

fn click(column: &str, v: &str) -> Option<Filter> {
    Some(filter(column, FilterOp::In, json!([v])))
}

#[test]
fn rows_behind_a_mark() {
    let e = plain("rows");
    let bar: Tile = serde_json::from_value(by_product()).unwrap();
    let r = rows_behind(&e, &bar, &[], click("product", "Arabica")).unwrap();
    assert_eq!(r.total, 1);
    assert_eq!(r.rows.len(), 1);
    let names: Vec<&str> = r.columns.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(
        names,
        ["date", "ts", "product", "region", "store", "qty", "amount"]
    );
    let amount_at = names.iter().position(|n| *n == "amount").unwrap();
    assert_eq!(r.rows[0][amount_at].as_f64(), Some(100.0));
    // Without a click: every row of the tile.
    assert_eq!(rows_behind(&e, &bar, &[], None).unwrap().total, 7);
    // The tile's filters count too, but not its own cross-filter.
    let f = |from: &str| DashFilter {
        filter: filter("region", FilterOp::In, json!(["North"])),
        from_tile: from.into(),
        to: None,
    };
    assert_eq!(rows_behind(&e, &bar, &[f("x")], None).unwrap().total, 3);
    assert_eq!(rows_behind(&e, &bar, &[f("bp")], None).unwrap().total, 7);
}

#[test]
fn rows_behind_include_columns_of_related_tables() {
    let e = plain("rowsrel");
    let city = json!({ "table": "s2/stores", "column": "city" });
    let t: Tile = serde_json::from_value(tile_json(
        "bc",
        "bar",
        sum_amount(),
        json!([city]),
        json!({}),
    ))
    .unwrap();
    let c = Some(Filter {
        table: "s2/stores".into(),
        column: "city".into(),
        grain: None,
        op: FilterOp::In,
        values: vec![json!("Bandung")],
        via: None,
    });
    let r = rows_behind(&e, &t, &[], c).unwrap();
    // Store 2: Robusta, Liberica_X and Kopi.
    assert_eq!(r.total, 3);
    let last = r.columns.last().unwrap();
    assert_eq!(last.label, "stores · city");
    assert!(
        r.rows
            .iter()
            .all(|row| row.last().unwrap() == &json!("Bandung"))
    );
}

#[test]
fn rows_behind_stop_at_500_and_say_how_many_match() {
    let e = plain("rowsbig");
    let t: Tile = serde_json::from_value(tile_json(
        "big",
        "bar",
        json!([{ "table": "s3/big", "column": "n", "agg": "sum" }]),
        json!([{ "table": "s3/big", "column": "v" }]),
        json!({}),
    ))
    .unwrap();
    let r = rows_behind(&e, &t, &[], None).unwrap();
    assert_eq!(r.total, 600);
    assert_eq!(r.rows.len(), 500);
    // A histogram bin is a range click.
    let h: Tile = serde_json::from_value(tile_json(
        "h",
        "histogram",
        json!([{ "table": "s3/big", "column": "n", "agg": "count" }]),
        json!([]),
        json!({}),
    ))
    .unwrap();
    let c = Some(Filter {
        table: "s3/big".into(),
        column: "n".into(),
        grain: None,
        op: FilterOp::Range,
        values: vec![json!(0), json!(100)],
        via: None,
    });
    assert_eq!(rows_behind(&e, &h, &[], c).unwrap().total, 100);
}
