//! Writes DuckDB SQL from a checked formula.
//!
//! Identifiers are quoted and came from the catalog. The only user text in the output is a
//! number or an escaped string literal. Every number is a DOUBLE, so `5 / 2` is 2.5 and a
//! sum of whole numbers never overflows.
//!
//! **No piece of SQL is ever written twice** (V4-D24). Some functions need a value two or
//! three times: a guard that turns infinity into blank reads the value to test it and to
//! return it. If each use wrote the value out again, a formula nested twenty deep would be a
//! million times longer than its text. So a value that is not a plain name or literal is
//! computed once and given a name, with a DuckDB lambda:
//! `list_transform([{'k0': value}], p -> … p.k0 …)[1]`.
//!
//! Two rules keep DuckDB itself fast, and both were found by measuring:
//! - `NULLIF` and `CASE x WHEN` copy their first value when DuckDB reads the SQL, so they are
//!   used on names and literals only.
//! - A lambda whose **body** holds another lambda takes twice as long for each level of
//!   nesting. So the values go in the **list** (`[{…}]`), never in the body: the body of every
//!   lambda holds only names, literals and operators. A function that binds any value binds all
//!   of its values in the same lambda (`Gen::bind_all`).

use super::check::{TExpr, TNode, Ty, Typed, unify};
use super::functions::{emit, emit_aggregate};
use super::parse::{BinOp, UnOp};
use super::{AggregateSql, Body, ColumnSql, Compiled, Context, FormulaError, Join, Kind, Part};
use crate::catalog::{ident, lit};
use crate::links::{Paths, join_on};

/// The most SQL a formula may become, in bytes (V4-D24).
pub const MAX_SQL: usize = 200_000;

pub fn compile(typed: &Typed, cx: &Context<'_>) -> Result<Compiled, FormulaError> {
    compile_within(typed, cx, MAX_SQL)
}

/// Like `compile`, with a limit of your own on the size of the SQL.
pub fn compile_within(
    typed: &Typed,
    cx: &Context<'_>,
    max_sql: usize,
) -> Result<Compiled, FormulaError> {
    let ty = typed.root.ty;
    let (body, size) = if typed.kind == Kind::Column {
        let home = typed.table.as_deref().expect("a column has a table");
        let mut g = Gen::new(cx, home);
        let sql = finish(g.expr(&typed.root), ty);
        let size = sql.len() + g.plan.joins.iter().map(|j| j.on.len()).sum::<usize>();
        (
            Body::Column(ColumnSql {
                sql,
                joins: g.plan.joins,
            }),
            size,
        )
    } else {
        let mut g = Gen::new(cx, "");
        let combine = finish(g.expr(&typed.root), ty);
        let size = combine.len() + g.parts.iter().map(|p| p.sql.len()).sum::<usize>();
        (
            Body::Aggregate(AggregateSql {
                parts: g.parts,
                combine,
            }),
            size,
        )
    };
    if size > max_sql {
        return Err(FormulaError::new(
            "This formula is too complex.",
            typed.root.at,
            typed.root.len,
        ));
    }
    Ok(Compiled {
        dtype: ty.dtype(),
        uses: typed.uses.clone(),
        body,
    })
}

/// A formula that is only `BLANK()` has no type in SQL, so it is given one.
fn finish(sql: String, ty: Ty) -> String {
    if ty == Ty::Any {
        typed_null(Ty::Text)
    } else {
        sql
    }
}

/// A blank of a given kind. DuckDB cannot work out the kind of a bare `NULL` for functions
/// such as `year()`.
pub(crate) fn typed_null(ty: Ty) -> String {
    match ty {
        Ty::Num => "CAST(NULL AS DOUBLE)",
        Ty::Text => "CAST(NULL AS VARCHAR)",
        Ty::Date => "CAST(NULL AS DATE)",
        Ty::DateTime => "CAST(NULL AS TIMESTAMP)",
        Ty::Bool => "CAST(NULL AS BOOLEAN)",
        Ty::Any => "NULL",
    }
    .into()
}

// ---- joins ----------------------------------------------------------------------------

/// The joins a formula needs from its table (`t0`) to the tables it reads: `r1`, `r2`, … in
/// the order they are first used. A join is named after the path that reaches it, so one table
/// read through two links is two joins (V4-D28).
struct JoinPlan<'a> {
    cx: &'a Context<'a>,
    paths: Paths<'a>,
    /// The path of each join, as the link ids crossed from the table of the formula. The alias
    /// of the join at index `i` is `r{i + 1}`.
    keys: Vec<Vec<String>>,
    joins: Vec<Join>,
}

impl<'a> JoinPlan<'a> {
    fn new(cx: &'a Context<'a>, root: &str) -> Self {
        JoinPlan {
            cx,
            paths: Paths::new(root, cx.links()),
            keys: Vec::new(),
            joins: Vec::new(),
        }
    }

    /// The alias of a table, adding the joins that reach it. `via` is the link the path must
    /// cross, when a link was named.
    fn alias(&mut self, table: &str, via: Option<&str>) -> String {
        // A table that cannot be reached has no path. The checker rules this out.
        let Ok(path) = self.paths.path(table, via) else {
            return "t0".into();
        };
        let mut key: Vec<String> = Vec::new();
        let mut from = "t0".to_string();
        for e in path {
            key.push(e.rel.clone());
            if let Some(i) = self.keys.iter().position(|k| *k == key) {
                from = format!("r{}", i + 1);
                continue;
            }
            let (Some(ft), Some(tt)) = (
                self.cx.catalog.table(&e.from_table),
                self.cx.catalog.table(&e.to_table),
            ) else {
                return "t0".into();
            };
            let (Some(fc), Some(tc)) = (ft.column(&e.from_col), tt.column(&e.to_col)) else {
                return "t0".into();
            };
            self.keys.push(key.clone());
            let alias = format!("r{}", self.keys.len());
            let on = join_on(
                &format!("{from}.{}", ident(&e.from_col)),
                &format!("{alias}.{}", ident(&e.to_col)),
                fc.dtype,
                tc.dtype,
            );
            self.joins.push(Join {
                table: e.to_table.clone(),
                sql_ref: e.target_sql(tt, fc.dtype != tc.dtype),
                alias: alias.clone(),
                on,
                rel: e.rel.clone(),
                warn: e.warn,
            });
            from = alias;
        }
        from
    }
}

// ---- the writer ------------------------------------------------------------------------

pub(crate) struct Gen<'a> {
    pub cx: &'a Context<'a>,
    plan: JoinPlan<'a>,
    /// The aggregates found so far (a measure writes its parts here).
    parts: Vec<Part>,
    /// The next name for a bound value: `v0`, `v1`…
    next_var: usize,
}

/// True for a value that costs nothing to write again: a number, a text, a column, a name.
pub(crate) fn atomic(e: &TExpr) -> bool {
    match &e.node {
        TNode::Num(_) | TNode::Str(_) | TNode::Bool(_) | TNode::Col { .. } => true,
        TNode::Sql { atomic, .. } => *atomic,
        TNode::Un(_, x) => atomic(x),
        TNode::Call { name, .. } => matches!(*name, "BLANK" | "TODAY" | "NOW"),
        _ => false,
    }
}

/// The number a literal stands for (`2` or `-2`), if the expression is one.
pub(crate) fn literal_number(e: &TExpr) -> Option<f64> {
    match &e.node {
        TNode::Num(n) => Some(*n),
        TNode::Un(UnOp::Neg, x) => literal_number(x).map(|n| -n),
        TNode::Un(UnOp::Pos, x) => literal_number(x),
        _ => None,
    }
}

impl<'a> Gen<'a> {
    fn new(cx: &'a Context<'a>, root: &str) -> Self {
        Gen {
            cx,
            plan: JoinPlan::new(cx, root),
            parts: Vec::new(),
            next_var: 0,
        }
    }

    pub fn expr(&mut self, e: &TExpr) -> String {
        match &e.node {
            TNode::Num(n) => num_lit(*n),
            TNode::Str(s) => lit(s),
            TNode::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
            TNode::Sql { sql, .. } => sql.clone(),
            TNode::Col { table, column, via } => {
                let a = self.plan.alias(table, via.as_deref());
                let r = format!("{a}.{}", ident(column));
                match e.ty {
                    Ty::Num => format!("CAST({r} AS DOUBLE)"),
                    Ty::Text => format!("CAST({r} AS VARCHAR)"),
                    _ => r,
                }
            }
            TNode::Un(op, x) => {
                let x = self.num(x);
                match op {
                    UnOp::Neg => format!("(-{x})"),
                    UnOp::Pos => x,
                }
            }
            // Arithmetic with a blank is blank, whatever the other side is.
            TNode::Bin(BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow, l, r)
                if l.ty == Ty::Any || r.ty == Ty::Any =>
            {
                typed_null(e.ty)
            }
            TNode::Bin(op, l, r) => self.binary(*op, l, r),
            TNode::Call { name, args } => emit(self, name, args, e.ty),
            TNode::Agg { name, args, home } => self.aggregate(name, args, home, e.ty),
        }
    }

    /// An aggregate becomes a part of its own, over its home table. Here it is a placeholder.
    fn aggregate(&mut self, name: &str, args: &[TExpr], home: &str, ty: Ty) -> String {
        let mut sub = Gen::new(self.cx, home);
        let sql = emit_aggregate(&mut sub, name, args);
        let part = Part {
            home_table: home.to_string(),
            sql,
            joins: sub.plan.joins,
            dtype: ty.dtype(),
            // A sum or a count of no rows is 0 (V4-D22). The rest stay blank.
            zero_when_empty: matches!(
                name,
                "SUM" | "SUMIF" | "COUNT" | "COUNTA" | "COUNTD" | "COUNTIF" | "COUNTROWS"
            ),
            additive: matches!(
                name,
                "SUM" | "SUMIF" | "COUNT" | "COUNTA" | "COUNTIF" | "COUNTROWS"
            ),
        };
        let i = match self.parts.iter().position(|p| *p == part) {
            Some(i) => i,
            None => {
                self.parts.push(part);
                self.parts.len() - 1
            }
        };
        format!("$p{i}")
    }

    /// The value as the kind `ty`. A date becomes a date and time when the other side is one,
    /// and a blank takes the SQL type of the place it is used in.
    pub fn as_ty(&mut self, e: &TExpr, ty: Ty) -> String {
        if e.ty == Ty::Any {
            return typed_null(ty);
        }
        let sql = self.expr(e);
        if e.ty == Ty::Date && ty == Ty::DateTime {
            format!("CAST({sql} AS TIMESTAMP)")
        } else {
            sql
        }
    }

    /// The value where a number is wanted.
    pub fn num(&mut self, e: &TExpr) -> String {
        self.as_ty(e, Ty::Num)
    }

    /// The value where a date or a date and time is wanted.
    pub fn date(&mut self, e: &TExpr) -> String {
        if e.ty == Ty::Any {
            typed_null(Ty::Date)
        } else {
            self.expr(e)
        }
    }

    /// The value as text, the way `&` writes it.
    pub fn text(&mut self, e: &TExpr) -> String {
        let sql = self.expr(e);
        to_text(&sql, e.ty)
    }

    fn fresh(&mut self) -> String {
        let n = self.next_var;
        self.next_var += 1;
        format!("v{n}")
    }

    /// Computes `sql` once and lets `body` read it by name. The body must hold only names.
    pub fn bind_sql(
        &mut self,
        sql: String,
        ty: Ty,
        body: impl FnOnce(&mut Self, &TExpr) -> String,
    ) -> String {
        let name = self.fresh();
        let var = sql_expr(name.clone(), ty, true);
        let inner = body(self, &var);
        format!("list_transform([{sql}], {name} -> {inner})[1]")
    }

    /// Computes every value in `values` that is not a plain name or literal once, in one
    /// lambda, and gives `body` a name (or the literal itself) for each, in the same order.
    /// The body must hold only those names.
    pub fn bind_all(
        &mut self,
        values: &[&TExpr],
        body: impl FnOnce(&mut Self, &[TExpr]) -> String,
    ) -> String {
        let name = self.fresh();
        let mut vars = Vec::with_capacity(values.len());
        let mut fields = Vec::new();
        for (i, v) in values.iter().enumerate() {
            if atomic(v) {
                vars.push((*v).clone());
            } else {
                let sql = self.expr(v);
                fields.push(format!("'k{i}': {sql}"));
                vars.push(sql_expr(format!("{name}.k{i}"), v.ty, true));
            }
        }
        let inner = body(self, &vars);
        if fields.is_empty() {
            return inner;
        }
        format!(
            "list_transform([{{{}}}], {name} -> {inner})[1]",
            fields.join(", ")
        )
    }

    /// The SQL of a text value as a value of its own: numbers and dates written as `&` writes
    /// them. It is a plain name or literal only when it is text already.
    pub fn text_expr(&mut self, e: &TExpr) -> TExpr {
        let sql = self.text(e);
        sql_expr(sql, Ty::Text, atomic(e) && e.ty == Ty::Text)
    }

    /// Blank for infinity and "not a number", the value itself otherwise.
    pub fn finite(&mut self, sql: String) -> String {
        self.bind_sql(sql, Ty::Num, |_, v| {
            let TNode::Sql { sql: v, .. } = &v.node else {
                unreachable!("bind_sql gives a name")
            };
            format!("CASE WHEN isfinite({v}) THEN {v} END")
        })
    }

    /// `x` rounded to `digits` places (none: 0). `Half` goes away from zero on a half, as in
    /// Excel: `ROUND(2.675, 2)` is 2.68. The product is cleaned to 8 places first, so
    /// 2.3 * 100 is 230 and not 229.999…. A value that cannot take that many places is blank.
    pub fn round(&mut self, x: &TExpr, digits: Option<&TExpr>, mode: Round) -> String {
        let places = match digits {
            None => Some(0.0),
            Some(d) => literal_number(d),
        };
        if let Some(n) = places {
            // The scale is a number written here, so there is nothing to bind for it.
            let s: f64 = format!("1e{}", n.trunc() as i64)
                .parse()
                .unwrap_or(f64::NAN);
            if !(s.is_finite() && s > 0.0) {
                return typed_null(Ty::Num);
            }
            let s = num_lit(s);
            return self.bind_all(&[x], |g, v| {
                let v = g.num(&v[0]);
                round_body(&v, &s, false, mode)
            });
        }
        // The scale comes from a value that is not a literal. It is computed with the number.
        let d = self.num(digits.expect("not a literal, so there are digits"));
        let scale = sql_expr(format!("power(1e1, trunc({d}))"), Ty::Num, false);
        self.bind_all(&[x, &scale], |g, v| {
            let (x, s) = (g.num(&v[0]), g.expr(&v[1]));
            round_body(&x, &s, true, mode)
        })
    }

    fn binary(&mut self, op: BinOp, l: &TExpr, r: &TExpr) -> String {
        match op {
            BinOp::Concat => format!("concat({}, {})", self.text(l), self.text(r)),
            BinOp::Add => self.add(l, r),
            BinOp::Sub => self.sub(l, r),
            BinOp::Mul => format!("({} * {})", self.num(l), self.num(r)),
            BinOp::Div => self.divide(l, r),
            BinOp::Pow => {
                let pow = format!("pow({}, {})", self.num(l), self.num(r));
                self.finite(pow)
            }
            _ => self.compare(op, l, r),
        }
    }

    /// `a / b`. Dividing by zero gives infinity or "not a number" in DuckDB, which is blank
    /// here. A divisor that is a number other than 0 needs no check.
    pub fn divide(&mut self, l: &TExpr, r: &TExpr) -> String {
        let (a, b) = (self.num(l), self.num(r));
        let quotient = format!("({a} / {b})");
        if literal_number(r).is_some_and(|n| n != 0.0) {
            quotient
        } else {
            self.finite(quotient)
        }
    }

    fn add(&mut self, l: &TExpr, r: &TExpr) -> String {
        // A date moves by days; the number is cut to whole days.
        match (l.ty, r.ty) {
            (Ty::Date, _) => format!("TRY({} + {})", self.expr(l), whole_days(&self.num(r))),
            (_, Ty::Date) => format!("TRY({} + {})", self.expr(r), whole_days(&self.num(l))),
            (Ty::DateTime, _) => {
                format!("TRY({} + {})", self.expr(l), day_fraction(&self.num(r)))
            }
            (_, Ty::DateTime) => {
                format!("TRY({} + {})", self.expr(r), day_fraction(&self.num(l)))
            }
            _ => format!("({} + {})", self.num(l), self.num(r)),
        }
    }

    fn sub(&mut self, l: &TExpr, r: &TExpr) -> String {
        match (l.ty, r.ty) {
            (Ty::Date, Ty::Date) => {
                format!("CAST({} - {} AS DOUBLE)", self.expr(l), self.expr(r))
            }
            (x, y) if x.is_temporal() && y.is_temporal() => {
                let (a, b) = (self.expr(l), self.expr(r));
                format!(
                    "((epoch(CAST({a} AS TIMESTAMP)) - epoch(CAST({b} AS TIMESTAMP))) / 86400e0)"
                )
            }
            (Ty::Date, _) => format!("TRY({} - {})", self.expr(l), whole_days(&self.num(r))),
            (Ty::DateTime, _) => {
                format!("TRY({} - {})", self.expr(l), day_fraction(&self.num(r)))
            }
            _ => format!("({} - {})", self.num(l), self.num(r)),
        }
    }

    /// A comparison. Text ignores case. `=` and `<>` never give blank: a blank cell equals
    /// only another blank cell, and `text = ""` also holds for a blank cell.
    pub fn compare(&mut self, op: BinOp, l: &TExpr, r: &TExpr) -> String {
        if unify(l.ty, r.ty) == Some(Ty::Text) && matches!(op, BinOp::Eq | BinOp::Ne) {
            for (x, y) in [(l, r), (r, l)] {
                if matches!(&y.node, TNode::Str(s) if s.is_empty()) {
                    let a = self.as_ty(x, Ty::Text);
                    let blank = format!("(COALESCE({a}, '') = '')");
                    return if op == BinOp::Eq {
                        blank
                    } else {
                        format!("(NOT {blank})")
                    };
                }
            }
        }
        if unify(l.ty, r.ty) == Some(Ty::Text) {
            let (a, b) = (self.as_ty(l, Ty::Text), self.as_ty(r, Ty::Text));
            return compare_sql(op, &format!("lower({a})"), &format!("lower({b})"));
        }
        let target = unify(l.ty, r.ty).unwrap_or(l.ty);
        let (a, b) = (self.as_ty(l, target), self.as_ty(r, target));
        compare_sql(op, &a, &b)
    }
}

fn compare_sql(op: BinOp, a: &str, b: &str) -> String {
    match op {
        BinOp::Eq => format!("({a} IS NOT DISTINCT FROM {b})"),
        BinOp::Ne => format!("({a} IS DISTINCT FROM {b})"),
        other => format!("({a} {} {b})", other.symbol()),
    }
}

// ---- small pieces ----------------------------------------------------------------------

/// A number as a DOUBLE literal: `2e0`, `1.25e1`.
pub(crate) fn num_lit(n: f64) -> String {
    format!("{n:e}")
}

/// SQL that is already written, as a value the writer can pass around.
pub(crate) fn sql_expr(sql: String, ty: Ty, atomic: bool) -> TExpr {
    TExpr {
        node: TNode::Sql { sql, atomic },
        ty,
        at: 0,
        len: 0,
    }
}

/// The rounding itself, over a number `v` and a scale `s` that are names or literals. Past 15
/// digits the value has no places left to round. The `+ 0e0` turns a negative zero into a
/// plain zero. With `guard`, a scale that is not a usable number gives blank.
fn round_body(v: &str, s: &str, guard: bool, mode: Round) -> String {
    let whole = match mode {
        Round::Half => format!("floor(round(abs({v}) * {s}, 8) + 5e-1)"),
        Round::Up => format!("ceil(round(abs({v}) * {s}, 8))"),
        Round::Down => format!("floor(round(abs({v}) * {s}, 8))"),
    };
    let core = format!(
        "CASE WHEN abs({v}) * {s} >= 1e15 THEN {v} ELSE (sign({v}) * {whole} / {s} + 0e0) END"
    );
    if guard {
        format!("CASE WHEN isfinite({s}) AND {s} > 0e0 THEN {core} END")
    } else {
        core
    }
}

/// A whole number, or blank when it does not fit.
pub(crate) fn int_of(sql: &str) -> String {
    format!("TRY(CAST(trunc({sql}) AS BIGINT))")
}

fn whole_days(sql: &str) -> String {
    format!("TRY(CAST(trunc({sql}) AS INTEGER))")
}

fn day_fraction(sql: &str) -> String {
    format!("to_seconds({sql} * 86400e0)")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Round {
    /// Half away from zero, as Excel rounds.
    Half,
    Up,
    Down,
}

/// A value as text. Numbers have no trailing `.0` (15 digits, like Excel), and a negative zero
/// is a plain zero.
pub(crate) fn to_text(sql: &str, ty: Ty) -> String {
    match ty {
        Ty::Text => sql.to_string(),
        Ty::Num => format!("printf('%.15g', {sql} + 0e0)"),
        Ty::Date => format!("strftime({sql}, '%Y-%m-%d')"),
        Ty::DateTime => format!("strftime({sql}, '%Y-%m-%d %H:%M:%S')"),
        Ty::Bool => format!("upper(CAST({sql} AS VARCHAR))"),
        Ty::Any => typed_null(Ty::Text),
    }
}

/// A value the way the Data view shows it: dates as text, the rest as it is.
pub fn display(sql: &str, ty: Ty) -> String {
    match ty {
        Ty::Date => format!("strftime({sql}, '%Y-%m-%d')"),
        Ty::DateTime => format!("strftime({sql}, '%Y-%m-%d %H:%M:%S')"),
        Ty::Any => typed_null(Ty::Text),
        _ => sql.to_string(),
    }
}
