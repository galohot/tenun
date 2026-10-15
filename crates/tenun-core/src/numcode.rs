//! Format codes (formats §2.1): the part of Excel's number format language a format's `code` may use, as the app's
//! `ui/src/lib/numcode.ts` reads it.
//!
//! Up to four sections split by `;`: positive, negative, zero, text (ignored). One section writes a negative with
//! `−`; the negative section of two or three is written without a sign. Conditions such as `[>=1000]` pick the first
//! section that holds, the last one is the else, and a negative gets `−`. `[Red]` marks the value bad, `[Green]`
//! good. Placeholders `0 # ?`, `.`, `,` between digits groups them, `,` after the last digit divides by 1,000, `%`
//! multiplies by 100, `E+00` writes a power of ten. `"text"`, `\c`, `[$€-407]` are written as they are; `_x` is a
//! space; `*x` and `@` are ignored; `General` is the default format. The marks follow the digit style. A code that
//! does not read gives `None`, so the default format is used.

use crate::numtext::{self, Marks, NumFormat, Tone};

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Lit(String),
    /// `0`, `#` or `?`.
    Digit(char),
    Dot,
    /// A `,` not yet sorted into grouping, scaling or text.
    Comma,
    Group,
    Scale,
    Percent,
    /// `E+00`: the letter as written, whether `+` is written, and the least number of digits.
    Exp(char, bool, usize),
    General,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, Default)]
struct Section {
    toks: Vec<Tok>,
    cond: Option<(Op, f64)>,
    tone: Option<Tone>,
}

/// A format code that reads.
#[derive(Debug, Clone)]
pub struct Code {
    sections: Vec<Section>,
}

/// What a code writes, and whether it wrote the value as a negative with `−`.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    pub text: String,
    pub negative: bool,
}

/// The sections of a code, split at `;` outside quotes and brackets. `None` when a quote or bracket is left open, an
/// escape has nothing after it, or there are more than four sections.
fn split(code: &str) -> Option<Vec<String>> {
    let mut out = vec![String::new()];
    let mut chars = code.chars();
    while let Some(c) = chars.next() {
        let cur = out.last_mut()?;
        match c {
            '"' => {
                cur.push(c);
                loop {
                    let d = chars.next()?;
                    cur.push(d);
                    if d == '"' {
                        break;
                    }
                }
            }
            '[' => {
                cur.push(c);
                loop {
                    let d = chars.next()?;
                    cur.push(d);
                    if d == ']' {
                        break;
                    }
                }
            }
            '\\' | '_' | '*' => {
                cur.push(c);
                cur.push(chars.next()?);
            }
            ';' => out.push(String::new()),
            _ => cur.push(c),
        }
    }
    (out.len() <= 4).then_some(out)
}

/// What is in a pair of brackets: a condition, a colour or a currency symbol.
fn bracket(inner: &str, sec: &mut Section, toks: &mut Vec<Tok>) -> Option<()> {
    if let Some(sym) = inner.strip_prefix('$') {
        let sym = sym.split('-').next().unwrap_or("");
        toks.push(Tok::Lit(sym.to_string()));
        return Some(());
    }
    if inner.starts_with(['<', '>', '=']) {
        if sec.cond.is_some() {
            return None;
        }
        let (op, rest) = [
            ("<=", Op::Le),
            (">=", Op::Ge),
            ("<>", Op::Ne),
            ("<", Op::Lt),
            (">", Op::Gt),
            ("=", Op::Eq),
        ]
        .iter()
        .find_map(|(s, op)| inner.strip_prefix(s).map(|r| (*op, r)))?;
        sec.cond = Some((op, rest.trim().parse().ok()?));
        return Some(());
    }
    let name = inner.to_ascii_lowercase();
    match name.as_str() {
        "red" => sec.tone = Some(Tone::Bad),
        "green" => sec.tone = Some(Tone::Good),
        "black" | "blue" | "cyan" | "magenta" | "white" | "yellow" => {}
        _ if name
            .strip_prefix("color")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) => {}
        _ => return None,
    }
    Some(())
}

fn section(s: &str) -> Option<Section> {
    let cs: Vec<char> = s.chars().collect();
    let mut sec = Section::default();
    let mut toks = Vec::new();
    let mut dot = false;
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        i += 1;
        match c {
            '"' => {
                let end = cs[i..].iter().position(|d| *d == '"')? + i;
                toks.push(Tok::Lit(cs[i..end].iter().collect()));
                i = end + 1;
            }
            '\\' => {
                toks.push(Tok::Lit(cs.get(i)?.to_string()));
                i += 1;
            }
            '_' => {
                cs.get(i)?;
                toks.push(Tok::Lit(" ".into()));
                i += 1;
            }
            '*' => {
                cs.get(i)?;
                i += 1;
            }
            '[' => {
                let end = cs[i..].iter().position(|d| *d == ']')? + i;
                let inner: String = cs[i..end].iter().collect();
                bracket(&inner, &mut sec, &mut toks)?;
                i = end + 1;
            }
            '0' | '#' | '?' => toks.push(Tok::Digit(c)),
            '.' if !dot => {
                dot = true;
                toks.push(Tok::Dot);
            }
            ',' => toks.push(Tok::Comma),
            '%' => toks.push(Tok::Percent),
            '@' => {}
            'E' | 'e'
                if matches!(cs.get(i), Some('+' | '-'))
                    && toks.iter().any(|t| matches!(t, Tok::Digit(_))) =>
            {
                let plus = cs[i] == '+';
                i += 1;
                let width = cs[i..]
                    .iter()
                    .take_while(|d| matches!(d, '0' | '#'))
                    .count();
                if width == 0 {
                    return None;
                }
                i += width;
                toks.push(Tok::Exp(c, plus, width));
            }
            'G' | 'g'
                if cs[i - 1..]
                    .iter()
                    .take(7)
                    .collect::<String>()
                    .eq_ignore_ascii_case("general") =>
            {
                toks.push(Tok::General);
                i += 6;
            }
            _ => toks.push(Tok::Lit(c.to_string())),
        }
    }
    // A comma between digits before the decimal point groups them; one after the last digit divides by 1,000.
    let n = toks.len();
    for k in 0..n {
        if toks[k] != Tok::Comma {
            continue;
        }
        let before = toks[..k]
            .iter()
            .rev()
            .find(|t| !matches!(t, Tok::Comma | Tok::Scale | Tok::Group));
        let after = toks[k + 1..].iter().find(|t| **t != Tok::Comma);
        let in_whole = !toks[..k].contains(&Tok::Dot);
        let digit_before = toks[..k].iter().any(|t| matches!(t, Tok::Digit(_)));
        toks[k] = if in_whole && digit_before && matches!(after, Some(Tok::Digit(_))) {
            Tok::Group
        } else if matches!(before, Some(Tok::Digit(_) | Tok::Dot)) && digit_before {
            Tok::Scale
        } else {
            Tok::Lit(",".into())
        };
    }
    sec.toks = toks;
    Some(sec)
}

fn holds((op, x): (Op, f64), n: f64) -> bool {
    match op {
        Op::Lt => n < x,
        Op::Le => n <= x,
        Op::Gt => n > x,
        Op::Ge => n >= x,
        Op::Eq => n == x,
        Op::Ne => n != x,
    }
}

/// The digits of whole placeholders, filled from the right. The first placeholder takes any digits left over.
fn fill(phs: &[char], digits: &str) -> Vec<String> {
    let d: Vec<char> = digits.chars().collect();
    let k = phs.len();
    let mut out = vec![String::new(); k];
    for (j, ph) in phs.iter().enumerate() {
        let from_right = k - 1 - j;
        if from_right < d.len() {
            out[j].push(d[d.len() - 1 - from_right]);
        } else if *ph == '0' {
            out[j].push('0');
        } else if *ph == '?' {
            out[j].push('\u{2007}');
        }
    }
    if d.len() > k && k > 0 {
        let extra: String = d[..d.len() - k].iter().collect();
        out[0] = format!("{extra}{}", out[0]);
    }
    out
}

/// The decimals of fraction placeholders: trailing zeros go for `#`, become a figure space for `?`.
fn fraction(phs: &[char], digits: &str) -> Vec<String> {
    let d: Vec<char> = digits.chars().collect();
    let mut out = vec![String::new(); phs.len()];
    let mut trailing = true;
    for j in (0..phs.len()).rev() {
        let c = d.get(j).copied().unwrap_or('0');
        if trailing && c == '0' && phs[j] != '0' {
            if phs[j] == '?' {
                out[j].push('\u{2007}');
            }
        } else {
            trailing = false;
            out[j].push(c);
        }
    }
    out
}

/// Writes `v` (not negative) with a section: the text, and whether any digit written is not zero.
fn render(sec: &Section, v: f64, m: Marks) -> (String, bool) {
    let toks = &sec.toks;
    if toks.contains(&Tok::General) {
        let body = numtext::full_body(v, &NumFormat::default(), m);
        let nonzero = body.chars().any(|c| c.is_ascii_digit() && c != '0');
        let mut out = String::new();
        for t in toks {
            match t {
                Tok::Lit(s) => out.push_str(s),
                Tok::General => out.push_str(&body),
                Tok::Percent => out.push('%'),
                _ => {}
            }
        }
        return (out, nonzero);
    }
    let count = |t: &Tok| toks.iter().filter(|x| *x == t).count() as i32;
    let v = v * 100f64.powi(count(&Tok::Percent)) / 1000f64.powi(count(&Tok::Scale));
    let exp_at = toks.iter().position(|t| matches!(t, Tok::Exp(..)));
    let dot_at = toks.iter().position(|t| *t == Tok::Dot);
    let end = exp_at.unwrap_or(toks.len());
    let whole_end = dot_at.unwrap_or(end).min(end);
    let phs = |a: usize, b: usize| -> Vec<char> {
        toks[a.min(b)..b]
            .iter()
            .filter_map(|t| match t {
                Tok::Digit(c) => Some(*c),
                _ => None,
            })
            .collect()
    };
    let whole_phs = phs(0, whole_end);
    let frac_phs = dot_at.map(|d| phs(d + 1, end)).unwrap_or_default();
    if whole_phs.is_empty() && frac_phs.is_empty() {
        let text = toks
            .iter()
            .map(|t| match t {
                Tok::Lit(s) => s.as_str(),
                Tok::Percent => "%",
                _ => "",
            })
            .collect();
        return (text, false);
    }
    let (whole, frac, power) = if exp_at.is_some() {
        let (digits, e) = numtext::exponential(v, 1 + frac_phs.len());
        (digits[..1].to_string(), digits[1..].to_string(), e)
    } else {
        let (w, f) = numtext::fixed(v, frac_phs.len());
        (w, f, 0)
    };
    let nonzero = whole.chars().chain(frac.chars()).any(|c| c != '0');
    let whole = if whole == "0" { String::new() } else { whole };
    let whole_out = if toks.contains(&Tok::Group) {
        // Grouped: the digits with their marks go in the first placeholder.
        let min = whole_phs
            .iter()
            .position(|c| *c != '#')
            .map_or(0, |i| whole_phs.len() - i);
        let padded = format!("{}{whole}", "0".repeat(min.saturating_sub(whole.len())));
        let mut out = vec![String::new(); whole_phs.len()];
        if let Some(first) = out.first_mut() {
            *first = numtext::group(&padded, m);
        }
        out
    } else {
        fill(&whole_phs, &whole)
    };
    let frac_out = fraction(&frac_phs, &frac);
    let mut text = String::new();
    let (mut wi, mut fi) = (0, 0);
    let mut past_dot = false;
    for t in toks {
        match t {
            Tok::Lit(s) => text.push_str(s),
            Tok::Digit(_) if !past_dot => {
                text.push_str(whole_out.get(wi).map_or("", String::as_str));
                wi += 1;
            }
            Tok::Digit(_) => {
                text.push_str(frac_out.get(fi).map_or("", String::as_str));
                fi += 1;
            }
            Tok::Dot => {
                // A code with no whole placeholders still writes the whole digits.
                if whole_phs.is_empty() {
                    text.push_str(&whole);
                }
                text.push(m.dec);
                past_dot = true;
            }
            Tok::Percent => text.push('%'),
            Tok::Exp(e, plus, width) => {
                past_dot = true;
                let sign = if power < 0 {
                    "-"
                } else if *plus {
                    "+"
                } else {
                    ""
                };
                text.push_str(&format!(
                    "{e}{sign}{:0>width$}",
                    power.unsigned_abs(),
                    width = *width
                ));
            }
            Tok::Comma | Tok::Group | Tok::Scale | Tok::General => {}
        }
    }
    (text, nonzero)
}

impl Code {
    /// Reads a code. `None` when it is empty or does not read.
    pub fn parse(code: &str) -> Option<Code> {
        if code.trim().is_empty() {
            return None;
        }
        let sections = split(code)?
            .iter()
            .map(|s| section(s))
            .collect::<Option<Vec<_>>>()?;
        Some(Code { sections })
    }

    /// The section a value is written with, and whether a negative is written with `−`.
    fn pick(&self, n: f64) -> (&Section, bool) {
        let secs = &self.sections[..self.sections.len().min(3)];
        if secs.iter().any(|s| s.cond.is_some()) {
            for s in secs {
                match s.cond {
                    None => return (s, true),
                    Some(c) if holds(c, n) => return (s, true),
                    _ => {}
                }
            }
            return (&secs[secs.len() - 1], true);
        }
        match secs.len() {
            1 => (&secs[0], true),
            2 if n < 0.0 => (&secs[1], false),
            2 => (&secs[0], true),
            _ if n > 0.0 => (&secs[0], true),
            _ if n < 0.0 => (&secs[1], false),
            _ => (&secs[2], true),
        }
    }

    /// Writes a value. A negative that reads as zero has no sign.
    pub fn write(&self, n: f64, m: Marks) -> Written {
        let (sec, signed) = self.pick(n);
        let (body, nonzero) = render(sec, n.abs(), m);
        let negative = signed && n < 0.0 && nonzero;
        Written {
            text: if negative { format!("−{body}") } else { body },
            negative,
        }
    }

    /// The colour of the section a value is written with.
    pub fn tone(&self, n: f64) -> Option<Tone> {
        self.pick(n).0.tone
    }
}

/// Whether a code reads; the app's editor says why when it does not.
pub fn reads(code: &str) -> bool {
    Code::parse(code).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Locale;

    fn w(code: &str, n: f64) -> String {
        Code::parse(code)
            .unwrap()
            .write(n, numtext::marks(Locale::En, None))
            .text
    }

    #[test]
    fn codes_beyond_the_shared_rows() {
        assert_eq!(w("#,##0,", 1_234_567.0), "1,235");
        assert_eq!(w("0.0?", 1.5), "1.5\u{2007}");
        assert_eq!(w("General;(General)", -1234.5), "(1,234.5)");
        assert_eq!(w(".00", 1.5), "1.50");
        assert_eq!(w("0;[Red]-0", -0.2), "-0");
        assert_eq!(w("[<0]\"neg\";0", -3.0), "neg");
        for bad in ["\"open", "[Blink]0", "0;0;0;0;0", "0E+", "\\", "[>x]0"] {
            assert!(!reads(bad), "{bad}");
        }
    }
}
