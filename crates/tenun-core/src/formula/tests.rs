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
        prep: false,
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
    }
}

struct World {
    db: Mutex<Connection>,
    catalog: Catalog,
    rels: Vec<Relationship>,
}

fn world() -> &'static World {
    static W: OnceLock<World> = OnceLock::new();
    W.get_or_init(|| {
        use DType::*;
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
             INSERT INTO odd VALUES (7, 8);",
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
                ],
            },
            rels: vec![
                rel("r1", ("s1/Orders", "store_id"), ("s2/Stores", "id")),
                rel("r2", ("s2/Stores", "region_code"), ("s3/Regions", "code")),
                rel("r3", ("s4/Targets", "store_id"), ("s2/Stores", "id")),
            ],
        }
    })
}

fn cx() -> Context<'static> {
    let w = world();
    let today = NaiveDate::from_ymd_opt(2026, 10, 11).unwrap();
    Context {
        catalog: &w.catalog,
        rels: &w.rels,
        today,
        now: today.and_hms_opt(14, 3, 22).unwrap(),
    }
}

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
    datedif_counts_whole_months: r#"DATEDIF(DATE(2000, 1, 31), DATE(2000, 2, 29), "M")"# => 1;
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
    assert_eq!(a.combine, "($p0 / NULLIF($p1, 0e0))");
    assert_eq!(a.fill(&["A".into(), "B".into()]), "(A / NULLIF(B, 0e0))");
}

#[test]
fn a_join_of_the_two_tables_would_count_orders_twice() {
    // This is what V4-D4 prevents: with a join, store 1's two orders meet its two targets.
    let naive = query(
        "SELECT sum(o.amount) FROM orders o JOIN stores s ON o.store_id = s.id \
         JOIN targets t ON t.store_id = s.id",
    );
    assert!(same(&naive[0], &json!(855.5)));
    assert!(!same(&naive[0], &json!(475.5)));
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
    for f in [
        "[AMOUNT]",
        "[Amount]",
        "[orders].[amount]",
        "[ORDERS].[Amount]",
        "[s1/Orders].[amount]",
        "sum([x])",
    ] {
        let kind = if f.starts_with("sum") {
            Kind::Measure
        } else {
            Kind::Column
        };
        let f = f.replace("[x]", "[orders].[amount]");
        assert!(compiled(&f, kind, Some("Orders")).is_ok(), "{f}");
    }
    assert!(compiled("if(TRUE, 1, 2)", Kind::Column, Some("orders")).is_ok());
    assert!(compiled("True & false", Kind::Column, Some("ORDERS")).is_ok());
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

#[test]
fn the_answer_never_holds_sql() {
    for (f, k, t) in [
        ("[amount] * 2", Kind::Column, Some("Orders")),
        ("SUM([Orders].[amount])", Kind::Measure, None),
        ("[amout]", Kind::Column, Some("Orders")),
        ("1 +", Kind::Column, Some("Orders")),
    ] {
        let text = serde_json::to_string(&ask(f, k, t)).unwrap().to_uppercase();
        for word in [
            "SELECT ",
            "CAST(",
            "T0.",
            "LEFT JOIN",
            "NULLIF(",
            "AS DOUBLE",
        ] {
            assert!(!text.contains(word), "{f}: {text}");
        }
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
        col_err(r#"TEXT([date], "0")"#).0,
        "TEXT needs a number, but [date] is a date."
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

#[test]
fn every_function_in_the_guide_has_an_example_that_runs() {
    let col = examples("Example:");
    let measure = examples("Measure example:");
    // One example for every function, a few more for the rules.
    assert!(
        col.len() + measure.len() >= functions::DEFS.len(),
        "{} examples",
        col.len() + measure.len()
    );
    for f in &col {
        let _ = column(f);
    }
    for f in &measure {
        let _ = total(f, None);
    }
    // The names an example uses cover the whole list.
    let text = format!("{} {}", col.join(" "), measure.join(" "));
    for d in functions::DEFS {
        assert!(
            text.contains(&format!("{}(", d.name)),
            "no example for {}",
            d.name
        );
    }
}
