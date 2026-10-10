//! JSON records to a table. Pure: no network, no files. A web link that answers with JSON is read
//! here: find the list of records, flatten each record into columns, and write CSV text. The CSV
//! then goes through the same path as any other link (V3-D80).

use serde::Serialize;
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// The most columns one table reads. The rest are left out and counted.
pub const MAX_COLUMNS: usize = 1000;
/// Lists are looked for down to this many steps from the answer.
const LIST_DEPTH: usize = 4;
/// A nested object becomes columns down to this many names (`a.b.c.d.e`); deeper parts are text.
const FLAT_DEPTH: usize = 5;
/// Where an envelope keeps its paging fields, next to the records.
const ENVELOPES: [&str; 5] = ["paging", "pagination", "links", "meta", "_links"];

/// A JSON value that keeps the order of an object's keys (serde_json's own map sorts them).
/// A number is kept as text.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Val>),
    Obj(Vec<(String, Val)>),
}

impl<'de> Deserialize<'de> for Val {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Val;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_unit<E>(self) -> Result<Val, E> {
                Ok(Val::Null)
            }
            fn visit_none<E>(self) -> Result<Val, E> {
                Ok(Val::Null)
            }
            fn visit_bool<E>(self, v: bool) -> Result<Val, E> {
                Ok(Val::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Val, E> {
                Ok(Val::Num(v.to_string()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Val, E> {
                Ok(Val::Num(v.to_string()))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Val, E> {
                Ok(Val::Num(
                    serde_json::Number::from_f64(v)
                        .map(|n| n.to_string())
                        .unwrap_or_default(),
                ))
            }
            fn visit_str<E>(self, v: &str) -> Result<Val, E> {
                Ok(Val::Str(v.to_owned()))
            }
            fn visit_string<E>(self, v: String) -> Result<Val, E> {
                Ok(Val::Str(v))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Val, A::Error> {
                let mut items = Vec::new();
                while let Some(v) = a.next_element::<Val>()? {
                    items.push(v);
                }
                Ok(Val::Arr(items))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Val, A::Error> {
                let mut items = Vec::new();
                while let Some((k, v)) = a.next_entry::<String, Val>()? {
                    items.push((k, v));
                }
                Ok(Val::Obj(items))
            }
        }
        d.deserialize_any(V)
    }
}

impl Val {
    /// The value as the text of a cell or a link: text and numbers only.
    pub fn text(&self) -> Option<&str> {
        match self {
            Val::Str(s) | Val::Num(s) => Some(s),
            _ => None,
        }
    }

    /// Compact JSON text, with the keys in their own order.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut String) {
        match self {
            Val::Null => out.push_str("null"),
            Val::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Val::Num(n) => out.push_str(n),
            Val::Str(s) => json_str(out, s),
            Val::Arr(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write_json(out);
                }
                out.push(']');
            }
            Val::Obj(items) => {
                out.push('{');
                for (i, (k, v)) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    json_str(out, k);
                    out.push(':');
                    v.write_json(out);
                }
                out.push('}');
            }
        }
    }
}

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

// ---- reading the answer ------------------------------------------------------------------

/// Reads a JSON answer. With `lines_first`, a body of one object per line (NDJSON) is tried
/// before a single JSON value; without it, NDJSON is the second try. The error has the place of
/// the fault and never a piece of the data.
pub fn parse(body: &[u8], lines_first: bool) -> Result<Val, String> {
    let body = body.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(body);
    if lines_first && let Some(v) = parse_lines(body) {
        return Ok(v);
    }
    match serde_json::from_slice::<Val>(body) {
        Ok(v) => Ok(v),
        Err(e) => parse_lines(body).ok_or_else(|| e.to_string()),
    }
}

/// One object per line, blank lines left out. `None` when any other kind of line is there.
fn parse_lines(body: &[u8]) -> Option<Val> {
    let mut out = Vec::new();
    for line in body.split(|&b| b == b'\n') {
        let line = line.trim_ascii();
        if line.is_empty() {
            continue;
        }
        if line[0] != b'{' {
            return None;
        }
        out.push(serde_json::from_slice::<Val>(line).ok()?);
    }
    (!out.is_empty()).then_some(Val::Arr(out))
}

/// True when the first byte that is not a space opens a JSON value.
pub fn looks_like_json(head: &[u8]) -> bool {
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    matches!(head.trim_ascii_start().first(), Some(b'{' | b'['))
}

// ---- paths -------------------------------------------------------------------------------

/// The value at `a.b.0.c`: keys between dots, a number for a list position. `.` or nothing is
/// the answer itself. A key that holds a dot (`@odata.nextLink`) works too.
pub fn get<'a>(v: &'a Val, path: &str) -> Option<&'a Val> {
    let path = path.trim();
    if path.is_empty() || path == "." {
        return Some(v);
    }
    match v {
        Val::Obj(items) => {
            // The whole rest as one key first, then shorter keys that end at a dot.
            let mut ends: Vec<usize> = path.match_indices('.').map(|(i, _)| i).collect();
            ends.push(path.len());
            for end in ends.into_iter().rev() {
                let Some((_, child)) = items.iter().find(|(k, _)| k == &path[..end]) else {
                    continue;
                };
                if end == path.len() {
                    return Some(child);
                }
                if let Some(found) = get(child, &path[end + 1..]) {
                    return Some(found);
                }
            }
            None
        }
        Val::Arr(items) => {
            let (head, rest) = path.split_once('.').unwrap_or((path, ""));
            let child = items.get(head.parse::<usize>().ok()?)?;
            if rest.is_empty() {
                Some(child)
            } else {
                get(child, rest)
            }
        }
        _ => None,
    }
}

/// The text or number at a path, when it is not empty.
pub fn text_at(v: &Val, path: &str) -> Option<String> {
    get(v, path)
        .and_then(Val::text)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

// ---- lists -------------------------------------------------------------------------------

/// A list of objects found in an answer.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ListInfo {
    /// Where it sits: `data.items`, `1`, or `.` for the answer itself.
    pub path: String,
    pub rows: usize,
    #[serde(skip)]
    depth: usize,
}

/// A list of records: it has objects, and nothing else but blanks.
fn is_records(items: &[Val]) -> bool {
    items.iter().any(|x| matches!(x, Val::Obj(_)))
        && items.iter().all(|x| matches!(x, Val::Obj(_) | Val::Null))
}

fn join(path: &[String]) -> String {
    if path.is_empty() {
        ".".into()
    } else {
        path.join(".")
    }
}

/// Every list of objects, down to four steps from the answer, the answer itself included. A list
/// of records is not searched further: what is inside a record is a column.
pub fn lists(v: &Val) -> Vec<ListInfo> {
    fn walk(v: &Val, path: &mut Vec<String>, out: &mut Vec<ListInfo>) {
        match v {
            Val::Arr(items) if is_records(items) => out.push(ListInfo {
                path: join(path),
                rows: items.len(),
                depth: path.len(),
            }),
            Val::Arr(items) if path.len() < LIST_DEPTH => {
                for (i, x) in items.iter().enumerate() {
                    path.push(i.to_string());
                    walk(x, path, out);
                    path.pop();
                }
            }
            Val::Obj(items) if path.len() < LIST_DEPTH => {
                for (k, x) in items {
                    path.push(k.clone());
                    walk(x, path, out);
                    path.pop();
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(v, &mut Vec::new(), &mut out);
    out
}

/// The list of records of an answer.
pub struct Picked<'a> {
    pub path: String,
    pub records: &'a [Val],
}

/// Takes the list the user named; else the answer itself when it is a list; else the longest list
/// of objects (a tie goes to the shallowest, then the first). Fails in words.
pub fn pick<'a>(v: &'a Val, want: Option<&str>) -> Result<Picked<'a>, String> {
    let want = want.map(str::trim).filter(|w| !w.is_empty());
    if let Some(w) = want {
        return match get(v, w) {
            Some(Val::Arr(items)) => Ok(Picked {
                path: w.to_string(),
                records: items,
            }),
            _ if w == "." => Err(format!(
                "The answer itself is not a list. {}",
                found_lists(&lists(v))
            )),
            _ => Err(format!(
                "There is no list at {w}. {}",
                found_lists(&lists(v))
            )),
        };
    }
    let all = lists(v);
    // The answer itself, when it is a list and holds no better list: records, or plain values.
    if let Val::Arr(items) = v
        && all.is_empty()
    {
        return Ok(Picked {
            path: ".".into(),
            records: items,
        });
    }
    let best = all
        .iter()
        .enumerate()
        .min_by_key(|(i, l)| (std::cmp::Reverse(l.rows), l.depth, *i))
        .map(|(_, l)| l.path.clone());
    match best {
        Some(path) => match get(v, &path) {
            Some(Val::Arr(items)) => Ok(Picked {
                path,
                records: items,
            }),
            _ => Err("Tenun found no list of records in this answer.".into()),
        },
        None => Err("Tenun found no list of records in this answer. If the records sit in a list, type its path in Records.".into()),
    }
}

fn found_lists(all: &[ListInfo]) -> String {
    if all.is_empty() {
        return "This answer has no list of records.".into();
    }
    let mut sorted: Vec<&ListInfo> = all.iter().collect();
    sorted.sort_by_key(|l| std::cmp::Reverse(l.rows));
    let shown: Vec<String> = sorted
        .iter()
        .take(6)
        .map(|l| format!("{} ({})", l.path, l.rows))
        .collect();
    let more = sorted.len().saturating_sub(6);
    let tail = if more > 0 {
        format!(", and {more} more")
    } else {
        String::new()
    };
    format!("Lists in this answer: {}{tail}.", shown.join(", "))
}

// ---- flattening --------------------------------------------------------------------------

/// Records turned into columns and rows. Columns keep the order in which they first appear.
#[derive(Default)]
pub struct Flat {
    names: Vec<String>,
    index: HashMap<String, usize>,
    /// True once a column has held something other than a blank.
    filled: Vec<bool>,
    rows: Vec<Vec<(usize, String)>>,
    left_out: HashSet<String>,
}

impl Flat {
    pub fn rows(&self) -> usize {
        self.rows.len()
    }

    /// The names of the columns the table has.
    pub fn columns(&self) -> Vec<String> {
        self.layout().0
    }

    /// How many different columns were left out because the table was full.
    pub fn left_out(&self) -> usize {
        self.left_out.len()
    }

    /// Adds records. An object gives its fields; any other value gives one cell in `value`.
    pub fn add(&mut self, records: &[Val]) {
        for r in records {
            let mut cells = Vec::new();
            match r {
                Val::Obj(items) => {
                    for (k, v) in items {
                        self.flatten(&mut cells, k, v, 1);
                    }
                }
                other => self.flatten(&mut cells, "value", other, FLAT_DEPTH),
            }
            self.rows.push(cells);
        }
    }

    fn flatten(&mut self, cells: &mut Vec<(usize, String)>, name: &str, v: &Val, depth: usize) {
        match v {
            Val::Null => self.put(cells, name, String::new()),
            Val::Bool(b) => self.put(cells, name, b.to_string()),
            Val::Num(s) | Val::Str(s) => self.put(cells, name, s.clone()),
            Val::Arr(_) => self.put(cells, name, v.to_json()),
            Val::Obj(items) if items.is_empty() => self.put(cells, name, String::new()),
            Val::Obj(_) if depth >= FLAT_DEPTH => self.put(cells, name, v.to_json()),
            Val::Obj(items) => {
                for (k, x) in items {
                    self.flatten(cells, &format!("{name}.{k}"), x, depth + 1);
                }
            }
        }
    }

    fn put(&mut self, cells: &mut Vec<(usize, String)>, name: &str, text: String) {
        let i = match self.index.get(name) {
            Some(i) => *i,
            None if self.names.len() >= MAX_COLUMNS => {
                if self.left_out.len() < 100_000 {
                    self.left_out.insert(name.to_string());
                }
                return;
            }
            None => {
                self.names.push(name.to_string());
                self.filled.push(false);
                self.index.insert(name.to_string(), self.names.len() - 1);
                self.names.len() - 1
            }
        };
        if !text.is_empty() {
            self.filled[i] = true;
        }
        cells.push((i, text));
    }

    /// The columns the table has, and where each raw column goes. A column that is blank in every
    /// record is left out when others are named below it: `owner` next to `owner.city` comes from
    /// a record where the owner is null.
    fn layout(&self) -> (Vec<String>, Vec<Option<usize>>) {
        let mut parents: HashSet<&str> = HashSet::new();
        for n in &self.names {
            for (i, _) in n.match_indices('.') {
                parents.insert(&n[..i]);
            }
        }
        let mut names = Vec::new();
        let mut at = vec![None; self.names.len()];
        for (i, n) in self.names.iter().enumerate() {
            if self.filled[i] || !parents.contains(n.as_str()) {
                at[i] = Some(names.len());
                names.push(n.clone());
            }
        }
        (names, at)
    }

    /// The first `n` rows as text, for a preview.
    pub fn head(&self, n: usize) -> Vec<Vec<String>> {
        let (names, at) = self.layout();
        let mut out = Vec::new();
        for row in self.rows.iter().take(n) {
            let mut cells = vec![String::new(); names.len()];
            for (i, t) in row {
                if let Some(c) = at[*i] {
                    cells[c] = t.clone();
                }
            }
            out.push(cells);
        }
        out
    }

    /// CSV text (RFC 4180 quoting): a header line, then one line for each record.
    pub fn to_csv(&self) -> String {
        let (names, at) = self.layout();
        let mut out = String::new();
        for (i, n) in names.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            quote(&mut out, n);
        }
        out.push('\n');
        let mut slots: Vec<Option<&str>> = vec![None; names.len()];
        for row in &self.rows {
            slots.fill(None);
            for (i, t) in row {
                if let Some(c) = at[*i] {
                    slots[c] = Some(t);
                }
            }
            for (i, s) in slots.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let text = s.unwrap_or("");
                // A line with nothing on it would be skipped by a reader: quote the blank.
                if text.is_empty() && slots.len() == 1 {
                    out.push_str("\"\"");
                } else {
                    quote(&mut out, text);
                }
            }
            out.push('\n');
        }
        out
    }
}

fn quote(out: &mut String, v: &str) {
    if v.contains([',', '"', '\n', '\r']) {
        out.push('"');
        out.push_str(&v.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(v);
    }
}

// ---- paging hints ------------------------------------------------------------------------

/// The objects that may hold paging fields: the answer, and the usual envelope objects inside it.
/// A bare list of records has none. A list like `[meta, [records]]` offers its objects.
fn places(v: &Val) -> Vec<(String, &Val)> {
    let mut roots: Vec<(String, &Val)> = Vec::new();
    match v {
        Val::Obj(_) => roots.push((String::new(), v)),
        Val::Arr(items) if !is_records(items) => {
            for (i, x) in items.iter().enumerate().take(3) {
                if matches!(x, Val::Obj(_)) {
                    roots.push((format!("{i}."), x));
                }
            }
        }
        _ => {}
    }
    let mut out = Vec::new();
    for (prefix, root) in roots {
        out.push((prefix.clone(), root));
        if let Val::Obj(items) = root {
            for name in ENVELOPES {
                if let Some((_, inner @ Val::Obj(_))) = items.iter().find(|(k, _)| k == name) {
                    out.push((format!("{prefix}{name}."), inner));
                }
            }
        }
    }
    out
}

/// The first of `names` that a place holds: its path, and its value.
fn field<'a>(v: &'a Val, names: &[&str]) -> Option<(String, &'a Val)> {
    for (prefix, place) in places(v) {
        let Val::Obj(items) = place else { continue };
        for name in names {
            if let Some((_, x)) = items.iter().find(|(k, _)| k == name) {
                return Some((format!("{prefix}{name}"), x));
            }
        }
    }
    None
}

fn is_link(t: &str) -> bool {
    t.starts_with("http://") || t.starts_with("https://") || t.starts_with('/') || t.contains('?')
}

/// How the answer says it has more pages: `field:<path>`, `cursor:<path>` or `page`; empty when
/// it does not say. `link` is for the `Link` header, which the caller reads.
pub fn pages_hint(v: &Val, link_header: bool, url_has_page: bool) -> String {
    if link_header {
        return "link".into();
    }
    const NEXT: [&str; 6] = [
        "next",
        "next_url",
        "nextUrl",
        "next_page_url",
        "nextPageUrl",
        "@odata.nextLink",
    ];
    if let Some((path, x)) = field(v, &NEXT)
        && match x {
            Val::Null => true,
            Val::Str(t) => is_link(t),
            _ => false,
        }
    {
        return format!("field:{path}");
    }
    const CURSOR: [&str; 4] = ["next_cursor", "nextCursor", "cursor", "next_page_token"];
    if let Some((path, x)) = field(v, &CURSOR)
        && matches!(x, Val::Null | Val::Str(_) | Val::Num(_))
    {
        return format!("cursor:{path}");
    }
    if url_has_page || (field(v, &["page"]).is_some() && total_pages(v).is_some()) {
        return "page".into();
    }
    String::new()
}

/// The number of pages the answer says there are.
pub fn total_pages(v: &Val) -> Option<u64> {
    let (_, x) = field(v, &["pages", "total_pages", "totalPages"])?;
    x.text()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(text: &str) -> Val {
        parse(text.as_bytes(), false).unwrap()
    }

    fn csv(records: &str) -> String {
        let v = val(records);
        let mut f = Flat::default();
        f.add(pick(&v, None).unwrap().records);
        f.to_csv()
    }

    #[test]
    fn the_saved_path_picks_the_list() {
        let v = val(r#"{"data":{"items":[{"a":1}],"rows":[{"a":1},{"a":2},{"a":3}]},"x":[0]}"#);
        let p = pick(&v, Some("data.items")).unwrap();
        assert_eq!((p.path.as_str(), p.records.len()), ("data.items", 1));
        let v = val(r#"{"result":[{"n":1},{"rows":[{"a":1},{"a":2}]}]}"#);
        let p = pick(&v, Some("result.1.rows")).unwrap();
        assert_eq!(p.records.len(), 2);
        // A key that holds a dot still works.
        let v = val(r#"{"@odata.nextLink":"https://h/p2","value":[{"a":1}]}"#);
        assert_eq!(
            text_at(&v, "@odata.nextLink").as_deref(),
            Some("https://h/p2")
        );
        // A path that is missing or not a list fails in words, with the lists there are.
        let v = val(r#"{"data":{"rows":[{"a":1},{"a":2}]},"meta":{"links":[{"u":1}]},"n":3}"#);
        let e = pick(&v, Some("data.items")).err().unwrap();
        assert_eq!(
            e,
            "There is no list at data.items. Lists in this answer: data.rows (2), meta.links (1)."
        );
        assert!(pick(&v, Some("n")).is_err(), "a number is not a list");
        let e = pick(&val(r#"{"a":1}"#), Some("data.items")).err().unwrap();
        assert!(e.ends_with("This answer has no list of records."), "{e}");
    }

    #[test]
    fn without_a_path_the_answer_or_the_longest_list_is_taken() {
        // The answer itself is a list.
        let v = val(r#"[{"a":1},{"a":2}]"#);
        assert_eq!(pick(&v, None).unwrap().path, ".");
        // A small list that comes first does not win over a longer one.
        let v = val(r#"{"errors":[{"e":1}],"data":{"rows":[{"a":1},{"a":2},{"a":3}]}}"#);
        let p = pick(&v, None).unwrap();
        assert_eq!((p.path.as_str(), p.records.len()), ("data.rows", 3));
        // Same length: the shallowest wins, then the first.
        let v = val(r#"{"a":{"b":[{"x":1},{"x":2}]},"c":[{"x":3},{"x":4}],"d":[{"x":5},{"x":6}]}"#);
        assert_eq!(pick(&v, None).unwrap().path, "c");
        // The World Bank shape: [meta, [records]].
        let v =
            val(r#"[{"page":1,"pages":3,"total":3},[{"id":"A","value":1},{"id":"B","value":2}]]"#);
        let p = pick(&v, None).unwrap();
        assert_eq!((p.path.as_str(), p.records.len()), ("1", 2));
        // Lists are found down to depth 4 only.
        let all = lists(&val(
            r#"{"a":{"b":{"c":{"d":[{"x":1}]}}},"e":{"f":[{"x":1}]}}"#,
        ));
        assert_eq!(
            all.iter().map(|l| l.path.as_str()).collect::<Vec<_>>(),
            ["a.b.c.d", "e.f"]
        );
        let all = lists(&val(r#"{"a":{"b":{"c":{"d":{"e":[{"x":1}]}}}}}"#));
        assert!(all.is_empty());
        // No list: it fails in words.
        assert!(
            pick(&val(r#"{"a":1}"#), None)
                .err()
                .unwrap()
                .contains("no list of records")
        );
    }

    #[test]
    fn records_are_flattened_into_columns() {
        let out = csv(
            r#"[{"id":1,"user":{"name":"Ann","geo":{"city":"Jakarta"}},"tags":["a","b"],"ok":true,"note":null,"score":2.50},
                {"id":2,"user":{"name":"Bo"},"extra":"x","ok":false,"score":1e3}]"#,
        );
        let mut lines = out.lines();
        // The order is the order of first appearance, across all records.
        assert_eq!(
            lines.next().unwrap(),
            "id,user.name,user.geo.city,tags,ok,note,score,extra"
        );
        assert_eq!(
            lines.next().unwrap(),
            r#"1,Ann,Jakarta,"[""a"",""b""]",true,,2.5,"#
        );
        assert_eq!(lines.next().unwrap(), "2,Bo,,,false,,1000.0,x");
        // Nested objects stop at five names; the rest is JSON text.
        let out = csv(r#"[{"a":{"b":{"c":{"d":{"e":{"f":1}}}}}}]"#);
        assert_eq!(out, "a.b.c.d.e\n\"{\"\"f\"\":1}\"\n");
        // A list of plain values is one column.
        assert_eq!(csv("[1,\"two\",null]"), "value\n1\ntwo\n\"\"\n");
        // Large whole numbers keep every digit.
        assert_eq!(
            csv(r#"[{"id":9007199254740993}]"#),
            "id\n9007199254740993\n"
        );
    }

    #[test]
    fn a_blank_parent_next_to_its_children_is_not_a_column() {
        // The owner is null in one record and an object in another: one set of columns.
        let out = csv(r#"[{"id":1,"owner":null},{"id":2,"owner":{"city":"A"}}]"#);
        assert_eq!(out, "id,owner.city\n1,\n2,A\n");
        // Blank everywhere and with no children, it stays.
        assert_eq!(csv(r#"[{"id":1,"note":null}]"#), "id,note\n1,\n");
    }

    #[test]
    fn csv_text_is_quoted_where_it_needs_to_be() {
        let out = csv(r#"[{"a":"x,y","b":"say \"hi\"","c":"two\nlines","d":"plain"}]"#);
        assert_eq!(
            out,
            "a,b,c,d\n\"x,y\",\"say \"\"hi\"\"\",\"two\nlines\",plain\n"
        );
        // A header name with a comma is quoted too.
        assert_eq!(csv(r#"[{"a,b":1}]"#), "\"a,b\"\n1\n");
    }

    #[test]
    fn a_table_holds_at_most_a_thousand_columns() {
        let mut obj = String::from("{");
        for i in 0..1012 {
            if i > 0 {
                obj.push(',');
            }
            obj.push_str(&format!("\"c{i}\":{i}"));
        }
        obj.push('}');
        let v = val(&format!("[{obj}]"));
        let mut f = Flat::default();
        f.add(pick(&v, None).unwrap().records);
        assert_eq!(f.columns().len(), MAX_COLUMNS);
        assert_eq!(f.left_out(), 12);
        assert_eq!(f.columns()[0], "c0");
        assert_eq!(f.head(1)[0].len(), MAX_COLUMNS);
    }

    #[test]
    fn json_and_one_object_per_line_are_both_read() {
        assert!(matches!(parse(b"\xEF\xBB\xBF[1]", false), Ok(Val::Arr(_))));
        let lines = "{\"a\":1}\n\n{\"a\":2}\r\n{\"a\":3}\n";
        let v = parse(lines.as_bytes(), false).unwrap();
        assert!(matches!(&v, Val::Arr(a) if a.len() == 3));
        assert!(matches!(parse(lines.as_bytes(), true), Ok(Val::Arr(a)) if a.len() == 3));
        // The fault is named by place, never by the data.
        let e = parse(br#"{"secret": nope}"#, false).unwrap_err();
        assert!(e.contains("line 1") && !e.contains("secret"), "{e}");
        assert!(looks_like_json(b"  \n[1]") && looks_like_json(b"{}"));
        assert!(!looks_like_json(b"a,b\n1,2") && !looks_like_json(b""));
    }

    #[test]
    fn paging_hints_follow_the_usual_names() {
        let hint = |t: &str, link: bool, page: bool| pages_hint(&val(t), link, page);
        assert_eq!(hint("[]", true, false), "link");
        assert_eq!(
            hint(r#"{"data":[],"next":"https://h/p2"}"#, false, false),
            "field:next"
        );
        assert_eq!(
            hint(r#"{"data":[],"paging":{"next":null}}"#, false, false),
            "field:paging.next"
        );
        assert_eq!(
            hint(r#"{"@odata.nextLink":"https://h/p2"}"#, false, false),
            "field:@odata.nextLink"
        );
        assert_eq!(
            hint(r#"{"meta":{"next_page_url":"/p2"}}"#, false, false),
            "field:meta.next_page_url"
        );
        assert_eq!(
            hint(r#"{"next_cursor":"abc","items":[]}"#, false, false),
            "cursor:next_cursor"
        );
        assert_eq!(
            hint(r#"{"pagination":{"next_page_token":null}}"#, false, false),
            "cursor:pagination.next_page_token"
        );
        // A number in `next` is not a link.
        assert_eq!(hint(r#"{"next":2}"#, false, false), "");
        // A page number: the link has one, or the answer says page and pages.
        assert_eq!(hint(r#"{"items":[]}"#, false, true), "page");
        assert_eq!(
            hint(
                r#"[{"page":1,"pages":3,"per_page":20},[{"id":1}]]"#,
                false,
                false
            ),
            "page"
        );
        assert_eq!(hint(r#"{"page":1,"items":[]}"#, false, false), "");
        // A plain list of records has no envelope: a record's own fields are not paging.
        assert_eq!(hint(r#"[{"next":"https://h/x"}]"#, false, false), "");
        assert_eq!(total_pages(&val(r#"[{"page":1,"pages":"3"},[]]"#)), Some(3));
    }
}
