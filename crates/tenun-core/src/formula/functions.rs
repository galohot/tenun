//! The function list (GRANDPLAN §4.2): what each function takes, what kind of value it gives
//! back, and the DuckDB SQL it becomes.
//!
//! Two rules hold for every function here:
//! - A function that could make DuckDB stop on odd data (a negative square root, a date that
//!   does not exist) gives blank instead. A query must never fail because of one row.
//! - Blank in means blank out, except where Excel does the same: `&`, `CONCAT` and
//!   `IFERROR` take blank as nothing.

use super::FormulaError;
use super::check::{Src, TExpr, TNode, Ty, unify};
use super::parse::BinOp;
use super::sql::{Gen, Round, int_of, num_lit, sql_expr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Works on one row at a time.
    Row,
    /// Works over many rows: a measure only.
    Agg,
    /// `MIN` and `MAX`: one value is an aggregate, two or more are row-wise.
    Either,
}

#[derive(Debug)]
pub struct Def {
    pub name: &'static str,
    pub min: usize,
    pub max: usize,
    pub class: Class,
}

const MANY: usize = usize::MAX;

const fn row(name: &'static str, min: usize, max: usize) -> Def {
    Def {
        name,
        min,
        max,
        class: Class::Row,
    }
}

const fn agg(name: &'static str, min: usize, max: usize) -> Def {
    Def {
        name,
        min,
        max,
        class: Class::Agg,
    }
}

/// Every function, in the order of GRANDPLAN §4.2.
pub const DEFS: &[Def] = &[
    // Logic
    row("IF", 2, 3),
    row("IFS", 2, MANY),
    row("SWITCH", 3, MANY),
    row("AND", 1, MANY),
    row("OR", 1, MANY),
    row("NOT", 1, 1),
    row("IFERROR", 2, 2),
    row("ISBLANK", 1, 1),
    row("BLANK", 0, 0),
    // Math
    row("ABS", 1, 1),
    row("ROUND", 1, 2),
    row("ROUNDUP", 1, 2),
    row("ROUNDDOWN", 1, 2),
    row("INT", 1, 1),
    row("MOD", 2, 2),
    row("POWER", 2, 2),
    row("SQRT", 1, 1),
    row("EXP", 1, 1),
    row("LN", 1, 1),
    row("LOG10", 1, 1),
    row("SIGN", 1, 1),
    Def {
        name: "MIN",
        min: 1,
        max: MANY,
        class: Class::Either,
    },
    Def {
        name: "MAX",
        min: 1,
        max: MANY,
        class: Class::Either,
    },
    row("DIVIDE", 2, 3),
    // Text
    row("CONCAT", 1, MANY),
    row("LEFT", 1, 2),
    row("RIGHT", 1, 2),
    row("MID", 3, 3),
    row("LEN", 1, 1),
    row("UPPER", 1, 1),
    row("LOWER", 1, 1),
    row("PROPER", 1, 1),
    row("TRIM", 1, 1),
    row("SUBSTITUTE", 3, 3),
    row("FIND", 2, 3),
    row("SEARCH", 2, 3),
    row("VALUE", 1, 1),
    row("TEXT", 2, 2),
    // Dates
    row("DATE", 3, 3),
    row("YEAR", 1, 1),
    row("MONTH", 1, 1),
    row("DAY", 1, 1),
    row("WEEKDAY", 1, 1),
    row("WEEKNUM", 1, 1),
    row("QUARTER", 1, 1),
    row("EOMONTH", 2, 2),
    row("EDATE", 2, 2),
    row("DATEDIF", 3, 3),
    row("DAYS", 2, 2),
    row("TODAY", 0, 0),
    row("NOW", 0, 0),
    // Aggregates
    agg("SUM", 1, 1),
    agg("AVERAGE", 1, 1),
    agg("COUNT", 1, 1),
    agg("COUNTA", 1, 1),
    agg("COUNTD", 1, 1),
    agg("MEDIAN", 1, 1),
    agg("PERCENTILE", 2, 2),
    agg("STDEV", 1, 1),
    agg("VAR", 1, 1),
    agg("SUMIF", 2, 2),
    agg("COUNTIF", 2, 2),
    agg("AVERAGEIF", 2, 2),
    agg("COUNTROWS", 1, 1),
];

/// The function with this (upper-case) name.
pub fn lookup(name: &str) -> Option<&'static Def> {
    DEFS.iter().find(|d| d.name == name)
}

/// True when a call with `n` values works over many rows.
pub fn is_aggregate(def: &Def, n: usize) -> bool {
    match def.class {
        Class::Row => false,
        Class::Agg => true,
        Class::Either => n == 1,
    }
}

pub fn count_words(n: usize) -> String {
    match n {
        0 => "none".into(),
        1 => "1 value".into(),
        n => format!("{n} values"),
    }
}

/// "2 values", "1 or 2 values", "at least 2 values".
pub fn arity_words(def: &Def) -> String {
    let s = |n: usize| if n == 1 { "" } else { "s" };
    if def.max == MANY {
        format!("at least {} value{}", def.min, s(def.min))
    } else if def.min == def.max {
        match def.min {
            0 => "no values".into(),
            n => format!("{n} value{}", s(n)),
        }
    } else {
        format!("{} to {} values", def.min, def.max)
    }
}

// ---- types ----------------------------------------------------------------------------

struct Chk<'a> {
    name: &'static str,
    args: &'a [TExpr],
    src: &'a Src,
}

impl Chk<'_> {
    fn bad(&self, i: usize, want: &str, hint: &str) -> FormulaError {
        let a = &self.args[i];
        FormulaError::new(
            format!(
                "{} needs {want}, but {} is {}.{hint}",
                self.name,
                self.src.of(a),
                a.ty.words()
            ),
            a.at,
            a.len,
        )
    }

    fn num(&self, i: usize) -> Result<(), FormulaError> {
        match self.args[i].ty {
            Ty::Num | Ty::Any => Ok(()),
            Ty::Text => Err(self.bad(i, "a number", " Use VALUE() to read text as a number.")),
            _ => Err(self.bad(i, "a number", "")),
        }
    }

    fn all_num(&self) -> Result<(), FormulaError> {
        (0..self.args.len()).try_for_each(|i| self.num(i))
    }

    fn boolean(&self, i: usize, want: &str) -> Result<(), FormulaError> {
        match self.args[i].ty {
            Ty::Bool | Ty::Any => Ok(()),
            _ => Err(self.bad(i, want, "")),
        }
    }

    fn date(&self, i: usize) -> Result<(), FormulaError> {
        match self.args[i].ty {
            Ty::Date | Ty::DateTime | Ty::Any => Ok(()),
            Ty::Text => Err(self.bad(
                i,
                "a date",
                " Set the column to the Date type in Data, or build one with DATE(year, month, day).",
            )),
            _ => Err(self.bad(i, "a date", "")),
        }
    }

    /// The one kind that all of these values share.
    fn unify(&self, idx: &[usize]) -> Result<Ty, FormulaError> {
        let mut ty = Ty::Any;
        let mut first: Option<usize> = None;
        for &i in idx {
            let t = self.args[i].ty;
            match unify(ty, t) {
                Some(u) => {
                    if t != Ty::Any && first.is_none() {
                        first = Some(i);
                    }
                    ty = u;
                }
                None => {
                    let f = &self.args[first.unwrap_or(0)];
                    let a = &self.args[i];
                    let hint = if matches!((ty, t), (Ty::Text, Ty::Num) | (Ty::Num, Ty::Text)) {
                        " Use TEXT() or VALUE() to make them match."
                    } else {
                        ""
                    };
                    return Err(FormulaError::new(
                        format!(
                            "{} needs values of the same kind, but {} is {} and {} is {}.{hint}",
                            self.name,
                            self.src.of(f),
                            f.ty.words(),
                            self.src.of(a),
                            a.ty.words()
                        ),
                        a.at,
                        a.len,
                    ));
                }
            }
        }
        Ok(ty)
    }

    /// A text value written in the formula itself, as in `DATEDIF(a, b, "D")`.
    fn literal_text(&self, i: usize, want: &str) -> Result<&str, FormulaError> {
        match &self.args[i].node {
            TNode::Str(s) => Ok(s),
            _ => Err(FormulaError::new(
                format!("{} needs {want}.", self.name),
                self.args[i].at,
                self.args[i].len,
            )),
        }
    }
}

/// The formats `TEXT` knows.
const TEXT_FORMATS: [&str; 4] = ["0", "0.0", "#,##0", "0%"];

pub fn type_of(def: &Def, args: &[TExpr], src: &Src) -> Result<Ty, FormulaError> {
    let c = Chk {
        name: def.name,
        args,
        src,
    };
    let n = args.len();
    Ok(match def.name {
        // ---- logic
        "IF" => {
            c.boolean(0, "a true-or-false test first, like [amount] > 100")?;
            if n == 3 {
                c.unify(&[1, 2])?
            } else {
                args[1].ty
            }
        }
        "IFS" => {
            if !n.is_multiple_of(2) {
                let last = &args[n - 1];
                return Err(FormulaError::new(
                    "IFS needs pairs: a test, then the result for it.",
                    last.at,
                    last.len,
                ));
            }
            for i in (0..n).step_by(2) {
                c.boolean(i, "a true-or-false test, like [amount] > 100")?;
            }
            let results: Vec<usize> = (1..n).step_by(2).collect();
            c.unify(&results)?
        }
        "SWITCH" => {
            let pairs = (n - 1) / 2;
            for k in 0..pairs {
                let key = &args[1 + 2 * k];
                if !comparable(args[0].ty, key.ty) {
                    return Err(FormulaError::new(
                        format!(
                            "SWITCH compares {} ({}) with {}, which is {}. They need to be the same kind.",
                            src.of(&args[0]),
                            args[0].ty.words(),
                            src.of(key),
                            key.ty.words()
                        ),
                        key.at,
                        key.len,
                    ));
                }
            }
            let mut results: Vec<usize> = (0..pairs).map(|k| 2 + 2 * k).collect();
            if (n - 1) % 2 == 1 {
                results.push(n - 1);
            }
            c.unify(&results)?
        }
        "AND" | "OR" => {
            for i in 0..n {
                c.boolean(i, "a true-or-false test, like [amount] > 100")?;
            }
            Ty::Bool
        }
        "NOT" => {
            c.boolean(0, "a true-or-false test, like [amount] > 100")?;
            Ty::Bool
        }
        "IFERROR" => c.unify(&[0, 1])?,
        "ISBLANK" => Ty::Bool,
        "BLANK" => Ty::Any,
        // ---- math
        "ABS" | "INT" | "SQRT" | "EXP" | "LN" | "LOG10" | "SIGN" | "ROUND" | "ROUNDUP"
        | "ROUNDDOWN" | "MOD" | "POWER" | "DIVIDE" => {
            c.all_num()?;
            Ty::Num
        }
        "MIN" | "MAX" if n == 1 => {
            // One value: the smallest or largest of all the rows.
            let a = &args[0];
            if !matches!(a.ty, Ty::Num | Ty::Date | Ty::DateTime | Ty::Text) {
                return Err(c.bad(0, "numbers, dates or text", ""));
            }
            a.ty
        }
        "MIN" | "MAX" => {
            // Two values or more: the smaller or larger of them, row by row.
            for (i, a) in args.iter().enumerate() {
                if !matches!(a.ty, Ty::Num | Ty::Date | Ty::DateTime | Ty::Any) {
                    return Err(c.bad(i, "numbers or dates", ""));
                }
            }
            c.unify(&(0..n).collect::<Vec<_>>())?
        }
        // ---- text
        "CONCAT" | "UPPER" | "LOWER" | "PROPER" | "TRIM" | "SUBSTITUTE" => Ty::Text,
        "LEFT" | "RIGHT" => {
            if n == 2 {
                c.num(1)?;
            }
            Ty::Text
        }
        "MID" => {
            c.num(1)?;
            c.num(2)?;
            Ty::Text
        }
        "LEN" => Ty::Num,
        "FIND" | "SEARCH" => {
            if n == 3 {
                c.num(2)?;
            }
            Ty::Num
        }
        "VALUE" => {
            if !matches!(args[0].ty, Ty::Text | Ty::Num | Ty::Any) {
                return Err(c.bad(0, "text or a number", ""));
            }
            Ty::Num
        }
        "TEXT" => {
            if args[0].ty.is_temporal() {
                return Err(FormulaError::new(
                    format!(
                        "TEXT formats numbers only, and {} is {}. To write a date as text, join its parts with &, like YEAR({0}) & \"-\" & MONTH({0}).",
                        src.of(&args[0]),
                        args[0].ty.words()
                    ),
                    args[0].at,
                    args[0].len,
                ));
            }
            c.num(0)?;
            let f = c.literal_text(1, "the format in double quotes, like \"0.0\"")?;
            if !TEXT_FORMATS.contains(&f) {
                return Err(FormulaError::new(
                    "TEXT knows these formats: \"0\", \"0.0\", \"#,##0\" and \"0%\".",
                    args[1].at,
                    args[1].len,
                ));
            }
            Ty::Text
        }
        // ---- dates
        "DATE" => {
            c.all_num()?;
            Ty::Date
        }
        "YEAR" | "MONTH" | "DAY" | "WEEKDAY" | "WEEKNUM" | "QUARTER" => {
            c.date(0)?;
            Ty::Num
        }
        "EOMONTH" | "EDATE" => {
            c.date(0)?;
            c.num(1)?;
            Ty::Date
        }
        "DATEDIF" => {
            c.date(0)?;
            c.date(1)?;
            let u = c.literal_text(2, "\"D\", \"M\" or \"Y\" as its last value")?;
            if !matches!(u.to_uppercase().as_str(), "D" | "M" | "Y") {
                return Err(FormulaError::new(
                    "DATEDIF counts in \"D\" (days), \"M\" (months) or \"Y\" (years).",
                    args[2].at,
                    args[2].len,
                ));
            }
            Ty::Num
        }
        "DAYS" => {
            c.date(0)?;
            c.date(1)?;
            Ty::Num
        }
        "TODAY" => Ty::Date,
        "NOW" => Ty::DateTime,
        // ---- aggregates
        "SUM" | "AVERAGE" | "MEDIAN" | "STDEV" | "VAR" => {
            c.num(0)?;
            Ty::Num
        }
        "COUNT" => {
            if args[0].ty == Ty::Text {
                return Err(c.bad(
                    0,
                    "numbers or dates",
                    " To count filled cells of any kind, use COUNTA.",
                ));
            }
            Ty::Num
        }
        "COUNTA" | "COUNTD" => Ty::Num,
        "PERCENTILE" => {
            c.num(0)?;
            match &args[1].node {
                TNode::Num(p) if (0.0..=1.0).contains(p) => {}
                _ => {
                    return Err(FormulaError::new(
                        "PERCENTILE needs a number between 0 and 1 as its second value, like 0.9.",
                        args[1].at,
                        args[1].len,
                    ));
                }
            }
            Ty::Num
        }
        "SUMIF" | "AVERAGEIF" => {
            c.num(0)?;
            c.boolean(
                1,
                "a true-or-false test over the same table, like [region] = \"Java\"",
            )?;
            Ty::Num
        }
        "COUNTIF" => {
            c.boolean(
                1,
                "a true-or-false test over the same table, like [region] = \"Java\"",
            )?;
            Ty::Num
        }
        "COUNTROWS" => Ty::Num,
        other => unreachable!("{other} has no type rule"),
    })
}

/// Whether `=` could compare these two kinds.
fn comparable(a: Ty, b: Ty) -> bool {
    a == Ty::Any || b == Ty::Any || a == b || (a.is_temporal() && b.is_temporal())
}

// ---- SQL ------------------------------------------------------------------------------
//
// No function writes one of its values twice, and none puts one value inside the body of a
// lambda. A function that needs a value more than once computes all its values together with
// `Gen::bind_all`, and its body reads only names (V4-D24, see `sql.rs`).

fn number(n: f64) -> TExpr {
    sql_expr(num_lit(n), Ty::Num, true)
}

/// A date as a value of its own: a date or a date and time cut to a date.
fn date_expr(g: &mut Gen<'_>, e: &TExpr) -> TExpr {
    let d = g.date(e);
    sql_expr(format!("CAST({d} AS DATE)"), Ty::Date, false)
}

/// The SQL of a row-wise function. `ty` is the kind it gives back.
pub(crate) fn emit(g: &mut Gen<'_>, name: &str, args: &[TExpr], ty: Ty) -> String {
    let n = args.len();
    match name {
        // ---- logic
        "IF" => {
            let c = g.as_ty(&args[0], Ty::Bool);
            let a = g.as_ty(&args[1], ty);
            match args.get(2) {
                Some(b) => {
                    let b = g.as_ty(b, ty);
                    format!("CASE WHEN {c} THEN {a} ELSE {b} END")
                }
                None => format!("CASE WHEN {c} THEN {a} END"),
            }
        }
        "IFS" => {
            let mut s = String::from("CASE");
            for pair in args.chunks(2) {
                let c = g.as_ty(&pair[0], Ty::Bool);
                let v = g.as_ty(&pair[1], ty);
                s.push_str(&format!(" WHEN {c} THEN {v}"));
            }
            s.push_str(" END");
            s
        }
        "SWITCH" => {
            let pairs = (n - 1) / 2;
            // The value and every key and result are computed together, once.
            let all: Vec<&TExpr> = args.iter().collect();
            g.bind_all(&all, |g, v| {
                let mut s = String::from("CASE");
                for k in 0..pairs {
                    let test = g.compare(BinOp::Eq, &v[0], &v[1 + 2 * k]);
                    let r = g.as_ty(&v[2 + 2 * k], ty);
                    s.push_str(&format!(" WHEN {test} THEN {r}"));
                }
                if (n - 1) % 2 == 1 {
                    let d = g.as_ty(&v[n - 1], ty);
                    s.push_str(&format!(" ELSE {d}"));
                }
                s.push_str(" END");
                s
            })
        }
        "AND" | "OR" => {
            let parts: Vec<String> = args.iter().map(|a| g.as_ty(a, Ty::Bool)).collect();
            format!("({})", parts.join(if name == "AND" { " AND " } else { " OR " }))
        }
        "NOT" => format!("(NOT {})", g.as_ty(&args[0], Ty::Bool)),
        "IFERROR" => {
            if ty == Ty::Text && args[0].ty == Ty::Text {
                // Empty text is blank here, as it is for ISBLANK.
                g.bind_all(&[&args[0], &args[1]], |g, v| {
                    let (x, alt) = (g.expr(&v[0]), g.as_ty(&v[1], ty));
                    format!("COALESCE(NULLIF({x}, ''), {alt})")
                })
            } else {
                let (a, alt) = (g.as_ty(&args[0], ty), g.as_ty(&args[1], ty));
                format!("COALESCE({a}, {alt})")
            }
        }
        "ISBLANK" => {
            let a = g.expr(&args[0]);
            if args[0].ty == Ty::Text {
                format!("(COALESCE({a}, '') = '')")
            } else {
                format!("({a} IS NULL)")
            }
        }
        "BLANK" => "NULL".into(),
        // ---- math
        "ABS" => format!("abs({})", g.num(&args[0])),
        "INT" => format!("floor({})", g.num(&args[0])),
        "SIGN" => format!("CAST(sign({}) AS DOUBLE)", g.num(&args[0])),
        "SQRT" => format!("TRY(sqrt({}))", g.num(&args[0])),
        "LN" => format!("TRY(ln({}))", g.num(&args[0])),
        "LOG10" => format!("TRY(log10({}))", g.num(&args[0])),
        "EXP" => {
            let e = format!("exp({})", g.num(&args[0]));
            g.finite(e)
        }
        "POWER" => {
            let p = format!("pow({}, {})", g.num(&args[0]), g.num(&args[1]));
            g.finite(p)
        }
        "MOD" => g.bind_all(&[&args[0], &args[1]], |g, v| {
            let (a, b) = (g.num(&v[0]), g.num(&v[1]));
            // Excel's MOD takes the sign of the divisor. The result is cleaned to the digits
            // the two numbers can hold, so MOD(5.1, 1) is 0.1.
            let places = format!(
                "CAST(least(greatest(14e0 - floor(log10(greatest(abs({a}), abs({b})))), 0e0), 300e0) AS INTEGER)"
            );
            format!(
                "CASE WHEN {b} = 0e0 THEN NULL ELSE TRY(round({a} - {b} * floor({a} / {b}), {places}) + 0e0) END"
            )
        }),
        "DIVIDE" => match args.get(2) {
            Some(alt) => g.bind_all(&[&args[0], &args[1], alt], |g, v| {
                let (a, b, alt) = (g.num(&v[0]), g.num(&v[1]), g.num(&v[2]));
                format!("CASE WHEN {b} IS NULL OR {b} = 0e0 THEN {alt} ELSE ({a} / {b}) END")
            }),
            None => g.divide(&args[0], &args[1]),
        },
        "ROUND" | "ROUNDUP" | "ROUNDDOWN" => {
            let mode = match name {
                "ROUND" => Round::Half,
                "ROUNDUP" => Round::Up,
                _ => Round::Down,
            };
            g.round(&args[0], args.get(1), mode)
        }
        "MIN" | "MAX" => {
            let ty = if ty == Ty::Any { Ty::Num } else { ty };
            let parts: Vec<String> = args.iter().map(|a| g.as_ty(a, ty)).collect();
            let f = if name == "MIN" { "least" } else { "greatest" };
            format!("{f}({})", parts.join(", "))
        }
        // ---- text
        "CONCAT" => {
            let parts: Vec<String> = args.iter().map(|a| g.text(a)).collect();
            format!("concat({})", parts.join(", "))
        }
        "LEFT" | "RIGHT" => {
            let t = g.text_expr(&args[0]);
            let f = if name == "LEFT" { "left" } else { "right" };
            match args.get(1) {
                Some(k) => g.bind_all(&[&t, k], |g, v| {
                    let (t, k) = (g.expr(&v[0]), g.num(&v[1]));
                    format!("CASE WHEN {k} >= 0e0 THEN {f}({t}, {}) END", int_of(&k))
                }),
                None => format!("{f}({}, 1)", g.expr(&t)),
            }
        }
        "MID" => {
            let t = g.text_expr(&args[0]);
            g.bind_all(&[&t, &args[1], &args[2]], |g, v| {
                let (t, s, k) = (g.expr(&v[0]), g.num(&v[1]), g.num(&v[2]));
                format!(
                    "CASE WHEN {s} >= 1e0 AND {k} >= 0e0 THEN substring({t}, {}, {}) END",
                    int_of(&s),
                    int_of(&k)
                )
            })
        }
        // A blank cell has no characters.
        "LEN" => format!("CAST(length(COALESCE({}, '')) AS DOUBLE)", g.text(&args[0])),
        "UPPER" => format!("upper({})", g.text(&args[0])),
        "LOWER" => format!("lower({})", g.text(&args[0])),
        "PROPER" => {
            let t = g.text(&args[0]);
            format!(
                "array_to_string(list_transform(regexp_extract_all(lower({t}), '\\p{{L}}+|\\P{{L}}+'), w -> upper(w[1]) || w[2:]), '')"
            )
        }
        "TRIM" => format!("regexp_replace(trim({}), ' +', ' ', 'g')", g.text(&args[0])),
        "SUBSTITUTE" => {
            let (t, old, new) = (g.text(&args[0]), g.text(&args[1]), g.text(&args[2]));
            format!("replace({t}, {old}, {new})")
        }
        "FIND" | "SEARCH" => {
            let (mut f, mut w) = (g.text_expr(&args[0]), g.text_expr(&args[1]));
            if name == "SEARCH" {
                for t in [&mut f, &mut w] {
                    let lower = format!("lower({})", g.expr(t));
                    *t = sql_expr(lower, Ty::Text, false);
                }
            }
            match args.get(2) {
                Some(st) => g.bind_all(&[&f, &w, st], |g, v| {
                    let (f, w, st) = (g.expr(&v[0]), g.expr(&v[1]), g.num(&v[2]));
                    let i = int_of(&st);
                    let found = format!("instr(substring({w}, {i}), {f})");
                    let at = g.bind_sql(found, Ty::Num, |g, p| {
                        let p = g.expr(p);
                        format!("CASE WHEN {p} = 0 THEN NULL ELSE CAST({p} + {i} - 1 AS DOUBLE) END")
                    });
                    format!("CASE WHEN {st} >= 1e0 THEN {at} END")
                }),
                None => {
                    let found = format!("instr({}, {})", g.expr(&w), g.expr(&f));
                    g.bind_sql(found, Ty::Num, |g, p| {
                        let p = g.expr(p);
                        format!("CASE WHEN {p} = 0 THEN NULL ELSE CAST({p} AS DOUBLE) END")
                    })
                }
            }
        }
        "VALUE" => {
            if args[0].ty == Ty::Text {
                let a = g.expr(&args[0]);
                g.finite(format!("TRY_CAST(trim({a}) AS DOUBLE)"))
            } else {
                g.num(&args[0])
            }
        }
        "TEXT" => {
            let TNode::Str(f) = &args[1].node else {
                unreachable!("checked: the format is written in the formula")
            };
            match f.as_str() {
                "0" => {
                    let r = g.round(&args[0], Some(&number(0.0)), Round::Half);
                    format!("printf('%.0f', {r})")
                }
                "0.0" => {
                    let r = g.round(&args[0], Some(&number(1.0)), Round::Half);
                    format!("printf('%.1f', {r})")
                }
                "#,##0" => {
                    let r = g.round(&args[0], Some(&number(0.0)), Round::Half);
                    format!("TRY(format('{{:,}}', CAST({r} AS BIGINT)))")
                }
                _ => {
                    let x = g.num(&args[0]);
                    let hundred = sql_expr(format!("({x} * 1e2)"), Ty::Num, false);
                    let r = g.round(&hundred, Some(&number(0.0)), Round::Half);
                    format!("(printf('%.0f', {r}) || '%')")
                }
            }
        }
        // ---- dates
        "DATE" => {
            let (y, m, d) = (g.num(&args[0]), g.num(&args[1]), g.num(&args[2]));
            // Counted from the first of January, so month 13 and day 32 roll over as in Excel.
            format!(
                "TRY(CAST(make_date(CAST(trunc({y}) AS INTEGER), 1, 1) + to_months(CAST(trunc({m}) AS INTEGER) - 1) + to_days(CAST(trunc({d}) AS INTEGER) - 1) AS DATE))"
            )
        }
        "YEAR" | "MONTH" | "DAY" | "WEEKDAY" | "WEEKNUM" | "QUARTER" => {
            let f = match name {
                "YEAR" => "year",
                "MONTH" => "month",
                "DAY" => "day",
                "WEEKDAY" => "isodow",
                "WEEKNUM" => "week",
                _ => "quarter",
            };
            format!("CAST({f}({}) AS DOUBLE)", g.date(&args[0]))
        }
        "EOMONTH" | "EDATE" => {
            let (d, m) = (g.date(&args[0]), g.num(&args[1]));
            let moved = format!("CAST({d} AS DATE) + to_months(CAST(trunc({m}) AS INTEGER))");
            if name == "EOMONTH" {
                format!("TRY(last_day({moved}))")
            } else {
                format!("TRY(CAST({moved} AS DATE))")
            }
        }
        "DATEDIF" => {
            let TNode::Str(u) = &args[2].node else {
                unreachable!("checked: the unit is written in the formula")
            };
            let unit = u.to_uppercase();
            let (a, b) = (date_expr(g, &args[0]), date_expr(g, &args[1]));
            g.bind_all(&[&a, &b], |g, v| {
                let (a, b) = (g.expr(&v[0]), g.expr(&v[1]));
                // Whole months and years, as Excel counts them: a month is not complete until
                // the day of the month has come round again.
                let count = match unit.as_str() {
                    "D" => format!("{b} - {a}"),
                    "M" => format!(
                        "(year({b}) - year({a})) * 12 + (month({b}) - month({a})) - (CASE WHEN day({b}) < day({a}) THEN 1 ELSE 0 END)"
                    ),
                    _ => format!(
                        "(year({b}) - year({a})) - (CASE WHEN (month({b}) * 100 + day({b})) < (month({a}) * 100 + day({a})) THEN 1 ELSE 0 END)"
                    ),
                };
                format!("CASE WHEN {b} >= {a} THEN CAST({count} AS DOUBLE) END")
            })
        }
        "DAYS" => {
            let (e, s) = (g.date(&args[0]), g.date(&args[1]));
            format!("CAST(CAST({e} AS DATE) - CAST({s} AS DATE) AS DOUBLE)")
        }
        "TODAY" => format!("DATE '{}'", g.cx.today.format("%Y-%m-%d")),
        "NOW" => format!("TIMESTAMP '{}'", g.cx.now.format("%Y-%m-%d %H:%M:%S")),
        other => unreachable!("{other} has no SQL"),
    }
}

/// The SQL of an aggregate, over the rows of its home table.
pub(crate) fn emit_aggregate(g: &mut Gen<'_>, name: &str, args: &[TExpr]) -> String {
    match name {
        "COUNTROWS" => "CAST(count(*) AS DOUBLE)".into(),
        "SUM" => format!("sum({})", g.num(&args[0])),
        "AVERAGE" => format!("avg({})", g.num(&args[0])),
        "MIN" | "MAX" => format!("{}({})", name.to_lowercase(), g.expr(&args[0])),
        "COUNT" => format!("CAST(count({}) AS DOUBLE)", g.expr(&args[0])),
        "COUNTA" => format!(
            "CAST(count({}) AS DOUBLE)",
            non_blank(&g.expr(&args[0]), args[0].ty)
        ),
        "COUNTD" => format!(
            "CAST(count(DISTINCT {}) AS DOUBLE)",
            non_blank(&g.expr(&args[0]), args[0].ty)
        ),
        "MEDIAN" => format!("CAST(median({}) AS DOUBLE)", g.num(&args[0])),
        "PERCENTILE" => {
            let TNode::Num(p) = &args[1].node else {
                unreachable!("checked: the share is written in the formula")
            };
            format!(
                "CAST(quantile_cont({}, {}) AS DOUBLE)",
                g.num(&args[0]),
                num_lit(*p)
            )
        }
        "STDEV" => format!("stddev_samp({})", g.num(&args[0])),
        "VAR" => format!("var_samp({})", g.num(&args[0])),
        "SUMIF" | "AVERAGEIF" => {
            let (x, c) = (g.num(&args[0]), g.as_ty(&args[1], Ty::Bool));
            let f = if name == "SUMIF" { "sum" } else { "avg" };
            format!("{f}({x}) FILTER (WHERE {c})")
        }
        "COUNTIF" => {
            let (x, c) = (
                non_blank(&g.expr(&args[0]), args[0].ty),
                g.as_ty(&args[1], Ty::Bool),
            );
            format!("CAST(count({x}) FILTER (WHERE {c}) AS DOUBLE)")
        }
        other => unreachable!("{other} has no aggregate SQL"),
    }
}

/// Empty text counts as blank when filled cells are counted. The value is the column of one
/// aggregate argument, so `NULLIF` copying it once does no harm.
fn non_blank(sql: &str, ty: Ty) -> String {
    if ty == Ty::Text {
        format!("NULLIF({sql}, '')")
    } else {
        sql.to_string()
    }
}
