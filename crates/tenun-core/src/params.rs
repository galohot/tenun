//! Parameters and what-if (v5 build 05, V5-D7): a named number (a slider or a box) or a choice (buttons or a
//! dropdown) that a measure reads with `PARAM("Price rise")`, and that a Top N, a target or a line can take. The value
//! is saved in the workspace; a saved view, a moved control or an export's steps can stand in for it for one call.

use duckdb::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Param {
    /// `p<n>`; a new one is sent with an empty id.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// `number` or `choice`.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A choice parameter offers at most this many values; an export holds a parameter's steps when it has at most
/// `MAX_STEPS` of them (V5-D5).
pub const MAX_CHOICES: usize = 50;
pub const MAX_STEPS: usize = 21;
/// A workspace has at most this many parameters.
pub const MAX_PARAMS: usize = 50;
/// A number parameter has at most this many values (a slider past that cannot be moved to each one).
pub const MAX_VALUES: f64 = 10_000.0;

impl Param {
    pub fn is_number(&self) -> bool {
        self.kind != "choice"
    }

    /// Every value the control can take, when there are at most `MAX_STEPS`: the steps of a number from its minimum to
    /// its maximum, or the choices. `None` past that.
    pub fn steps(&self) -> Option<Vec<Value>> {
        if !self.is_number() {
            return (self.choices.len() <= MAX_STEPS).then(|| self.choices.clone());
        }
        let (min, max, step) = (self.min?, self.max?, self.step?);
        if step.is_nan() || step <= 0.0 || max < min {
            return None;
        }
        let n = ((max - min) / step + 1e-9).floor() as usize + 1;
        (n <= MAX_STEPS).then(|| {
            (0..n)
                .map(|i| serde_json::json!(round(min + step * i as f64)))
                .collect()
        })
    }

    /// A value this parameter can take, as it takes it: a number kept inside its range, or one of its choices. A view
    /// saved before the range or the list changed may hold another.
    pub fn allows(&self, v: &Value) -> Option<Value> {
        if self.is_number() {
            let n = v.as_f64()?;
            let (min, max) = (self.min.unwrap_or(f64::MIN), self.max.unwrap_or(f64::MAX));
            return Some(serde_json::json!(n.clamp(min, max)));
        }
        let key = crate::snapshot::js_key(v);
        self.choices
            .iter()
            .find(|c| crate::snapshot::js_key(c) == key)
            .cloned()
    }

    /// The kind of value `PARAM` gives: text for a choice with a value that is not a number.
    pub fn is_text(&self) -> bool {
        !self.is_number() && self.choices.iter().any(|c| !c.is_number())
    }

    /// The value as a number, for a Top N, a target or a line.
    pub fn number(&self) -> Option<f64> {
        self.value.as_f64()
    }
}

/// Rounds away the float noise of `min + step * i` (0.1 * 3 = 0.30000000000000004).
fn round(x: f64) -> f64 {
    (x * 1e9).round() / 1e9
}

/// Checks a list of parameters and gives new ones an id. Names are unique (whatever the case) and not empty; a number
/// has a step above 0 and a maximum not below its minimum, and its value is kept inside them; a choice has 1 to 50
/// values and its value is one of them.
pub fn check(mut list: Vec<Param>) -> anyhow::Result<Vec<Param>> {
    let mut next = list
        .iter()
        .filter_map(|p| p.id.strip_prefix('p')?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    let mut seen = std::collections::HashSet::new();
    let mut ids = std::collections::HashSet::new();
    for p in &mut list {
        p.name = p.name.trim().to_string();
        if p.name.is_empty() {
            anyhow::bail!("A parameter needs a name.");
        }
        if p.name.chars().count() > 60 {
            anyhow::bail!("Keep the name of a parameter to 60 letters or fewer.");
        }
        if p.name.contains('"') {
            anyhow::bail!("{}: a name cannot hold a double quote.", p.name);
        }
        if !seen.insert(p.name.to_lowercase()) {
            anyhow::bail!("Two parameters are called {}.", p.name);
        }
        if p.is_number() {
            p.kind = "number".into();
            let min = p.min.unwrap_or(0.0);
            let max = p.max.unwrap_or(100.0);
            let step = p.step.unwrap_or(1.0);
            if step.is_nan() || step <= 0.0 || max < min || !min.is_finite() || !max.is_finite() {
                anyhow::bail!(
                    "{}: the step must be above 0 and the largest value not below the smallest.",
                    p.name
                );
            }
            if (max - min) / step + 1.0 > MAX_VALUES {
                anyhow::bail!(
                    "{}: that is more than 10,000 steps. Use a larger step.",
                    p.name
                );
            }
            let v = p.value.as_f64().unwrap_or(min).clamp(min, max);
            (p.min, p.max, p.step, p.value) =
                (Some(min), Some(max), Some(step), serde_json::json!(v));
            p.choices.clear();
        } else {
            // Text and numbers only, each once.
            let mut keys = std::collections::HashSet::new();
            p.choices.retain(|c| {
                (c.is_string() || c.is_number()) && keys.insert(crate::snapshot::js_key(c))
            });
            if p.choices.is_empty() || p.choices.len() > MAX_CHOICES {
                anyhow::bail!("{}: a choice needs 1 to {MAX_CHOICES} values.", p.name);
            }
            p.value = p.allows(&p.value).unwrap_or_else(|| p.choices[0].clone());
            (p.min, p.max, p.step) = (None, None, None);
        }
        // A new parameter, or one whose id is taken already: the next `p<n>`.
        if !valid_id(&p.id) || ids.contains(&p.id) {
            next += 1;
            p.id = format!("p{next}");
        }
        ids.insert(p.id.clone());
    }
    Ok(list)
}

/// The parameter ids the tiles of these dashboards name: a parameter tile's, a Top N's, a target's, a line's. A
/// deleted parameter's id is not given to a new one while a tile still names it.
pub fn ids_named_by(dashboards: &[Value]) -> Vec<String> {
    let mut out = Vec::new();
    let tiles = dashboards
        .iter()
        .filter_map(|d| d.get("tiles")?.as_array())
        .flatten();
    for t in tiles {
        let lines = t
            .get("lines")
            .and_then(Value::as_array)
            .into_iter()
            .flatten();
        let found = [
            t.pointer("/param/id"),
            t.pointer("/top/param"),
            t.pointer("/target/param"),
        ]
        .into_iter()
        .chain(lines.map(|l| l.get("param")))
        .flatten()
        .filter_map(Value::as_str);
        out.extend(found.map(str::to_string));
    }
    out
}

/// An id is `p` and a number: it goes into the SQL of `PARAM`, so nothing else may.
pub fn valid_id(id: &str) -> bool {
    id.strip_prefix('p')
        .is_some_and(|n| !n.is_empty() && n.len() <= 9 && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The parameters of a file as it is opened: the ones [`check`] would refuse are dropped one by one, so a file edited
/// by hand still opens.
pub fn clean(list: Vec<Param>) -> Vec<Param> {
    let mut out: Vec<Param> = Vec::new();
    for p in list {
        let mut next = out.clone();
        next.push(p);
        if let Ok(ok) = check(next) {
            out = ok;
        }
    }
    out
}

/// The table the SQL of `PARAM` reads: one of the connection's own, filled with bound values before a query runs.
pub const TABLE: &str = "temp.main.tenun_param";

/// The SQL of `PARAM("…")` for the parameter `id`: its value as a number, or as text.
pub fn sql(id: &str, text: bool) -> String {
    let id = if valid_id(id) { id } else { "p0" };
    format!(
        "(SELECT {} FROM {TABLE} WHERE id = '{id}')",
        if text { "txt" } else { "num" }
    )
}

/// The length of the SQL of a number parameter's value at the start of `s`, when it starts with one: the measures read
/// it as a number, so `SUM([a]) * (1 + PARAM("Price rise"))` still adds up.
pub fn number_sql_at(s: &[u8]) -> Option<usize> {
    let head = format!("(SELECT num FROM {TABLE} WHERE id = 'p");
    let rest = s.strip_prefix(head.as_bytes())?;
    let digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
    (digits > 0 && rest[digits..].starts_with(b"')")).then_some(head.len() + digits + 2)
}

/// The ids of the parameters a piece of SQL reads.
pub fn ids_in(sql: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = sql;
    let mark = format!("{TABLE} WHERE id = '");
    while let Some(at) = rest.find(&mark) {
        rest = &rest[at + mark.len()..];
        if let Some(end) = rest.find('\'') {
            out.insert(rest[..end].to_string());
        }
    }
    out
}

thread_local! {
    /// The parameters, with the values of this call, for the queries that run on this thread.
    static BOUND: RefCell<Vec<Param>> = const { RefCell::new(Vec::new()) };
    /// The ids of the parameters the queries read, while a tile runs.
    static USED: RefCell<Option<BTreeSet<String>>> = const { RefCell::new(None) };
}

/// Runs `f` with these parameters (and their values) bound for every query it runs on this thread.
pub fn using<R>(list: Vec<Param>, f: impl FnOnce() -> R) -> R {
    let before = BOUND.with(|b| std::mem::replace(&mut *b.borrow_mut(), list));
    let out = f();
    BOUND.with(|b| *b.borrow_mut() = before);
    out
}

/// Runs `f` and gives the ids of the parameters its queries read.
pub fn collecting<R>(f: impl FnOnce() -> R) -> (R, BTreeSet<String>) {
    let before = USED.with(|u| u.borrow_mut().replace(BTreeSet::new()));
    let out = f();
    let used = USED
        .with(|u| std::mem::replace(&mut *u.borrow_mut(), before))
        .unwrap_or_default();
    // A tile inside a tile (a number tile's side questions) counts for the outer one too.
    USED.with(|u| {
        if let Some(outer) = u.borrow_mut().as_mut() {
            outer.extend(used.iter().cloned());
        }
    });
    (out, used)
}

/// Before a query runs: when its SQL reads a parameter, the values go into the connection's own table, as bound
/// values, never as SQL text (V5-D7).
pub fn bind(conn: &Connection, sql: &str) -> anyhow::Result<()> {
    if !sql.contains(TABLE) {
        return Ok(());
    }
    let ids = ids_in(sql);
    conn.execute_batch(
        "CREATE OR REPLACE TEMP TABLE tenun_param (id VARCHAR, num DOUBLE, txt VARCHAR)",
    )?;
    let mut ins = conn.prepare(&format!("INSERT INTO {TABLE} VALUES (?, ?, ?)"))?;
    BOUND.with(|b| -> anyhow::Result<()> {
        for p in b.borrow().iter().filter(|p| ids.contains(&p.id)) {
            let text = match &p.value {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            };
            ins.execute(duckdb::params![p.id, p.value.as_f64(), text])?;
        }
        Ok(())
    })?;
    USED.with(|u| {
        if let Some(set) = u.borrow_mut().as_mut() {
            set.extend(ids);
        }
    });
    Ok(())
}

/// The values standing in for the saved ones on this thread, to carry into another thread.
pub fn overrides() -> BTreeMap<String, Value> {
    OVERRIDES.with(|o| o.borrow().clone())
}

thread_local! {
    /// The values that stand in for the saved ones during one call: a saved view's, a moved control's, an export step's.
    static OVERRIDES: RefCell<BTreeMap<String, Value>> = const { RefCell::new(BTreeMap::new()) };
}

/// Runs `f` with these values standing in for the saved ones (by parameter id), and puts the ones before back.
pub fn with<R>(values: &BTreeMap<String, Value>, f: impl FnOnce() -> R) -> R {
    if values.is_empty() {
        return f();
    }
    let before = OVERRIDES.with(|o| {
        let mut o = o.borrow_mut();
        let before = o.clone();
        o.extend(values.iter().map(|(k, v)| (k.clone(), v.clone())));
        before
    });
    let out = f();
    OVERRIDES.with(|o| *o.borrow_mut() = before);
    out
}

/// The parameters with the values of this call.
pub fn current(saved: &[Param]) -> Vec<Param> {
    OVERRIDES.with(|o| {
        let o = o.borrow();
        saved
            .iter()
            .map(|p| match o.get(&p.id).and_then(|v| p.allows(v)) {
                Some(v) => Param {
                    value: v,
                    ..p.clone()
                },
                None => p.clone(),
            })
            .collect()
    })
}

/// A formula with each `PARAM("old")` written as `PARAM("new")`, whatever the case the name was written in. All the
/// names change in one pass, so two parameters can swap their names.
pub fn rename_in(formula: &str, renames: &[(String, String)]) -> String {
    let wanted: Vec<(String, &str)> = renames
        .iter()
        .map(|(old, new)| (old.trim().to_lowercase(), new.as_str()))
        .collect();
    let bytes = formula.as_bytes();
    let mut out = String::new();
    // `formula[..copied]` is in `out` already.
    let mut copied = 0;
    let mut i = 0;
    while i + 5 <= bytes.len() {
        // The word PARAM, not the end of a longer word. Its letters are ASCII, so a byte is a character here.
        let word = bytes[i..i + 5].eq_ignore_ascii_case(b"param")
            && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'));
        if !word {
            i += 1;
            continue;
        }
        i += 5;
        let Some(q) = formula[i..]
            .trim_start()
            .strip_prefix('(')
            .map(str::trim_start)
            .and_then(|s| s.strip_prefix('"'))
        else {
            continue;
        };
        let Some(end) = q.find('"') else { continue };
        let name = q[..end].trim().to_lowercase();
        let Some((_, new)) = wanted.iter().find(|(old, _)| *old == name) else {
            continue;
        };
        // The name starts where `q` starts; the closing quote and what follows are copied later.
        let start = formula.len() - q.len();
        out.push_str(&formula[copied..start]);
        out.push_str(new);
        copied = start + end;
        i = copied;
    }
    out.push_str(&formula[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn number(name: &str, min: f64, max: f64, step: f64, value: f64) -> Param {
        Param {
            name: name.into(),
            kind: "number".into(),
            value: json!(value),
            min: Some(min),
            max: Some(max),
            step: Some(step),
            ..Param::default()
        }
    }

    #[test]
    fn steps_of_a_number_and_of_a_choice() {
        assert_eq!(number("a", 0.0, 0.2, 0.01, 0.0).steps().unwrap().len(), 21);
        assert_eq!(
            number("a", 0.0, 0.2, 0.01, 0.0).steps().unwrap()[3],
            json!(0.03)
        );
        assert!(
            number("a", 0.0, 0.22, 0.01, 0.0).steps().is_none(),
            "23 steps is more than a file holds"
        );
        let c = Param {
            name: "c".into(),
            kind: "choice".into(),
            choices: vec![json!("A"), json!("B")],
            ..Param::default()
        };
        assert_eq!(c.steps().unwrap(), vec![json!("A"), json!("B")]);
    }

    #[test]
    fn a_list_is_checked_and_given_ids() {
        let list = check(vec![
            number(" Price rise ", 0.0, 0.2, 0.01, 0.5),
            Param {
                id: "p4".into(),
                ..number("Top", 3.0, 20.0, 1.0, 5.0)
            },
        ])
        .unwrap();
        assert_eq!(
            (
                list[0].id.as_str(),
                list[0].name.as_str(),
                list[0].value.clone()
            ),
            ("p5", "Price rise", json!(0.2))
        );
        assert!(
            check(vec![
                number("a", 0.0, 1.0, 1.0, 0.0),
                number("A", 0.0, 1.0, 1.0, 0.0)
            ])
            .is_err()
        );
        assert!(check(vec![number("a", 1.0, 0.0, 1.0, 0.0)]).is_err());
        assert!(check(vec![number("a", 0.0, 1.0, 0.0, 0.0)]).is_err());
        assert!(check(vec![number(&"x".repeat(61), 0.0, 1.0, 1.0, 0.0)]).is_err());
        assert!(
            check(vec![number("a", 0.0, 1.0, 0.00001, 0.0)]).is_err(),
            "100,001 steps"
        );
        // A repeated id gets the next one; repeated choices and ones that are not text or a number go.
        let twice = check(vec![
            Param {
                id: "p3".into(),
                ..number("a", 0.0, 1.0, 1.0, 0.0)
            },
            Param {
                id: "p3".into(),
                ..number("b", 0.0, 1.0, 1.0, 0.0)
            },
        ])
        .unwrap();
        assert_eq!((twice[0].id.as_str(), twice[1].id.as_str()), ("p3", "p4"));
        let c = check(vec![Param {
            name: "c".into(),
            kind: "choice".into(),
            choices: vec![json!("A"), json!(null), json!("A"), json!(5), json!([1])],
            value: json!("Z"),
            ..Param::default()
        }])
        .unwrap();
        assert_eq!(c[0].value, json!("A"));
        assert_eq!(c[0].choices, [json!("A"), json!(5)]);
    }

    #[test]
    fn a_rename_writes_every_param_of_the_name_anew() {
        let r = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(
            rename_in(
                r#"SUM([a]) * (1 + param ( "price RISE" )) - PARAM("other")"#,
                &[r("Price rise", "Kenaikan")]
            ),
            r#"SUM([a]) * (1 + param ( "Kenaikan" )) - PARAM("other")"#
        );
        // Two names swap in one pass; a word that ends in "param" is not the function; text before is kept whole.
        assert_eq!(
            rename_in(
                r#"SUM([İstanbul]) * PARAM("A") + PARAM("B") + MYPARAM("A")"#,
                &[r("A", "B"), r("B", "A")]
            ),
            r#"SUM([İstanbul]) * PARAM("B") + PARAM("A") + MYPARAM("A")"#
        );
        assert_eq!(rename_in("PARAM(", &[r("A", "B")]), "PARAM(");
        assert_eq!(rename_in(r#"PARAM("A"#, &[r("A", "B")]), r#"PARAM("A"#);
    }

    #[test]
    fn values_stand_in_for_one_call() {
        let saved = vec![Param {
            id: "p1".into(),
            ..number("a", 0.0, 1.0, 0.1, 0.0)
        }];
        let over = BTreeMap::from([("p1".to_string(), json!(0.5))]);
        assert_eq!(with(&over, || current(&saved)[0].value.clone()), json!(0.5));
        assert_eq!(current(&saved)[0].value, json!(0.0));
    }
}
