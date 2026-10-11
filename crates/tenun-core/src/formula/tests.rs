//! Tests of the whole formula path: text in, values out of a real DuckDB.
//!
//! One in-memory database holds a small world: orders, the stores they went to, the regions
//! of the stores, and monthly targets for each store. The numbers are small enough to total
//! by hand, and the tests say how.

use super::check::Ty;
use super::*;
use crate::catalog::{Catalog, Column, Table};
use crate::workspace::FieldRef;
use chrono::NaiveDate;
use duckdb::types::Value as V;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ---- the world -------------------------------------------------------------------------

fn table(id: &str, name: &str, cols: &[(&str, DType)]) -> Table {
    let (src, _) = id.split_once('/').unwrap();
    Table {
        id: id.into(),
        source_id: src.into(),
        name: name.into(),
        label: name.into(),
        // The database name is lower case, so a test shows that the label is what people type.
        sql_ref: format!("\"{}\"", name.to_lowercase()),
        columns: cols
            .iter()
            .map(|(n, d)| Column::new(n.to_string(), *d, String::new()))
            .collect(),
        rows: None,
        notes: vec![],
        header_row: None,
        // The tables stand for files: a join into them reads them as they are.
        prep: true,
    }
}

fn rel(id: &str, from: (&str, &str), to: (&str, &str)) -> Relationship {
    Relationship {
        id: id.into(),
        from: FieldRef {
            table: from.0.into(),
            column: from.1.into(),
        },
        to: FieldRef {
            table: to.0.into(),
            column: to.1.into(),
        },
        name: None,
    }
}

struct World {
    db: Mutex<Connection>,
    catalog: Catalog,
    rels: Vec<Relationship>,
    /// Every link as the data of a usual one to many link says it: clean on the one side.
    health: crate::links::Health,
}

fn world() -> &'static World {
    static W: OnceLock<World> = OnceLock::new();
    W.get_or_init(|| {
        use DType::*;
        let rels = vec![
            rel("r1", ("s1/Orders", "store_id"), ("s2/Stores", "id")),
            rel("r2", ("s2/Stores", "region_code"), ("s3/Regions", "code")),
            rel("r3", ("s4/Targets", "store_id"), ("s2/Stores", "id")),
        ];
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE orders (id INTEGER, date DATE, store_id INTEGER, customer VARCHAR,
                amount DOUBLE, qty INTEGER, ts TIMESTAMP, paid BOOLEAN, note VARCHAR, region VARCHAR);
             INSERT INTO orders VALUES
              (1, DATE '2026-01-15', 1, 'Ana',   100,  2, TIMESTAMP '2026-01-15 10:30:00', true,  'hello world', 'Java'),
              (2, DATE '2026-01-20', 1, 'Budi',  250,  5, TIMESTAMP '2026-01-20 18:00:00', true,  NULL, 'Java'),
              (3, DATE '2026-02-03', 2, 'ana',   40,   1, TIMESTAMP '2026-02-03 08:15:00', false, '', 'Bali'),
              (4, DATE '2026-02-10', 2, 'Citra', 0,    0, TIMESTAMP '2026-02-10 12:00:00', true,  '  padded   text ', 'Java'),
              (5, DATE '2026-03-31', 3, NULL,    75.5, 3, TIMESTAMP '2026-03-31 23:59:59', NULL,  'x', 'Sumatra'),
              (6, DATE '2026-03-05', 9, 'Dewi',  10,   1, TIMESTAMP '2026-03-05 00:00:00', true,  'jean-luc o''brien', 'Bali');
             CREATE TABLE stores (id INTEGER, name VARCHAR, region_code VARCHAR, rent DOUBLE);
             INSERT INTO stores VALUES (1, 'Kopi A', 'JB', 500), (2, 'Kopi B', 'JT', 300), (3, 'Kopi C', 'JB', 200);
             CREATE TABLE regions (code VARCHAR, name VARCHAR);
             INSERT INTO regions VALUES ('JB', 'Jawa Barat'), ('JT', 'Jawa Tengah');
             CREATE TABLE targets (store_id INTEGER, month VARCHAR, target DOUBLE);
             INSERT INTO targets VALUES (1, '2026-01', 300), (1, '2026-02', 300), (2, '2026-01', 100),
                                        (2, '2026-02', 100), (3, '2026-01', 150);
             CREATE TABLE odd (\"a]b\" DOUBLE, \"x\"\"y\" DOUBLE);
             INSERT INTO odd VALUES (7, 8);
             CREATE TABLE mismatch (bad VARCHAR);
             INSERT INTO mismatch VALUES ('abc');
             CREATE TABLE empty (v DOUBLE, t VARCHAR, d DATE);",
        )
        .unwrap();
        let mut orders = table(
            "s1/Orders",
            "Orders",
            &[
                ("id", Number),
                ("date", Date),
                ("store_id", Number),
                ("customer", Text),
                ("amount", Number),
                ("qty", Number),
                ("ts", DateTime),
                ("paid", Bool),
                ("note", Text),
                ("region", Text),
            ],
        );
        // A name the app shows is a name a formula accepts.
        orders.columns[5].label = Some("Cups sold".into());
        let mut pair = table("s8/Pair", "Pair", &[("x", Number), ("y", Number)]);
        pair.columns[1].label = Some("x".into());
        World {
            db: Mutex::new(db),
            catalog: Catalog {
                tables: vec![
                    orders,
                    table(
                        "s2/Stores",
                        "Stores",
                        &[
                            ("id", Number),
                            ("name", Text),
                            ("region_code", Text),
                            ("rent", Number),
                        ],
                    ),
                    table("s3/Regions", "Regions", &[("code", Text), ("name", Text)]),
                    table(
                        "s4/Targets",
                        "Targets",
                        &[("store_id", Number), ("month", Text), ("target", Number)],
                    ),
                    table("s5/Odd", "Odd", &[("a]b", Number), ("x\"y", Number)]),
                    // The catalog says Number, the data is text: a formula passes the check
                    // and fails on the data.
                    table("s6/Mismatch", "Mismatch", &[("bad", Number)]),
                    // In the catalog, but not in the database: its source is gone.
                    table("s7/Ghost", "Ghost", &[("g", Number)]),
                    // The column y is shown as "x", the name of the column x.
                    pair,
                    table(
                        "s9/Empty",
                        "Empty",
                        &[("v", Number), ("t", Text), ("d", Date)],
                    ),
                ],
            },
            rels: rels.clone(),
            health: rels
                .iter()
                .map(|r| {
                    (
                        r.id.clone(),
                        crate::links::LinkHealth {
                            one_rows: 100,
                            many_rows: 100,
                            many_dups: 50,
                            ..Default::default()
                        },
                    )
                })
                .collect(),
        }
    })
}

fn cx() -> Context<'static> {
    let w = world();
    let today = NaiveDate::from_ymd_opt(2026, 10, 11).unwrap();
    Context {
        catalog: &w.catalog,
        rels: &w.rels,
        health: &w.health,
        today,
        now: today.and_hms_opt(14, 3, 22).unwrap(),
    }
}

/// The orders (store, amount) and the targets (store, target) as plain numbers, so the right
/// answers can be worked out here without any SQL. Store 9 is not in `stores`.
const ORDERS: [(u32, f64); 6] = [
    (1, 100.0),
    (1, 250.0),
    (2, 40.0),
    (2, 0.0),
    (3, 75.5),
    (9, 10.0),
];
const TARGETS: [(u32, f64); 5] = [(1, 300.0), (1, 300.0), (2, 100.0), (2, 100.0), (3, 150.0)];
const STORES: [u32; 3] = [1, 2, 3];

fn to_json(v: V) -> Value {
    match v {
        V::Null => Value::Null,
        V::Boolean(b) => json!(b),
        V::TinyInt(i) => json!(i),
        V::SmallInt(i) => json!(i),
        V::Int(i) => json!(i),
        V::BigInt(i) => json!(i),
        V::Float(f) => json!(f),
        V::Double(f) => json!(f),
        V::Text(s) => json!(s),
        other => panic!("unexpected value {other:?}"),
    }
}

/// Runs a SELECT and returns the first column of every row.
fn query(sql: &str) -> Vec<Value> {
    let db = world().db.lock().unwrap_or_else(|e| e.into_inner());
    let mut stmt = db.prepare(sql).unwrap_or_else(|e| panic!("{sql}\n{e}"));
    let rows = stmt
        .query_map([], |r| r.get::<_, V>(0))
        .unwrap_or_else(|e| panic!("{sql}\n{e}"));
    rows.map(|v| to_json(v.unwrap())).collect()
}

fn compiled(formula: &str, kind: Kind, table: Option<&str>) -> Result<Compiled, FormulaError> {
    compile(formula, kind, table, &cx())
}

/// The values of a column formula over Orders, in the order of `id`.
fn column(formula: &str) -> Vec<Value> {
    let c = compiled(formula, Kind::Column, Some("Orders"))
        .unwrap_or_else(|e| panic!("{formula}: {e:?}"));
    let Body::Column(col) = &c.body else {
        panic!("{formula}: not a column")
    };
    let joins: String = col.joins.iter().map(Join::to_sql).collect();
    query(&format!(
        "SELECT {} FROM orders AS t0{joins} ORDER BY t0.id",
        sql::display(&col.sql, Ty::from_dtype(c.dtype))
    ))
}

fn at(row: usize, formula: &str) -> Value {
    column(formula).swap_remove(row - 1)
}

/// The grand total of a measure.
fn total(formula: &str, table: Option<&str>) -> Value {
    let c = compiled(formula, Kind::Measure, table).unwrap_or_else(|e| panic!("{formula}: {e:?}"));
    let Body::Aggregate(agg) = &c.body else {
        panic!("{formula}: not a measure")
    };
    let sql = agg
        .grand_total(&world().catalog, Ty::from_dtype(c.dtype))
        .unwrap();
    query(&sql).swap_remove(0)
}

fn parts(formula: &str) -> AggregateSql {
    match compiled(formula, Kind::Measure, None).unwrap().body {
        Body::Aggregate(a) => a,
        Body::Column(_) => panic!("not a measure"),
    }
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 1e-9
        }
        _ => a == b,
    }
}

fn expect(row: usize, formula: &str, want: Value) {
    let got = at(row, formula);
    assert!(
        same(&got, &want),
        "{formula} (row {row})\n got  {got}\n want {want}"
    );
}

fn expect_total(formula: &str, want: Value) {
    let got = total(formula, None);
    assert!(same(&got, &want), "{formula}\n got  {got}\n want {want}");
}

fn err_in(formula: &str, kind: Kind, table: Option<&str>) -> (String, usize, usize) {
    match compiled(formula, kind, table) {
        Err(e) => (e.message, e.at, e.len),
        Ok(_) => panic!("{formula} should not compile"),
    }
}

/// The message, and the piece of the formula it underlines.
fn err(formula: &str, kind: Kind, table: Option<&str>) -> (String, String) {
    let (m, at, len) = err_in(formula, kind, table);
    let piece: String = formula.chars().skip(at).take(len).collect();
    (m, piece)
}

fn col_err(formula: &str) -> (String, String) {
    err(formula, Kind::Column, Some("Orders"))
}

fn m_err(formula: &str) -> (String, String) {
    err(formula, Kind::Measure, None)
}

/// A test for each formula and its value in one row of Orders (row 1 unless named).
macro_rules! values {
    ($( $name:ident: $formula:expr => $want:expr; )*) => {
        $( #[test] fn $name() { expect(1, $formula, json!($want)); } )*
    };
}

macro_rules! values_at {
    ($( $name:ident: $row:expr, $formula:expr => $want:expr; )*) => {
        $( #[test] fn $name() { expect($row, $formula, json!($want)); } )*
    };
}

macro_rules! totals {
    ($( $name:ident: $formula:expr => $want:expr; )*) => {
        $( #[test] fn $name() { expect_total($formula, json!($want)); } )*
    };
}

#[test]
fn placeholders_are_filled_whole() {
    let v: Vec<String> = (0..12).map(|i| format!("<{i}>")).collect();
    assert_eq!(fill("$p1 + $p10 / $p0", &v), "<1> + <10> / <0>");
    // A number with no part keeps its text.
    assert_eq!(fill("$p99 and $px", &v), "$p99 and $px");
}

#[test]
fn placeholders_inside_quotes_are_left_alone() {
    let v = vec!["X".to_string()];
    assert_eq!(fill("'$p0' || $p0 || \"$p0\"", &v), "'$p0' || X || \"$p0\"");
    // A quote written twice stays inside the text.
    assert_eq!(fill("'it''s $p0' || $p0", &v), "'it''s $p0' || X");
}

#[test]
fn a_users_text_that_looks_like_a_placeholder_is_kept() {
    let got = total(r#""cost $p0 " & TEXT(SUM([Orders].[amount]), "0")"#, None);
    assert_eq!(got, json!("cost $p0 476"));
    // And a question mark or a dollar sign is only text.
    assert_eq!(at(1, r#""?$1 $name""#), json!("?$1 $name"));
}

// ---- logic -----------------------------------------------------------------------------

values! {
    if_true: r#"IF([amount] > 50, "big", "small")"# => "big";
    if_false: r#"IF([amount] > 500, "big", "small")"# => "small";
    if_two_values_is_blank_when_false: r#"IF([amount] > 500, "big")"# => Value::Null;
    if_blank_test_takes_the_else: "IF([amount] > BLANK(), 1, 2)" => 2;
    if_mixes_date_and_datetime: "IF(TRUE, [date], [ts])" => "2026-01-15 00:00:00";
    ifs_takes_the_first_true: r#"IFS([amount] > 500, "a", [amount] > 50, "b", TRUE, "c")"# => "b";
    ifs_with_no_match_is_blank: r#"IFS([amount] > 500, "a")"# => Value::Null;
    switch_matches_without_case: r#"SWITCH([customer], "ana", 1, "budi", 2, 0)"# => 1;
    switch_falls_to_the_default: r#"SWITCH([qty], 9, "x", "other")"# => "other";
    switch_without_default_is_blank: r#"SWITCH([qty], 9, "x")"# => Value::Null;
    and_all_true: "AND([amount] > 50, [paid])" => true;
    and_one_false: "AND([amount] > 500, [paid])" => false;
    or_one_true: "OR(FALSE, [paid])" => true;
    not_flips: "NOT([paid])" => false;
    iferror_replaces_a_blank: "IFERROR(1 / 0, -1)" => -1;
    iferror_keeps_a_value: "IFERROR(4, 0)" => 4;
    isblank_of_blank: "ISBLANK(BLANK())" => true;
    isblank_of_a_number: "ISBLANK([amount])" => false;
    blank_alone_is_blank: "BLANK()" => Value::Null;
}

values_at! {
    isblank_of_empty_text: 3, "ISBLANK([note])" => true;
    isblank_of_missing_text: 2, "ISBLANK([note])" => true;
    isblank_of_text: 1, "ISBLANK([note])" => false;
    or_with_a_blank_stays_blank: 5, "OR(FALSE, [paid])" => Value::Null;
    a_blank_cell_is_not_true: 5, r#"IF([paid], "yes", "no")"# => "no";
}

// ---- math ------------------------------------------------------------------------------

values! {
    abs_of_a_negative: "ABS(-3.5)" => 3.5;
    round_half_up: "ROUND(2.5, 0)" => 3;
    round_half_away_from_zero: "ROUND(-2.5)" => -3;
    round_to_places: "ROUND(2.675, 2)" => 2.68;
    round_without_places: "ROUND(7.4)" => 7;
    round_to_hundreds: "ROUND(1234.5, -2)" => 1200;
    round_ignores_float_noise: "ROUND(1.005, 2)" => 1.01;
    roundup_goes_away_from_zero: "ROUNDUP(2.1, 0)" => 3;
    roundup_negative: "ROUNDUP(-2.1)" => -3;
    roundup_ignores_float_noise: "ROUNDUP(2.3 * 100, 0)" => 230;
    rounddown_goes_toward_zero: "ROUNDDOWN(2.9)" => 2;
    rounddown_negative: "ROUNDDOWN(-2.9)" => -2;
    rounddown_places: "ROUNDDOWN(7.4159, 2)" => 7.41;
    int_rounds_down: "INT(-2.5)" => -3;
    mod_of_positives: "MOD(5, 3)" => 2;
    mod_takes_the_sign_of_the_divisor: "MOD(-5, 3)" => 1;
    mod_negative_divisor: "MOD(5, -3)" => -1;
    mod_by_zero_is_blank: "MOD(5, 0)" => Value::Null;
    power_function: "POWER(2, 10)" => 1024;
    power_operator_goes_right_to_left: "2^3^2" => 512;
    power_overflow_is_blank: "POWER(10, 400)" => Value::Null;
    power_of_a_negative_root_is_blank: "POWER(-8, 1 / 3)" => Value::Null;
    sqrt_function: "SQRT(16)" => 4;
    sqrt_of_a_negative_is_blank: "SQRT(-1)" => Value::Null;
    exp_function: "EXP(0)" => 1;
    exp_overflow_is_blank: "EXP(1000)" => Value::Null;
    ln_function: "LN(1)" => 0;
    ln_of_zero_is_blank: "LN(0)" => Value::Null;
    ln_of_a_negative_is_blank: "LN(-5)" => Value::Null;
    log10_function: "LOG10(1000)" => 3;
    sign_of_a_negative: "SIGN(-9)" => -1;
    sign_of_zero: "SIGN(0)" => 0;
    min_of_values: "MIN(3, 1, 2)" => 1;
    max_of_values: "MAX(3, 1, 2)" => 3;
    max_ignores_a_blank: "MAX(BLANK(), 4)" => 4;
    max_of_dates: "MAX([date], DATE(2026, 2, 1))" => "2026-02-01";
    divide_function: "DIVIDE(10, 4)" => 2.5;
    divide_by_zero_is_blank: "DIVIDE(1, 0)" => Value::Null;
    divide_by_zero_takes_the_alternative: "DIVIDE(1, 0, -1)" => -1;
    divide_with_an_alternative_still_divides: "DIVIDE(9, 3, -1)" => 3;
    slash_by_zero_is_blank: "1 / 0" => Value::Null;
    precedence_of_the_four_operators: "2+3*4^2" => 50;
    the_minus_sign_binds_before_the_power: "-2^2" => 4;
    brackets_change_the_order: "(2+3)*4" => 20;
    whole_numbers_divide_into_fractions: "5 / 2" => 2.5;
    exponent_notation: "1e3 + .5" => 1000.5;
}

values_at! {
    division_of_zero_by_zero_is_blank: 4, "[amount] / [qty]" => Value::Null;
    a_blank_makes_the_sum_blank: 5, "[amount] + [Stores].[rent] * BLANK()" => Value::Null;
}

#[test]
fn a_rounded_negative_zero_is_a_plain_zero() {
    let v = at(1, "ROUND(-0.4)");
    assert!(!v.as_f64().unwrap().is_sign_negative(), "{v}");
}

// ---- text ------------------------------------------------------------------------------

values! {
    ampersand_drops_the_trailing_zero: r#""n=" & 1"# => "n=1";
    ampersand_keeps_a_fraction: r#""x" & 1.5"# => "x1.5";
    ampersand_cleans_float_noise: r#""v" & (0.1 + 0.2)"# => "v0.3";
    ampersand_writes_a_big_whole_number: r#""n" & 1000000"# => "n1000000";
    ampersand_writes_a_date: r#""d=" & [date]"# => "d=2026-01-15";
    ampersand_writes_a_date_and_time: r#""t=" & [ts]"# => "t=2026-01-15 10:30:00";
    ampersand_writes_true: r#""p=" & [paid]"# => "p=TRUE";
    ampersand_skips_a_blank: r#""a" & BLANK()"# => "a";
    concat_function: r#"CONCAT("a", "b", 3)"# => "ab3";
    left_function: r#"LEFT("abcdef", 2)"# => "ab";
    left_defaults_to_one: r#"LEFT("abc")"# => "a";
    left_of_a_number: "LEFT(12345, 2)" => "12";
    left_negative_is_blank: r#"LEFT("abc", -1)"# => Value::Null;
    right_function: r#"RIGHT("abcdef", 2)"# => "ef";
    right_defaults_to_one: r#"RIGHT("abc")"# => "c";
    mid_function: r#"MID("abcdef", 2, 3)"# => "bcd";
    mid_from_zero_is_blank: r#"MID("abcdef", 0, 2)"# => Value::Null;
    len_counts_characters: r#"LEN("héllo")"# => 5;
    upper_function: r#"UPPER("abc")"# => "ABC";
    lower_function: r#"LOWER("ABC")"# => "abc";
    proper_function: r#"PROPER("jean-luc o'brien")"# => "Jean-Luc O'Brien";
    proper_of_empty_text: r#"PROPER("")"# => "";
    trim_collapses_the_spaces: r#"TRIM("  a   b  ")"# => "a b";
    substitute_function: r#"SUBSTITUTE("a-b-c", "-", "+")"# => "a+b+c";
    substitute_cares_about_case: r#"SUBSTITUTE("Aa", "a", "x")"# => "Ax";
    find_function: r#"FIND("l", "hello")"# => 3;
    find_not_found_is_blank: r#"FIND("z", "hello")"# => Value::Null;
    find_cares_about_case: r#"FIND("L", "hello")"# => Value::Null;
    find_from_a_position: r#"FIND("l", "hello", 4)"# => 4;
    search_ignores_case: r#"SEARCH("L", "hello")"# => 3;
    value_reads_text: r#"VALUE(" 12.5 ")"# => 12.5;
    value_of_bad_text_is_blank: r#"VALUE("abc")"# => Value::Null;
    value_of_nan_is_blank: r#"VALUE("nan")"# => Value::Null;
    value_of_a_number: "VALUE(7)" => 7;
    text_whole: r#"TEXT(2.5, "0")"# => "3";
    text_whole_of_a_small_negative: r#"TEXT(-0.4, "0")"# => "0";
    text_one_place: r#"TEXT(3.14159, "0.0")"# => "3.1";
    text_with_thousands: r##"TEXT(1234567.891, "#,##0")"## => "1,234,568";
    text_with_thousands_negative: r##"TEXT(-1234.5, "#,##0")"## => "-1,235";
    text_percent: r#"TEXT(0.256, "0%")"# => "26%";
    text_in_a_sentence: r##"TEXT([amount], "#,##0") & " rupiah""## => "100 rupiah";
}

values_at! {
    proper_of_a_cell: 6, "PROPER([note])" => "Jean-Luc O'Brien";
    ampersand_skips_a_blank_cell: 5, r#"[customer] & "!""# => "!";
    ampersand_writes_a_blank_true_as_nothing: 5, r#""p=" & [paid]"# => "p=";
    trim_a_cell: 4, "TRIM([note])" => "padded text";
}

// ---- dates -----------------------------------------------------------------------------

values! {
    date_function: "DATE(2026, 3, 9)" => "2026-03-09";
    date_rolls_a_month_over: "DATE(2026, 13, 1)" => "2027-01-01";
    date_rolls_a_day_over: "DATE(2026, 2, 31)" => "2026-03-03";
    date_month_zero_is_last_december: "DATE(2026, 0, 1)" => "2025-12-01";
    year_function: "YEAR([date])" => 2026;
    month_function: "MONTH([date])" => 1;
    day_function: "DAY([date])" => 15;
    year_of_a_date_and_time: "YEAR([ts])" => 2026;
    weekday_counts_from_monday: "WEEKDAY([date])" => 4;
    weekday_of_a_sunday: "WEEKDAY(DATE(2026, 10, 11))" => 7;
    weekday_of_a_monday: "WEEKDAY(DATE(2026, 10, 12))" => 1;
    weeknum_of_the_first_week: "WEEKNUM(DATE(2026, 1, 1))" => 1;
    weeknum_of_a_long_year: "WEEKNUM(DATE(2026, 12, 31))" => 53;
    quarter_function: "QUARTER(DATE(2026, 5, 5))" => 2;
    eomonth_this_month: "EOMONTH([date], 0)" => "2026-01-31";
    eomonth_next_month: "EOMONTH([date], 1)" => "2026-02-28";
    eomonth_last_month: "EOMONTH(DATE(2026, 1, 31), -1)" => "2025-12-31";
    edate_clamps_to_the_month_end: "EDATE(DATE(2026, 1, 31), 1)" => "2026-02-28";
    edate_backwards: "EDATE([date], -2)" => "2025-11-15";
    datedif_in_days: r#"DATEDIF(DATE(2026, 1, 1), DATE(2026, 1, 31), "D")"# => 30;
    datedif_counts_whole_months: r#"DATEDIF(DATE(2000, 1, 31), DATE(2000, 3, 30), "M")"# => 1;
    datedif_a_month_is_not_complete_before_its_day: r#"DATEDIF(DATE(2000, 1, 31), DATE(2000, 2, 29), "M")"# => 0;
    datedif_a_month_is_complete_on_its_day: r#"DATEDIF(DATE(2000, 1, 15), DATE(2000, 2, 15), "M")"# => 1;
    datedif_months_across_years: r#"DATEDIF(DATE(2025, 11, 20), DATE(2026, 2, 19), "M")"# => 2;
    datedif_a_leap_day_is_not_a_year_on_the_28th: r#"DATEDIF(DATE(2000, 2, 29), DATE(2001, 2, 28), "Y")"# => 0;
    datedif_a_year_is_complete_on_its_day: r#"DATEDIF(DATE(2000, 2, 29), DATE(2001, 3, 1), "Y")"# => 1;
    datedif_a_day_short_of_a_month: r#"DATEDIF(DATE(2000, 1, 31), DATE(2000, 2, 1), "M")"# => 0;
    datedif_counts_whole_years: r#"DATEDIF(DATE(2000, 3, 1), DATE(2002, 2, 28), "Y")"# => 1;
    datedif_unit_in_lower_case: r#"DATEDIF(DATE(2026, 1, 1), DATE(2026, 1, 31), "d")"# => 30;
    datedif_backwards_is_blank: r#"DATEDIF(DATE(2026, 2, 1), DATE(2026, 1, 1), "D")"# => Value::Null;
    days_function: "DAYS(DATE(2026, 3, 1), DATE(2026, 2, 1))" => 28;
    today_is_the_context_date: "TODAY()" => "2026-10-11";
    now_is_the_context_time: "NOW()" => "2026-10-11 14:03:22";
    today_in_a_sum: "TODAY() - DATE(2026, 10, 1)" => 10;
    a_date_plus_days: "[date] + 30" => "2026-02-14";
    days_plus_a_date: "30 + [date]" => "2026-02-14";
    a_date_minus_days: "[date] - 15" => "2025-12-31";
    a_date_moves_by_whole_days: "[date] + 0.9" => "2026-01-15";
    two_dates_make_days: "DATE(2026, 3, 1) - DATE(2026, 2, 1)" => 28;
    a_date_and_time_plus_a_day: "[ts] + 1" => "2026-01-16 10:30:00";
    a_date_and_time_plus_half_a_day: "[ts] + 0.5" => "2026-01-15 22:30:00";
    a_date_and_time_minus_itself: "[ts] - [ts]" => 0;
    a_date_and_time_minus_a_date: "[ts] - DATE(2026, 1, 15)" => 0.4375;
    a_date_after_a_date: "[date] > DATE(2026, 1, 1)" => true;
    a_date_equals_a_date: "[date] = DATE(2026, 1, 15)" => true;
    a_date_and_time_is_after_its_date: "[ts] > [date]" => true;
}

// ---- comparison ------------------------------------------------------------------------

values! {
    text_equals_without_case: r#""Ana" = "ana""# => true;
    text_cell_equals_without_case: r#"[customer] = "ANA""# => true;
    text_differs: r#"[customer] <> "budi""# => true;
    text_orders_without_case: r#""apple" < "Banana""# => true;
    numbers_compare: "[amount] >= 100" => true;
    blank_equals_blank: "BLANK() = BLANK()" => true;
    number_does_not_equal_blank: "[amount] = BLANK()" => false;
    text_is_not_empty: r#"[note] <> """# => true;
    not_equal_operator: "[qty] <> 2" => false;
}

values_at! {
    empty_text_equals_empty: 3, r#"[note] = """# => true;
    missing_text_equals_empty: 2, r#"[note] = """# => true;
    a_blank_cell_does_not_equal_text: 5, r#"[customer] = "x""# => false;
    a_blank_cell_differs_from_text: 5, r#"[customer] <> "x""# => true;
    ordering_with_a_blank_is_blank: 5, "[customer] < BLANK()" => Value::Null;
}

// ---- columns across rows ---------------------------------------------------------------

#[test]
fn a_formula_runs_on_every_row() {
    let got = column(r#"IF([paid], "paid", "unpaid")"#);
    assert_eq!(
        got,
        [
            json!("paid"),
            json!("paid"),
            json!("unpaid"),
            json!("paid"),
            json!("unpaid"),
            json!("paid")
        ]
    );
}

#[test]
fn a_column_reads_the_numbers_as_doubles() {
    let got = column("[amount] * 1.11");
    let want = [111.0, 277.5, 44.4, 0.0, 83.805, 11.1];
    for (g, w) in got.iter().zip(want) {
        assert!(same(g, &json!(w)), "{g} vs {w}");
    }
}

#[test]
fn a_whole_number_column_divides_into_fractions() {
    // qty is an INTEGER column. 5 / 2 must not become 2.
    assert!(same(&at(2, "[qty] / 2"), &json!(2.5)));
}

#[test]
fn a_column_reads_a_related_column() {
    assert_eq!(
        column("[Stores].[name]"),
        [
            json!("Kopi A"),
            json!("Kopi A"),
            json!("Kopi B"),
            json!("Kopi B"),
            json!("Kopi C"),
            Value::Null // store 9 does not exist
        ]
    );
}

#[test]
fn a_column_reads_a_column_two_steps_away() {
    assert_eq!(
        column("[Regions].[name]"),
        [
            json!("Jawa Barat"),
            json!("Jawa Barat"),
            json!("Jawa Tengah"),
            json!("Jawa Tengah"),
            json!("Jawa Barat"),
            Value::Null
        ]
    );
}

#[test]
fn related_columns_join_without_adding_rows() {
    let got = column("[Stores].[rent] + [amount]");
    assert_eq!(got.len(), 6);
    assert!(same(&got[0], &json!(600.0)));
    assert_eq!(got[5], Value::Null);
}

#[test]
fn a_related_column_writes_its_joins() {
    let Body::Column(c) = compiled("[Regions].[name]", Kind::Column, Some("Orders"))
        .unwrap()
        .body
    else {
        panic!()
    };
    assert_eq!(c.joins.len(), 2);
    assert_eq!(c.joins[0].alias, "r1");
    assert_eq!(c.joins[0].on, "t0.\"store_id\" = r1.\"id\"");
    assert_eq!(c.joins[1].alias, "r2");
    assert_eq!(c.joins[1].on, "r1.\"region_code\" = r2.\"code\"");
    assert!(c.sql.contains("r2.\"name\""));
}

#[test]
fn a_table_is_joined_once_however_often_it_is_read() {
    let Body::Column(c) = compiled(
        "[Stores].[rent] + [Stores].[id] + LEN([Regions].[code])",
        Kind::Column,
        Some("Orders"),
    )
    .unwrap()
    .body
    else {
        panic!()
    };
    assert_eq!(c.joins.len(), 2);
}

#[test]
fn a_column_only_reaches_a_table_by_its_own_name_too() {
    // [Orders].[amount] inside an Orders column is the same as [amount].
    assert!(same(&at(1, "[Orders].[amount] + [amount]"), &json!(200.0)));
}

// ---- measures and groups ---------------------------------------------------------------

totals! {
    sum_function: "SUM([Orders].[amount])" => 475.5;
    average_function: "AVERAGE([Orders].[amount])" => 79.25;
    min_function: "MIN([Orders].[amount])" => 0;
    max_function: "MAX([Orders].[amount])" => 250;
    min_of_dates: "MIN([Orders].[date])" => "2026-01-15";
    max_of_text: "MAX([Orders].[customer])" => "ana";
    count_function: "COUNT([Orders].[amount])" => 6;
    count_skips_blanks: "COUNT([Orders].[paid])" => 5;
    counta_skips_blank_text: "COUNTA([Orders].[customer])" => 5;
    counta_skips_empty_text: "COUNTA([Orders].[note])" => 4;
    countd_function: "COUNTD([Orders].[customer])" => 5;
    countd_of_numbers: "COUNTD([Orders].[store_id])" => 4;
    median_function: "MEDIAN([Orders].[amount])" => 57.75;
    percentile_median: "PERCENTILE([Orders].[amount], 0.5)" => 57.75;
    percentile_ninety: "PERCENTILE([Orders].[amount], 0.9)" => 175;
    sumif_function: "SUMIF([Orders].[amount], [Orders].[paid])" => 360;
    sumif_ignores_case: r#"SUMIF([Orders].[amount], [Orders].[customer] = "ana")"# => 140;
    countif_function: "COUNTIF([Orders].[amount], [Orders].[amount] > 50)" => 3;
    averageif_function: "AVERAGEIF([Orders].[amount], [Orders].[qty] >= 2)" => 141.833333333333;
    countrows_function: "COUNTROWS([Orders])" => 6;
    countrows_of_another_table: "COUNTROWS([Stores])" => 3;
    the_ratio_of_two_tables: "SUM([Orders].[amount]) / SUM([Targets].[target])" => 475.5 / 950.0;
    a_related_column_inside_a_total: "SUM([Orders].[amount] * [Stores].[rent])" => 202100;
    a_total_rounded: "ROUND(SUM([Orders].[amount]) / COUNT([Orders].[amount]), 1)" => 79.3;
    a_test_on_a_total: r#"IF(SUM([Orders].[amount]) > 400, "high", "low")"# => "high";
    the_larger_of_a_total_and_a_number: "MAX(SUM([Orders].[amount]), 1000)" => 1000;
    a_total_and_a_literal: "SUM([Orders].[amount]) + 24.5" => 500;
    division_of_totals_by_zero_is_blank: "SUM([Orders].[amount]) / (COUNT([Orders].[amount]) - 6)" => Value::Null;
    a_total_in_text: r##""Total: " & TEXT(SUM([Orders].[amount]), "#,##0")"## => "Total: 476";
    a_total_of_a_date_function: "SUM(YEAR([Orders].[date]))" => 12156;
}

#[test]
fn stdev_and_var_use_the_sample() {
    let xs = [100.0, 250.0, 40.0, 0.0, 75.5, 10.0];
    let mean = xs.iter().sum::<f64>() / 6.0;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / 5.0;
    expect_total("VAR([Orders].[amount])", json!(var));
    expect_total("STDEV([Orders].[amount])", json!(var.sqrt()));
}

#[test]
fn a_total_in_a_measure_with_a_table_reads_bare_columns() {
    assert!(same(&total("SUM([amount])", Some("Orders")), &json!(475.5)));
    assert!(same(
        &total("SUM([amount]) / COUNTROWS([Orders])", Some("Orders")),
        &json!(79.25)
    ));
}

#[test]
fn each_table_is_totalled_on_its_own() {
    let a = parts("SUM([Orders].[amount]) / SUM([Targets].[target])");
    assert_eq!(a.parts.len(), 2);
    assert_eq!(a.parts[0].home_table, "s1/Orders");
    assert_eq!(a.parts[1].home_table, "s4/Targets");
    assert_eq!(a.parts[0].sql, "sum(CAST(t0.\"amount\" AS DOUBLE))");
    assert!(a.parts.iter().all(|p| p.zero_when_empty));
    // The parts stand for themselves; the combining expression only divides.
    assert!(a.combine.contains("($p0 / $p1)"), "{}", a.combine);
    let filled = a.fill(&["A".into(), "B".into()]);
    assert!(
        filled.contains("(COALESCE(A, 0e0) / COALESCE(B, 0e0))"),
        "{filled}"
    );
}

#[test]
fn a_join_of_the_two_tables_would_count_orders_twice() {
    // The right answer, from the numbers alone.
    let sold: f64 = ORDERS.iter().map(|o| o.1).sum();
    let target: f64 = TARGETS.iter().map(|t| t.1).sum();
    // What a join of the orders to the targets of their store adds up: each order once for
    // each target of its store.
    let fan_out: f64 = ORDERS
        .iter()
        .filter(|o| STORES.contains(&o.0))
        .map(|o| o.1 * TARGETS.iter().filter(|t| t.0 == o.0).count() as f64)
        .sum();
    assert!(fan_out > sold, "a join must count orders more than once");
    // The formula gives the right answer, not the joined one.
    expect_total(
        "SUM([Orders].[amount]) / SUM([Targets].[target])",
        json!(sold / target),
    );
    expect_total("SUM([Orders].[amount])", json!(sold));
    // The database agrees with the fan-out worked out above, so the test is not checking itself.
    let naive = query(
        "SELECT sum(o.amount) FROM orders o JOIN stores s ON o.store_id = s.id \
         JOIN targets t ON t.store_id = s.id",
    );
    assert!(same(&naive[0], &json!(fan_out)));
    // The parts do not join anything but their own table.
    let a = parts("SUM([Orders].[amount]) / SUM([Targets].[target])");
    assert!(a.parts.iter().all(|p| p.joins.is_empty()));
}

#[test]
fn the_same_total_twice_is_one_part() {
    let a = parts("SUM([Orders].[amount]) + SUM([Orders].[amount])");
    assert_eq!(a.parts.len(), 1);
    assert_eq!(a.combine, "($p0 + $p0)");
}

#[test]
fn a_part_that_reads_a_related_table_carries_its_join() {
    let a = parts("SUM([Orders].[amount] * [Stores].[rent])");
    assert_eq!(a.parts.len(), 1);
    assert_eq!(a.parts[0].home_table, "s1/Orders");
    assert_eq!(a.parts[0].joins.len(), 1);
    assert_eq!(a.parts[0].joins[0].alias, "r1");
}

#[test]
fn the_home_of_a_total_is_the_many_side() {
    // Stores is read first, but Orders is where each store is reached from.
    let a = parts("SUM([Stores].[rent] * [Orders].[amount])");
    assert_eq!(a.parts[0].home_table, "s1/Orders");
}

#[test]
fn a_group_value_reads_its_own_table() {
    let c = compiled(
        "SUM([amount]) / COUNTROWS([Orders])",
        Kind::Group,
        Some("Orders"),
    )
    .unwrap();
    let Body::Aggregate(a) = c.body else { panic!() };
    assert!(a.parts.iter().all(|p| p.home_table == "s1/Orders"));
    assert_eq!(c.dtype, DType::Number);
}

#[test]
fn a_group_value_reads_a_related_table_through_its_own() {
    let c = compiled("SUM([Stores].[rent])", Kind::Group, Some("Orders")).unwrap();
    let Body::Aggregate(a) = c.body else { panic!() };
    assert_eq!(a.parts[0].home_table, "s1/Orders");
    let sql = a.grand_total(&world().catalog, Ty::Num).unwrap();
    assert!(same(&query(&sql)[0], &json!(1800.0)));
}

#[test]
fn a_measure_has_a_type_and_the_tables_it_reads() {
    let c = compiled(
        "SUM([Orders].[amount]) / SUM([Targets].[target])",
        Kind::Measure,
        None,
    )
    .unwrap();
    assert_eq!(c.dtype, DType::Number);
    assert_eq!(c.uses, ["s1/Orders", "s4/Targets"]);
    let d = compiled("MIN([Orders].[date])", Kind::Measure, None).unwrap();
    assert_eq!(d.dtype, DType::Date);
}

// ---- types and names that came out right -----------------------------------------------

#[test]
fn types_of_whole_formulas() {
    let ty = |f: &str| compiled(f, Kind::Column, Some("Orders")).unwrap().dtype;
    assert_eq!(ty("[amount] * 2"), DType::Number);
    assert_eq!(ty("[amount] > 1"), DType::Bool);
    assert_eq!(ty(r#"[customer] & "!""#), DType::Text);
    assert_eq!(ty("[date] + 1"), DType::Date);
    assert_eq!(ty("[ts] + 1"), DType::DateTime);
    assert_eq!(ty("[date] - [date]"), DType::Number);
    assert_eq!(ty("BLANK()"), DType::Text);
    assert_eq!(ty("IF(TRUE, 1, BLANK())"), DType::Number);
    assert_eq!(ty("IF(TRUE, [date], [ts])"), DType::DateTime);
    assert_eq!(ty("TODAY()"), DType::Date);
    assert_eq!(ty("NOW()"), DType::DateTime);
}

#[test]
fn a_formula_of_only_blank_is_text_in_sql() {
    let c = compiled("BLANK()", Kind::Column, Some("Orders")).unwrap();
    let Body::Column(col) = c.body else { panic!() };
    assert_eq!(col.sql, "CAST(NULL AS VARCHAR)");
}

#[test]
fn names_ignore_case() {
    let want = column("[amount]");
    for f in [
        "[AMOUNT]",
        "[Amount]",
        "[orders].[amount]",
        "[ORDERS].[Amount]",
        "[s1/Orders].[amount]",
        "[S1/ORDERS].[AMOUNT]",
    ] {
        assert_eq!(column(f), want, "{f}");
    }
    assert_eq!(column("if(TRUE, 1, 2)"), column("IF(TRUE, 1, 2)"));
    assert_eq!(column("IF(true, 1, 2)")[0], json!(1.0));
    assert_eq!(column("True & false")[0], json!("TRUEFALSE"));
    // A function in any case, a total in any case.
    expect_total("sum([orders].[amount])", json!(475.5));
    expect_total("Sum([ORDERS].[Amount])", json!(475.5));
    // The table of the formula, in any case.
    let c = compiled("[amount]", Kind::Column, Some("ORDERS")).unwrap();
    assert_eq!(c.uses, ["s1/Orders"]);
}

#[test]
fn a_column_is_found_by_the_name_the_app_shows() {
    assert!(same(&at(1, "[Cups sold] * 2"), &json!(4.0)));
    assert!(same(&at(1, "[cups SOLD] * 2"), &json!(4.0)));
    // The real name works too.
    assert!(same(&at(1, "[qty] * 2"), &json!(4.0)));
}

#[test]
fn a_table_is_found_by_its_label_first_then_its_id() {
    let by_label = compiled("[amount]", Kind::Column, Some("Orders")).unwrap();
    let by_id = compiled("[amount]", Kind::Column, Some("s1/Orders")).unwrap();
    assert_eq!(by_label.uses, by_id.uses);
}

#[test]
fn a_bracket_in_a_name_is_written_twice() {
    let c = compiled("[a]]b] + [x\"y]", Kind::Column, Some("Odd")).unwrap();
    let Body::Column(col) = c.body else { panic!() };
    // Both names are quoted identifiers from the catalog.
    assert!(col.sql.contains("t0.\"a]b\""));
    assert!(col.sql.contains("t0.\"x\"\"y\""));
    let v = query(&format!("SELECT {} FROM odd AS t0", col.sql));
    assert!(same(&v[0], &json!(15.0)));
}

#[test]
fn text_is_escaped_never_run() {
    let evil = "x'); DROP TABLE orders; --";
    let f = format!("\"{evil}\"");
    assert_eq!(at(1, &f), json!(evil));
    // The table is still there.
    assert_eq!(query("SELECT count(*) FROM orders"), [json!(6)]);
    assert_eq!(at(1, r#""it's""#), json!("it's"));
    assert_eq!(at(1, r#""say ""hi"" now""#), json!("say \"hi\" now"));
}

#[test]
fn only_numbers_and_escaped_text_come_from_the_formula() {
    let c = compiled(r#"[amount] * 2 & "it's""#, Kind::Column, Some("Orders")).unwrap();
    let Body::Column(col) = c.body else { panic!() };
    assert!(col.sql.contains("'it''s'"));
    assert!(col.sql.contains("2e0"));
    assert!(col.sql.contains("t0.\"amount\""));
}

#[test]
fn the_kind_reads_from_json() {
    let r: CheckRequest =
        serde_json::from_value(json!({ "formula": "1", "kind": "measure" })).unwrap();
    assert_eq!(r.kind, Kind::Measure);
    assert!(r.table.is_none());
    assert!(
        serde_json::from_value::<CheckRequest>(json!({ "formula": "1", "kind": "row" })).is_err()
    );
}

// ---- the check answer ------------------------------------------------------------------

fn ask(formula: &str, kind: Kind, table: Option<&str>) -> CheckReply {
    let req = CheckRequest {
        formula: formula.into(),
        kind,
        table: table.map(String::from),
    };
    let db = world().db.lock().unwrap_or_else(|e| e.into_inner());
    answer(&req, &cx(), &db)
}

#[test]
fn a_good_column_answers_with_a_type_and_five_values() {
    let r = ask("[amount] * 2", Kind::Column, Some("Orders"));
    assert!(r.ok, "{r:?}");
    assert_eq!(r.dtype, Some(DType::Number));
    assert_eq!(
        r.preview,
        Some(vec![
            json!(200.0),
            json!(500.0),
            json!(80.0),
            json!(0.0),
            json!(151.0)
        ])
    );
    assert_eq!(r.uses, Some(vec!["s1/Orders".to_string()]));
    assert!(r.error.is_none());
}

#[test]
fn a_date_column_previews_as_text() {
    let r = ask("[date] + 1", Kind::Column, Some("Orders"));
    assert_eq!(r.dtype, Some(DType::Date));
    assert_eq!(r.preview.unwrap()[0], json!("2026-01-16"));
}

#[test]
fn a_good_measure_answers_with_its_total() {
    let r = ask("SUM([Orders].[amount])", Kind::Measure, None);
    assert!(r.ok, "{r:?}");
    assert_eq!(r.preview, Some(vec![json!(475.5)]));
}

#[test]
fn a_bad_formula_answers_with_the_message_and_the_place() {
    let r = ask("[amout] * 2", Kind::Column, Some("Orders"));
    assert!(!r.ok);
    assert!(r.preview.is_none() && r.dtype.is_none());
    let e = r.error.unwrap();
    assert_eq!(e.message, "No column [amout]. Did you mean [amount]?");
    assert_eq!((e.at, e.len), (0, 7));
}

/// The quoted names (`"amount"`) in a piece of SQL.
fn quoted_names(sql: &str) -> Vec<String> {
    sql.split('"')
        .skip(1)
        .step_by(2)
        .map(String::from)
        .collect()
}

/// A reply, as the UI gets it, must not hold SQL: no SQL keyword sequences, none of the
/// quoted names or aliases of the SQL that was compiled for it.
fn assert_no_sql(f: &str, reply: &CheckReply, compiled_sql: &[String]) {
    let text = serde_json::to_string(reply).unwrap();
    let up = text.to_uppercase();
    for word in [
        "SELECT ",
        "CASE WHEN",
        "LEFT JOIN",
        "LIST_TRANSFORM",
        "COALESCE(",
        "NULLIF(",
        "CAST(",
        "TRY(",
        "ISFINITE",
        "IS NOT DISTINCT",
        "AS DOUBLE",
        "ORDER BY",
        "LIMIT 5",
    ] {
        assert!(!up.contains(word), "{f}: {text}");
    }
    // The aliases and names of the SQL: t0, r1, v0, p0, "amount", "orders".
    for alias in ["t0.", "r1.", "r2.", "g.p0", " AS p0", "v0 ->"] {
        assert!(!text.contains(alias), "{f}: {text}");
    }
    for sql in compiled_sql {
        for name in quoted_names(sql) {
            // A name in a JSON string is written with escaped quotes.
            assert!(
                !text.contains(&format!("\\\"{name}\\\"")),
                "{f}: {name} in {text}"
            );
        }
    }
}

/// The SQL that the formula compiles to, when it compiles.
fn sql_of(f: &str, kind: Kind, table: Option<&str>) -> Vec<String> {
    match compiled(f, kind, table) {
        Ok(Compiled {
            body: Body::Column(c),
            ..
        }) => vec![c.sql]
            .into_iter()
            .chain(c.joins.iter().map(Join::to_sql))
            .collect(),
        Ok(Compiled {
            body: Body::Aggregate(a),
            ..
        }) => a
            .parts
            .iter()
            .map(|p| p.sql.clone())
            .chain([a.combine.clone()])
            .collect(),
        Err(_) => vec![],
    }
}

#[test]
fn the_answer_never_holds_sql() {
    for (f, k, t) in [
        ("[amount] * 2", Kind::Column, Some("Orders")),
        ("[Stores].[rent] + [amount]", Kind::Column, Some("Orders")),
        (
            "ROUND(ROUND([amount] / 3, 2), 1)",
            Kind::Column,
            Some("Orders"),
        ),
        ("[amount] ^ 2", Kind::Column, Some("Orders")),
        ("SUM([Orders].[amount])", Kind::Measure, None),
        (
            "SUM([Orders].[amount]) / SUM([Targets].[target])",
            Kind::Measure,
            None,
        ),
        ("[amout]", Kind::Column, Some("Orders")),
        ("1 +", Kind::Column, Some("Orders")),
        ("[bad] + 1", Kind::Column, Some("Mismatch")),
        ("[g] + 1", Kind::Column, Some("Ghost")),
        ("SUM([amount])", Kind::Column, Some("Orders")),
    ] {
        let reply = ask(f, k, t);
        assert_no_sql(f, &reply, &sql_of(f, k, t));
    }
}

#[test]
fn a_column_needs_its_table() {
    let (m, at, len) = err_in("1", Kind::Column, None);
    assert_eq!(m, "Pick the table this formula belongs to first.");
    assert_eq!((at, len), (0, 0));
    assert_eq!(
        err_in("1", Kind::Group, None).0,
        "Pick the table this formula belongs to first."
    );
    assert_eq!(
        err_in("1", Kind::Column, Some("Nope")).0,
        "The table Nope is not in this workspace."
    );
}

// ---- errors, in words, with the place -------------------------------------------------

#[test]
fn an_unknown_column_suggests_the_closest() {
    assert_eq!(
        col_err("[amout] * 2"),
        (
            "No column [amout]. Did you mean [amount]?".into(),
            "[amout]".into()
        )
    );
    assert_eq!(
        col_err("[CUSTOMR]").0,
        "No column [CUSTOMR]. Did you mean [customer]?"
    );
}

#[test]
fn an_unknown_column_with_no_close_name() {
    assert_eq!(
        col_err("[zzzzzz]"),
        ("No column [zzzzzz] in Orders.".into(), "[zzzzzz]".into())
    );
}

#[test]
fn an_unknown_column_in_another_table() {
    assert_eq!(
        col_err("[Stores].[rnt]"),
        (
            "No column [rnt]. Did you mean [rent]?".into(),
            "[Stores].[rnt]".into()
        )
    );
}

#[test]
fn an_unknown_table_suggests_the_closest() {
    assert_eq!(
        col_err("[Ordrs].[amount]"),
        (
            "No table [Ordrs]. Did you mean [Orders]?".into(),
            "[Ordrs].[amount]".into()
        )
    );
    assert_eq!(
        col_err("[Zebra].[amount]").0,
        "No table [Zebra] in this workspace."
    );
}

#[test]
fn a_bare_reference_in_a_measure_names_the_fix() {
    assert_eq!(
        m_err("SUM([Orders].[amount]) + [Orders].[qty]"),
        (
            "[Orders].[qty] needs a total here, like SUM([Orders].[qty]).".into(),
            "[Orders].[qty]".into()
        )
    );
    assert_eq!(
        err("[amount] * 2", Kind::Measure, Some("Orders")),
        (
            "[amount] needs a total here, like SUM([amount]).".into(),
            "[amount]".into()
        )
    );
}

#[test]
fn a_bare_reference_in_a_group_names_the_fix() {
    assert_eq!(
        err("[amount]", Kind::Group, Some("Orders")).0,
        "[amount] needs a total here, like SUM([amount])."
    );
}

#[test]
fn a_measure_with_no_table_asks_for_one() {
    assert_eq!(
        m_err("SUM([amount])"),
        (
            "[amount] needs its table here. Write the table first, like [Table].[amount].".into(),
            "[amount]".into()
        )
    );
}

#[test]
fn a_total_in_a_column_points_to_measures() {
    assert_eq!(
        col_err("SUM([amount])"),
        (
            "SUM works over many rows, so it belongs in a measure, not in a calculated column. To read one value from a related table, use [Table].[Column]."
                .into(),
            "SUM".into()
        )
    );
    assert_eq!(col_err("COUNTROWS([Orders])").1, "COUNTROWS");
    // One value is a total; two are the smaller of them.
    assert_eq!(col_err("MIN([amount])").1, "MIN");
    assert!(compiled("MIN([amount], 5)", Kind::Column, Some("Orders")).is_ok());
}

#[test]
fn a_total_cannot_sit_in_a_total() {
    assert_eq!(
        m_err("SUM(AVERAGE([Orders].[amount]))"),
        ("AVERAGE cannot sit inside SUM.".into(), "AVERAGE".into())
    );
}

#[test]
fn a_measure_needs_a_total() {
    let (m, p) = m_err("1 + 2");
    assert!(m.starts_with("This formula has no total in it."), "{m}");
    assert_eq!(p, "1 + 2");
}

#[test]
fn a_total_needs_a_column() {
    assert_eq!(
        m_err("SUM(1)").0,
        "SUM needs a column to work on, like SUM([amount])."
    );
}

#[test]
fn tables_that_are_not_related_one_to_many_cannot_share_a_total() {
    assert_eq!(
        m_err("SUM([Orders].[amount] * [Targets].[target])").0,
        "SUM reads Orders and Targets, which are not related one to many. Total each table on its own, like SUM([A].[x]) / SUM([B].[y]), or draw a line between them in Model."
    );
}

#[test]
fn an_unknown_function_suggests_the_closest() {
    assert_eq!(
        col_err("SUMM([amount])"),
        ("No function SUMM. Did you mean SUM?".into(), "SUMM".into())
    );
    assert_eq!(
        col_err("ROUDN(1)").0,
        "No function ROUDN. Did you mean ROUND?"
    );
    assert_eq!(
        col_err("FOOBAR(1)").0,
        "No function FOOBAR. The list of functions is in the formulas guide."
    );
}

#[test]
fn the_wrong_number_of_values() {
    assert_eq!(
        col_err("ROUND(1, 2, 3)"),
        (
            "ROUND needs 1 to 2 values, but this has 3 values.".into(),
            "3".into()
        )
    );
    assert_eq!(
        col_err("IF(TRUE)"),
        (
            "IF needs 2 to 3 values, but this has 1 value.".into(),
            "IF(TRUE)".into()
        )
    );
    assert_eq!(
        col_err("NOW(1)").0,
        "NOW needs no values, but this has 1 value."
    );
    assert_eq!(
        col_err("MOD(1)").0,
        "MOD needs 2 values, but this has 1 value."
    );
    assert_eq!(col_err("ABS()").0, "ABS needs 1 value, but this has none.");
    assert_eq!(
        col_err("CONCAT()").0,
        "CONCAT needs at least 1 value, but this has none."
    );
}

#[test]
fn adding_text_to_a_number() {
    assert_eq!(
        col_err(r#""a" + 1"#),
        (
            "+ works on numbers and dates, but \"a\" is text. To join text, use &.".into(),
            "\"a\"".into()
        )
    );
    assert_eq!(col_err(r#"1 + "a""#).1, "\"a\"");
}

#[test]
fn arithmetic_on_the_wrong_kind() {
    assert_eq!(
        col_err("TRUE * 2"),
        (
            "* works on numbers, but TRUE is true or false. Use IF() to turn it into a number."
                .into(),
            "TRUE".into()
        )
    );
    assert_eq!(
        col_err(r#"[customer] * 2"#).0,
        "* works on numbers, but [customer] is text. Use VALUE() to read text as a number."
    );
    assert_eq!(
        col_err("[date] * 2").0,
        "* works on numbers, but [date] is a date."
    );
    assert_eq!(
        col_err(r#"-"a""#),
        (
            "- works on numbers, but \"a\" is text.".into(),
            "\"a\"".into()
        )
    );
    assert_eq!(col_err("2 ^ [customer]").1, "[customer]");
}

#[test]
fn dates_added_to_dates() {
    assert_eq!(
        col_err("[date] + [date]"),
        (
            "Two dates cannot be added. Add a number of days to a date, or subtract one date from another."
                .into(),
            "[date] + [date]".into()
        )
    );
    assert!(
        col_err("5 - [date]")
            .0
            .starts_with("A date cannot be taken from a number.")
    );
}

#[test]
fn comparing_unlike_kinds() {
    assert_eq!(
        col_err(r#"[customer] = 5"#),
        (
            "[customer] is text, but 5 is a number. = cannot compare them. Use VALUE() to read text as a number, or put the number in quotes to compare it as text."
                .into(),
            "[customer] = 5".into()
        )
    );
    assert_eq!(
        col_err("TRUE < FALSE").0,
        "TRUE is true or false, but FALSE is true or false. < cannot compare them. True and false can only be compared with = and <>."
    );
    assert!(
        col_err("[date] = [amount]")
            .0
            .contains("cannot compare them")
    );
}

#[test]
fn if_wants_a_test_and_two_results_of_one_kind() {
    assert_eq!(
        col_err("IF(1, 2, 3)"),
        (
            "IF needs a true-or-false test first, like [amount] > 100, but 1 is a number.".into(),
            "1".into()
        )
    );
    assert_eq!(
        col_err(r#"IF(TRUE, "a", 1)"#),
        (
            "IF needs values of the same kind, but \"a\" is text and 1 is a number. Use TEXT() or VALUE() to make them match."
                .into(),
            "1".into()
        )
    );
    assert_eq!(col_err("IFERROR(1, \"a\")").1, "\"a\"");
}

#[test]
fn ifs_wants_pairs() {
    assert_eq!(
        col_err("IFS(TRUE, 1, FALSE)"),
        (
            "IFS needs pairs: a test, then the result for it.".into(),
            "FALSE".into()
        )
    );
    assert_eq!(col_err("IFS(1, 1)").1, "1");
}

#[test]
fn switch_keys_must_match_the_value() {
    assert_eq!(
        col_err(r#"SWITCH([qty], "a", 1)"#),
        (
            "SWITCH compares [qty] (a number) with \"a\", which is text. They need to be the same kind."
                .into(),
            "\"a\"".into()
        )
    );
}

#[test]
fn and_or_not_want_tests() {
    assert_eq!(
        col_err("AND([paid], 1)").0,
        "AND needs a true-or-false test, like [amount] > 100, but 1 is a number."
    );
    assert!(col_err("NOT(1)").0.starts_with("NOT needs"));
}

#[test]
fn functions_that_want_numbers() {
    assert_eq!(
        col_err(r#"SQRT("x")"#),
        (
            "SQRT needs a number, but \"x\" is text. Use VALUE() to read text as a number.".into(),
            "\"x\"".into()
        )
    );
    assert_eq!(
        col_err("ROUND([date])").0,
        "ROUND needs a number, but [date] is a date."
    );
    assert_eq!(col_err(r#"LEFT("abc", "x")"#).1, "\"x\"");
    assert_eq!(col_err("MAX([customer], [customer])").1, "[customer]");
}

#[test]
fn functions_that_want_dates() {
    assert_eq!(
        col_err("YEAR([customer])"),
        (
            "YEAR needs a date, but [customer] is text. Set the column to the Date type in Data, or build one with DATE(year, month, day)."
                .into(),
            "[customer]".into()
        )
    );
    assert_eq!(col_err("EOMONTH([date], [customer])").1, "[customer]");
    assert_eq!(col_err("DAYS(1, [date])").1, "1");
}

#[test]
fn text_knows_four_formats() {
    assert_eq!(
        col_err(r#"TEXT(1, "0.00")"#),
        (
            "TEXT knows these formats: \"0\", \"0.0\", \"#,##0\" and \"0%\".".into(),
            "\"0.00\"".into()
        )
    );
    assert_eq!(
        col_err(r#"TEXT(1, [customer])"#).0,
        "TEXT needs the format in double quotes, like \"0.0\"."
    );
    assert_eq!(
        col_err(r#"TEXT([date], "0")"#),
        (
            "TEXT formats numbers only, and [date] is a date. To write a date as text, join its parts with &, like YEAR([date]) & \"-\" & MONTH([date])."
                .into(),
            "[date]".into()
        )
    );
}

#[test]
fn datedif_knows_three_units() {
    assert_eq!(
        col_err(r#"DATEDIF([date], [date], "W")"#),
        (
            "DATEDIF counts in \"D\" (days), \"M\" (months) or \"Y\" (years).".into(),
            "\"W\"".into()
        )
    );
    assert_eq!(
        col_err("DATEDIF([date], [date], 1)").0,
        "DATEDIF needs \"D\", \"M\" or \"Y\" as its last value."
    );
}

#[test]
fn percentile_needs_a_share() {
    let bad = "PERCENTILE([Orders].[amount], 90)";
    assert_eq!(
        m_err(bad),
        (
            "PERCENTILE needs a number between 0 and 1 as its second value, like 0.9.".into(),
            "90".into()
        )
    );
    assert!(
        m_err("PERCENTILE([Orders].[amount], [Orders].[qty])")
            .0
            .starts_with("PERCENTILE needs a number between")
    );
}

#[test]
fn totals_check_the_kind_of_what_they_read() {
    assert_eq!(
        m_err("SUM([Orders].[customer])"),
        (
            "SUM needs a number, but [Orders].[customer] is text. Use VALUE() to read text as a number."
                .into(),
            "[Orders].[customer]".into()
        )
    );
    assert_eq!(
        m_err("COUNT([Orders].[customer])"),
        (
            "COUNT needs numbers or dates, but [Orders].[customer] is text. To count filled cells of any kind, use COUNTA."
                .into(),
            "[Orders].[customer]".into()
        )
    );
    assert_eq!(
        m_err("SUMIF([Orders].[amount], [Orders].[qty])").0,
        "SUMIF needs a true-or-false test over the same table, like [region] = \"Java\", but [Orders].[qty] is a number."
    );
    assert!(
        m_err("MIN([Orders].[paid])")
            .0
            .starts_with("MIN needs numbers, dates or text")
    );
}

#[test]
fn countrows_wants_a_table() {
    assert_eq!(
        m_err("COUNTROWS(1)"),
        (
            "COUNTROWS needs a table name in brackets, like COUNTROWS([Orders]).".into(),
            "1".into()
        )
    );
    assert_eq!(
        m_err("COUNTROWS([Ordrs])").0,
        "No table [Ordrs]. Did you mean [Orders]?"
    );
    assert_eq!(
        m_err("COUNTROWS([Orders].[id])").0,
        "COUNTROWS needs a table name in brackets, like COUNTROWS([Orders])."
    );
}

#[test]
fn a_group_value_stays_with_its_table() {
    assert_eq!(
        err("COUNTROWS([Stores])", Kind::Group, Some("Orders")).0,
        "A group value counts the rows of Orders. Stores is another table."
    );
    // Targets is not on the way from Orders to any one row of it.
    assert_eq!(
        err("SUM([Targets].[target])", Kind::Group, Some("Orders")).0,
        "Targets is not related to Orders in the right direction, so a group value cannot read [Targets].[target]. Check the line between them in Model."
    );
}

// ---- the related-column rule, both ways ------------------------------------------------

#[test]
fn a_column_reads_towards_the_one_side() {
    // Orders -> Stores -> Regions: many to one, in one step and in two.
    assert!(compiled("[Stores].[rent]", Kind::Column, Some("Orders")).is_ok());
    assert!(compiled("[Regions].[name]", Kind::Column, Some("Orders")).is_ok());
    assert!(compiled("[Regions].[name]", Kind::Column, Some("Stores")).is_ok());
    assert!(compiled("[Regions].[name]", Kind::Column, Some("Targets")).is_ok());
}

#[test]
fn a_column_cannot_read_towards_the_many_side() {
    let (m, p) = err("[Orders].[amount]", Kind::Column, Some("Stores"));
    assert_eq!(
        m,
        "[Orders].[amount] has many values for each row of Stores. Total it in a measure with SUMIF or COUNTROWS, or check the line between Orders and Stores in Model."
    );
    assert_eq!(p, "[Orders].[amount]");
    // Two steps away is the same.
    assert!(
        err("[Orders].[amount]", Kind::Column, Some("Regions"))
            .0
            .contains("has many values")
    );
}

#[test]
fn a_column_cannot_read_a_table_it_is_not_related_to() {
    assert_eq!(
        col_err("[Targets].[target]"),
        (
            "Targets is not related to Orders. Draw a line between them in Model.".into(),
            "[Targets].[target]".into()
        )
    );
    assert!(
        err("[Odd].[x\"y]", Kind::Column, Some("Orders"))
            .0
            .contains("is not related")
    );
}

#[test]
fn what_a_column_reads_is_listed() {
    let c = compiled("[Stores].[rent] + [amount]", Kind::Column, Some("Orders")).unwrap();
    assert_eq!(c.uses, ["s2/Stores", "s1/Orders"]);
}

// ---- the guide -------------------------------------------------------------------------

const GUIDE: &str = include_str!("../../../../docs/formulas.md");

/// The formulas written after `marker` in the guide, between backticks.
fn examples(marker: &str) -> Vec<String> {
    GUIDE
        .lines()
        .filter_map(|l| {
            let rest = l.trim_start().strip_prefix(marker)?;
            let code = rest.trim().strip_prefix('`')?.strip_suffix('`')?;
            Some(code.to_string())
        })
        .collect()
}

/// The function names the guide lists, in its bullets: "- `YEAR(date)`, `MONTH(date)`: …".
fn guide_functions() -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    for l in GUIDE.lines().filter(|l| l.starts_with("- `")) {
        let head = l.split("`: ").next().unwrap_or(l);
        for span in head.split('`').skip(1).step_by(2) {
            if let Some((name, _)) = span.split_once('(')
                && !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            {
                names.insert(name.to_string());
            }
        }
    }
    names
}

#[test]
fn the_guide_lists_every_function_and_no_other() {
    let code: std::collections::BTreeSet<String> =
        functions::DEFS.iter().map(|d| d.name.to_string()).collect();
    assert_eq!(guide_functions(), code);
}

/// What each example of the guide gives in row 1 of the orders (2026-01-15, store 1, Ana, 100,
/// 2 cups, paid, "hello world", Java), with today fixed at 2026-10-11.
fn guide_columns() -> Vec<(&'static str, Value)> {
    vec![
        (r#"IF([amount] > 100, "big", "small")"#, json!("small")),
        (
            r#"IFS([amount] > 200, "high", [amount] > 50, "mid", TRUE, "low")"#,
            json!("mid"),
        ),
        (r#"SWITCH([region], "Java", 1, "Bali", 2, 0)"#, json!(1)),
        ("AND([amount] > 50, [paid])", json!(true)),
        (r#"OR([region] = "Java", [region] = "Bali")"#, json!(true)),
        ("NOT([paid])", json!(false)),
        ("IFERROR([amount] / [qty], 0)", json!(50)),
        ("ISBLANK([note])", json!(false)),
        ("IF([amount] > 0, [amount], BLANK())", json!(100)),
        ("ABS([amount] - 100)", json!(0)),
        ("ROUND([amount] / 3, 2)", json!(33.33)),
        ("ROUNDUP([amount] / 1000, 0)", json!(1)),
        ("ROUNDDOWN([amount], -2)", json!(100)),
        ("INT([amount] / 7)", json!(14)),
        ("MOD([qty], 2)", json!(0)),
        ("POWER([qty], 2)", json!(4)),
        ("SQRT([amount])", json!(10)),
        ("EXP([qty])", json!(7.38905609893065)),
        ("LN([amount])", json!(4.605170185988092)),
        ("LOG10([amount])", json!(2)),
        ("SIGN([amount] - 100)", json!(0)),
        ("MIN([amount], 500)", json!(100)),
        ("MAX([amount], 0)", json!(100)),
        ("DIVIDE([amount], [qty], 0)", json!(50)),
        (r#"[customer] & " (" & [region] & ")""#, json!("Ana (Java)")),
        (
            r#"CONCAT([customer], " - ", [region])"#,
            json!("Ana - Java"),
        ),
        ("LEFT([customer], 3)", json!("Ana")),
        ("RIGHT([region], 2)", json!("va")),
        ("MID([region], 2, 3)", json!("ava")),
        ("LEN([note])", json!(11)),
        ("UPPER([region])", json!("JAVA")),
        ("LOWER([region])", json!("java")),
        ("PROPER([customer])", json!("Ana")),
        ("TRIM([note])", json!("hello world")),
        (r#"SUBSTITUTE([note], "hello", "hi")"#, json!("hi world")),
        (r#"FIND("o", [note])"#, json!(5)),
        (r#"SEARCH("WORLD", [note])"#, json!(7)),
        ("VALUE([note])", Value::Null),
        (
            r#"VALUE(SUBSTITUTE(SUBSTITUTE([note], ".", ""), ",", "."))"#,
            Value::Null,
        ),
        (r##"TEXT([amount], "#,##0")"##, json!("100")),
        ("DATE(2026, [qty], 1)", json!("2026-02-01")),
        ("YEAR([date])", json!(2026)),
        ("MONTH([date])", json!(1)),
        ("DAY([date])", json!(15)),
        ("WEEKDAY([date])", json!(4)),
        ("WEEKNUM([date])", json!(3)),
        ("QUARTER([date])", json!(1)),
        ("EOMONTH([date], 0)", json!("2026-01-31")),
        ("EDATE([date], 1)", json!("2026-02-15")),
        (r#"DATEDIF([date], TODAY(), "M")"#, json!(8)),
        ("DAYS(TODAY(), [date])", json!(269)),
        ("TODAY()", json!("2026-10-11")),
        ("NOW()", json!("2026-10-11 14:03:22")),
        ("[amount] / [Stores].[rent]", json!(0.2)),
    ]
}

fn guide_totals() -> Vec<(&'static str, Value)> {
    let xs = [100.0, 250.0, 40.0, 0.0, 75.5, 10.0];
    let mean = xs.iter().sum::<f64>() / 6.0;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / 5.0;
    vec![
        ("SUM([Orders].[amount])", json!(475.5)),
        ("AVERAGE([Orders].[amount])", json!(79.25)),
        ("MAX([Orders].[date])", json!("2026-03-31")),
        ("MIN([Orders].[amount])", json!(0)),
        ("COUNT([Orders].[amount])", json!(6)),
        ("COUNTA([Orders].[customer])", json!(5)),
        ("COUNTD([Orders].[customer])", json!(5)),
        ("MEDIAN([Orders].[amount])", json!(57.75)),
        ("PERCENTILE([Orders].[amount], 0.9)", json!(175)),
        ("STDEV([Orders].[amount])", json!(var.sqrt())),
        ("VAR([Orders].[amount])", json!(var)),
        (
            r#"SUMIF([Orders].[amount], [Orders].[region] = "Java")"#,
            json!(350),
        ),
        (
            "COUNTIF([Orders].[amount], [Orders].[amount] > 100)",
            json!(1),
        ),
        (
            "AVERAGEIF([Orders].[amount], [Orders].[qty] >= 2)",
            json!(425.5 / 3.0),
        ),
        ("COUNTROWS([Orders])", json!(6)),
        (
            "SUM([Orders].[amount]) / SUM([Targets].[target])",
            json!(475.5 / 950.0),
        ),
    ]
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

#[test]
fn every_example_in_the_guide_is_listed_with_the_value_it_gives() {
    let col = guide_columns();
    let measure = guide_totals();
    // The guide and these lists hold the same formulas, so a changed example needs a new value.
    assert_eq!(
        sorted(examples("Example:")),
        sorted(col.iter().map(|c| c.0.to_string()).collect())
    );
    assert_eq!(
        sorted(examples("Measure example:")),
        sorted(measure.iter().map(|c| c.0.to_string()).collect())
    );
    for (f, want) in col {
        expect(1, f, want);
    }
    for (f, want) in measure {
        expect_total(f, want);
    }
    // The names an example uses cover the whole list of functions.
    let text = examples("Example:").join(" ") + &examples("Measure example:").join(" ");
    for d in functions::DEFS {
        assert!(
            text.contains(&format!("{}(", d.name)),
            "no example for {}",
            d.name
        );
    }
}

// ---- limits and size (V4-D24) ----------------------------------------------------------

fn wrap(open: &str, inner: &str, close: &str, n: usize) -> String {
    format!("{}{inner}{}", open.repeat(n), close.repeat(n))
}

/// What compiling gives for a formula on a thread with the stack of a server thread that runs
/// blocking work (2 MiB in tokio). A formula that could overflow a stack must be refused before
/// it gets deep. This test build has no optimisation, so its frames are the biggest there are.
fn on_a_small_stack(f: String) -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || compiled(&f, Kind::Column, Some("Orders")).map(|_| ()))
        .unwrap()
        .join()
        .expect("the thread must not overflow its stack")
        .map_err(|e| e.message)
}

const TOO_DEEP: &str = "This formula is nested too deeply.";

#[test]
fn a_formula_of_ten_thousand_characters_is_the_longest() {
    let f = format!("\"{}\"", "x".repeat(9998));
    assert_eq!(f.chars().count(), 10_000);
    assert!(compiled(&f, Kind::Column, Some("Orders")).is_ok());
    let f = format!("\"{}\"", "x".repeat(9999));
    assert_eq!(
        err_in(&f, Kind::Column, Some("Orders")),
        ("This formula is too long.".to_string(), 10_000, 1)
    );
}

#[test]
fn a_hundred_levels_are_allowed_and_a_hundred_and_one_are_not() {
    let shapes: [(&str, String, String); 6] = [
        ("brackets", "(".into(), ")".into()),
        ("minus signs", "-".into(), String::new()),
        ("NOT(", "NOT(".into(), ")".into()),
        ("ABS(", "ABS(".into(), ")".into()),
        ("carets", "2^".into(), String::new()),
        ("plus signs", String::new(), String::new()),
    ];
    for (what, open, close) in shapes {
        let inner = match what {
            "NOT(" => "TRUE",
            "carets" => "2",
            _ => "1",
        };
        let build = |n: usize| {
            if what == "plus signs" {
                format!("1{}", "+1".repeat(n))
            } else {
                wrap(&open, inner, &close, n)
            }
        };
        assert_eq!(on_a_small_stack(build(100)), Ok(()), "{what}");
        assert_eq!(on_a_small_stack(build(101)), Err(TOO_DEEP.into()), "{what}");
    }
}

#[test]
fn a_formula_nested_to_the_length_limit_is_refused_without_a_crash() {
    for f in [
        wrap("(", "1", ")", 3000),
        format!("{}1", "-".repeat(9000)),
        format!("{}1", "+".repeat(9000)),
        wrap("NOT(", "TRUE", ")", 1500),
        format!("{}2", "2^".repeat(4000)),
        format!("1{}", "+1".repeat(4000)),
        wrap("IF(TRUE, ", "1", ", 2)", 500),
    ] {
        assert_eq!(on_a_small_stack(f), Err(TOO_DEEP.into()));
    }
}

#[test]
fn the_nesting_error_points_at_the_level_that_breaks_it() {
    let (m, at, len) = err_in(&wrap("(", "1", ")", 101), Kind::Column, Some("Orders"));
    assert_eq!(m, TOO_DEEP);
    assert_eq!((at, len), (101, 1));
    // A chain breaks at the whole chain.
    let (m, at, _) = err_in(
        &format!("1{}", "+1".repeat(101)),
        Kind::Column,
        Some("Orders"),
    );
    assert_eq!((m.as_str(), at), (TOO_DEEP, 0));
}

fn nest(template: &str, depth: usize) -> String {
    let mut f = "[amount]".to_string();
    for _ in 0..depth {
        f = template.replace('@', &f);
    }
    f
}

fn column_sql(f: &str) -> String {
    match compiled(f, Kind::Column, Some("Orders")).unwrap().body {
        Body::Column(c) => c.sql,
        Body::Aggregate(_) => panic!("not a column"),
    }
}

/// Every way a value is used more than once in a function, nested.
const NESTED: [&str; 30] = [
    "ROUND(@, 2)",
    "ROUNDUP(@, 1)",
    "ROUNDDOWN(@ / 7, 1)",
    "ROUND(1.5, @ / 100)",
    "@ ^ 2",
    "2 ^ @",
    "POWER(@, 2)",
    "EXP(@ / 1000)",
    "1 / (@ + 1)",
    "MOD(@, 7)",
    "MOD(7, @ + 1)",
    "DIVIDE(@, 3, 1)",
    "DIVIDE(1, @ + 1, 0)",
    "DIVIDE(1, @ + 1)",
    "IFERROR(@, 1)",
    "SQRT(@ + 1)",
    "LEN(TEXT(@, \"0\")) * 10",
    "VALUE(TEXT(@, \"0.0\"))",
    "VALUE(TEXT(@, \"#,##0\"))",
    "VALUE(TEXT(@ / 100, \"0%\"))",
    "VALUE(\"1\" & @)",
    "FIND(\"1\", \"123\" & @)",
    "FIND(\"1\", \"123\", @ / 100 + 1)",
    "SEARCH(\"1\", \"123\", @ / 100 + 1)",
    "LEN(LEFT(\"abcdef\", @ / 100))",
    "LEN(RIGHT(\"abcdef\", @ / 100))",
    "LEN(MID(\"abcdef\", @ / 100 + 1, 2))",
    "SWITCH(@, 1, 2, 3)",
    "DATEDIF(DATE(2026, 1, 1), DATE(2026, 1, 1) + @, \"M\")",
    "YEAR(DATE(2026, 1, @))",
];

#[test]
fn nesting_never_writes_a_value_twice() {
    for t in NESTED {
        let (a, b) = (
            column_sql(&nest(t, 12)).len(),
            column_sql(&nest(t, 24)).len(),
        );
        // Twice the nesting is about twice the SQL. A value written twice at each level would
        // be thousands of times longer.
        assert!(b < 3 * a, "{t}: {a} bytes at 12 levels, {b} at 24");
        assert!(b < 20_000, "{t}: {b} bytes at 24 levels");
    }
}

#[test]
fn nested_formulas_run_fast_on_the_database() {
    for t in NESTED {
        let f = nest(t, 20);
        let start = Instant::now();
        let got = column(&f);
        assert_eq!(got.len(), 6, "{t}");
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{t}: {:?}",
            start.elapsed()
        );
    }
}

#[test]
fn twenty_carets_and_twenty_rounds_are_small_and_fast() {
    let carets = |n: usize| format!("{}2", "2^".repeat(n));
    let rounds = |n: usize| wrap("ROUND(", "1.5", ", 1)", n);
    for (what, build) in [
        ("^", &carets as &dyn Fn(usize) -> String),
        ("ROUND", &rounds),
    ] {
        let (a, b, c) = (
            column_sql(&build(10)).len(),
            column_sql(&build(20)).len(),
            column_sql(&build(40)).len(),
        );
        assert!(b < 3 * a && c < 3 * b, "{what}: {a}, {b}, {c}");
        let start = Instant::now();
        let _ = column(&build(20));
        assert!(start.elapsed() < Duration::from_secs(5), "{what}");
    }
    // 2^2^…^2 overflows to blank; the rounds stay at 1.5.
    expect(1, &carets(20), Value::Null);
    expect(1, &rounds(20), json!(1.5));
}

#[test]
fn a_formula_that_would_become_too_much_sql_is_refused() {
    assert_eq!(sql::MAX_SQL, 200_000);
    // A flat list of calls: no depth, so only the size can stop it.
    let list = |n: usize| format!("MAX({})", vec!["MOD([amount],[qty])"; n].join(","));
    let f = list(200);
    let typed = check::check(&f, Kind::Column, Some("Orders"), &cx()).unwrap();
    // About 68 KB of SQL. A limit of 50 KB refuses it, the real limit does not.
    let e = sql::compile_within(&typed, &cx(), 50_000).unwrap_err();
    assert_eq!(e.message, "This formula is too complex.");
    assert_eq!((e.at, e.len), (0, f.chars().count()));
    assert!(sql::compile_within(&typed, &cx(), sql::MAX_SQL).is_ok());
    // The longest formula that is allowed in characters stays under the real limit, so the
    // size limit is a safety net and not something a user meets by writing a long formula.
    let longest = list(490);
    assert!(longest.chars().count() <= 10_000);
    assert!(compiled(&longest, Kind::Column, Some("Orders")).is_ok());
}

// ---- totals of nothing (V4-D22) --------------------------------------------------------

#[test]
fn a_sum_of_nothing_is_zero_so_a_difference_survives() {
    let t = |f: &str| total(f, Some("Orders"));
    assert!(same(
        &t(r#"SUM([qty]) - SUMIF([qty], [customer] = "none")"#),
        &json!(12)
    ));
    assert!(same(&t(r#"SUMIF([qty], [customer] = "none")"#), &json!(0)));
    assert!(same(&t("COUNTIF([qty], [qty] > 100)"), &json!(0)));
}

#[test]
fn a_sum_across_tables_with_no_matching_rows_still_adds_up() {
    expect_total(
        r#"SUM([Orders].[amount]) - SUMIF([Targets].[target], [Targets].[month] = "2099-01")"#,
        json!(475.5),
    );
    expect_total(
        r#"SUM([Orders].[amount]) / (SUM([Targets].[target]) + SUMIF([Targets].[target], [Targets].[month] = "2099-01"))"#,
        json!(475.5 / 950.0),
    );
}

#[test]
fn counts_and_sums_of_an_empty_table_are_zero_and_the_rest_are_blank() {
    for f in [
        "SUM([Empty].[v])",
        "COUNT([Empty].[v])",
        "COUNTA([Empty].[t])",
        "COUNTD([Empty].[t])",
        "COUNTROWS([Empty])",
        "SUMIF([Empty].[v], [Empty].[v] > 1)",
        "COUNTIF([Empty].[v], [Empty].[v] > 1)",
    ] {
        expect_total(f, json!(0));
    }
    for f in [
        "AVERAGE([Empty].[v])",
        "MEDIAN([Empty].[v])",
        "MIN([Empty].[v])",
        "MAX([Empty].[v])",
        "MIN([Empty].[d])",
        "STDEV([Empty].[v])",
        "VAR([Empty].[v])",
        "PERCENTILE([Empty].[v], 0.5)",
        "AVERAGEIF([Empty].[v], [Empty].[v] > 1)",
        // Blank still spreads through the four operators.
        "AVERAGE([Empty].[v]) + 1",
        "SUM([Empty].[v]) + AVERAGE([Empty].[v])",
    ] {
        expect_total(f, Value::Null);
    }
}

#[test]
fn only_the_sums_and_counts_carry_the_zero() {
    let flags = |f: &str| {
        parts(f)
            .parts
            .iter()
            .map(|p| p.zero_when_empty)
            .collect::<Vec<_>>()
    };
    assert_eq!(flags("SUM([Orders].[amount])"), [true]);
    assert_eq!(flags("AVERAGE([Orders].[amount])"), [false]);
    assert_eq!(
        flags("SUM([Orders].[amount]) + MIN([Orders].[amount]) + COUNTD([Orders].[customer])"),
        [true, false, true]
    );
    let a = parts("SUM([Orders].[amount]) - AVERAGE([Orders].[amount])");
    let filled = a.fill(&["A".into(), "B".into()]);
    assert!(
        filled.contains("COALESCE(A, 0e0)") && !filled.contains("COALESCE(B"),
        "{filled}"
    );
}

// ---- blank takes a kind where it is used ------------------------------------------------

#[test]
fn a_blank_in_a_function_that_wants_a_kind_gives_blank() {
    for f in [
        "YEAR(BLANK())",
        "MONTH(BLANK())",
        "DAY(BLANK())",
        "QUARTER(BLANK())",
        "WEEKDAY(BLANK())",
        "WEEKNUM(BLANK())",
        "EOMONTH(BLANK(), 1)",
        "EDATE(BLANK(), 1)",
        "EOMONTH([date], BLANK())",
        r#"DATEDIF(BLANK(), [date], "D")"#,
        "DAYS(BLANK(), [date])",
        "DATE(BLANK(), 1, 1)",
        "SQRT(BLANK())",
        "ABS(BLANK())",
        "ROUND(BLANK(), 2)",
        "ROUND([amount], BLANK())",
        "MOD(BLANK(), 3)",
        "MOD(3, BLANK())",
        "POWER(BLANK(), 2)",
        "EXP(BLANK())",
        "-BLANK()",
        "BLANK() + 1",
        "BLANK() / 2",
        "BLANK() ^ 2",
        "[date] + BLANK()",
        "MAX(BLANK(), BLANK())",
        "LEFT(BLANK(), 2)",
        "MID(BLANK(), 1, 2)",
        "FIND(BLANK(), [note])",
        "VALUE(BLANK())",
        "UPPER(BLANK())",
        "TEXT(BLANK(), \"0\")",
        "IF(BLANK(), 1, 2) * BLANK()",
        "DIVIDE(BLANK(), 2, 0) * BLANK()",
    ] {
        expect(1, f, Value::Null);
    }
    // And a blank that meets a value in a test or a branch keeps working.
    expect(1, "IF(BLANK(), 1, 2)", json!(2));
    expect(1, "AND(BLANK(), TRUE)", Value::Null);
    expect(1, r#"LEN(BLANK())"#, json!(0));
}

// ---- a failure at run time -------------------------------------------------------------

#[test]
fn data_that_does_not_fit_the_column_kind_is_a_problem_with_the_kinds() {
    let r = ask("[bad] + 1", Kind::Column, Some("Mismatch"));
    assert!(!r.ok);
    let e = r.error.unwrap();
    assert_eq!(
        e.message,
        "The data could not be read with this formula. Check the kinds of the columns it uses."
    );
    // Over the whole formula.
    assert_eq!((e.at, e.len), (0, "[bad] + 1".chars().count()));
}

#[test]
fn a_source_that_is_gone_is_a_different_problem_with_nothing_to_underline() {
    let r = ask("[g] + 1", Kind::Column, Some("Ghost"));
    assert!(!r.ok);
    let e = r.error.unwrap();
    assert_eq!(
        e.message,
        "The data of this table could not be read. Check that its source is there, then try again."
    );
    assert_eq!((e.at, e.len), (0, 0));
}

// ---- a leading =, names without brackets, hints -----------------------------------------

#[test]
fn one_leading_equals_sign_is_ignored() {
    assert_eq!(column("=[amount] * 2"), column("[amount] * 2"));
    assert_eq!(column("  =  [amount] * 2"), column("[amount] * 2"));
    expect_total("=SUM([Orders].[amount])", json!(475.5));
    // Offsets still point into the text as it was written.
    assert_eq!(
        err_in("= [amout]", Kind::Column, Some("Orders")),
        (
            "No column [amout]. Did you mean [amount]?".to_string(),
            2,
            7
        )
    );
    assert_eq!(
        err_in("=", Kind::Column, Some("Orders")),
        ("Write a formula after the =.".to_string(), 0, 1)
    );
    // A second one is a mistake.
    assert_eq!(
        err_in("= = 1", Kind::Column, Some("Orders")).0,
        "A value is missing before =."
    );
}

#[test]
fn a_function_name_without_brackets_says_so() {
    assert_eq!(
        col_err("TODAY + 1"),
        ("TODAY needs brackets: TODAY().".into(), "TODAY".into())
    );
    assert_eq!(
        col_err("now"),
        ("NOW needs brackets: NOW().".into(), "now".into())
    );
    assert_eq!(
        col_err("blank"),
        ("BLANK needs brackets: BLANK().".into(), "blank".into())
    );
    assert_eq!(
        m_err("Sum + 1"),
        (
            "SUM needs brackets and its values, like SUM(...).".into(),
            "Sum".into()
        )
    );
    // A word that is not a function keeps its hint.
    assert_eq!(
        col_err("amount + 1").0,
        "amount is not a column or a function. Write column names in square brackets, like [amount]."
    );
}

#[test]
fn a_date_compared_with_text_shows_how_to_write_a_date() {
    assert_eq!(
        col_err(r#"[date] = "2026-01-15""#),
        (
            "[date] is a date, but \"2026-01-15\" is text. = cannot compare them. Use DATE(2026, 1, 15)."
                .into(),
            r#"[date] = "2026-01-15""#.into()
        )
    );
    assert_eq!(
        col_err(r#""2026-03-09" < [ts]"#).0,
        "\"2026-03-09\" is text, but [ts] is a date and time. < cannot compare them. Use DATE(2026, 3, 9)."
    );
    assert_eq!(
        col_err(r#"[date] > "15/01/2026""#).0,
        "[date] is a date, but \"15/01/2026\" is text. > cannot compare them. Use DATE(2026, 1, 1) to write a date."
    );
    assert_eq!(
        col_err("[date] = [customer]").0,
        "[date] is a date, but [customer] is text. = cannot compare them. Set the column to the Date type in Data, or write a date as DATE(2026, 1, 1)."
    );
    expect(1, r#"[date] = DATE(2026, 1, 15)"#, json!(true));
}

#[test]
fn if_results_of_two_kinds_show_the_text_way() {
    let (m, p) = col_err(r#"IF([amount] > 0, [amount], "n/a")"#);
    assert!(m.starts_with("IF needs values of the same kind"), "{m}");
    assert_eq!(p, "\"n/a\"");
    expect(
        1,
        r#"IF([amount] > 0, TEXT([amount], "0"), "n/a")"#,
        json!("100"),
    );
}

// ---- offsets in UTF-16 (V4-D23) ---------------------------------------------------------

#[test]
fn the_checker_counts_code_points_and_the_answer_counts_utf16_units() {
    let f = r#""😀" & [amout]"#;
    // Inside Rust: the emoji is one character.
    assert_eq!(
        err_in(f, Kind::Column, Some("Orders")),
        (
            "No column [amout]. Did you mean [amount]?".to_string(),
            6,
            7
        )
    );
    // At the API: it is two UTF-16 units, so everything after it moves by one.
    let e = ask(f, Kind::Column, Some("Orders")).error.unwrap();
    assert_eq!((e.at, e.len), (7, 7));
    // JavaScript's slice, done here with UTF-16 units, gets the underlined text.
    let units: Vec<u16> = f.encode_utf16().collect();
    assert_eq!(
        String::from_utf16(&units[e.at..e.at + e.len]).unwrap(),
        "[amout]"
    );
}

#[test]
fn letters_with_accents_take_one_unit() {
    let f = r#""año" & "niño" & [amout]"#;
    let (_, at, len) = err_in(f, Kind::Column, Some("Orders"));
    let e = ask(f, Kind::Column, Some("Orders")).error.unwrap();
    assert_eq!((e.at, e.len), (at, len));
    let units: Vec<u16> = f.encode_utf16().collect();
    assert_eq!(
        String::from_utf16(&units[e.at..e.at + e.len]).unwrap(),
        "[amout]"
    );
}

#[test]
fn a_span_that_holds_an_emoji_is_longer_in_utf16() {
    let f = "[a😀b] + 1";
    assert_eq!(
        err_in(f, Kind::Column, Some("Orders")),
        ("No column [a😀b] in Orders.".to_string(), 0, 5)
    );
    let e = ask(f, Kind::Column, Some("Orders")).error.unwrap();
    assert_eq!((e.at, e.len), (0, 6));
    // Past the end of the text, each missing character counts one.
    let e = FormulaError::new("x", 12, 0).in_utf16("😀");
    assert_eq!((e.at, e.len), (13, 0));
}

// ---- the rest of the review ------------------------------------------------------------

#[test]
fn the_length_of_a_blank_cell_is_zero() {
    expect(2, "LEN([note])", json!(0));
    expect(3, "LEN([note])", json!(0));
    expect(5, "LEN([customer])", json!(0));
    expect(1, "LEN([note])", json!(11));
}

#[test]
fn empty_text_is_blank_for_iferror_as_it_is_for_isblank() {
    expect(1, r#"IFERROR([note], "none")"#, json!("hello world"));
    expect(2, r#"IFERROR([note], "none")"#, json!("none"));
    expect(3, r#"IFERROR([note], "none")"#, json!("none"));
    // The same, when the first value is itself a formula.
    expect(3, r#"IFERROR(TRIM([note]), "none")"#, json!("none"));
    expect(3, "ISBLANK(TRIM([note]))", json!(true));
    expect(3, r#"IFERROR(UPPER([note]), "none")"#, json!("none"));
}

#[test]
fn a_negative_zero_is_written_as_zero() {
    expect(1, r#""a" & (0 * -1)"#, json!("a0"));
    expect(1, r#""a" & ROUND(-0.4)"#, json!("a0"));
    expect(1, r#"CONCAT("a", 0 * -1)"#, json!("a0"));
    expect(1, r#"TEXT(0 * -1, "0")"#, json!("0"));
}

#[test]
fn rounding_to_more_places_than_a_number_has_is_blank_not_nan() {
    for f in [
        "ROUND(1.5, 400)",
        "ROUND(1.5, -400)",
        "ROUNDUP(1.5, 400)",
        "ROUNDDOWN(1.5, 400)",
        "ROUND(1.5, [qty] * 200)",
        "ROUND(1.5, [qty] * -200)",
        "ROUND([amount], BLANK())",
    ] {
        expect(1, f, Value::Null);
    }
    expect(1, "ROUND(1.5, [qty] - 1)", json!(1.5));
    expect(1, "ROUND(1234.5678, [qty])", json!(1234.57));
    expect(1, "ROUND(1e300, 5)", json!(1e300));
    expect(1, "ROUND(2.5, 308)", json!(2.5));
}

#[test]
fn mod_is_cleaned_of_float_noise() {
    expect(1, r#"MOD(5.1, 1) & """#, json!("0.1"));
    expect(1, r#"MOD(0.3, 0.1) & """#, json!("0.1"));
    expect(1, r#"MOD(10.2, 5) & """#, json!("0.2"));
    expect(1, "MOD(7, 2.5)", json!(2));
    expect(1, "MOD(-7.3, 2)", json!(0.7));
    expect(1, "MOD(1000000, 7)", json!(1));
    expect(1, "MOD(5, 0)", Value::Null);
    expect(1, "MOD(0, 5)", json!(0));
}

#[test]
fn percentile_reaches_both_ends() {
    expect_total("PERCENTILE([Orders].[amount], 0)", json!(0));
    expect_total("PERCENTILE([Orders].[amount], 1)", json!(250));
}

#[test]
fn a_condition_can_read_a_related_table() {
    // Orders of the stores in region JB: stores 1 and 3.
    expect_total(
        r#"SUMIF([Orders].[amount], [Stores].[region_code] = "JB")"#,
        json!(100.0 + 250.0 + 75.5),
    );
    expect_total(
        r#"COUNTIF([Orders].[amount], [Regions].[name] = "jawa tengah")"#,
        json!(2),
    );
}

#[test]
fn value_reads_plain_dot_decimal_numbers_only() {
    for (f, want) in [
        (r#"VALUE("1234.5")"#, json!(1234.5)),
        (r#"VALUE("1e3")"#, json!(1000)),
        (r#"VALUE("-12")"#, json!(-12)),
        (r#"VALUE("1,234")"#, Value::Null),
        (r#"VALUE("12%")"#, Value::Null),
        (r#"VALUE("1,5")"#, Value::Null),
        (r#"VALUE("1.500,50")"#, Value::Null),
        (r#"VALUE("")"#, Value::Null),
        // The Indonesian way, turned into a plain number first.
        (
            r#"VALUE(SUBSTITUTE(SUBSTITUTE("1.500,50", ".", ""), ",", "."))"#,
            json!(1500.5),
        ),
    ] {
        expect(1, f, want);
    }
}

#[test]
fn the_minus_sign_binds_tighter_than_the_power_as_in_excel() {
    expect(1, "-2^2", json!(4));
    expect(1, "-(2^2)", json!(-4));
    expect(1, "2^-1", json!(0.5));
}

#[test]
fn weeknum_is_the_iso_week() {
    expect(1, "WEEKNUM(DATE(2026, 1, 4))", json!(1));
    expect(1, "WEEKNUM(DATE(2026, 1, 5))", json!(2));
    expect(1, "WEEKNUM(DATE(2021, 1, 3))", json!(53));
}

#[test]
fn division_by_zero_and_by_an_expression_that_is_zero_are_blank() {
    expect(1, "[amount] / 0", Value::Null);
    expect(1, "[amount] / ([qty] - 2)", Value::Null);
    expect(1, "[amount] / 4", json!(25));
    expect(1, "[amount] / -4", json!(-25));
    expect(1, "0 / 0", Value::Null);
    expect(1, "1e300 / 1e-300", Value::Null);
    expect(1, "DIVIDE([amount], [qty] - 2)", Value::Null);
    expect(1, "DIVIDE([amount], [qty] - 2, -1)", json!(-1));
}

// ---- a name that two columns answer to --------------------------------------------------

#[test]
fn a_name_that_two_columns_answer_to_is_an_error_that_names_both() {
    let msg = |f: &str| err(f, Kind::Column, Some("Pair"));
    assert_eq!(
        msg("[x] + 1"),
        (
            "[x] matches two columns: [x] by its name in the file, and [y], which the app shows as [x]. Rename one of them so a formula can tell them apart."
                .into(),
            "[x]".into()
        )
    );
    // Whatever the case.
    assert!(msg("[X] + 1").0.contains("matches two columns"));
    assert!(msg("[Pair].[x]").0.contains("matches two columns"));
    // The other column is still found by its own real name.
    assert!(compiled("[y] + 1", Kind::Column, Some("Pair")).is_ok());
}

#[test]
fn two_columns_that_show_the_same_name_are_ambiguous_too() {
    // The name "Cups sold" is the shown name of qty only, so it is fine; a second column
    // shown the same way would not be.
    assert!(compiled("[Cups sold]", Kind::Column, Some("Orders")).is_ok());
}
