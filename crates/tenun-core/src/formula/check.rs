//! The checker: resolves names, infers types and enforces the rules of the three contexts.
//!
//! It turns the syntax tree into a typed tree (`TExpr`) whose references point at real tables
//! and columns. `sql` writes SQL from that tree and trusts it.

use super::functions::{self, Def};
use super::parse::{BinOp, Expr, ExprKind, UnOp, parse};
use super::{Context, FormulaError, Kind};
use crate::catalog::{Catalog, DType, Table};
use crate::query::reach;

/// The kind of value a part of a formula has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Num,
    Text,
    Date,
    DateTime,
    Bool,
    /// `BLANK()`: it takes the kind of whatever it meets.
    Any,
}

impl Ty {
    pub fn from_dtype(d: DType) -> Ty {
        match d {
            DType::Number => Ty::Num,
            DType::Date => Ty::Date,
            DType::DateTime => Ty::DateTime,
            DType::Text => Ty::Text,
            DType::Bool => Ty::Bool,
        }
    }

    /// A formula that is only `BLANK()` reads as text.
    pub fn dtype(self) -> DType {
        match self {
            Ty::Num => DType::Number,
            Ty::Date => DType::Date,
            Ty::DateTime => DType::DateTime,
            Ty::Bool => DType::Bool,
            Ty::Text | Ty::Any => DType::Text,
        }
    }

    pub fn is_temporal(self) -> bool {
        matches!(self, Ty::Date | Ty::DateTime)
    }

    /// For error messages: "a number", "text".
    pub fn words(self) -> &'static str {
        match self {
            Ty::Num => "a number",
            Ty::Text => "text",
            Ty::Date => "a date",
            Ty::DateTime => "a date and time",
            Ty::Bool => "true or false",
            Ty::Any => "blank",
        }
    }
}

/// The kind that holds both values, when there is one. A date meets a date and time as a date
/// and time. `BLANK()` meets anything.
pub fn unify(a: Ty, b: Ty) -> Option<Ty> {
    match (a, b) {
        _ if a == b => Some(a),
        (Ty::Any, x) | (x, Ty::Any) => Some(x),
        (Ty::Date, Ty::DateTime) | (Ty::DateTime, Ty::Date) => Some(Ty::DateTime),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct TExpr {
    pub node: TNode,
    pub ty: Ty,
    pub at: usize,
    pub len: usize,
}

#[derive(Debug, Clone)]
pub enum TNode {
    Num(f64),
    Str(String),
    Bool(bool),
    /// A column. `table` is a table id.
    Col {
        table: String,
        column: String,
    },
    /// SQL that is already written. The SQL writer uses it for a value it has bound to a name,
    /// or built itself. `atomic` is true when it is a name or a literal, which is safe to repeat.
    Sql {
        sql: String,
        atomic: bool,
    },
    Un(UnOp, Box<TExpr>),
    Bin(BinOp, Box<TExpr>, Box<TExpr>),
    /// A row-wise function. `name` is the canonical upper-case name.
    Call {
        name: &'static str,
        args: Vec<TExpr>,
    },
    /// An aggregate over the rows of the table `home`.
    Agg {
        name: &'static str,
        args: Vec<TExpr>,
        home: String,
    },
}

/// A checked formula.
#[derive(Debug, Clone)]
pub struct Typed {
    pub root: TExpr,
    pub kind: Kind,
    /// The id of the table the formula belongs to (a column, a group, or a measure given one).
    pub table: Option<String>,
    /// The ids of the tables it reads, in the order it first reads them.
    pub uses: Vec<String>,
}

/// The formula text, for quoting a piece of it in a message.
pub struct Src {
    chars: Vec<char>,
}

impl Src {
    pub fn new(s: &str) -> Self {
        Src {
            chars: s.chars().collect(),
        }
    }

    /// The piece of the formula at `at`, `len`, short enough for a message.
    pub fn snip(&self, at: usize, len: usize) -> String {
        let end = (at + len).min(self.chars.len());
        let s: String = self.chars[at.min(end)..end]
            .iter()
            .map(|c| if c.is_whitespace() { ' ' } else { *c })
            .collect();
        if s.chars().count() > 40 {
            format!("{}…", s.chars().take(39).collect::<String>())
        } else {
            s
        }
    }

    pub fn of(&self, e: &TExpr) -> String {
        self.snip(e.at, e.len)
    }
}

/// A table by its label first, then by its id, whatever the case.
pub fn find_table<'a>(catalog: &'a Catalog, name: &str) -> Option<&'a Table> {
    let n = name.trim();
    let same = |a: &str| a.eq_ignore_ascii_case(n) || a.to_lowercase() == n.to_lowercase();
    catalog
        .tables
        .iter()
        .find(|t| t.label == n)
        .or_else(|| catalog.tables.iter().find(|t| t.id == n))
        .or_else(|| catalog.tables.iter().find(|t| same(&t.label)))
        .or_else(|| catalog.tables.iter().find(|t| same(&t.id)))
}

/// How a name was found in a table.
pub(crate) enum Found<'a> {
    No,
    One(&'a crate::catalog::Column),
    /// Two columns answer to the name: one by its real name, the other by the name the app shows
    /// (or two columns show the same name). The first is the real-name match when there is one.
    Two(&'a crate::catalog::Column, &'a crate::catalog::Column),
}

/// The column a name stands for: its real name or the name the app shows, exact first, then
/// whatever the case. When two columns answer to the name, that is an ambiguity, not a choice.
pub(crate) fn find_column<'a>(t: &'a Table, name: &str) -> Found<'a> {
    let n = name.trim();
    let lower = n.to_lowercase();
    for exact in [true, false] {
        let same = |a: &str| {
            if exact {
                a == n
            } else {
                a.to_lowercase() == lower
            }
        };
        // Columns that answer by their real name come first, then those by the shown name.
        let mut found: Vec<&crate::catalog::Column> =
            t.columns.iter().filter(|c| same(&c.name)).collect();
        for c in &t.columns {
            if c.label.as_deref().is_some_and(same) && !found.iter().any(|f| std::ptr::eq(*f, c)) {
                found.push(c);
            }
        }
        match found.as_slice() {
            [] => {}
            [one] => return Found::One(one),
            [a, b, ..] => return Found::Two(a, b),
        }
    }
    Found::No
}

/// The name closest to `wanted`, when it is close enough to be a typing slip.
pub(crate) fn closest<'a>(wanted: &str, names: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let w: Vec<char> = wanted.trim().to_lowercase().chars().collect();
    let limit = (w.len() / 3).max(1);
    names
        .map(|n| {
            (
                distance(&w, &n.to_lowercase().chars().collect::<Vec<_>>()),
                n,
            )
        })
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, n)| n)
}

/// The number of single-character changes between two names. Two letters swapped count as
/// one change, because that is the commonest slip.
fn distance(a: &[char], b: &[char]) -> usize {
    let mut rows: Vec<Vec<usize>> = vec![(0..=b.len()).collect()];
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            let mut d = (rows[i][j] + cost).min(rows[i][j + 1] + 1).min(cur[j] + 1);
            if i > 0 && j > 0 && *ca == b[j - 1] && a[i - 1] == *cb {
                d = d.min(rows[i - 1][j - 1] + 1);
            }
            cur.push(d);
        }
        rows.push(cur);
    }
    rows[a.len()][b.len()]
}

pub fn check(
    src: &str,
    kind: Kind,
    table: Option<&str>,
    cx: &Context<'_>,
) -> Result<Typed, FormulaError> {
    let ast = parse(src)?;
    let current = match table {
        Some(t) => Some(find_table(cx.catalog, t).ok_or_else(|| {
            FormulaError::new(format!("The table {t} is not in this workspace."), 0, 0)
        })?),
        None => None,
    };
    if kind != Kind::Measure && current.is_none() {
        return Err(FormulaError::new(
            "Pick the table this formula belongs to first.",
            0,
            0,
        ));
    }
    let mut c = Checker {
        src: Src::new(src),
        cx,
        kind,
        current,
        uses: Vec::new(),
        agg: None,
    };
    let root = c.expr(&ast)?;
    if kind != Kind::Column && !has_agg(&root) {
        return Err(FormulaError::new(
            "This formula has no total in it. Use SUM, AVERAGE, COUNT or another total, like SUM([amount]).",
            ast.at,
            ast.len,
        ));
    }
    Ok(Typed {
        root,
        kind,
        table: current.map(|t| t.id.clone()),
        uses: c.uses,
    })
}

fn has_agg(e: &TExpr) -> bool {
    match &e.node {
        TNode::Agg { .. } => true,
        TNode::Un(_, x) => has_agg(x),
        TNode::Bin(_, l, r) => has_agg(l) || has_agg(r),
        TNode::Call { args, .. } => args.iter().any(has_agg),
        _ => false,
    }
}

/// The columns an aggregate has read so far.
struct AggScope {
    name: &'static str,
    tables: Vec<String>,
}

struct Checker<'a> {
    src: Src,
    cx: &'a Context<'a>,
    kind: Kind,
    current: Option<&'a Table>,
    uses: Vec<String>,
    /// Set while the arguments of an aggregate are checked.
    agg: Option<AggScope>,
}

impl<'a> Checker<'a> {
    fn expr(&mut self, e: &Expr) -> Result<TExpr, FormulaError> {
        let (at, len) = (e.at, e.len);
        let make = |node, ty| Ok(TExpr { node, ty, at, len });
        match &e.kind {
            ExprKind::Num(n) => make(TNode::Num(*n), Ty::Num),
            ExprKind::Str(s) => make(TNode::Str(s.clone()), Ty::Text),
            ExprKind::Bool(b) => make(TNode::Bool(*b), Ty::Bool),
            ExprKind::Ref { table, column } => self.reference(e, table.as_deref(), column),
            ExprKind::Unary { op, expr } => {
                let x = self.expr(expr)?;
                if !matches!(x.ty, Ty::Num | Ty::Any) {
                    let sym = if *op == UnOp::Neg { "-" } else { "+" };
                    return Err(FormulaError::new(
                        format!(
                            "{sym} works on numbers, but {} is {}.",
                            self.src.of(&x),
                            x.ty.words()
                        ),
                        x.at,
                        x.len,
                    ));
                }
                make(TNode::Un(*op, Box::new(x)), Ty::Num)
            }
            ExprKind::Binary { op, left, right } => {
                let l = self.expr(left)?;
                let r = self.expr(right)?;
                let ty = self.binary(*op, &l, &r)?;
                make(TNode::Bin(*op, Box::new(l), Box::new(r)), ty)
            }
            ExprKind::Call { name, args } => self.call(e, name, args),
        }
    }

    // ---- references --------------------------------------------------------------------

    fn reference(
        &mut self,
        e: &Expr,
        table: Option<&str>,
        column: &str,
    ) -> Result<TExpr, FormulaError> {
        let text = self.src.snip(e.at, e.len);
        if self.kind != Kind::Column && self.agg.is_none() {
            return Err(FormulaError::new(
                format!("{text} needs a total here, like SUM({text})."),
                e.at,
                e.len,
            ));
        }
        let t: &Table = match table {
            Some(name) => self.table_named(name, e)?,
            None => match self.current {
                Some(t) => t,
                None => {
                    return Err(FormulaError::new(
                        format!(
                            "{text} needs its table here. Write the table first, like [Table].[{column}]."
                        ),
                        e.at,
                        e.len,
                    ));
                }
            },
        };
        let col = match find_column(t, column) {
            Found::One(c) => c,
            Found::Two(a, b) => {
                // How each column answers to the name: by its real name, or by the name the
                // app shows for it.
                let wanted = column.trim().to_lowercase();
                let how = |c: &crate::catalog::Column| {
                    let by_name = c.name.to_lowercase() == wanted;
                    match (by_name, c.label.as_deref()) {
                        (false, Some(l)) => format!("[{}], which the app shows as [{l}]", c.name),
                        _ => format!("[{}] by its name in the file", c.name),
                    }
                };
                return Err(FormulaError::new(
                    format!(
                        "{text} matches two columns: {}, and {}. Rename one of them so a formula can tell them apart.",
                        how(a),
                        how(b)
                    ),
                    e.at,
                    e.len,
                ));
            }
            Found::No => {
                let names = t
                    .columns
                    .iter()
                    .flat_map(|c| std::iter::once(c.name.as_str()).chain(c.label.as_deref()));
                return Err(FormulaError::new(
                    match closest(column, names) {
                        Some(s) => format!("No column [{column}]. Did you mean [{s}]?"),
                        None => format!("No column [{column}] in {}.", t.label),
                    },
                    e.at,
                    e.len,
                ));
            }
        };
        // A column or a group reads related tables through the many-to-one path from its own.
        if self.kind != Kind::Measure
            && let Some(cur) = self.current
            && t.id != cur.id
            && !reach(&cur.id, self.cx.rels).contains_key(&t.id)
        {
            return Err(FormulaError::new(
                self.unreachable_message(&text, cur, t),
                e.at,
                e.len,
            ));
        }
        self.note_use(&t.id);
        if let Some(a) = &mut self.agg
            && !a.tables.contains(&t.id)
        {
            a.tables.push(t.id.clone());
        }
        Ok(TExpr {
            node: TNode::Col {
                table: t.id.clone(),
                column: col.name.clone(),
            },
            ty: Ty::from_dtype(col.dtype),
            at: e.at,
            len: e.len,
        })
    }

    fn unreachable_message(&self, text: &str, cur: &Table, t: &Table) -> String {
        if self.kind == Kind::Group {
            return format!(
                "{} is not related to {} in the right direction, so a group value cannot read {text}. Check the line between them in Model.",
                t.label, cur.label
            );
        }
        if reach(&t.id, self.cx.rels).contains_key(&cur.id) {
            format!(
                "{text} has many values for each row of {cur_label}. Total it in a measure with SUMIF or COUNTROWS, or check the line between {t_label} and {cur_label} in Model.",
                cur_label = cur.label,
                t_label = t.label
            )
        } else {
            format!(
                "{} is not related to {}. Draw a line between them in Model.",
                t.label, cur.label
            )
        }
    }

    fn table_named(&self, name: &str, e: &Expr) -> Result<&'a Table, FormulaError> {
        find_table(self.cx.catalog, name).ok_or_else(|| {
            let names = self.cx.catalog.tables.iter().map(|t| t.label.as_str());
            FormulaError::new(
                match closest(name, names) {
                    Some(s) => format!("No table [{name}]. Did you mean [{s}]?"),
                    None => format!("No table [{name}] in this workspace."),
                },
                e.at,
                e.len,
            )
        })
    }

    fn note_use(&mut self, id: &str) {
        if !self.uses.iter().any(|u| u == id) {
            self.uses.push(id.to_string());
        }
    }

    // ---- operators ---------------------------------------------------------------------

    fn binary(&self, op: BinOp, l: &TExpr, r: &TExpr) -> Result<Ty, FormulaError> {
        use Ty::*;
        let (a, b) = (l.ty, r.ty);
        if op == BinOp::Concat {
            return Ok(Text);
        }
        if op.is_comparison() {
            return self.comparison(op, l, r);
        }
        // BLANK() does the work of the other side.
        // A blank next to a date is days (to add) or a date (to take a date from).
        let (a, b) = match (a, b) {
            (Any, Any) => (Num, Num),
            (Any, x) if x.is_temporal() && op == BinOp::Add => (Num, x),
            (x, Any) if x.is_temporal() => (x, Num),
            (Any, x) => (x, x),
            (x, Any) => (x, x),
            pair => pair,
        };
        let ok = match op {
            BinOp::Add => match (a, b) {
                (Num, Num) => Some(Num),
                (Date, Num) | (Num, Date) => Some(Date),
                (DateTime, Num) | (Num, DateTime) => Some(DateTime),
                _ => None,
            },
            BinOp::Sub => match (a, b) {
                (Num, Num) => Some(Num),
                (Date, Num) => Some(Date),
                (DateTime, Num) => Some(DateTime),
                (x, y) if x.is_temporal() && y.is_temporal() => Some(Num),
                _ => None,
            },
            _ => (a == Num && b == Num).then_some(Num),
        };
        match ok {
            Some(t) => Ok(t),
            None => Err(self.arithmetic_error(op, l, r, a, b)),
        }
    }

    /// Why `l op r` does not work: the first side of the wrong kind, or the pair as a whole.
    fn arithmetic_error(&self, op: BinOp, l: &TExpr, r: &TExpr, a: Ty, b: Ty) -> FormulaError {
        let sym = op.symbol();
        let dates_ok = matches!(op, BinOp::Add | BinOp::Sub);
        let wrong = |t: Ty| {
            if dates_ok {
                !matches!(t, Ty::Num | Ty::Date | Ty::DateTime)
            } else {
                t != Ty::Num
            }
        };
        let culprit = if wrong(a) {
            Some((l, a))
        } else if wrong(b) {
            Some((r, b))
        } else {
            None
        };
        if let Some((x, t)) = culprit {
            let hint = match (op, t) {
                (BinOp::Add, Ty::Text) => " To join text, use &.",
                (_, Ty::Text) => " Use VALUE() to read text as a number.",
                (_, Ty::Bool) => " Use IF() to turn it into a number.",
                _ => "",
            };
            return FormulaError::new(
                format!(
                    "{sym} works on {}, but {} is {}.{hint}",
                    if dates_ok {
                        "numbers and dates"
                    } else {
                        "numbers"
                    },
                    self.src.of(x),
                    t.words()
                ),
                x.at,
                x.len,
            );
        }
        // Two dates added, or a date taken from a number.
        let msg = if op == BinOp::Add {
            "Two dates cannot be added. Add a number of days to a date, or subtract one date from another."
        } else {
            "A date cannot be taken from a number. Subtract a number of days from a date, or one date from another."
        };
        FormulaError::new(msg, l.at, r.at + r.len - l.at)
    }

    fn comparison(&self, op: BinOp, l: &TExpr, r: &TExpr) -> Result<Ty, FormulaError> {
        use Ty::*;
        let ok = match (l.ty, r.ty) {
            (Any, _) | (_, Any) => true,
            (a, b) if a == b => !(a == Bool && !matches!(op, BinOp::Eq | BinOp::Ne)),
            (a, b) => a.is_temporal() && b.is_temporal(),
        };
        if ok {
            return Ok(Bool);
        }
        let hint = match (l.ty, r.ty) {
            (Text, Num) | (Num, Text) => {
                " Use VALUE() to read text as a number, or put the number in quotes to compare it as text.".to_string()
            }
            (Bool, Bool) => " True and false can only be compared with = and <>.".to_string(),
            (Text, d) | (d, Text) if d.is_temporal() => {
                // A date written in quotes is text. Show how to write it as a date.
                let text = if l.ty == Text { l } else { r };
                match &text.node {
                    TNode::Str(s) => match chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d") {
                        Ok(d) => format!(
                            " Use DATE({}, {}, {}).",
                            chrono::Datelike::year(&d),
                            chrono::Datelike::month(&d),
                            chrono::Datelike::day(&d)
                        ),
                        Err(_) => " Use DATE(2026, 1, 1) to write a date.".to_string(),
                    },
                    _ => " Set the column to the Date type in Data, or write a date as DATE(2026, 1, 1)."
                        .to_string(),
                }
            }
            _ => String::new(),
        };
        Err(FormulaError::new(
            format!(
                "{} is {}, but {} is {}. {} cannot compare them.{hint}",
                self.src.of(l),
                l.ty.words(),
                self.src.of(r),
                r.ty.words(),
                op.symbol()
            ),
            l.at,
            r.at + r.len - l.at,
        ))
    }

    // ---- calls -------------------------------------------------------------------------

    fn call(&mut self, e: &Expr, name: &str, args: &[Expr]) -> Result<TExpr, FormulaError> {
        let name_len = name.chars().count();
        let upper = name.to_uppercase();
        let Some(def) = functions::lookup(&upper) else {
            let names = functions::DEFS.iter().map(|d| d.name);
            return Err(FormulaError::new(
                match closest(&upper, names) {
                    Some(s) => format!("No function {upper}. Did you mean {s}?"),
                    None => format!(
                        "No function {upper}. The list of functions is in the formulas guide."
                    ),
                },
                e.at,
                name_len,
            ));
        };
        let n = args.len();
        if n < def.min || n > def.max {
            let (at, len) = if n > def.max {
                // From the first value that is too many to the last.
                let (first, last) = (&args[def.max], &args[n - 1]);
                (first.at, last.at + last.len - first.at)
            } else {
                (e.at, e.len)
            };
            return Err(FormulaError::new(
                format!(
                    "{} needs {}, but this has {}.",
                    def.name,
                    functions::arity_words(def),
                    functions::count_words(n)
                ),
                at,
                len,
            ));
        }
        if functions::is_aggregate(def, n) {
            return self.aggregate(e, def, args, name_len);
        }
        let targs = args
            .iter()
            .map(|a| self.expr(a))
            .collect::<Result<Vec<_>, _>>()?;
        let ty = functions::type_of(def, &targs, &self.src)?;
        Ok(TExpr {
            node: TNode::Call {
                name: def.name,
                args: targs,
            },
            ty,
            at: e.at,
            len: e.len,
        })
    }

    fn aggregate(
        &mut self,
        e: &Expr,
        def: &'static Def,
        args: &[Expr],
        name_len: usize,
    ) -> Result<TExpr, FormulaError> {
        if self.kind == Kind::Column {
            return Err(FormulaError::new(
                format!(
                    "{} works over many rows, so it belongs in a measure, not in a calculated column. To read one value from a related table, use [Table].[Column].",
                    def.name
                ),
                e.at,
                name_len,
            ));
        }
        if let Some(outer) = &self.agg {
            return Err(FormulaError::new(
                format!("{} cannot sit inside {}.", def.name, outer.name),
                e.at,
                name_len,
            ));
        }
        // COUNTROWS names a table, not a column.
        if def.name == "COUNTROWS" {
            return self.count_rows(e, &args[0]);
        }
        self.agg = Some(AggScope {
            name: def.name,
            tables: Vec::new(),
        });
        let targs = args
            .iter()
            .map(|a| self.expr(a))
            .collect::<Result<Vec<_>, _>>();
        let scope = self.agg.take().expect("set above");
        let targs = targs?;
        let home = self.home_of(def, &scope.tables, e, name_len)?;
        let ty = functions::type_of(def, &targs, &self.src)?;
        Ok(TExpr {
            node: TNode::Agg {
                name: def.name,
                args: targs,
                home,
            },
            ty,
            at: e.at,
            len: e.len,
        })
    }

    fn count_rows(&mut self, e: &Expr, arg: &Expr) -> Result<TExpr, FormulaError> {
        let ExprKind::Ref {
            table: None,
            column,
        } = &arg.kind
        else {
            return Err(FormulaError::new(
                "COUNTROWS needs a table name in brackets, like COUNTROWS([Orders]).",
                arg.at,
                arg.len,
            ));
        };
        let t = self.table_named(column, arg)?;
        if self.kind == Kind::Group
            && let Some(cur) = self.current
            && cur.id != t.id
        {
            return Err(FormulaError::new(
                format!(
                    "A group value counts the rows of {}. {} is another table.",
                    cur.label, t.label
                ),
                arg.at,
                arg.len,
            ));
        }
        self.note_use(&t.id);
        Ok(TExpr {
            node: TNode::Agg {
                name: "COUNTROWS",
                args: Vec::new(),
                home: t.id.clone(),
            },
            ty: Ty::Num,
            at: e.at,
            len: e.len,
        })
    }

    /// The table whose rows an aggregate reads: the one from which all the others are reached
    /// on the many-to-one path.
    fn home_of(
        &self,
        def: &Def,
        tables: &[String],
        e: &Expr,
        name_len: usize,
    ) -> Result<String, FormulaError> {
        let label = |id: &str| {
            self.cx
                .catalog
                .table(id)
                .map_or_else(|| id.to_string(), |t| t.label.clone())
        };
        if tables.is_empty() {
            return Err(FormulaError::new(
                format!(
                    "{} needs a column to work on, like {}([amount]).",
                    def.name, def.name
                ),
                e.at,
                e.len.max(name_len),
            ));
        }
        if self.kind == Kind::Group {
            // The checks at each reference already made sure the table reaches them all.
            return Ok(self.current.expect("a group has a table").id.clone());
        }
        let home = tables.iter().find(|h| {
            let paths = reach(h, self.cx.rels);
            tables.iter().all(|t| paths.contains_key(t))
        });
        match home {
            Some(h) => Ok(h.clone()),
            None => {
                let names: Vec<String> = tables.iter().map(|t| label(t)).collect();
                Err(FormulaError::new(
                    format!(
                        "{} reads {}, which are not related one to many. Total each table on its own, like SUM([A].[x]) / SUM([B].[y]), or draw a line between them in Model.",
                        def.name,
                        names.join(" and ")
                    ),
                    e.at,
                    e.len,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(a: &str, b: &str) -> usize {
        distance(
            &a.chars().collect::<Vec<_>>(),
            &b.chars().collect::<Vec<_>>(),
        )
    }

    #[test]
    fn edit_distance() {
        assert_eq!(d("amout", "amount"), 1);
        assert_eq!(d("kitten", "sitting"), 3);
        assert_eq!(d("amuont", "amount"), 1);
        assert_eq!(d("", "abc"), 3);
    }

    #[test]
    fn suggestions_need_to_be_close() {
        let names = ["amount", "region", "date"];
        assert_eq!(closest("amout", names.into_iter()), Some("amount"));
        assert_eq!(closest("AMOUNT", names.into_iter()), Some("amount"));
        assert_eq!(closest("banana", names.into_iter()), None);
    }

    #[test]
    fn types_meet() {
        assert_eq!(unify(Ty::Num, Ty::Any), Some(Ty::Num));
        assert_eq!(unify(Ty::Date, Ty::DateTime), Some(Ty::DateTime));
        assert_eq!(unify(Ty::Text, Ty::Num), None);
    }
}
