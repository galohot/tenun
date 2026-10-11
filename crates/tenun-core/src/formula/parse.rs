//! The formula parser: tokens in, a syntax tree out. A Pratt parser, written by hand.
//!
//! From the loosest to the tightest: comparison, `&`, `+ -`, `* /`, `^` (right to left),
//! unary `-` and `+`, then calls, references and literals. As in Excel, `-2^2` is 4: the
//! minus binds tighter than the power.

use super::FormulaError;
use super::lex::{Tok, Token, lex};

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    /// Character offset of the first character, and how many characters it covers.
    pub at: usize,
    pub len: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Num(f64),
    Str(String),
    Bool(bool),
    Ref {
        table: Option<String>,
        column: String,
    },
    /// `name` is written as the user typed it. Names are matched without regard to case.
    Call {
        name: String,
        args: Vec<Expr>,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Pow => "^",
            BinOp::Concat => "&",
            BinOp::Eq => "=",
            BinOp::Ne => "<>",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
        }
    }

    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
        )
    }
}

/// Binding powers: how hard each side of an operator pulls on its neighbour.
const UNARY: u8 = 11;

fn infix(tok: &Tok) -> Option<(BinOp, u8, u8)> {
    Some(match tok {
        Tok::Eq => (BinOp::Eq, 1, 2),
        Tok::Ne => (BinOp::Ne, 1, 2),
        Tok::Lt => (BinOp::Lt, 1, 2),
        Tok::Gt => (BinOp::Gt, 1, 2),
        Tok::Le => (BinOp::Le, 1, 2),
        Tok::Ge => (BinOp::Ge, 1, 2),
        Tok::Amp => (BinOp::Concat, 3, 4),
        Tok::Plus => (BinOp::Add, 5, 6),
        Tok::Minus => (BinOp::Sub, 5, 6),
        Tok::Star => (BinOp::Mul, 7, 8),
        Tok::Slash => (BinOp::Div, 7, 8),
        // Right to left: 2^3^2 is 2^(3^2).
        Tok::Caret => (BinOp::Pow, 10, 9),
        _ => return None,
    })
}

/// The formula as a syntax tree. An empty formula is an error.
pub fn parse(src: &str) -> Result<Expr, FormulaError> {
    let end = src.chars().count();
    if src.trim().is_empty() {
        return Err(FormulaError::new(
            "Write a formula, like [amount] * 2.",
            0,
            0,
        ));
    }
    let mut p = Parser {
        toks: lex(src),
        pos: 0,
        end,
    };
    let e = p.expr(0)?;
    match p.toks.get(p.pos) {
        None => Ok(e),
        Some(t) => Err(p.stray(t)),
    }
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    /// The length of the formula, where "the end" sits.
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.pos)
    }

    fn expr(&mut self, min_bp: u8) -> Result<Expr, FormulaError> {
        let mut left = self.prefix()?;
        while let Some(t) = self.peek() {
            let Some((op, lbp, rbp)) = infix(&t.tok) else {
                break;
            };
            if lbp < min_bp {
                break;
            }
            self.pos += 1;
            let right = self.expr(rbp)?;
            let (at, len) = (left.at, right.at + right.len - left.at);
            left = Expr {
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                at,
                len,
            };
        }
        Ok(left)
    }

    fn prefix(&mut self) -> Result<Expr, FormulaError> {
        let Some(t) = self.toks.get(self.pos).cloned() else {
            return Err(FormulaError::new(
                "The formula stops too early. A value is missing at the end.",
                self.end,
                0,
            ));
        };
        self.pos += 1;
        let leaf = |kind| {
            Ok(Expr {
                kind,
                at: t.at,
                len: t.len,
            })
        };
        match &t.tok {
            Tok::Num(n) => leaf(ExprKind::Num(*n)),
            Tok::Str(s) => leaf(ExprKind::Str(s.clone())),
            Tok::Ref { table, column } => leaf(ExprKind::Ref {
                table: table.clone(),
                column: column.clone(),
            }),
            Tok::Name(name) => {
                if matches!(self.peek(), Some(n) if n.tok == Tok::LParen) {
                    return self.call(name.clone(), &t);
                }
                match name.to_ascii_uppercase().as_str() {
                    "TRUE" => leaf(ExprKind::Bool(true)),
                    "FALSE" => leaf(ExprKind::Bool(false)),
                    _ => Err(FormulaError::new(
                        format!(
                            "{name} is not a column or a function. Write column names in square brackets, like [{name}]."
                        ),
                        t.at,
                        t.len,
                    )),
                }
            }
            Tok::LParen => {
                let mut inner = self.expr(0)?;
                match self.peek() {
                    Some(c) if c.tok == Tok::RParen => {
                        let close = c.at + c.len;
                        self.pos += 1;
                        // The brackets belong to the value, so an error underlines them too.
                        inner.at = t.at;
                        inner.len = close - t.at;
                        Ok(inner)
                    }
                    Some(other) if matches!(other.tok, Tok::Bad(_) | Tok::Semi) => {
                        let other = other.clone();
                        Err(self.stray(&other))
                    }
                    _ => Err(FormulaError::new("This ( has no closing ).", t.at, t.len)),
                }
            }
            Tok::Minus | Tok::Plus => {
                let operand = self.expr(UNARY)?;
                let op = if t.tok == Tok::Minus {
                    UnOp::Neg
                } else {
                    UnOp::Pos
                };
                let len = operand.at + operand.len - t.at;
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op,
                        expr: Box::new(operand),
                    },
                    at: t.at,
                    len,
                })
            }
            _ => Err(self.stray_before(&t)),
        }
    }

    /// `NAME(arg, arg, …)`. `open` is the name token; the `(` is next.
    fn call(&mut self, name: String, open: &Token) -> Result<Expr, FormulaError> {
        let lparen = self.toks[self.pos].clone();
        self.pos += 1;
        let mut args = Vec::new();
        if matches!(self.peek(), Some(t) if t.tok == Tok::RParen) {
            let close = self.toks[self.pos].clone();
            self.pos += 1;
            return Ok(call_expr(name, args, open, &close));
        }
        loop {
            if let Some(t) = self.peek()
                && t.tok == Tok::Comma
            {
                return Err(FormulaError::new(
                    "A value is missing before this comma.",
                    t.at,
                    t.len,
                ));
            }
            args.push(self.expr(0)?);
            let Some(t) = self.toks.get(self.pos).cloned() else {
                return Err(FormulaError::new(
                    format!("The ( after {name} has no closing )."),
                    lparen.at,
                    lparen.len,
                ));
            };
            self.pos += 1;
            match t.tok {
                Tok::Comma => {
                    if matches!(self.peek(), Some(n) if n.tok == Tok::RParen) {
                        return Err(FormulaError::new(
                            "A value is missing after this comma.",
                            t.at,
                            t.len,
                        ));
                    }
                }
                Tok::RParen => return Ok(call_expr(name, args, open, &t)),
                Tok::Semi | Tok::Bad(_) => return Err(self.stray(&t)),
                _ => {
                    return Err(FormulaError::new(
                        "Put a comma between the parts of a function, or close it with ).",
                        t.at,
                        t.len,
                    ));
                }
            }
        }
    }

    /// A token that cannot follow what came before it.
    fn stray(&self, t: &Token) -> FormulaError {
        let (msg, len) = match &t.tok {
            Tok::Bad(m) => (m.clone(), t.len),
            Tok::Semi => ("Use a comma between the parts of a function.".into(), t.len),
            Tok::RParen => ("This ) has no matching (.".into(), t.len),
            Tok::Comma => (
                "A comma only belongs between the parts of a function, like IF(a, b, c).".into(),
                t.len,
            ),
            _ => (
                "Two values sit side by side. Put an operator between them, like + or &.".into(),
                t.len,
            ),
        };
        FormulaError::new(msg, t.at, len)
    }

    /// A token where a value should be.
    fn stray_before(&self, t: &Token) -> FormulaError {
        match &t.tok {
            Tok::Bad(_) | Tok::Semi | Tok::RParen | Tok::Comma => self.stray(t),
            _ => {
                let sym = match infix(&t.tok) {
                    Some((op, _, _)) => op.symbol(),
                    None => "this",
                };
                FormulaError::new(format!("A value is missing before {sym}."), t.at, t.len)
            }
        }
    }
}

fn call_expr(name: String, args: Vec<Expr>, open: &Token, close: &Token) -> Expr {
    Expr {
        kind: ExprKind::Call { name, args },
        at: open.at,
        len: close.at + close.len - open.at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree as a bracketed string, so tests read like the formula.
    fn show(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Num(n) => format!("{n}"),
            ExprKind::Str(s) => format!("{s:?}"),
            ExprKind::Bool(b) => format!("{b}"),
            ExprKind::Ref { table, column } => match table {
                Some(t) => format!("[{t}].[{column}]"),
                None => format!("[{column}]"),
            },
            ExprKind::Call { name, args } => format!(
                "{}({})",
                name.to_ascii_uppercase(),
                args.iter().map(show).collect::<Vec<_>>().join(", ")
            ),
            ExprKind::Unary { op, expr } => format!(
                "({}{})",
                if *op == UnOp::Neg { "-" } else { "+" },
                show(expr)
            ),
            ExprKind::Binary { op, left, right } => {
                format!("({} {} {})", show(left), op.symbol(), show(right))
            }
        }
    }

    fn tree(s: &str) -> String {
        show(&parse(s).unwrap())
    }

    fn err(s: &str) -> (String, usize, usize) {
        let e = parse(s).unwrap_err();
        (e.message, e.at, e.len)
    }

    #[test]
    fn precedence_from_loose_to_tight() {
        assert_eq!(tree("2+3*4^2"), "(2 + (3 * (4 ^ 2)))");
        assert_eq!(tree("1 + 2 & 3"), "((1 + 2) & 3)");
        assert_eq!(tree("1 & 2 = 3"), "((1 & 2) = 3)");
    }

    #[test]
    fn power_goes_right_to_left_and_minus_binds_tighter() {
        assert_eq!(tree("2^3^2"), "(2 ^ (3 ^ 2))");
        assert_eq!(tree("-2^2"), "((-2) ^ 2)");
        assert_eq!(tree("2^-1"), "(2 ^ (-1))");
        assert_eq!(tree("1 - -1"), "(1 - (-1))");
        assert_eq!(tree("10 - 4 - 3"), "((10 - 4) - 3)");
    }

    #[test]
    fn brackets_and_calls() {
        assert_eq!(tree("(1+2)*3"), "((1 + 2) * 3)");
        assert_eq!(
            tree("if([a] > 1, \"x\", \"y\")"),
            "IF(([a] > 1), \"x\", \"y\")"
        );
        assert_eq!(tree("BLANK()"), "BLANK()");
        assert_eq!(tree("true & FALSE"), "(true & false)");
        assert_eq!(tree("[T].[c] + 1"), "([T].[c] + 1)");
    }

    #[test]
    fn spans_cover_the_whole_expression() {
        let e = parse("(1 + 2) * [x]").unwrap();
        assert_eq!((e.at, e.len), (0, 13));
        let ExprKind::Binary { left, .. } = &e.kind else {
            panic!()
        };
        // The brackets are part of the left value.
        assert_eq!((left.at, left.len), (0, 7));
        let c = parse("  SUM([a], 1)").unwrap();
        assert_eq!((c.at, c.len), (2, 11));
    }

    #[test]
    fn a_semicolon_asks_for_a_comma() {
        let (m, at, len) = err("IF([a] > 1; 2; 3)");
        assert_eq!(m, "Use a comma between the parts of a function.");
        assert_eq!((at, len), (10, 1));
    }

    #[test]
    fn errors_say_what_and_where() {
        assert_eq!(
            err(""),
            ("Write a formula, like [amount] * 2.".into(), 0, 0)
        );
        assert_eq!(
            err("1 +"),
            (
                "The formula stops too early. A value is missing at the end.".into(),
                3,
                0
            )
        );
        assert_eq!(
            err("1 + * 2"),
            ("A value is missing before *.".into(), 4, 1)
        );
        assert_eq!(err("(1 + 2"), ("This ( has no closing ).".into(), 0, 1));
        assert_eq!(err("1 + 2)"), ("This ) has no matching (.".into(), 5, 1));
        assert_eq!(
            err("SUM([a]"),
            ("The ( after SUM has no closing ).".into(), 3, 1)
        );
        assert_eq!(
            err("IF(1,,2)"),
            ("A value is missing before this comma.".into(), 5, 1)
        );
        assert_eq!(
            err("IF(1,)"),
            ("A value is missing after this comma.".into(), 4, 1)
        );
        assert_eq!(
            err("[a] [b]"),
            (
                "Two values sit side by side. Put an operator between them, like + or &.".into(),
                4,
                3
            )
        );
        assert_eq!(
            err("1, 2"),
            (
                "A comma only belongs between the parts of a function, like IF(a, b, c).".into(),
                1,
                1
            )
        );
        assert_eq!(
            err("amount * 2"),
            (
                "amount is not a column or a function. Write column names in square brackets, like [amount]."
                    .into(),
                0,
                6
            )
        );
    }

    #[test]
    fn the_first_problem_from_the_left_wins() {
        // The lexer stops at the quote, but the missing value comes first.
        assert_eq!(err("1 + * \"x").0, "A value is missing before *.");
        let (m, at, _) = err("1 + 'x'");
        assert!(m.contains("double quotes"));
        assert_eq!(at, 4);
    }
}
