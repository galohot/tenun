//! The check of a calculated column written as SQL (Advanced, V4-D62).
//!
//! A `.tenun` file can come from someone else, and a published page may show what a column
//! holds. So opening a file must never run a statement that reads a local file, the environment
//! or another table into a column. The text is **one expression** over the columns of the table.
//!
//! DuckDB's own parser would be the best judge, but the bundled build has no JSON extension, so
//! `json_serialize_sql` is not there, and adding the extension rebuilds DuckDB (V4-D49). The
//! check is a small reader of its own instead, and it **denies by default**: every word of the
//! text must be one of a short list of words, a function DuckDB marks as a plain built-in scalar,
//! or a column of the table. A subquery needs `SELECT`, `WITH`, `FROM`, `VALUES`, `TABLE` or
//! another reserved word, none of which is on the list, and a table function can only follow
//! `FROM`. So neither can be written.
//!
//! What it refuses (each with a plain sentence and the place to underline):
//! - `;`, comments (`--`, `/* */`), `$`, `?`, `.`, brackets and braces, `->`, and a quote that
//!   follows a letter (`E'..'`), because they can hide what the reader sees from what DuckDB sees.
//! - Any reserved word that is not on the list: `SELECT`, `FROM`, `WITH`, `JOIN`, `OVER`…
//! - Any function that is not a built-in scalar (or a plain macro) without side effects, or that
//!   is on the deny list: `getenv`, `current_setting`, `read_*`, `glob`, `query*`, `sniff_csv`,
//!   `*_scan`, `http*`, `s3*`.
//! - Any name that is not a column of the table.
//! - Two or more expressions (a comma at the top level) and unbalanced brackets.
//!
//! After the words are checked, DuckDB binds the expression with `EXPLAIN`, and a message that
//! comes back names a type that does not fit, not a value.

use super::FormulaError;
use crate::catalog::ident;
use duckdb::Connection;
use std::collections::HashSet;

/// The most characters a SQL column may have, as for a formula (V4-D24).
pub const MAX_CHARS: usize = 10_000;

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

/// Functions that are never allowed, whatever DuckDB says about them.
fn denied(name: &str) -> bool {
    name.starts_with("read_")
        || name.starts_with("query")
        || name.starts_with("http")
        || name.starts_with("s3")
        || name.starts_with("pg_")
        || name.starts_with("duckdb_")
        || name.ends_with("_scan")
        || matches!(
            name,
            "glob" | "getenv" | "current_setting" | "sniff_csv" | "current_query"
        )
}

/// The functions an expression may call. DuckDB's own list, `duckdb_functions()`: built-in
/// scalars with no side effects that give the same answer for the same input, and macros whose
/// definition holds no query. `COALESCE` and `IFNULL` are not in the list because the parser
/// handles them itself.
fn allowed_functions(db: &Connection) -> HashSet<String> {
    let mut out: HashSet<String> = ["coalesce", "ifnull"].map(String::from).into();
    let sql = "SELECT DISTINCT lower(function_name) FROM duckdb_functions() WHERE internal AND (\
        (function_type = 'scalar' AND NOT coalesce(has_side_effects, false) \
            AND coalesce(stability::VARCHAR, '') <> 'VOLATILE') \
        OR (function_type = 'macro' AND macro_definition IS NOT NULL \
            AND lower(macro_definition) NOT LIKE '%select%' \
            AND lower(macro_definition) NOT LIKE '%system%' \
            AND lower(macro_definition) NOT LIKE '%read_%' \
            AND lower(macro_definition) NOT LIKE '%query%' \
            AND lower(macro_definition) NOT LIKE '%getenv%' \
            AND lower(macro_definition) NOT LIKE '%glob%' \
            AND lower(macro_definition) NOT LIKE '%setting%'))";
    out.extend(names(db, sql).into_iter().filter(|n| !denied(n)));
    out
}

/// The words DuckDB will not take as a name unless they are quoted.
fn reserved_words(db: &Connection) -> HashSet<String> {
    names(
        db,
        "SELECT lower(keyword_name) FROM duckdb_keywords() WHERE keyword_category = 'reserved'",
    )
    .into_iter()
    .collect()
}

fn names(db: &Connection, sql: &str) -> Vec<String> {
    let Ok(mut stmt) = db.prepare(sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
        return Vec::new();
    };
    rows.filter_map(Result::ok).collect()
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
    let functions = allowed_functions(db);
    let reserved = reserved_words(db);
    judge(&tokens, columns, table, &functions, &reserved)?;
    // DuckDB binds it. The wrapper is the one the view uses.
    let sql = format!("EXPLAIN SELECT TRY(({text})) FROM ({source}) AS t0 LIMIT 0");
    if let Err(e) = db.prepare(&sql).and_then(|mut s| s.query([]).map(|_| ())) {
        return Err(bad(bind_message(&format!("{e}")), 0, len));
    }
    Ok(())
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

fn judge(
    tokens: &[Tk],
    columns: &[String],
    table: &str,
    functions: &HashSet<String>,
    reserved: &HashSet<String>,
) -> Result<(), FormulaError> {
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
                    function(name, tk, functions)?;
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
                } else if STARTS_A_QUERY.contains(&lc.as_str()) || reserved.contains(&lc) {
                    return Err(starts_query(&lc, tk));
                } else if next_is_paren {
                    function(w, tk, functions)?;
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

fn function(name: &str, tk: &Tk, functions: &HashSet<String>) -> Result<(), FormulaError> {
    if functions.contains(&name.to_lowercase()) {
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
    fn the_functions_that_may_be_called_are_the_plain_ones() {
        let c = db();
        let allowed = allowed_functions(&c);
        for yes in [
            "lower",
            "upper",
            "round",
            "abs",
            "coalesce",
            "ifnull",
            "nullif",
            "strftime",
            "date_part",
            "year",
            "substr",
            "replace",
            "regexp_matches",
            "greatest",
            "least",
            "date_trunc",
            "split_part",
            "concat",
            "length",
            "list_value",
        ] {
            assert!(allowed.contains(yes), "{yes} should be allowed");
        }
        for no in [
            "getenv",
            "read_text",
            "read_csv",
            "read_blob",
            "glob",
            "query",
            "query_table",
            "current_setting",
            "sniff_csv",
            "nextval",
            "random",
            "uuid",
            "gen_random_uuid",
            "current_query",
            "sum",
            "count",
            "avg",
            "unnest",
            "http_get",
            "s3_scan",
            "pg_typeof",
            "duckdb_settings",
            "error",
            "sleep_ms",
        ] {
            assert!(!allowed.contains(no), "{no} must not be allowed");
        }
        // The reserved words that start a query are not names.
        let reserved = reserved_words(&c);
        for w in ["select", "from", "with", "table", "union", "where", "join"] {
            assert!(
                reserved.contains(w) || STARTS_A_QUERY.contains(&w),
                "{w} should be refused"
            );
        }
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
