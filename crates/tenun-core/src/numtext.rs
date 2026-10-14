//! Numbers in words, as the app writes them (`ui/src/lib/locale.ts`): the short form of a number tile ("18.4K",
//! "Rp 4,2 jt") and the full form ("18,412"). An alert's message uses them (v5 build 12), so a message says the
//! number the way the tile shows it.
//!
//! Rounding follows JavaScript's `toFixed` and `toExponential`, which d3 uses: the exact value of the number is
//! rounded, and a tie goes up (Rust's own formatting sends a tie to the even digit: 18,450 would read "18.4K").

use crate::workspace::Locale;
use serde::Deserialize;

/// How a value is written (the app's `NumFormat`). Every field is optional.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct NumFormat {
    /// 0–4. Unset: the number as it is, up to 2 decimals.
    #[serde(default)]
    pub decimals: Option<u32>,
    /// IDR, USD, EUR, SGD or MZN; any other code is written before the number.
    #[serde(default)]
    pub currency: Option<String>,
    /// Short (1,2 jt) or full. Unset: short on number tiles, full in tables.
    #[serde(default)]
    pub compact: Option<bool>,
    /// Multiply by 100 and add %.
    #[serde(default)]
    pub percent: Option<bool>,
}

/// Below this a short number keeps all its digits.
const COMPACT_FROM: f64 = 10_000.0;

/// The decimal mark and the thousands mark.
fn marks(loc: Locale) -> (char, char) {
    match loc {
        Locale::En => ('.', ','),
        Locale::Id => (',', '.'),
    }
}

/// The words for 10³, 10⁶, 10⁹ and 10¹² (the app's `UNITS`).
fn unit(loc: Locale, power: i32) -> &'static str {
    match (loc, power) {
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

/// Where a currency goes: before the number, after it.
fn currency(code: &str, loc: Locale) -> (String, String) {
    let (pre, post) = match (code, loc) {
        ("IDR", _) => ("Rp ", ""),
        ("USD", Locale::Id) => ("US$", ""),
        ("USD", Locale::En) => ("$", ""),
        ("EUR", _) => ("€", ""),
        ("SGD", _) => ("S$", ""),
        ("MZN", Locale::Id) => ("", " MT"),
        ("MZN", Locale::En) => ("MT ", ""),
        _ => return (format!("{code} "), String::new()),
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
fn fixed(a: f64, d: usize) -> (String, String) {
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

fn group(whole: &str, mark: char) -> String {
    let n = whole.len();
    let mut out = String::with_capacity(n + n / 3);
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (n - i).is_multiple_of(3) {
            out.push(mark);
        }
        out.push(c);
    }
    out
}

/// Grouped digits with `d` decimals; `trim` drops the trailing zeros of the decimals (d3's `~`).
fn grouped(a: f64, d: usize, trim: bool, loc: Locale) -> String {
    let (dec, thou) = marks(loc);
    let (whole, mut kept) = fixed(a, d);
    if trim {
        kept = kept.trim_end_matches('0').to_string();
    }
    let whole = group(&whole, thou);
    if kept.is_empty() {
        whole
    } else {
        format!("{whole}{dec}{kept}")
    }
}

/// The short form with three significant digits and a unit word: 12.9K, 4.2M, 1.23B (d3's `.3~s`).
fn three_digits(a: f64, loc: Locale) -> String {
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
    let (dec, _) = marks(loc);
    let body = if rest.is_empty() {
        int.to_string()
    } else {
        format!("{int}{dec}{rest}")
    };
    format!("{body}{}", unit(loc, unit_power))
}

/// The digits of a number tile's value without sign or currency, and whether it is below zero.
fn compact_body(n: f64, decimals: Option<u32>, loc: Locale) -> (bool, String) {
    let a = n.abs();
    if a < COMPACT_FROM {
        return match decimals {
            Some(d) => (n < 0.0, grouped(a, d as usize, false, loc)),
            // Rounded as JavaScript's Math.round does: a half goes up, also below zero (−150.5 → −150).
            None if a >= 100.0 || n.fract() == 0.0 => {
                let r = (n + 0.5).floor();
                (r < 0.0, grouped(r.abs(), 0, false, loc))
            }
            None => (n < 0.0, grouped(a, 2, true, loc)),
        };
    }
    let Some(d) = decimals else {
        return (n < 0.0, three_digits(a, loc));
    };
    let steps = [(1e12, 12), (1e9, 9), (1e6, 6), (1e3, 3)];
    let Some(mut i) = steps.iter().position(|(v, _)| a >= *v) else {
        return (n < 0.0, grouped(a, d as usize, false, loc));
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
        format!("{}{}", grouped(a / v, d as usize, false, loc), unit(loc, p)),
    )
}

fn full_body(a: f64, decimals: Option<u32>, loc: Locale) -> String {
    match decimals {
        Some(d) => grouped(a, d as usize, false, loc),
        None if a.fract() == 0.0 => grouped(a, 0, false, loc),
        None => grouped(a, 2, true, loc),
    }
}

/// The sign, the currency and the digits. A number that reads as 0 has no sign.
fn decorate(negative: bool, body: String, loc: Locale, f: &NumFormat) -> String {
    let (pre, post) = f
        .currency
        .as_deref()
        .filter(|c| !c.is_empty())
        .map(|c| currency(c, loc))
        .unwrap_or_default();
    let zero = !body.chars().any(|c| c.is_ascii_digit() && c != '0');
    let sign = if negative && !zero { "−" } else { "" };
    format!("{sign}{pre}{body}{post}")
}

fn percent_text(n: f64, loc: Locale, f: &NumFormat) -> String {
    let v = n * 100.0;
    let body = match f.decimals {
        Some(d) => grouped(v.abs(), d as usize, false, loc),
        None => grouped(v.abs(), 1, true, loc),
    };
    let zero = !body.chars().any(|c| c.is_ascii_digit() && c != '0');
    format!("{}{body}%", if v < 0.0 && !zero { "−" } else { "" })
}

/// The short form, as a number tile shows it: 18.4K, 1,2 jt, Rp 1,2 jt. `compact: false` keeps every digit.
pub fn short(n: f64, loc: Locale, f: &NumFormat) -> String {
    if !n.is_finite() {
        return "–".into();
    }
    if f.percent == Some(true) {
        return percent_text(n, loc, f);
    }
    let (negative, body) = if f.compact == Some(false) {
        (n < 0.0, full_body(n.abs(), f.decimals, loc))
    } else {
        compact_body(n, f.decimals, loc)
    };
    decorate(negative, body, loc, f)
}

/// The full form: grouped digits, up to 2 decimals. `compact: true` asks for the short form.
pub fn long(n: f64, loc: Locale, f: &NumFormat) -> String {
    if !n.is_finite() {
        return "–".into();
    }
    if f.percent == Some(true) {
        return percent_text(n, loc, f);
    }
    let (negative, body) = if f.compact == Some(true) {
        compact_body(n, f.decimals, loc)
    } else {
        (n < 0.0, full_body(n.abs(), f.decimals, loc))
    };
    decorate(negative, body, loc, f)
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
}
