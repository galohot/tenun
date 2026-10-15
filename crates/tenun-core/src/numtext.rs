//! Numbers in words, as the app writes them (`ui/src/lib/locale.ts`): the short form of a number tile ("18.4K",
//! "Rp 4,2 jt") and the full form ("18,412"). An alert's message uses them (v5 build 12), so a message says the
//! number the way the tile shows it.
//!
//! Rounding follows JavaScript's `toFixed` and `toExponential`, which d3 uses: the exact value of the number is
//! rounded, and a tie goes up (Rust's own formatting sends a tie to the even digit: 18,450 would read "18.4K").
//!
//! The shared rows in `ui/tests/fixtures/numfmt.json` say what each format writes. The app runs the same rows.

use crate::numcode;
use crate::workspace::{Digits, Locale};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

/// How a negative value is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Negative {
    /// `−1,234` (the default).
    Minus,
    /// `(1,234)`.
    Parens,
    /// `−1,234`, marked bad.
    Red,
    /// `(1,234)`, marked bad.
    ParensRed,
}

/// What a duration value counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
}

impl DurationUnit {
    /// Seconds in one unit.
    pub fn seconds(self) -> f64 {
        match self {
            DurationUnit::Seconds => 1.0,
            DurationUnit::Minutes => 60.0,
            DurationUnit::Hours => 3600.0,
            DurationUnit::Days => 86_400.0,
        }
    }
}

/// How a written value is marked: tables, pivots and number tiles colour it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Bad,
    Good,
}

/// How a value is written (the app's `NumFormat`). Every field is optional; a missing field reads as before. A field
/// with a value this version does not know is ignored, so a format never fails to read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NumFormat {
    /// 0–6 fixed decimals. Unset: the number as it is, up to 2 decimals.
    pub decimals: Option<u32>,
    /// 1–6 significant digits. Wins over `decimals`.
    pub sig: Option<u32>,
    /// Any ISO 4217 code. Its symbol and place come from the currency table.
    pub currency: Option<String>,
    /// Write the code instead of the symbol: `USD 1,234`.
    pub iso: Option<bool>,
    /// Short (1,2 jt) or full. Unset: short on number tiles, full in tables.
    pub compact: Option<bool>,
    /// 3, 6, 9 or 12: divide by 10^scale and add the unit word, in both forms.
    pub scale: Option<u32>,
    /// Multiply by 100 and add %.
    pub percent: Option<bool>,
    pub negative: Option<Negative>,
    /// Text before the number. A space follows it when it ends in a letter or a digit.
    pub prefix: Option<String>,
    /// Text after the number. A space comes before it when it starts with a letter or a digit.
    pub suffix: Option<String>,
    /// The value counts seconds, minutes, hours or days.
    pub duration: Option<DurationUnit>,
    /// Written for an exact 0.
    pub zero: Option<String>,
    /// Written for an empty cell (tables and pivots in the app).
    pub blank: Option<String>,
    /// A format code in Excel's language (see [`crate::numcode`]). When it reads, it decides the text.
    pub code: Option<String>,
}

impl<'de> Deserialize<'de> for NumFormat {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::from_value(&Value::deserialize(d)?))
    }
}

impl NumFormat {
    /// Reads a format from its JSON. A field of the wrong type or out of range is left unset.
    pub fn from_value(v: &Value) -> Self {
        let Some(o) = v.as_object() else {
            return Self::default();
        };
        let whole = |k: &str| {
            o.get(k)
                .and_then(Value::as_f64)
                .filter(|x| x.fract() == 0.0 && *x >= 0.0)
        };
        let text = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
        let flag = |k: &str| o.get(k).and_then(Value::as_bool);
        Self {
            decimals: whole("decimals").map(|x| x.min(6.0) as u32),
            sig: whole("sig")
                .filter(|x| *x >= 1.0)
                .map(|x| x.min(6.0) as u32),
            currency: text("currency"),
            iso: flag("iso"),
            compact: flag("compact"),
            scale: whole("scale")
                .map(|x| x as u32)
                .filter(|x| matches!(x, 3 | 6 | 9 | 12)),
            percent: flag("percent"),
            negative: match o.get("negative").and_then(Value::as_str) {
                Some("minus") => Some(Negative::Minus),
                Some("parens") => Some(Negative::Parens),
                Some("red") => Some(Negative::Red),
                Some("parens_red") => Some(Negative::ParensRed),
                _ => None,
            },
            prefix: text("prefix"),
            suffix: text("suffix"),
            duration: match o.get("duration").and_then(Value::as_str) {
                Some("s") => Some(DurationUnit::Seconds),
                Some("m") => Some(DurationUnit::Minutes),
                Some("h") => Some(DurationUnit::Hours),
                Some("d") => Some(DurationUnit::Days),
                _ => None,
            },
            zero: text("zero"),
            blank: text("blank"),
            code: text("code"),
        }
    }
}

/// Below this a short number keeps all its digits.
const COMPACT_FROM: f64 = 10_000.0;

/// The decimal mark, the thousands mark, and Indian grouping (3, then 2): what a digit style writes.
#[derive(Debug, Clone, Copy)]
pub struct Marks {
    pub dec: char,
    pub thou: char,
    pub indian: bool,
}

/// The marks of a digit style; unset follows the locale.
pub fn marks(loc: Locale, digits: Option<Digits>) -> Marks {
    let (dec, thou, indian) = match (digits, loc) {
        (None, Locale::En) | (Some(Digits::CommaDot), _) => ('.', ',', false),
        (None, Locale::Id) | (Some(Digits::DotComma), _) => (',', '.', false),
        (Some(Digits::SpaceComma), _) => (',', '\u{202F}', false),
        (Some(Digits::AposDot), _) => ('.', '\u{2019}', false),
        (Some(Digits::Indian), _) => ('.', ',', true),
    };
    Marks { dec, thou, indian }
}

/// The words for 10³, 10⁶, 10⁹ and 10¹² (the app's `UNITS`).
fn unit(loc: Locale, power: i32) -> &'static str {
    match (loc, power) {
        (_, 0) => "",
        (Locale::En, 3) => "K",
        (Locale::En, 6) => "M",
        (Locale::En, 9) => "B",
        (Locale::En, 12) => "T",
        (Locale::Id, 3) => " rb",
        (Locale::Id, 6) => " jt",
        (Locale::Id, 9) => " M",
        (Locale::Id, 12) => " T",
        (_, 15) => "P",
        (_, 18) => "E",
        (_, 21) => "Z",
        _ => "Y",
    }
}

/// Every ISO 4217 code and its symbol, shared with the app.
fn symbols() -> &'static HashMap<String, String> {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(include_str!("../../../ui/src/lib/currencies.json"))
            .unwrap_or_default()
    })
}

/// Where a currency goes: the text before the number and the text after it. A symbol that ends in a letter is
/// followed by a space; `iso` writes the code.
pub fn currency(code: &str, loc: Locale, iso: bool) -> (String, String) {
    if iso {
        return (format!("{code} "), String::new());
    }
    let (pre, post) = match (code, loc) {
        ("IDR", _) => ("Rp ", ""),
        ("USD", Locale::Id) => ("US$", ""),
        ("USD", Locale::En) => ("$", ""),
        ("EUR", _) => ("€", ""),
        ("SGD", _) => ("S$", ""),
        ("MZN", Locale::Id) => ("", " MT"),
        ("MZN", Locale::En) => ("MT ", ""),
        _ => {
            return match symbols().get(code) {
                Some(s) if s.chars().last().is_some_and(char::is_alphabetic) => {
                    (format!("{s} "), String::new())
                }
                Some(s) => (s.clone(), String::new()),
                None => (format!("{code} "), String::new()),
            };
        }
    };
    (pre.to_string(), post.to_string())
}

/// The exact decimal digits of `a` (not negative): the whole part and the fraction, without trailing zeros.
fn exact(a: f64) -> (String, String) {
    let s = format!("{a:.1100}");
    let (whole, frac) = s.split_once('.').unwrap_or((&s, ""));
    (whole.to_string(), frac.trim_end_matches('0').to_string())
}

/// Adds one to a string of decimal digits; "999" becomes "1000".
fn bump(digits: &str) -> String {
    let mut d: Vec<u8> = digits.bytes().collect();
    let mut i = d.len();
    loop {
        if i == 0 {
            d.insert(0, b'1');
            break;
        }
        i -= 1;
        if d[i] == b'9' {
            d[i] = b'0';
        } else {
            d[i] += 1;
            break;
        }
    }
    String::from_utf8(d).unwrap_or_default()
}

/// `a` (not negative) with `d` decimals, as `toFixed`: the whole part and the decimals.
pub(crate) fn fixed(a: f64, d: usize) -> (String, String) {
    let (whole, frac) = exact(a);
    let mut kept: String = frac.chars().chain(std::iter::repeat('0')).take(d).collect();
    let mut whole = whole;
    // The digit after the last one kept decides: 5 or more rounds up (a tie too).
    if frac.as_bytes().get(d).is_some_and(|c| *c >= b'5') {
        let joined = bump(&format!("{whole}{kept}"));
        let cut = joined.len() - d;
        kept = joined[cut..].to_string();
        whole = joined[..cut].to_string();
    }
    (whole, kept)
}

/// `a` (not negative) with `p` significant digits, as `toExponential(p - 1)`: the digits and the power of ten of the
/// first one.
pub(crate) fn exponential(a: f64, p: usize) -> (String, i32) {
    let p = p.max(1);
    let (whole, frac) = exact(a);
    let all = format!("{whole}{frac}");
    let Some(first) = all.bytes().position(|c| c != b'0') else {
        return ("0".repeat(p), 0);
    };
    let mut power = whole.len() as i32 - 1 - first as i32;
    let sig = &all[first..];
    let mut digits: String = sig.chars().chain(std::iter::repeat('0')).take(p).collect();
    if sig.as_bytes().get(p).is_some_and(|c| *c >= b'5') {
        digits = bump(&digits);
        if digits.len() > p {
            digits.truncate(p);
            power += 1;
        }
    }
    (digits, power)
}

/// Whole digits with the thousands mark of the style.
pub(crate) fn group(whole: &str, m: Marks) -> String {
    let n = whole.len();
    let mut out = String::with_capacity(n + n / 2);
    for (i, c) in whole.chars().enumerate() {
        let left = n - i;
        let mark = if m.indian {
            left == 3 || (left > 3 && (left - 3).is_multiple_of(2))
        } else {
            left.is_multiple_of(3)
        };
        if i > 0 && mark {
            out.push(m.thou);
        }
        out.push(c);
    }
    out
}

fn join(whole: &str, kept: &str, grouping: bool, m: Marks) -> String {
    let whole = if grouping {
        group(whole, m)
    } else {
        whole.to_string()
    };
    if kept.is_empty() {
        whole
    } else {
        format!("{whole}{}{kept}", m.dec)
    }
}

/// Grouped digits with `d` decimals; `trim` drops the trailing zeros of the decimals (d3's `~`).
fn grouped(a: f64, d: usize, trim: bool, m: Marks) -> String {
    let (whole, mut kept) = fixed(a, d);
    if trim {
        kept = kept.trim_end_matches('0').to_string();
    }
    join(&whole, &kept, true, m)
}

/// `p` significant digits, grouped, trailing zeros kept (d3's `,.{p}r`): 1,230 · 1.50 · 0.012.
fn rounded(a: f64, p: usize, m: Marks) -> String {
    let (digits, e) = exponential(a, p);
    if e < 0 {
        return format!("0{}{}{digits}", m.dec, "0".repeat((-e - 1) as usize));
    }
    let head = e as usize + 1;
    if digits.len() > head {
        join(&digits[..head], &digits[head..], true, m)
    } else {
        group(&format!("{digits}{}", "0".repeat(head - digits.len())), m)
    }
}

/// `p` significant digits with a unit word, trailing zeros kept (d3's `.{p}s`): 1.2M · 12.0K. For 10,000 and more.
fn prefixed(a: f64, p: usize, loc: Locale, m: Marks) -> String {
    let (digits, e) = exponential(a, p);
    let unit_power = (e.div_euclid(3) * 3).clamp(0, 24);
    let head = (e - unit_power + 1) as usize;
    let body = if digits.len() > head {
        format!("{}{}{}", &digits[..head], m.dec, &digits[head..])
    } else {
        format!("{digits}{}", "0".repeat(head - digits.len()))
    };
    format!("{body}{}", unit(loc, unit_power))
}

/// The short form with three significant digits and a unit word: 12.9K, 4.2M, 1.23B (d3's `.3~s`).
fn three_digits(a: f64, loc: Locale, m: Marks) -> String {
    let (whole, frac) = exact(a);
    // The significant digits of a number of at least 10,000 start in its whole part.
    let all = format!("{whole}{frac}");
    let mut power = whole.len() as i32 - 1;
    let mut digits = all[..3].to_string();
    if all.as_bytes().get(3).is_some_and(|c| *c >= b'5') {
        digits = bump(&digits);
        if digits.len() > 3 {
            digits.truncate(3);
            power += 1;
        }
    }
    let unit_power = (power / 3) * 3;
    let head = (power - unit_power + 1) as usize;
    let (int, rest) = digits.split_at(head);
    let rest = rest.trim_end_matches('0');
    let body = if rest.is_empty() {
        int.to_string()
    } else {
        format!("{int}{}{rest}", m.dec)
    };
    format!("{body}{}", unit(loc, unit_power))
}

/// The digits of a number tile's value without sign or currency, and whether it is below zero.
fn compact_body(n: f64, f: &NumFormat, loc: Locale, m: Marks) -> (bool, String) {
    let a = n.abs();
    if let Some(p) = f.sig {
        let body = if a < COMPACT_FROM {
            rounded(a, p as usize, m)
        } else {
            prefixed(a, p as usize, loc, m)
        };
        return (n < 0.0, body);
    }
    let decimals = f.decimals;
    if a < COMPACT_FROM {
        return match decimals {
            Some(d) => (n < 0.0, grouped(a, d as usize, false, m)),
            // Rounded as JavaScript's Math.round does: a half goes up, also below zero (−150.5 → −150).
            None if a >= 100.0 || n.fract() == 0.0 => {
                let r = (n + 0.5).floor();
                (r < 0.0, grouped(r.abs(), 0, false, m))
            }
            None => (n < 0.0, grouped(a, 2, true, m)),
        };
    }
    let Some(d) = decimals else {
        return (n < 0.0, three_digits(a, loc, m));
    };
    let steps = [(1e12, 12), (1e9, 9), (1e6, 6), (1e3, 3)];
    let Some(mut i) = steps.iter().position(|(v, _)| a >= *v) else {
        return (n < 0.0, grouped(a, d as usize, false, m));
    };
    // 999,999 with 1 decimal reads 1,000.0K: the next unit up is used instead.
    if i > 0 {
        let (whole, _) = fixed(a / steps[i].0, d as usize);
        if whole.len() >= 4 {
            i -= 1;
        }
    }
    let (v, p) = steps[i];
    (
        n < 0.0,
        format!("{}{}", grouped(a / v, d as usize, false, m), unit(loc, p)),
    )
}

/// Every digit: grouped, up to 2 decimals unless the format says.
pub(crate) fn full_body(a: f64, f: &NumFormat, m: Marks) -> String {
    if let Some(p) = f.sig {
        return rounded(a, p as usize, m);
    }
    match f.decimals {
        Some(d) => grouped(a, d as usize, false, m),
        None if a.fract() == 0.0 => grouped(a, 0, false, m),
        None => grouped(a, 2, true, m),
    }
}

/// A fixed unit: the value in thousands, millions… with 1 decimal unless the format says, and the unit word.
fn scaled_body(n: f64, power: u32, f: &NumFormat, loc: Locale, m: Marks) -> String {
    let a = n.abs() / 10f64.powi(power as i32);
    let body = match (f.sig, f.decimals) {
        (Some(p), _) => rounded(a, p as usize, m),
        (None, d) => grouped(a, d.unwrap_or(1) as usize, false, m),
    };
    format!("{body}{}", unit(loc, power as i32))
}

fn percent_body(n: f64, f: &NumFormat, m: Marks) -> String {
    let a = (n * 100.0).abs();
    let body = match (f.sig, f.decimals) {
        (Some(p), _) => rounded(a, p as usize, m),
        (None, Some(d)) => grouped(a, d as usize, false, m),
        (None, None) => grouped(a, 1, true, m),
    };
    format!("{body}%")
}

/// A duration: full `26:03:04` (or `26:03` for minutes, hours and days), short the two largest parts `1d 2h`.
fn duration_body(n: f64, u: DurationUnit, short: bool, loc: Locale) -> String {
    let secs = n.abs() * u.seconds();
    let with_secs = u == DurationUnit::Seconds;
    // Rounded to the smallest part shown, a half up.
    let total = if with_secs { secs } else { secs / 60.0 };
    let total = (total + 0.5).floor() as u64;
    let (h, mm, ss) = if with_secs {
        (total / 3600, total / 60 % 60, total % 60)
    } else {
        (total / 60, total % 60, 0)
    };
    if !short {
        return if with_secs {
            format!("{h}:{mm:02}:{ss:02}")
        } else {
            format!("{h}:{mm:02}")
        };
    }
    // d, h, m, s in English; hari, jam, menit, detik in Indonesian.
    let names = match loc {
        Locale::En => ["d", "h", "m", "s"],
        Locale::Id => ["h", "j", "m", "d"],
    };
    let parts = [h / 24, h % 24, mm, ss];
    let last = if with_secs { 3 } else { 2 };
    let Some(first) = parts[..=last].iter().position(|p| *p > 0) else {
        return format!("0{}", names[last]);
    };
    let mut out = format!("{}{}", parts[first], names[first]);
    if first < last && parts[first + 1] > 0 {
        out.push_str(&format!(" {}{}", parts[first + 1], names[first + 1]));
    }
    out
}

/// Whether written digits read as zero (then no sign is written).
fn reads_zero(body: &str) -> bool {
    !body.chars().any(|c| c.is_ascii_digit() && c != '0')
}

/// The text and whether it was written as a negative value.
fn write(
    n: f64,
    loc: Locale,
    digits: Option<Digits>,
    f: &NumFormat,
    short: bool,
) -> (String, bool) {
    if !n.is_finite() {
        return ("–".into(), false);
    }
    let m = marks(loc, digits);
    if let Some(code) = f.code.as_deref().and_then(numcode::Code::parse) {
        let out = code.write(n, m);
        return (out.text, out.negative);
    }
    if n == 0.0
        && let Some(z) = &f.zero
    {
        return (z.clone(), false);
    }
    let mut money = true;
    let (negative, body) = if let Some(u) = f.duration {
        money = false;
        (n < 0.0, duration_body(n, u, short, loc))
    } else if f.percent == Some(true) {
        money = false;
        (n < 0.0, percent_body(n, f, m))
    } else if let Some(p) = f.scale {
        (n < 0.0, scaled_body(n, p, f, loc, m))
    } else if f.compact.unwrap_or(short) {
        compact_body(n, f, loc, m)
    } else {
        (n < 0.0, full_body(n.abs(), f, m))
    };
    let negative = negative && !reads_zero(&body);
    let (pre, post) = f
        .currency
        .as_deref()
        .filter(|c| money && !c.is_empty())
        .map(|c| currency(c, loc, f.iso == Some(true)))
        .unwrap_or_default();
    let parens = matches!(f.negative, Some(Negative::Parens | Negative::ParensRed));
    let (open, close) = match (negative, parens) {
        (false, _) => ("", ""),
        (true, false) => ("−", ""),
        (true, true) => ("(", ")"),
    };
    let mut text = format!("{open}{pre}{body}{post}{close}");
    if let Some(p) = f.prefix.as_deref().filter(|p| !p.is_empty()) {
        let gap = if p.chars().last().is_some_and(char::is_alphanumeric) {
            " "
        } else {
            ""
        };
        text = format!("{p}{gap}{text}");
    }
    if let Some(s) = f.suffix.as_deref().filter(|s| !s.is_empty()) {
        let gap = if s.chars().next().is_some_and(char::is_alphanumeric) {
            " "
        } else {
            ""
        };
        text = format!("{text}{gap}{s}");
    }
    (text, negative)
}

/// A change as a number tile writes it after its arrow: 0.042 → "4.2%", 0.31 → "31%" (one decimal under 10 %).
pub fn change_pct(delta: f64, loc: Locale) -> String {
    change_pct_in(delta, loc, None)
}

/// [`change_pct`] with a digit style.
pub fn change_pct_in(delta: f64, loc: Locale, digits: Option<Digits>) -> String {
    let d = if delta.abs() < 0.1 { 1 } else { 0 };
    let (whole, kept) = fixed((delta * 100.0).abs(), d);
    format!("{}%", join(&whole, &kept, false, marks(loc, digits)))
}

/// The short form, as a number tile shows it: 18.4K, 1,2 jt, Rp 1,2 jt. `compact: false` keeps every digit.
pub fn short(n: f64, loc: Locale, f: &NumFormat) -> String {
    short_in(n, loc, None, f)
}

/// The full form: grouped digits, up to 2 decimals. `compact: true` asks for the short form.
pub fn long(n: f64, loc: Locale, f: &NumFormat) -> String {
    long_in(n, loc, None, f)
}

/// [`short`] with a digit style (the workspace's `digits`); `None` follows the locale.
pub fn short_in(n: f64, loc: Locale, digits: Option<Digits>, f: &NumFormat) -> String {
    write(n, loc, digits, f, true).0
}

/// [`long`] with a digit style (the workspace's `digits`); `None` follows the locale.
pub fn long_in(n: f64, loc: Locale, digits: Option<Digits>, f: &NumFormat) -> String {
    write(n, loc, digits, f, false).0
}

/// How the value is marked: bad for a negative written in a `red` style, or the colour of the code's section.
pub fn tone(n: f64, f: &NumFormat) -> Option<Tone> {
    if !n.is_finite() {
        return None;
    }
    if let Some(code) = f.code.as_deref().and_then(numcode::Code::parse) {
        return code.tone(n);
    }
    let red = matches!(f.negative, Some(Negative::Red | Negative::ParensRed));
    (red && write(n, Locale::En, None, f, false).1).then_some(Tone::Bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(
        decimals: Option<u32>,
        currency: Option<&str>,
        compact: Option<bool>,
        percent: bool,
    ) -> NumFormat {
        NumFormat {
            decimals,
            currency: currency.map(str::to_string),
            compact,
            percent: percent.then_some(true),
            ..Default::default()
        }
    }

    /// Each row is what the app writes (`shortNumber`, `longNumber` in `ui/src/lib/locale.ts`), copied from its
    /// output.
    #[test]
    fn numbers_read_as_the_app_writes_them() {
        let none = NumFormat::default();
        let rows: Vec<(Locale, f64, NumFormat, &str, &str)> = vec![
            (Locale::En, 18412.0, none.clone(), "18.4K", "18,412"),
            (Locale::En, 9999.5, none.clone(), "10,000", "9,999.5"),
            (Locale::En, 12.345, none.clone(), "12.35", "12.35"),
            (Locale::En, -18412.0, none.clone(), "−18.4K", "−18,412"),
            (Locale::En, 999999.0, none.clone(), "1M", "999,999"),
            (
                Locale::En,
                1234567890.0,
                none.clone(),
                "1.23B",
                "1,234,567,890",
            ),
            (
                Locale::En,
                999999.0,
                f(Some(1), None, None, false),
                "1.0M",
                "999,999.0",
            ),
            (
                Locale::En,
                -4200000.0,
                f(None, Some("USD"), None, false),
                "−$4.2M",
                "−$4,200,000",
            ),
            (
                Locale::En,
                0.1234,
                f(None, None, None, true),
                "12.3%",
                "12.3%",
            ),
            (Locale::En, -0.004, none.clone(), "0", "0"),
            // A tie goes up, as in JavaScript.
            (Locale::En, 18450.0, none.clone(), "18.5K", "18,450"),
            (Locale::En, 0.125, none.clone(), "0.13", "0.13"),
            (Locale::En, -150.5, none.clone(), "−150", "−150.5"),
            (
                Locale::En,
                99950.0,
                f(Some(1), None, None, false),
                "100.0K",
                "99,950.0",
            ),
            (Locale::Id, 18412.0, none.clone(), "18,4 rb", "18.412"),
            (
                Locale::Id,
                4200000.0,
                f(None, Some("IDR"), None, false),
                "Rp 4,2 jt",
                "Rp 4.200.000",
            ),
            (
                Locale::Id,
                4200000.0,
                f(None, Some("MZN"), None, false),
                "4,2 jt MT",
                "4.200.000 MT",
            ),
            (
                Locale::Id,
                12345.678,
                f(Some(1), None, Some(false), false),
                "12.345,7",
                "12.345,7",
            ),
            (
                Locale::Id,
                1234567890.0,
                none.clone(),
                "1,23 M",
                "1.234.567.890",
            ),
        ];
        for (loc, n, fmt, s, l) in rows {
            assert_eq!(
                (short(n, loc, &fmt).as_str(), long(n, loc, &fmt).as_str()),
                (s, l),
                "{loc:?} {n} {fmt:?}"
            );
        }
        assert_eq!(short(f64::NAN, Locale::En, &NumFormat::default()), "–");
    }

    /// Every row of the rows the app shares (`ui/tests/fixtures/numfmt.json`): the short form, the full form and the
    /// tone. All failing rows are listed, not only the first.
    #[test]
    fn the_shared_rows_read_the_same() {
        let rows: Value =
            serde_json::from_str(include_str!("../../../ui/tests/fixtures/numfmt.json")).unwrap();
        let rows = rows["rows"].as_array().unwrap();
        assert!(rows.len() > 100);
        let mut bad = Vec::new();
        for r in rows {
            let n = r["n"].as_f64().unwrap();
            let loc: Locale = serde_json::from_value(r["loc"].clone()).unwrap();
            let digits: Option<Digits> = r
                .get("digits")
                .map(|d| serde_json::from_value(d.clone()).unwrap());
            let f = NumFormat::from_value(&r["f"]);
            let got = (
                short_in(n, loc, digits, &f),
                long_in(n, loc, digits, &f),
                match tone(n, &f) {
                    Some(Tone::Bad) => Some("bad"),
                    Some(Tone::Good) => Some("good"),
                    None => None,
                },
            );
            let want = (
                r["short"].as_str().unwrap().to_string(),
                r["full"].as_str().unwrap().to_string(),
                r.get("tone").and_then(Value::as_str),
            );
            if got != want {
                bad.push(format!("{r}\n  got  {got:?}\n  want {want:?}"));
            }
        }
        assert!(
            bad.is_empty(),
            "{} rows differ:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }

    #[test]
    fn a_bad_field_is_ignored_not_an_error() {
        let f: NumFormat = serde_json::from_str(
            r#"{"decimals":"two","currency":"USD","negative":"wavy","scale":5,"duration":"y","sig":0,"extra":1}"#,
        )
        .unwrap();
        assert_eq!(
            f,
            NumFormat {
                currency: Some("USD".into()),
                ..Default::default()
            }
        );
        let f: NumFormat = serde_json::from_str(r#"{"decimals":9}"#).unwrap();
        assert_eq!(f.decimals, Some(6));
        let f: NumFormat = serde_json::from_str("null").unwrap();
        assert_eq!(f, NumFormat::default());
    }
}
