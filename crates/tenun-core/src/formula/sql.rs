//! Writes DuckDB SQL from a checked formula.
//!
//! Identifiers are quoted and came from the catalog. The only user text in the output is a
//! number or an escaped string literal. Every number is a DOUBLE, so `5 / 2` is 2.5 and a
//! sum of whole numbers never overflows.

use super::check::{TExpr, TNode, Ty, Typed, unify};
use super::functions::{emit, emit_aggregate};
use super::parse::{BinOp, UnOp};
use super::{AggregateSql, Body, ColumnSql, Compiled, Context, Join, Kind, Part};
use crate::catalog::{ident, lit};
use crate::query::{Edge, join_on, reach};
use std::collections::HashMap;

pub fn compile(typed: &Typed, cx: &Context<'_>) -> Compiled {
    let ty = typed.root.ty;
    let body = if typed.kind == Kind::Column {
        let home = typed.table.as_deref().expect("a column has a table");
        let mut g = Gen::new(cx, home);
        let sql = finish(g.expr(&typed.root), ty);
        Body::Column(ColumnSql {
            sql,
            joins: g.plan.joins,
        })
    } else {
        let mut g = Gen::new(cx, "");
        let combine = finish(g.expr(&typed.root), ty);
        Body::Aggregate(AggregateSql {
            parts: g.parts,
            combine,
        })
    };
    Compiled {
        dtype: ty.dtype(),
        uses: typed.uses.clone(),
        body,
    }
}

/// A formula that is only `BLANK()` has no type in SQL, so it is given one.
fn finish(sql: String, ty: Ty) -> String {
    if ty == Ty::Any {
        "CAST(NULL AS VARCHAR)".into()
    } else {
        sql
    }
}

// ---- joins ----------------------------------------------------------------------------

/// The joins a formula needs from its table (`t0`) to the tables it reads: `r1`, `r2`, … in
/// the order they are first used.
struct JoinPlan<'a> {
    cx: &'a Context<'a>,
    root: String,
    paths: HashMap<String, Option<Edge>>,
    /// Table ids; the alias of the table at index `i` is `r{i + 1}`.
    joined: Vec<String>,
    joins: Vec<Join>,
}

impl<'a> JoinPlan<'a> {
    fn new(cx: &'a Context<'a>, root: &str) -> Self {
        JoinPlan {
            cx,
            root: root.to_string(),
            paths: reach(root, cx.rels),
            joined: Vec::new(),
            joins: Vec::new(),
        }
    }

    fn existing(&self, table: &str) -> Option<String> {
        if table == self.root {
            return Some("t0".into());
        }
        self.joined
            .iter()
            .position(|t| t == table)
            .map(|i| format!("r{}", i + 1))
    }

    /// The alias of a table, adding the joins that reach it.
    fn alias(&mut self, table: &str) -> String {
        if let Some(a) = self.existing(table) {
            return a;
        }
        // The chain back to the table of the formula, then joined root-first.
        let mut chain = Vec::new();
        let mut cur = table.to_string();
        while let Some(Some(e)) = self.paths.get(&cur) {
            chain.push(e.clone());
            cur = e.from_table.clone();
        }
        for e in chain.into_iter().rev() {
            if self.existing(&e.to_table).is_some() {
                continue;
            }
            let from = self.existing(&e.from_table).unwrap_or_else(|| "t0".into());
            self.joined.push(e.to_table.clone());
            let alias = format!("r{}", self.joined.len());
            let (Some(ft), Some(tt)) = (
                self.cx.catalog.table(&e.from_table),
                self.cx.catalog.table(&e.to_table),
            ) else {
                continue;
            };
            let (Some(fc), Some(tc)) = (ft.column(&e.from_col), tt.column(&e.to_col)) else {
                continue;
            };
            let on = join_on(
                &format!("{from}.{}", ident(&e.from_col)),
                &format!("{alias}.{}", ident(&e.to_col)),
                fc.dtype,
                tc.dtype,
            );
            self.joins.push(Join {
                table: e.to_table.clone(),
                sql_ref: tt.sql_ref.clone(),
                alias,
                on,
            });
        }
        // A table that cannot be reached has no alias. The checker rules this out.
        self.existing(table).unwrap_or_else(|| "t0".into())
    }
}

// ---- the writer ------------------------------------------------------------------------

pub(crate) struct Gen<'a> {
    pub cx: &'a Context<'a>,
    plan: JoinPlan<'a>,
    /// The aggregates found so far (a measure writes its parts here).
    parts: Vec<Part>,
}

impl<'a> Gen<'a> {
    fn new(cx: &'a Context<'a>, root: &str) -> Self {
        Gen {
            cx,
            plan: JoinPlan::new(cx, root),
            parts: Vec::new(),
        }
    }

    pub fn expr(&mut self, e: &TExpr) -> String {
        match &e.node {
            TNode::Num(n) => num_lit(*n),
            TNode::Str(s) => lit(s),
            TNode::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
            TNode::Col { table, column } => {
                let a = self.plan.alias(table);
                let r = format!("{a}.{}", ident(column));
                match e.ty {
                    Ty::Num => format!("CAST({r} AS DOUBLE)"),
                    Ty::Text => format!("CAST({r} AS VARCHAR)"),
                    _ => r,
                }
            }
            TNode::Un(op, x) => {
                let x = self.expr(x);
                match op {
                    UnOp::Neg => format!("(-{x})"),
                    UnOp::Pos => x,
                }
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

    /// The value as the kind `ty` (a date becomes a date and time when the other side is one).
    pub fn as_ty(&mut self, e: &TExpr, ty: Ty) -> String {
        let sql = self.expr(e);
        if e.ty == Ty::Date && ty == Ty::DateTime {
            format!("CAST({sql} AS TIMESTAMP)")
        } else {
            sql
        }
    }

    /// The value as text, the way `&` writes it.
    pub fn text(&mut self, e: &TExpr) -> String {
        let sql = self.expr(e);
        to_text(&sql, e.ty)
    }

    fn binary(&mut self, op: BinOp, l: &TExpr, r: &TExpr) -> String {
        match op {
            BinOp::Concat => format!("concat({}, {})", self.text(l), self.text(r)),
            BinOp::Add => self.add(l, r),
            BinOp::Sub => self.sub(l, r),
            BinOp::Mul => format!("({} * {})", self.expr(l), self.expr(r)),
            BinOp::Div => format!("({} / NULLIF({}, 0e0))", self.expr(l), self.expr(r)),
            BinOp::Pow => finite(&format!("pow({}, {})", self.expr(l), self.expr(r))),
            _ => self.compare(op, l, r),
        }
    }

    fn add(&mut self, l: &TExpr, r: &TExpr) -> String {
        let (a, b) = (self.expr(l), self.expr(r));
        // A date moves by days; the number is cut to whole days.
        match (l.ty, r.ty) {
            (Ty::Date, _) => format!("TRY({a} + {})", whole_days(&b)),
            (_, Ty::Date) => format!("TRY({b} + {})", whole_days(&a)),
            (Ty::DateTime, _) => format!("TRY({a} + {})", day_fraction(&b)),
            (_, Ty::DateTime) => format!("TRY({b} + {})", day_fraction(&a)),
            _ => format!("({a} + {b})"),
        }
    }

    fn sub(&mut self, l: &TExpr, r: &TExpr) -> String {
        let (a, b) = (self.expr(l), self.expr(r));
        match (l.ty, r.ty) {
            (Ty::Date, Ty::Date) => format!("CAST({a} - {b} AS DOUBLE)"),
            (x, y) if x.is_temporal() && y.is_temporal() => {
                format!(
                    "((epoch(CAST({a} AS TIMESTAMP)) - epoch(CAST({b} AS TIMESTAMP))) / 86400e0)"
                )
            }
            (Ty::Date, _) => format!("TRY({a} - {})", whole_days(&b)),
            (Ty::DateTime, _) => format!("TRY({a} - {})", day_fraction(&b)),
            _ => format!("({a} - {b})"),
        }
    }

    /// A comparison. Text ignores case. `=` and `<>` never give blank: a blank cell equals
    /// only another blank cell, and `text = ""` also holds for a blank cell.
    pub fn compare(&mut self, op: BinOp, l: &TExpr, r: &TExpr) -> String {
        if unify(l.ty, r.ty) == Some(Ty::Text) && matches!(op, BinOp::Eq | BinOp::Ne) {
            for (x, y) in [(l, r), (r, l)] {
                if matches!(&y.node, TNode::Str(s) if s.is_empty()) {
                    let a = self.expr(x);
                    let blank = format!("({a} IS NULL OR {a} = '')");
                    return if op == BinOp::Eq {
                        blank
                    } else {
                        format!("(NOT {blank})")
                    };
                }
            }
        }
        if unify(l.ty, r.ty) == Some(Ty::Text) {
            let (a, b) = (self.expr(l), self.expr(r));
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

/// Blank when the value is infinite or not a number (an overflow, the root of a negative).
pub(crate) fn finite(sql: &str) -> String {
    format!("CASE WHEN isfinite({sql}) THEN {sql} END")
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

/// Rounds `x` to `digits` places, away from zero on a half like Excel (`ROUND(2.675, 2)` is
/// 2.68). The product is cleaned to 8 places first, so 2.3 * 100 is 230 and not 229.999….
/// The `+ 0e0` turns a negative zero into a plain zero.
pub(crate) fn round_sql(x: &str, digits: &str, mode: Round) -> String {
    let scale = format!("power(1e1, trunc({digits}))");
    let scaled = format!("round(abs({x}) * {scale}, 8)");
    let whole = match mode {
        Round::Half => format!("floor({scaled} + 5e-1)"),
        Round::Up => format!("ceil({scaled})"),
        Round::Down => format!("floor({scaled})"),
    };
    format!("(sign({x}) * {whole} / {scale} + 0e0)")
}

/// A value as text. Numbers have no trailing `.0` (15 digits, like Excel).
pub(crate) fn to_text(sql: &str, ty: Ty) -> String {
    match ty {
        Ty::Text => sql.to_string(),
        Ty::Num => format!("printf('%.15g', {sql})"),
        Ty::Date => format!("strftime({sql}, '%Y-%m-%d')"),
        Ty::DateTime => format!("strftime({sql}, '%Y-%m-%d %H:%M:%S')"),
        Ty::Bool => format!("CASE WHEN {sql} THEN 'TRUE' WHEN NOT {sql} THEN 'FALSE' END"),
        Ty::Any => "CAST(NULL AS VARCHAR)".into(),
    }
}

/// A value the way the Data view shows it: dates as text, the rest as it is.
pub fn display(sql: &str, ty: Ty) -> String {
    match ty {
        Ty::Date => format!("strftime({sql}, '%Y-%m-%d')"),
        Ty::DateTime => format!("strftime({sql}, '%Y-%m-%d %H:%M:%S')"),
        Ty::Any => "CAST(NULL AS VARCHAR)".into(),
        _ => sql.to_string(),
    }
}
