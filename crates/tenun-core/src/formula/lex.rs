//! The formula lexer: text in, tokens out.
//!
//! Every token keeps where it sits as a character offset (a character, not a byte), so the
//! editor can underline it. Something that cannot be read becomes a `Bad` token with a plain
//! message, and lexing stops there. The parser reports it when it reaches it, so the first
//! problem from the left is the one the user sees.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(f64),
    Str(String),
    /// A bare word: `TRUE`, `FALSE` or the name of a function.
    Name(String),
    /// `[Column]` or `[Table].[Column]`.
    Ref {
        table: Option<String>,
        column: String,
    },
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Amp,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    LParen,
    RParen,
    Comma,
    Semi,
    /// Text that cannot be read. The string says what is wrong.
    Bad(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    /// Character offset of the first character.
    pub at: usize,
    pub len: usize,
}

pub fn lex(src: &str) -> Vec<Token> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let tok = if c.is_ascii_digit() || (c == '.' && next_is_digit(&chars, i + 1)) {
            number(&chars, &mut i)
        } else if c == '"' {
            string(&chars, &mut i)
        } else if c == '[' {
            reference(&chars, &mut i)
        } else if c.is_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            Tok::Name(chars[start..i].iter().collect())
        } else {
            i += 1;
            let two = chars.get(i).copied();
            match c {
                '+' => Tok::Plus,
                '-' => Tok::Minus,
                '*' => Tok::Star,
                '/' => Tok::Slash,
                '^' => Tok::Caret,
                '&' => Tok::Amp,
                '(' => Tok::LParen,
                ')' => Tok::RParen,
                ',' => Tok::Comma,
                ';' => Tok::Semi,
                '=' if two == Some('=') => {
                    i += 1;
                    Tok::Bad("Use a single = to compare two values.".into())
                }
                '=' => Tok::Eq,
                '<' if two == Some('>') => {
                    i += 1;
                    Tok::Ne
                }
                '<' if two == Some('=') => {
                    i += 1;
                    Tok::Le
                }
                '<' => Tok::Lt,
                '>' if two == Some('=') => {
                    i += 1;
                    Tok::Ge
                }
                '>' => Tok::Gt,
                '!' if two == Some('=') => {
                    i += 1;
                    Tok::Bad("Use <> to test that two values differ.".into())
                }
                '\'' | '‘' | '’' => {
                    Tok::Bad("Put text in double quotes (\"like this\"), not single quotes.".into())
                }
                '“' | '”' => Tok::Bad(
                    "Use straight double quotes (\") around text, not curly quotes.".into(),
                ),
                '%' => Tok::Bad("There is no % sign in formulas. Write 25% as 0.25.".into()),
                other => Tok::Bad(format!("Tenun does not understand the character {other}.")),
            }
        };
        let bad = matches!(tok, Tok::Bad(_));
        out.push(Token {
            tok,
            at: start,
            len: i - start,
        });
        if bad {
            break;
        }
    }
    out
}

fn next_is_digit(chars: &[char], i: usize) -> bool {
    chars.get(i).is_some_and(char::is_ascii_digit)
}

/// `12`, `1.5`, `.5`, `1e3`, `2.5E-3`. The `e` only counts when digits follow it.
fn number(chars: &[char], i: &mut usize) -> Tok {
    let start = *i;
    while chars.get(*i).is_some_and(char::is_ascii_digit) {
        *i += 1;
    }
    if chars.get(*i) == Some(&'.') {
        *i += 1;
        while chars.get(*i).is_some_and(char::is_ascii_digit) {
            *i += 1;
        }
    }
    if matches!(chars.get(*i), Some('e' | 'E')) {
        let mut j = *i + 1;
        if matches!(chars.get(j), Some('+' | '-')) {
            j += 1;
        }
        if next_is_digit(chars, j) {
            while chars.get(j).is_some_and(char::is_ascii_digit) {
                j += 1;
            }
            *i = j;
        }
    }
    let text: String = chars[start..*i].iter().collect();
    match text.parse::<f64>() {
        Ok(n) if n.is_finite() => Tok::Num(n),
        _ => Tok::Bad("This number is too large.".into()),
    }
}

/// `"text"`, with `""` for a quote inside the text.
fn string(chars: &[char], i: &mut usize) -> Tok {
    *i += 1;
    let mut s = String::new();
    loop {
        match chars.get(*i) {
            None => {
                // The whole rest of the formula is the culprit.
                *i = chars.len();
                return Tok::Bad("This text has no closing double quote.".into());
            }
            Some('"') if chars.get(*i + 1) == Some(&'"') => {
                s.push('"');
                *i += 2;
            }
            Some('"') => {
                *i += 1;
                break;
            }
            Some('\0') => {
                *i += 1;
                return Tok::Bad("Text cannot hold a null character.".into());
            }
            Some(&c) => {
                s.push(c);
                *i += 1;
            }
        }
    }
    Tok::Str(s)
}

/// `[Column]` or `[Table].[Column]`. A `]` inside a name is written `]]`.
fn reference(chars: &[char], i: &mut usize) -> Tok {
    let first = match bracket(chars, i) {
        Ok(s) => s,
        Err(m) => return Tok::Bad(m),
    };
    if chars.get(*i) == Some(&'.') && chars.get(*i + 1) == Some(&'[') {
        *i += 1;
        return match bracket(chars, i) {
            Ok(second) => Tok::Ref {
                table: Some(first),
                column: second,
            },
            Err(m) => Tok::Bad(m),
        };
    }
    Tok::Ref {
        table: None,
        column: first,
    }
}

/// Reads one `[name]` starting at `[`. Leaves `i` after the closing `]`.
fn bracket(chars: &[char], i: &mut usize) -> Result<String, String> {
    *i += 1;
    let mut s = String::new();
    loop {
        match chars.get(*i) {
            None => {
                *i = chars.len();
                return Err("This name has no closing ].".into());
            }
            Some(']') if chars.get(*i + 1) == Some(&']') => {
                s.push(']');
                *i += 2;
            }
            Some(']') => {
                *i += 1;
                break;
            }
            Some(&c) => {
                s.push(c);
                *i += 1;
            }
        }
    }
    if s.trim().is_empty() {
        return Err("This name is empty. Write the column name between the brackets.".into());
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        lex(s).into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn numbers_in_every_written_form() {
        assert_eq!(toks("12"), [Tok::Num(12.0)]);
        assert_eq!(toks("1.5"), [Tok::Num(1.5)]);
        assert_eq!(toks(".5"), [Tok::Num(0.5)]);
        assert_eq!(toks("1e3"), [Tok::Num(1000.0)]);
        assert_eq!(toks("2.5E-3"), [Tok::Num(0.0025)]);
        // An e with no digits after it is not an exponent.
        assert_eq!(toks("2e"), [Tok::Num(2.0), Tok::Name("e".into())]);
    }

    #[test]
    fn a_number_too_large_is_refused() {
        assert_eq!(
            toks("1e999"),
            [Tok::Bad("This number is too large.".into())]
        );
    }

    #[test]
    fn strings_with_an_escaped_quote() {
        assert_eq!(
            toks(r#""say ""hi"" now""#),
            [Tok::Str("say \"hi\" now".into())]
        );
        assert_eq!(toks(r#""""#), [Tok::Str(String::new())]);
    }

    #[test]
    fn an_open_string_runs_to_the_end() {
        let t = lex(r#"1 + "abc"#);
        let bad = t.last().unwrap();
        assert!(matches!(bad.tok, Tok::Bad(_)));
        assert_eq!((bad.at, bad.len), (4, 4));
    }

    #[test]
    fn references_and_brackets_in_names() {
        assert_eq!(
            toks("[amount]"),
            [Tok::Ref {
                table: None,
                column: "amount".into()
            }]
        );
        assert_eq!(
            toks("[Orders].[net amount]"),
            [Tok::Ref {
                table: Some("Orders".into()),
                column: "net amount".into()
            }]
        );
        assert_eq!(
            toks("[a]]b]"),
            [Tok::Ref {
                table: None,
                column: "a]b".into()
            }]
        );
    }

    #[test]
    fn offsets_count_characters_not_bytes() {
        let t = lex("[kota é] + 1");
        assert_eq!((t[0].at, t[0].len), (0, 8));
        assert_eq!((t[1].at, t[1].len), (9, 1));
        assert_eq!((t[2].at, t[2].len), (11, 1));
    }

    #[test]
    fn every_operator() {
        use Tok::*;
        assert_eq!(
            toks("+ - * / ^ & = <> < > <= >= ( ) , ;"),
            [
                Plus, Minus, Star, Slash, Caret, Amp, Eq, Ne, Lt, Gt, Le, Ge, LParen, RParen,
                Comma, Semi
            ]
        );
    }

    #[test]
    fn habits_from_other_languages_get_a_hint() {
        let msg = |s: &str| match lex(s).pop().unwrap().tok {
            Tok::Bad(m) => m,
            other => panic!("{other:?}"),
        };
        assert_eq!(msg("1 != 2"), "Use <> to test that two values differ.");
        assert_eq!(msg("1 == 2"), "Use a single = to compare two values.");
        assert!(msg("'a'").contains("double quotes"));
        assert!(msg("“a”").contains("straight double quotes"));
        assert!(msg("5%").contains("0.25"));
        assert_eq!(msg("1 @ 2"), "Tenun does not understand the character @.");
    }

    #[test]
    fn an_empty_name_is_refused() {
        assert!(matches!(lex("[ ]")[0].tok, Tok::Bad(_)));
        assert!(matches!(lex("[open")[0].tok, Tok::Bad(_)));
    }
}
