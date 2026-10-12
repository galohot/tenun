//! The check of a calculated column written as SQL (Advanced, V4-D62).
//!
//! A `.tenun` file can come from someone else, and a published page may show what a column
//! holds. So opening a file must never run a statement that reads a local file, the environment
//! or another table into a column. The text is **one expression** over the columns of the table.
//!
//! DuckDB's own parser would be the best judge, but the bundled build has no JSON extension, so
//! `json_serialize_sql` is not there, and adding the extension rebuilds DuckDB (V4-D49). The
//! check is a small reader of its own instead, and it **denies by default**: every word of the
//! text must be one of a short list of words, a function on a hand-reviewed list ([`FUNCTIONS`],
//! V4-D75), or a column of the table. A subquery needs `SELECT`, `WITH`, `FROM`, `VALUES`, `TABLE`
//! or another reserved word, none of which is on the list, and a table function can only follow
//! `FROM`. So neither can be written.
//!
//! What it refuses (each with a plain sentence and the place to underline):
//! - `;`, comments (`--`, `/* */`), `$`, `?`, `.`, brackets and braces, `->`, and a quote that
//!   follows a letter (`E'..'`), because they can hide what the reader sees from what DuckDB sees.
//! - Any reserved word that is not on the list: `SELECT`, `FROM`, `WITH`, `JOIN`, `OVER`, `LAMBDA`…
//! - Any function that is not on the list. Not on it: what reads a file, the environment or the
//!   network, what changes from call to call, what runs a lambda, and what makes a result as big as
//!   an argument says (`repeat`, `lpad`, `range`, `printf`…).
//! - Any name that is not a column of the table.
//! - Two or more expressions (a comma at the top level) and unbalanced brackets.
//!
//! After the words are checked, DuckDB binds the expression with `EXPLAIN`, and a message that
//! comes back names a type that does not fit, not a value. The word check and the binding together
//! run under a limit of [`CHECK_LIMIT`] (2 seconds); past it the column is refused.

use super::FormulaError;
use crate::catalog::ident;
use duckdb::Connection;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The most characters a SQL column may have, as for a formula (V4-D24).
pub const MAX_CHARS: usize = 10_000;

/// The longest the check of one column may take: the words, then DuckDB's binding (V4-D75).
pub const CHECK_LIMIT: Duration = Duration::from_secs(2);

/// Words that are part of an expression and not a name: logic, `CASE`, `CAST`, tests and the
/// values that have no digits.
const WORDS: &[&str] = &[
    "and",
    "or",
    "not",
    "is",
    "null",
    "true",
    "false",
    "in",
    "between",
    "like",
    "ilike",
    "similar",
    "to",
    "escape",
    "case",
    "when",
    "then",
    "else",
    "end",
    "cast",
    "try_cast",
    "as",
    "distinct",
    "from",
    "interval",
    "current_date",
    "current_time",
    "current_timestamp",
    "localtime",
    "localtimestamp",
];

/// Types that may follow `AS` in a cast, or `::`.
const TYPES: &[&str] = &[
    "integer",
    "int",
    "int4",
    "int8",
    "bigint",
    "smallint",
    "tinyint",
    "hugeint",
    "double",
    "float",
    "real",
    "decimal",
    "numeric",
    "varchar",
    "text",
    "string",
    "char",
    "bpchar",
    "boolean",
    "bool",
    "date",
    "time",
    "timestamp",
    "timestamptz",
    "datetime",
    "interval",
];

/// The types that may come before a text, as in `DATE '2026-01-31'`.
const TYPED_LITERALS: &[&str] = &["date", "time", "timestamp", "timestamptz", "interval"];

/// The units after `INTERVAL`.
const UNITS: &[&str] = &[
    "year",
    "years",
    "quarter",
    "quarters",
    "month",
    "months",
    "week",
    "weeks",
    "day",
    "days",
    "hour",
    "hours",
    "minute",
    "minutes",
    "second",
    "seconds",
    "millisecond",
    "milliseconds",
    "microsecond",
    "microseconds",
];

/// Words that start a statement or a subquery. A reader who meets one is told why it is refused.
const STARTS_A_QUERY: &[&str] = &[
    "select",
    "with",
    "from",
    "values",
    "table",
    "pivot",
    "unpivot",
    "describe",
    "show",
    "summarize",
    "insert",
    "update",
    "delete",
    "create",
    "drop",
    "attach",
    "detach",
    "copy",
    "install",
    "load",
    "pragma",
    "call",
    "set",
    "union",
    "except",
    "intersect",
    "join",
    "exists",
];

/// The functions an expression may call (V4-D75). **This list is the rule.** It is written by hand
/// and reviewed: no list is read from DuckDB at run time, so what passes never depends on what ran
/// before (a name that only exists once the ICU extension has loaded is not on it), and a new
/// DuckDB function is refused until someone adds it here. A test fails when a name below is missing
/// from the bundled DuckDB, and when one of the names in [`REFUSED`] is on it.
///
/// What is on it: plain scalar functions of numbers, text, dates and lists. What is not: anything
/// that reads a file, the environment, the clock of the system or the catalog (`getenv`,
/// `current_setting`, `read_*`, `glob`, `query*`, `pg_*`, `current_*`), anything that changes from
/// one call to the next (`random`, `uuid`, `nextval`), anything that takes a function to run
/// (`list_transform`, `list_reduce`, `apply`: lambdas are refused), and anything whose result is as
/// big as an argument says (see [`REFUSED`]). Sorted, for the binary search.
#[rustfmt::skip]
const FUNCTIONS: &[&str] = &[
    "abs", "acos", "acosh", "add", "age", "array_append",
    "array_cat", "array_concat", "array_contains", "array_distinct", "array_extract", "array_has",
    "array_has_all", "array_has_any", "array_indexof", "array_intersect", "array_length", "array_pop_back",
    "array_pop_front", "array_position", "array_prepend", "array_push_back", "array_push_front", "array_reverse",
    "array_reverse_sort", "array_slice", "array_sort", "array_to_string", "array_unique", "array_value",
    "array_zip", "ascii", "asin", "asinh", "atan", "atan2",
    "atanh", "base64", "bin", "bit_count", "bit_length", "cardinality",
    "cbrt", "ceil", "ceiling", "century", "char_length", "character_length",
    "chr", "concat", "concat_ws", "contains", "cos", "cosh",
    "cot", "damerau_levenshtein", "date_add", "date_diff", "date_part", "date_sub",
    "date_trunc", "datediff", "datepart", "datesub", "datetrunc", "day",
    "dayname", "dayofmonth", "dayofweek", "dayofyear", "days_in_month", "decade",
    "decode", "degrees", "divide", "editdist3", "element_at", "encode",
    "ends_with", "epoch", "epoch_ms", "epoch_ns", "epoch_us", "era",
    "even", "exp", "factorial", "fdiv", "flatten", "floor",
    "fmod", "format_bytes", "formatreadabledecimalsize", "formatreadablesize", "from_base64", "from_binary",
    "from_hex", "gamma", "gcd", "greatest", "greatest_common_divisor", "hamming",
    "hash", "hex", "hour", "instr", "isfinite", "isinf",
    "isnan", "isodow", "isoyear", "jaccard", "jaro_similarity", "jaro_winkler_similarity",
    "julian", "last_day", "lcase", "lcm", "least", "least_common_multiple",
    "left", "left_grapheme", "len", "length", "length_grapheme", "levenshtein",
    "lgamma", "list_any_value", "list_append", "list_approx_count_distinct", "list_avg", "list_bool_and",
    "list_bool_or", "list_cat", "list_concat", "list_contains", "list_count", "list_distinct",
    "list_element", "list_entropy", "list_extract", "list_first", "list_has", "list_has_all",
    "list_has_any", "list_indexof", "list_intersect", "list_kurtosis", "list_kurtosis_pop", "list_last",
    "list_mad", "list_max", "list_median", "list_min", "list_mode", "list_pack",
    "list_position", "list_prepend", "list_product", "list_reverse", "list_reverse_sort", "list_select",
    "list_sem", "list_skewness", "list_slice", "list_sort", "list_stddev_pop", "list_stddev_samp",
    "list_string_agg", "list_sum", "list_unique", "list_value", "list_var_pop", "list_var_samp",
    "list_where", "list_zip", "ln", "log", "log10", "log2",
    "lower", "ltrim", "make_date", "make_time", "make_timestamp", "make_timestamp_ms",
    "make_timestamp_ns", "md5", "md5_number", "md5_number_lower", "md5_number_upper", "microsecond",
    "millennium", "millisecond", "minute", "mismatches", "mod", "month",
    "monthname", "multiply", "nanosecond", "nextafter", "nfc_normalize", "now",
    "nullif", "octet_length", "ord", "pi", "position", "pow",
    "power", "prefix", "quarter", "radians", "regexp_escape", "regexp_extract",
    "regexp_extract_all", "regexp_full_match", "regexp_matches", "regexp_replace", "regexp_split_to_array", "replace",
    "reverse", "right", "right_grapheme", "round", "round_even", "roundbankers",
    "rtrim", "second", "sha1", "sha256", "sign", "signbit",
    "sin", "sinh", "split", "split_part", "sqrt", "starts_with",
    "str_split", "str_split_regex", "strftime", "string_split", "string_split_regex", "string_to_array",
    "strip_accents", "strlen", "strpos", "strptime", "substr", "substring",
    "substring_grapheme", "subtract", "suffix", "tan", "tanh", "time_bucket",
    "to_base64", "to_binary", "to_centuries", "to_days", "to_decades", "to_hex",
    "to_hours", "to_microseconds", "to_millennia", "to_milliseconds", "to_minutes", "to_months",
    "to_quarters", "to_seconds", "to_timestamp", "to_weeks", "to_years", "translate",
    "trim", "trunc", "try_strptime", "typeof", "ucase", "unbin",
    "unhex", "unicode", "upper", "url_decode", "url_encode", "uuid_extract_timestamp",
    "uuid_extract_version", "week", "weekday", "weekofyear", "xor", "year",
    "yearweek",
];

/// The functions the parser handles itself, so DuckDB does not list them.
const PARSER_FUNCTIONS: &[&str] = &["coalesce", "ifnull"];

/// Functions that were looked at and are refused, with the reason (V4-D75). The test of the list
/// checks that none of them is allowed.
#[allow(dead_code)]
const REFUSED: &[(&str, &str)] = &[
    ("repeat", "its result is as long as an argument says"),
    ("lpad", "its result is as long as an argument says"),
    ("rpad", "its result is as long as an argument says"),
    ("range", "its result has as many items as an argument says"),
    (
        "generate_series",
        "its result has as many items as an argument says",
    ),
    (
        "list_resize",
        "its result has as many items as an argument says",
    ),
    (
        "array_resize",
        "its result has as many items as an argument says",
    ),
    ("printf", "a format can ask for any width"),
    ("format", "a format can ask for any width"),
    ("bar", "its result is as wide as an argument says"),
    ("bitstring", "its result is as long as an argument says"),
    ("to_base", "its result is as long as an argument says"),
    ("list_transform", "it runs a lambda"),
    ("list_filter", "it runs a lambda"),
    (
        "list_reduce",
        "it runs a lambda: it can double a text over and over",
    ),
    ("apply", "it runs a lambda"),
    ("reduce", "it runs a lambda"),
    ("filter", "it runs a lambda"),
    ("getenv", "it reads the environment"),
    ("current_setting", "it reads the settings"),
    ("read_text", "it reads a file"),
    ("read_csv", "it reads a file"),
    ("glob", "it lists files"),
    ("query", "it runs a statement"),
    ("query_table", "it reads a table"),
    ("sniff_csv", "it reads a file"),
    ("random", "it changes from one call to the next"),
    ("uuid", "it changes from one call to the next"),
    ("nextval", "it changes a sequence"),
    ("today", "it exists only once the ICU extension has loaded"),
    ("current_date", "a word of the language, not a function"),
];

/// The words DuckDB will not take as a name unless they are quoted (`duckdb_keywords()`, the
/// category `reserved`). A test checks the list against the bundled DuckDB, so a new reserved word
/// shows up as a failing test.
#[rustfmt::skip]
const RESERVED: &[&str] = &[
    "all", "analyse", "analyze", "and", "any", "array", "as", "asc",
    "asymmetric", "both", "case", "cast", "check", "collate", "column", "constraint",
    "create", "default", "deferrable", "desc", "describe", "distinct", "do", "else",
    "end", "except", "false", "fetch", "for", "foreign", "from", "group",
    "having", "in", "initially", "intersect", "into", "lambda", "lateral", "leading",
    "limit", "not", "null", "offset", "on", "only", "or", "order",
    "pivot", "pivot_longer", "pivot_wider", "placing", "primary", "qualify", "references", "returning",
    "select", "show", "some", "summarize", "symmetric", "table", "then", "to",
    "trailing", "true", "union", "unique", "unpivot", "using", "variadic", "when",
    "where", "window", "with",
];

fn function_allowed(name: &str) -> bool {
    let lc = name.to_ascii_lowercase();
    PARSER_FUNCTIONS.contains(&lc.as_str()) || FUNCTIONS.binary_search(&lc.as_str()).is_ok()
}

fn reserved(word: &str) -> bool {
    RESERVED.contains(&word)
}

// ---- reading -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum T {
    Num,
    Str,
    /// An unquoted word, as written.
    Word(String),
    /// A name between double quotes, without the quotes.
    Quoted(String),
    Op(&'static str),
}

#[derive(Debug, Clone)]
struct Tk {
    t: T,
    at: usize,
    len: usize,
}

const OPS2: &[&str] = &[
    "<=", ">=", "<>", "!=", "==", "||", "<<", ">>", "::", "**", "//",
];
const OPS1: &[&str] = &[
    "+", "-", "*", "/", "%", "^", "&", "|", "~", "=", "<", ">", "(", ")", ",",
];

fn bad(msg: impl Into<String>, at: usize, len: usize) -> FormulaError {
    FormulaError::new(msg, at, len)
}

fn lex(text: &str) -> Result<Vec<Tk>, FormulaError> {
    let c: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        let at = i;
        match ch {
            ' ' | '\t' | '\n' | '\r' => {
                i += 1;
            }
            '0'..='9' | '.' if ch != '.' || c.get(i + 1).is_some_and(char::is_ascii_digit) => {
                while c.get(i).is_some_and(char::is_ascii_digit) {
                    i += 1;
                }
                if c.get(i) == Some(&'.') {
                    i += 1;
                    while c.get(i).is_some_and(char::is_ascii_digit) {
                        i += 1;
                    }
                }
                if matches!(c.get(i), Some('e' | 'E')) {
                    let mut j = i + 1;
                    if matches!(c.get(j), Some('+' | '-')) {
                        j += 1;
                    }
                    if c.get(j).is_some_and(char::is_ascii_digit) {
                        while c.get(j).is_some_and(char::is_ascii_digit) {
                            j += 1;
                        }
                        i = j;
                    }
                }
                if c.get(i).is_some_and(|x| x.is_alphanumeric() || *x == '_') {
                    return Err(bad(
                        "A number is followed by letters. Put a space or an operator after it.",
                        at,
                        i - at + 1,
                    ));
                }
                out.push(Tk {
                    t: T::Num,
                    at,
                    len: i - at,
                });
            }
            '\'' => {
                i += 1;
                loop {
                    match c.get(i) {
                        None => {
                            return Err(bad(
                                "This text starts with a quote that is never closed.",
                                at,
                                1,
                            ));
                        }
                        Some('\'') if c.get(i + 1) == Some(&'\'') => i += 2,
                        Some('\'') => {
                            i += 1;
                            break;
                        }
                        Some(_) => i += 1,
                    }
                }
                out.push(Tk {
                    t: T::Str,
                    at,
                    len: i - at,
                });
            }
            '"' => {
                i += 1;
                let mut name = String::new();
                loop {
                    match c.get(i) {
                        None => {
                            return Err(bad(
                                "This name starts with a double quote that is never closed.",
                                at,
                                1,
                            ));
                        }
                        Some('"') if c.get(i + 1) == Some(&'"') => {
                            name.push('"');
                            i += 2;
                        }
                        Some('"') => {
                            i += 1;
                            break;
                        }
                        Some(x) => {
                            name.push(*x);
                            i += 1;
                        }
                    }
                }
                out.push(Tk {
                    t: T::Quoted(name),
                    at,
                    len: i - at,
                });
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                while c
                    .get(i)
                    .is_some_and(|x| x.is_ascii_alphanumeric() || *x == '_')
                {
                    i += 1;
                }
                if c.get(i).is_some_and(|x| x.is_alphanumeric()) {
                    return Err(bad(
                        "Write a name with accents between double quotes, like \"Total sales\".",
                        at,
                        i - at + 1,
                    ));
                }
                if c.get(i) == Some(&'\'') {
                    return Err(bad(
                        "Put a space between a word and a quote.",
                        at,
                        i - at + 1,
                    ));
                }
                out.push(Tk {
                    t: T::Word(c[at..i].iter().collect()),
                    at,
                    len: i - at,
                });
            }
            ';' => {
                return Err(bad(
                    "A ; ends a statement. A column formula is one expression, so leave it out.",
                    at,
                    1,
                ));
            }
            '-' if c.get(i + 1) == Some(&'-') => {
                return Err(bad(
                    "Comments (--) are not allowed in a column formula.",
                    at,
                    2,
                ));
            }
            '/' if c.get(i + 1) == Some(&'*') => {
                return Err(bad(
                    "Comments (/* */) are not allowed in a column formula.",
                    at,
                    2,
                ));
            }
            '-' if c.get(i + 1) == Some(&'>') => {
                return Err(bad(
                    "The arrow -> is not available in a column formula.",
                    at,
                    2,
                ));
            }
            '.' => {
                return Err(bad(
                    "A dot is not available here. Write the name of a column of this table, between double quotes if it has spaces.",
                    at,
                    1,
                ));
            }
            '$' | '?' | '@' | '#' | '[' | ']' | '{' | '}' | '`' | '\\' | ':' | '!' => {
                // `!=` and `::` are operators; the rest of these have no place in an expression.
                let two: String = c[i..(i + 2).min(c.len())].iter().collect();
                if two == "!=" || two == "::" {
                    // Handled below.
                } else {
                    return Err(bad(
                        format!("The character {ch} is not available in a column formula."),
                        at,
                        1,
                    ));
                }
                let op = OPS2.iter().find(|o| **o == two).copied().expect("listed");
                out.push(Tk {
                    t: T::Op(op),
                    at,
                    len: 2,
                });
                i += 2;
            }
            _ => {
                let two: String = c[i..(i + 2).min(c.len())].iter().collect();
                if let Some(op) = OPS2.iter().find(|o| **o == two).copied() {
                    out.push(Tk {
                        t: T::Op(op),
                        at,
                        len: 2,
                    });
                    i += 2;
                } else if let Some(op) = OPS1.iter().find(|o| o.starts_with(ch)).copied() {
                    out.push(Tk {
                        t: T::Op(op),
                        at,
                        len: 1,
                    });
                    i += 1;
                } else if ch.is_alphabetic() {
                    return Err(bad(
                        "Write a name with accents between double quotes, like \"Total sales\".",
                        at,
                        1,
                    ));
                } else {
                    return Err(bad(
                        format!("The character {ch} is not available in a column formula."),
                        at,
                        1,
                    ));
                }
            }
        }
    }
    Ok(out)
}

// ---- judging -----------------------------------------------------------------------------

/// Checks that `text` is one expression made only of what a column formula may use, then has
/// DuckDB bind it over `source` (a SELECT that gives the columns the expression may read).
///
/// `columns` are the names of those columns. `table` is the label of the table, for the words of
/// a message.
pub fn check(
    db: &Connection,
    text: &str,
    columns: &[String],
    table: &str,
    source: &str,
) -> Result<(), FormulaError> {
    check_within(db, text, columns, table, source, CHECK_LIMIT)
}

/// Like [`check`], with a limit of your own on the time (the tests use a tiny one). A check that
/// takes the limit or longer is refused, whether DuckDB had finished or not.
pub fn check_within(
    db: &Connection,
    text: &str,
    columns: &[String],
    table: &str,
    source: &str,
    limit: Duration,
) -> Result<(), FormulaError> {
    let started = Instant::now();
    let len = text.chars().count();
    if text.trim().is_empty() {
        return Err(bad("Write the expression for this column.", 0, 0));
    }
    if len > MAX_CHARS {
        return Err(bad(
            format!("This formula has more than {MAX_CHARS} characters. Make it shorter."),
            MAX_CHARS,
            len - MAX_CHARS,
        ));
    }
    let tokens = lex(text)?;
    if tokens.is_empty() {
        return Err(bad("Write the expression for this column.", 0, 0));
    }
    judge(&tokens, columns, table)?;
    // DuckDB binds it, under the limit. The wrapper is the one the view uses. A watchdog stops the
    // query when the time is up; one that ends in time cancels the watchdog and waits for it, so
    // it can never stop the next query.
    let sql = format!("EXPLAIN SELECT TRY(({text})) FROM ({source}) AS t0 LIMIT 0");
    let handle = db.interrupt_handle();
    let (done, wait) = mpsc::channel::<()>();
    let watchdog = std::thread::spawn(move || {
        if wait.recv_timeout(limit) == Err(mpsc::RecvTimeoutError::Timeout) {
            handle.interrupt();
        }
    });
    let outcome = db.prepare(&sql).and_then(|mut s| s.query([]).map(|_| ()));
    let _ = done.send(());
    let _ = watchdog.join();
    let late = started.elapsed() >= limit;
    match outcome {
        _ if late => Err(bad(
            format!(
                "Tenun stopped checking this column after {} seconds.",
                CHECK_LIMIT.as_secs()
            ),
            0,
            len,
        )),
        Err(e) => Err(bad(bind_message(&format!("{e}")), 0, len)),
        Ok(()) => Ok(()),
    }
}

/// The first line of what DuckDB says about an expression it cannot bind, shortened. It names a
/// function and the types it was given, and the user's own text; it holds no data.
fn bind_message(msg: &str) -> String {
    let first = msg.lines().next().unwrap_or("").trim();
    let first = first
        .strip_prefix("Binder Error: ")
        .or_else(|| first.strip_prefix("Parser Error: "))
        .or_else(|| first.strip_prefix("Conversion Error: "))
        .unwrap_or(first);
    let short: String = first.chars().take(160).collect();
    format!("DuckDB cannot use this expression: {short}")
}

fn is_word(t: Option<&Tk>, w: &str) -> bool {
    matches!(t, Some(Tk { t: T::Word(x), .. }) if x.eq_ignore_ascii_case(w))
}

fn judge(tokens: &[Tk], columns: &[String], table: &str) -> Result<(), FormulaError> {
    let mut depth = 0usize;
    let mut opened: Vec<&Tk> = Vec::new();
    let mut seen_interval = false;
    let mut i = 0;
    while i < tokens.len() {
        let tk = &tokens[i];
        let prev = i.checked_sub(1).map(|j| &tokens[j]);
        let next = tokens.get(i + 1);
        let next_is_paren = matches!(next, Some(Tk { t: T::Op("("), .. }));
        match &tk.t {
            T::Num | T::Str => {}
            T::Op("(") => {
                depth += 1;
                opened.push(tk);
            }
            T::Op(")") => {
                if depth == 0 {
                    return Err(bad("This ) has no ( before it.", tk.at, 1));
                }
                depth -= 1;
                opened.pop();
            }
            T::Op(",") => {
                if depth == 0 {
                    return Err(bad(
                        "A column holds one value. Use one expression, with no comma outside brackets.",
                        tk.at,
                        1,
                    ));
                }
            }
            T::Op("::") => {
                let ok = matches!(next, Some(Tk { t: T::Word(w), .. }) if TYPES.contains(&w.to_ascii_lowercase().as_str()));
                if !ok {
                    return Err(bad(
                        "After :: write a type, like DOUBLE, VARCHAR or DATE.",
                        tk.at,
                        2,
                    ));
                }
            }
            T::Op(_) => {}
            T::Quoted(name) => {
                if next_is_paren {
                    function(name, tk)?;
                } else {
                    column(name, tk, columns, table)?;
                }
            }
            T::Word(w) => {
                let lc = w.to_ascii_lowercase();
                let after_cast_type = |j: usize| -> bool {
                    // `AS` or `::` before this word.
                    j.checked_sub(1).is_some_and(|k| {
                        matches!(&tokens[k].t, T::Op("::")) || is_word(Some(&tokens[k]), "as")
                    })
                };
                if lc == "as" {
                    let ok = matches!(next, Some(Tk { t: T::Word(t), .. }) if TYPES.contains(&t.to_ascii_lowercase().as_str()));
                    if !ok {
                        return Err(bad(
                            "After AS write a type, like DOUBLE, VARCHAR or DATE.",
                            tk.at,
                            tk.len,
                        ));
                    }
                } else if lc == "distinct" {
                    let ok = is_word(prev, "is")
                        || (is_word(prev, "not") && i >= 2 && is_word(tokens.get(i - 2), "is"));
                    if !ok || !is_word(next, "from") {
                        return Err(bad(
                            "DISTINCT is only available as IS DISTINCT FROM.",
                            tk.at,
                            tk.len,
                        ));
                    }
                } else if lc == "from" {
                    if !is_word(prev, "distinct") {
                        return Err(starts_query(&lc, tk));
                    }
                } else if lc == "to" {
                    if !is_word(prev, "similar") {
                        return Err(bad("TO is only available as SIMILAR TO.", tk.at, tk.len));
                    }
                } else if lc == "interval" {
                    seen_interval = true;
                } else if WORDS.contains(&lc.as_str()) {
                    // A plain word of an expression.
                } else if TYPES.contains(&lc.as_str()) && after_cast_type(i) {
                    // A type name after AS or ::. Its size may follow: DECIMAL(18, 2).
                    if next_is_paren {
                        let mut j = i + 2;
                        let mut ok = false;
                        while let Some(t) = tokens.get(j) {
                            match &t.t {
                                T::Num | T::Op(",") => j += 1,
                                T::Op(")") => {
                                    ok = true;
                                    break;
                                }
                                _ => break,
                            }
                        }
                        if !ok {
                            return Err(bad(
                                "A type may have a size in numbers only, like DECIMAL(18, 2).",
                                tk.at,
                                tk.len,
                            ));
                        }
                        i = j; // the `)`
                    }
                } else if TYPED_LITERALS.contains(&lc.as_str())
                    && matches!(next, Some(Tk { t: T::Str, .. }))
                {
                    // `DATE '2026-01-31'`
                } else if seen_interval && UNITS.contains(&lc.as_str()) {
                    // The unit of an interval.
                } else if STARTS_A_QUERY.contains(&lc.as_str()) || reserved(&lc) {
                    return Err(starts_query(&lc, tk));
                } else if next_is_paren {
                    function(w, tk)?;
                } else {
                    column(w, tk, columns, table)?;
                }
            }
        }
        i += 1;
    }
    if let Some(open) = opened.last() {
        return Err(bad("This ( is never closed.", open.at, 1));
    }
    Ok(())
}

fn starts_query(word: &str, tk: &Tk) -> FormulaError {
    let w = word.to_uppercase();
    if STARTS_A_QUERY.contains(&word) {
        bad(
            format!(
                "{w} is not available. A column formula is one expression over the columns of its table. It cannot read other tables or files."
            ),
            tk.at,
            tk.len,
        )
    } else {
        bad(
            format!("{w} is not available in a column formula."),
            tk.at,
            tk.len,
        )
    }
}

fn function(name: &str, tk: &Tk) -> Result<(), FormulaError> {
    if function_allowed(name) {
        return Ok(());
    }
    Err(bad(
        format!("The function {name} is not available in a column formula."),
        tk.at,
        tk.len,
    ))
}

fn column(name: &str, tk: &Tk, columns: &[String], table: &str) -> Result<(), FormulaError> {
    let lc = name.to_lowercase();
    if columns.iter().any(|c| c.to_lowercase() == lc) {
        return Ok(());
    }
    let msg = match super::check::closest(name, columns.iter().map(String::as_str)) {
        Some(s) => format!("No column {name} in {table}. Did you mean \"{s}\"?"),
        None => format!(
            "No column {name} in {table}. Write a name with spaces or capitals between double quotes."
        ),
    };
    Err(bad(msg, tk.at, tk.len))
}

/// The expression as the view holds it: a failure on one row gives a blank, as a formula does.
pub fn wrap(text: &str) -> String {
    format!("TRY(({text}))")
}

/// The columns of a SELECT and their DuckDB types, in order.
pub fn describe(db: &Connection, select: &str) -> Result<Vec<(String, String)>, duckdb::Error> {
    let sql = format!("DESCRIBE {select}");
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    rows.collect()
}

/// A blank column of the type `ty`, for a calculated column that could not be computed.
pub fn blank(ty: &str, name: &str) -> String {
    format!("CAST(NULL AS {ty}) AS {}", ident(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE t(a INTEGER, b VARCHAR, \"Total sales\" DOUBLE, d DATE);
             INSERT INTO t VALUES (1, 'x', 2.5, DATE '2026-01-31'), (2, 'y;z', 5.0, DATE '2026-02-01');",
        )
        .unwrap();
        c
    }

    fn run_check(c: &Connection, text: &str) -> Result<(), FormulaError> {
        let columns: Vec<String> = ["a", "b", "Total sales", "d"].map(String::from).into();
        check(c, text, &columns, "t", "SELECT * FROM t")
    }

    #[test]
    fn plain_expressions_pass() {
        let c = db();
        for ok in [
            "a * 2",
            "\"Total sales\" / 2",
            "CASE WHEN a > 1 THEN 'big' ELSE 'small' END",
            // A `;` and a quote inside a text are only characters.
            "replace(b, ';', ',')",
            "b || ';' || 'it''s'",
            "CAST(a AS DOUBLE) / 3",
            "a::DOUBLE / 3",
            "CAST(a AS DECIMAL(18, 2))",
            "coalesce(b, 'none')",
            "ifnull(b, 'none')",
            "nullif(a, 1)",
            "a IS DISTINCT FROM 1",
            "a IS NOT DISTINCT FROM 1",
            "date_part('year', d)",
            "d + INTERVAL 1 DAY",
            "d + INTERVAL '2' MONTH",
            "d > DATE '2026-01-31'",
            "a IN (1, 2, 3)",
            "a NOT IN (1, 2)",
            "b LIKE 'x%'",
            "a BETWEEN 1 AND 5",
            "NOT (a = 1 OR b = 'y') AND TRUE",
            "round(\"Total sales\" * 1.5e0, 1)",
            "-a + .5",
            "a % 2 = 0",
            "a // 2",
            "upper(left(b, 1))",
            "strftime(d, '%Y-%m')",
            "year(d) * 100 + month(d)",
            // A column called like a type, a unit or a function is still a column when quoted.
            "length(\"b\")",
        ] {
            run_check(&c, ok).unwrap_or_else(|e| panic!("{ok}: {}", e.message));
        }
    }

    #[test]
    fn a_subquery_or_a_file_or_the_environment_is_refused() {
        let c = db();
        for (bad, says) in [
            (
                "1 + (SELECT length(content) FROM read_text('/etc/hostname'))",
                "SELECT is not available",
            ),
            ("(FROM read_text('/etc/hostname'))", "FROM is not available"),
            ("length(read_text('/etc/hostname'))", "read_text"),
            ("\"read_text\"('/etc/hostname')", "read_text"),
            ("read_csv('/etc/passwd')", "read_csv"),
            ("query_table('t')", "query_table"),
            ("query('SELECT 1')", "query"),
            ("getenv('HOME')", "getenv"),
            ("current_setting('home_directory')", "current_setting"),
            ("glob('*')", "glob"),
            ("sniff_csv('x')", "sniff_csv"),
            ("WITH x AS (SELECT 1) SELECT 1", "WITH is not available"),
            ("a IN (SELECT 1)", "SELECT is not available"),
            ("EXISTS(SELECT 1)", "EXISTS is not available"),
            ("(VALUES (1))", "VALUES"),
            ("(TABLE t)", "TABLE is not available"),
            ("a UNION SELECT 1", "UNION is not available"),
            ("sum(a)", "sum"),
            ("row_number() OVER ()", "row_number"),
            ("nextval('s')", "nextval"),
            ("random()", "random"),
            ("a, b", "no comma"),
            ("t.a", "dot"),
            ("zzz", "No column zzz"),
            ("a +", ""),
        ] {
            let e = run_check(&c, bad).expect_err(bad);
            assert!(e.message.contains(says), "{bad}: {}", e.message);
        }
    }

    #[test]
    fn a_statement_end_a_comment_and_odd_characters_are_refused() {
        let c = db();
        for (bad, says) in [
            ("a; DROP TABLE t", "A ; ends a statement"),
            ("a;", "A ; ends a statement"),
            ("a -- the rest", "Comments"),
            ("a /* x */", "Comments"),
            ("a -> 1", "arrow"),
            ("E'x'", "space between a word and a quote"),
            ("$$x$$", "$"),
            ("$1", "$"),
            ("?", "?"),
            ("a[1]", "["),
            ("{'a': 1}", "{"),
            ("`a`", "`"),
            ("'open", "never closed"),
            ("\"open", "never closed"),
            ("(a", "never closed"),
            ("a)", "no ("),
            ("a::nothing", "type"),
            ("CAST(a AS nothing)", "type"),
            ("1abc", "letters"),
            ("é", "double quotes"),
            ("", "Write the expression"),
        ] {
            let e = run_check(&c, bad).expect_err(bad);
            assert!(e.message.contains(says), "{bad}: {}", e.message);
        }
    }

    #[test]
    fn the_underline_sits_on_the_word_that_is_refused() {
        let c = db();
        let e = run_check(&c, "a + (SELECT 2)").unwrap_err();
        assert_eq!((e.at, e.len), (5, 6));
        let e = run_check(&c, "a + getenv('X')").unwrap_err();
        assert_eq!((e.at, e.len), (4, 6));
        let e = run_check(&c, "a;b").unwrap_err();
        assert_eq!((e.at, e.len), (1, 1));
    }

    #[test]
    fn every_function_on_the_list_is_in_the_bundled_duckdb() {
        // On a fresh connection: a name that only exists after the ICU extension has loaded would
        // make the answer depend on what ran before, so it is not allowed on the list.
        let c = Connection::open_in_memory().unwrap();
        let mut stmt = c
            .prepare(
                "SELECT DISTINCT lower(function_name) FROM duckdb_functions() \
                 WHERE function_type IN ('scalar', 'macro')",
            )
            .unwrap();
        let there: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let missing: Vec<&&str> = FUNCTIONS
            .iter()
            .filter(|f| !there.iter().any(|t| t == **f))
            .collect();
        assert!(
            missing.is_empty(),
            "DuckDB has no function called {missing:?}"
        );
        // The list is sorted for the binary search, and has each name once.
        assert!(FUNCTIONS.windows(2).all(|w| w[0] < w[1]), "sort the list");
        // What is on it never changes with ICU: a name present now is present after it loads.
        let _ = c.execute_batch("SELECT current_date, current_timestamp, localtime;");
        let mut again = c
            .prepare(
                "SELECT DISTINCT lower(function_name) FROM duckdb_functions() \
                 WHERE function_type IN ('scalar', 'macro')",
            )
            .unwrap();
        let later: Vec<String> = again
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(FUNCTIONS.iter().all(|f| later.iter().any(|t| t == *f)));
    }

    #[test]
    fn what_was_looked_at_and_refused_is_not_on_the_list() {
        for (name, why) in REFUSED {
            assert!(
                !function_allowed(name),
                "{name} ({why}) must not be allowed"
            );
        }
        // Whatever reads a file, the environment or the catalog is not there either.
        for name in FUNCTIONS {
            for start in [
                "read_", "query", "http", "s3", "pg_", "duckdb_", "has_", "current_", "st_",
            ] {
                assert!(!name.starts_with(start), "{name}");
            }
            assert!(
                !name.ends_with("_scan") && !name.contains("setting") && !name.contains("env"),
                "{name}"
            );
        }
        // The plain ones are.
        for name in [
            "lower",
            "round",
            "coalesce",
            "ifnull",
            "nullif",
            "strftime",
            "date_part",
            "split_part",
            "now",
        ] {
            assert!(function_allowed(name), "{name}");
        }
    }

    #[test]
    fn the_reserved_words_are_those_of_the_bundled_duckdb() {
        let c = Connection::open_in_memory().unwrap();
        let mut stmt = c
            .prepare(
                "SELECT lower(keyword_name) FROM duckdb_keywords() WHERE keyword_category = 'reserved' ORDER BY 1",
            )
            .unwrap();
        let now: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let mut listed: Vec<&str> = RESERVED.to_vec();
        listed.sort_unstable();
        assert_eq!(now, listed, "update RESERVED");
        for w in [
            "select", "from", "with", "table", "union", "where", "lambda",
        ] {
            assert!(reserved(w) || STARTS_A_QUERY.contains(&w), "{w}");
        }
    }

    #[test]
    fn a_function_that_makes_a_big_result_or_runs_a_lambda_is_refused() {
        let c = db();
        for (bad, says) in [
            ("repeat(b, 1000000000)", "repeat"),
            ("lpad(b, 1000000000, 'x')", "lpad"),
            ("rpad(b, 1000000000, 'x')", "rpad"),
            ("range(1000000000)", "range"),
            ("generate_series(1, 1000000000)", "generate_series"),
            ("list_resize(list_value(1), 1000000000)", "list_resize"),
            ("array_resize(array_value(1), 1000000000)", "array_resize"),
            ("printf('%1000000000d', a)", "printf"),
            ("format('{:>1000000000}', b)", "format"),
            ("bar(a, 0, 1, 1000000000)", "bar"),
            // A lambda: the arrow, and the word. The functions that take one are not on the list either.
            ("list_reduce(list_value(b, b), (x, y) -> x || y)", "arrow"),
            ("list_reduce(list_value(b, b), lambda x)", "list_reduce"),
            ("list_transform(list_value(1), x -> x + 1)", "arrow"),
            // A function that is allowed, given a lambda: the word is refused by itself.
            ("lower(lambda x)", "LAMBDA is not available"),
            (
                "array_to_string(list_value(b, b), lambda x, y: x)",
                "not available",
            ),
        ] {
            let e = run_check(&c, bad).expect_err(bad);
            assert!(e.message.contains(says), "{bad}: {}", e.message);
        }
        // Even by a quoted name.
        assert!(run_check(&c, "\"repeat\"(b, 3)").is_err());
    }

    #[test]
    fn a_check_that_takes_the_limit_is_refused_in_words() {
        let c = db();
        // The same expression passes with the time it has, and is stopped with none.
        assert!(run_check(&c, "a * 2").is_ok());
        let columns: Vec<String> = ["a", "b", "Total sales", "d"].map(String::from).into();
        let e = check_within(
            &c,
            "a * 2",
            &columns,
            "t",
            "SELECT * FROM t",
            Duration::ZERO,
        )
        .unwrap_err();
        assert_eq!(
            e.message,
            "Tenun stopped checking this column after 2 seconds."
        );
        // The connection is fine afterwards: no interrupt is left behind for the next query.
        assert!(run_check(&c, "a * 2").is_ok());
        let n: i64 = c
            .query_row("SELECT count(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn a_slow_expression_is_interrupted_by_the_watchdog() {
        // DuckDB binds `a * 2` in well under a millisecond, so the honest slow form is a source that
        // takes a while to bind: a view over many nested layers. The limit is small, the work is not.
        let c = db();
        let mut source = String::from("SELECT * FROM t");
        for _ in 0..400 {
            source = format!("SELECT * FROM ({source}) AS x");
        }
        let columns: Vec<String> = ["a", "b", "Total sales", "d"].map(String::from).into();
        let started = Instant::now();
        let r = check_within(
            &c,
            "a * 2",
            &columns,
            "t",
            &source,
            Duration::from_millis(1),
        );
        let took = started.elapsed();
        assert!(r.is_err());
        assert!(took < Duration::from_secs(10), "{took:?}");
        // And the connection still works.
        assert!(run_check(&c, "a * 2").is_ok());
    }

    #[test]
    fn a_length_limit_holds() {
        let c = db();
        let long = format!("a + {}", "1 + ".repeat(MAX_CHARS) + "1");
        assert!(
            run_check(&c, &long)
                .unwrap_err()
                .message
                .contains("more than")
        );
    }

    #[test]
    fn duckdb_binds_what_is_left() {
        let c = db();
        // The words are fine, the kinds are not.
        let e = run_check(&c, "lower(a)").unwrap_err();
        assert!(
            e.message.starts_with("DuckDB cannot use this expression"),
            "{}",
            e.message
        );
        assert_eq!(e.at, 0);
    }
}
