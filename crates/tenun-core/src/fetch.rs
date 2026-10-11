//! Fetches a web link into a cache file. The answer is CSV, or JSON that is turned into CSV here
//! (see [`crate::jsonrec`]). The link is https only (a local address may use http, so a test server
//! and a file served on this computer work). No cookies are kept or sent, and a download over 50 MB
//! is refused.
//!
//! A link may carry a key. The key goes only to the host of the saved link: this module follows
//! redirects itself, so a redirect or a page link to another host gets none. The value is taken
//! out of every error text before the text leaves this module.

use crate::jsonrec::{self, Flat, ListInfo, Val};
use crate::paste;
use crate::workspace::{KeyPlace, Paging};
use serde::Serialize;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;
use ureq::http::{HeaderMap, HeaderName, HeaderValue, Uri};

/// The biggest download: 50 MB. For a link with pages, this is the total of all pages.
pub const MAX_BYTES: u64 = 50 * 1024 * 1024;
/// The most pages one fetch reads.
pub const MAX_PAGES: usize = 200;
const MAX_REDIRECTS: usize = 5;
/// How much of a CSV answer a preview reads.
const PREVIEW_BYTES: u64 = 256 * 1024;
/// Stands for a key value in a text that is shown.
const HIDDEN: &str = "•••";
/// A key shorter than this is not taken out of the data an answer holds.
const MIN_HIDDEN: usize = 6;

const NOT_DATA: &str = "That link gives a web page, not data. In Google Sheets, publish the sheet as CSV and use that link.";

/// What to fetch: the link, where its records sit, how its pages follow each other, and a key.
pub struct Spec<'a> {
    pub url: &'a str,
    pub records: Option<&'a str>,
    pub pages: Option<&'a Paging>,
    pub key: Option<KeyArg<'a>>,
}

/// A key with its value. The value lives in memory for the length of one fetch.
#[derive(Clone, Copy)]
pub struct KeyArg<'a> {
    pub place: KeyPlace,
    pub name: &'a str,
    pub value: &'a str,
}

/// What the Add sheet shows after a look at the first page.
#[derive(Debug, Serialize)]
pub struct Preview {
    /// `csv` or `json`.
    pub format: &'static str,
    /// JSON: every list of records in the answer.
    pub lists: Vec<ListInfo>,
    /// JSON: the list that was read, or `.` for the answer itself.
    pub records: String,
    pub columns: Vec<String>,
    /// The first 5 rows.
    pub rows: Vec<Vec<String>>,
    /// Rows on this page.
    pub total: usize,
    /// How the answer points at more pages: `link`, `field:<path>`, `cursor:<path>`, `page`, or empty.
    pub pages_hint: String,
}

/// The host of a link, or an error that says what is wrong in words.
pub fn check(url: &str) -> anyhow::Result<String> {
    let url = url.trim();
    let uri: Uri = url
        .parse()
        .map_err(|_| anyhow::anyhow!("That does not look like a web link."))?;
    let host = uri.host().unwrap_or_default().to_string();
    if host.is_empty() {
        anyhow::bail!("That does not look like a web link.");
    }
    match uri.scheme_str() {
        Some("https") => Ok(host),
        Some("http") if is_local(&host) => Ok(host),
        Some("http") => {
            anyhow::bail!("Only https links work. Use the link that starts with https://.")
        }
        _ => anyhow::bail!("The link must start with https://."),
    }
}

fn is_local(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// True when a host name stands for this computer, however it is written: `localhost` in any
/// case, with a dot at the end or as `*.localhost`; an address in 127.0.0.0/8; `0.0.0.0`; `::1`,
/// `::`; and `::ffff:127.0.0.1`. A public link must not lead Tenun to any of these (V3-D86).
fn is_loopback(host: &str) -> bool {
    use std::net::IpAddr;
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let h = h.trim_start_matches('[').trim_end_matches(']');
    if h == "localhost" || h.ends_with(".localhost") {
        return true;
    }
    match h.parse::<IpAddr>() {
        Ok(IpAddr::V4(a)) => a.is_loopback() || a.is_unspecified(),
        Ok(IpAddr::V6(a)) => {
            a.is_loopback()
                || a.is_unspecified()
                || a.to_ipv4_mapped()
                    .is_some_and(|v| v.is_loopback() || v.is_unspecified())
        }
        Err(_) => false,
    }
}

// ---- links, by hand: no new crate ---------------------------------------------------------

fn pct_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len()
                && b[i + 1].is_ascii_hexdigit()
                && b[i + 2].is_ascii_hexdigit() =>
            {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("00");
                out.push(u8::from_str_radix(hex, 16).unwrap_or(b'%'));
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The link with one query parameter set. An existing parameter of that name is replaced where it
/// stands; otherwise the parameter is added at the end. The part after `#` is dropped.
pub fn set_param(url: &str, name: &str, value: &str) -> String {
    let url = url.split('#').next().unwrap_or(url);
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let pair = format!("{}={}", pct_encode(name), pct_encode(value));
    let mut parts: Vec<String> = Vec::new();
    let mut done = false;
    for seg in query.split('&').filter(|s| !s.is_empty()) {
        if pct_decode(seg.split('=').next().unwrap_or("")) == name {
            if !done {
                parts.push(pair.clone());
                done = true;
            }
        } else {
            parts.push(seg.to_string());
        }
    }
    if !done {
        parts.push(pair);
    }
    format!("{base}?{}", parts.join("&"))
}

/// The value of one query parameter of a link.
pub fn param(url: &str, name: &str) -> Option<String> {
    let url = url.split('#').next().unwrap_or(url);
    let (_, query) = url.split_once('?')?;
    query.split('&').find_map(|seg| {
        let (n, v) = seg.split_once('=').unwrap_or((seg, ""));
        (pct_decode(n) == name).then(|| pct_decode(v))
    })
}

/// A link found in an answer or a header, made complete against the link it came from.
pub fn resolve(base: &str, href: &str) -> String {
    let href = href.trim();
    if let Some(colon) = href.find(':')
        && href[colon..].starts_with("://")
        && href[..colon]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        return href.to_string();
    }
    let Ok(uri) = base.parse::<Uri>() else {
        return href.to_string();
    };
    let scheme = uri.scheme_str().unwrap_or("https");
    let Some(auth) = uri.authority() else {
        return href.to_string();
    };
    if href.starts_with("//") {
        return format!("{scheme}:{href}");
    }
    let origin = format!("{scheme}://{auth}");
    let path = uri.path();
    if href.starts_with('/') {
        return format!("{origin}{}", normalize(href));
    }
    if href.starts_with('?') {
        return format!("{origin}{path}{href}");
    }
    let dir = &path[..path.rfind('/').map_or(0, |i| i + 1)];
    format!("{origin}{}", normalize(&format!("{dir}{href}")))
}

/// Takes `.` and `..` steps out of the path part of a link (a query is left alone).
fn normalize(link: &str) -> String {
    let (path, rest) = match link.find(['?', '#']) {
        Some(i) => link.split_at(i),
        None => (link, ""),
    };
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "." => {}
            ".." => {
                if out.len() > 1 {
                    out.pop();
                }
            }
            s => out.push(s),
        }
    }
    format!("{}{rest}", out.join("/"))
}

/// The host and port of a link, for comparing where a request goes.
fn origin_of(url: &str) -> Option<(String, u16)> {
    let uri: Uri = url.parse().ok()?;
    let port = uri
        .port_u16()
        .unwrap_or(if uri.scheme_str() == Some("https") {
            443
        } else {
            80
        });
    Some((uri.host()?.to_ascii_lowercase(), port))
}

/// The target of `rel="next"` in the `Link` headers.
fn link_next(headers: &HeaderMap, base: &str) -> Option<String> {
    for v in headers.get_all("link") {
        let Ok(text) = v.to_str() else { continue };
        let mut rest = text;
        while let Some(start) = rest.find('<') {
            let Some(len) = rest[start..].find('>') else {
                break;
            };
            let target = &rest[start + 1..start + len];
            let after = &rest[start + len + 1..];
            let params = after[..after.find('<').unwrap_or(after.len())]
                .trim()
                .trim_end_matches(',');
            let is_next = params
                .split(';')
                .filter_map(|p| p.split_once('='))
                .any(|(k, v)| {
                    k.trim().eq_ignore_ascii_case("rel")
                        && v.trim()
                            .trim_matches('"')
                            .split_whitespace()
                            .any(|r| r.eq_ignore_ascii_case("next"))
                });
            if is_next && !target.trim().is_empty() {
                return Some(resolve(base, target));
            }
            rest = after;
        }
    }
    None
}

// ---- the client ---------------------------------------------------------------------------

struct Key {
    place: KeyPlace,
    name: String,
    value: String,
}

struct Client {
    agent: ureq::Agent,
    key: Option<Key>,
    /// Host and port of the saved link: the only place the key goes.
    home: (String, u16),
    home_is_local: bool,
}

struct Reply {
    res: ureq::http::Response<ureq::Body>,
    host: String,
    /// The link that answered, after redirects.
    url: String,
}

impl Client {
    fn new(spec: &Spec) -> anyhow::Result<Client> {
        let url = spec.url.trim();
        let host = check(url)?;
        let home =
            origin_of(url).ok_or_else(|| anyhow::anyhow!("That does not look like a web link."))?;
        let key = match spec.key {
            Some(k) if !k.name.trim().is_empty() && !k.value.is_empty() => {
                if k.place == KeyPlace::Header {
                    HeaderName::from_bytes(k.name.trim().as_bytes()).map_err(|_| {
                        anyhow::anyhow!("{} cannot be the name of a header.", k.name.trim())
                    })?;
                    HeaderValue::from_str(k.value).map_err(|_| {
                        anyhow::anyhow!("The key has a character that a header cannot hold.")
                    })?;
                }
                Some(Key {
                    place: k.place,
                    name: k.name.trim().to_string(),
                    value: k.value.to_string(),
                })
            }
            _ => None,
        };
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .https_only(!is_local(&host))
            // Redirects are followed by hand: ureq would send a header key to any host.
            .max_redirects(0)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(120)))
            .http_status_as_error(false)
            .user_agent(format!("tenun/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Ok(Client {
            agent,
            key,
            home,
            home_is_local: is_loopback(&host),
        })
    }

    /// Takes the key value out of a text, written plain or percent-encoded.
    fn scrub(&self, text: String) -> String {
        let Some(k) = &self.key else { return text };
        if k.value.is_empty() {
            return text;
        }
        text.replace(&k.value, HIDDEN)
            .replace(&pct_encode(&k.value), HIDDEN)
    }

    /// Takes the key value out of data an answer holds (an API that repeats the link it was
    /// called with, key and all), and says if it was there. A short value is left: it would hit
    /// ordinary text.
    fn hide(&self, text: String) -> (String, bool) {
        match &self.key {
            Some(k)
                if k.value.chars().count() >= MIN_HIDDEN
                    && (text.contains(&k.value) || text.contains(&pct_encode(&k.value))) =>
            {
                (self.scrub(text), true)
            }
            _ => (text, false),
        }
    }

    /// The host of a link that Tenun may request: a web link, and not an address on this computer
    /// unless the saved link is one itself.
    fn allowed(&self, url: &str) -> anyhow::Result<String> {
        let host = check(url)?;
        if is_loopback(&host) && !self.home_is_local {
            anyhow::bail!(
                "{} sent Tenun to an address on this computer. Tenun does not follow that.",
                self.home.0
            );
        }
        Ok(host)
    }

    /// GET with the key for the home host only. Redirects are followed here, up to 5.
    fn get(&self, first: &str) -> anyhow::Result<Reply> {
        let mut url = first.trim().to_string();
        for _ in 0..=MAX_REDIRECTS {
            let host = self.allowed(&url)?;
            let key = self
                .key
                .as_ref()
                .filter(|_| origin_of(&url).as_ref() == Some(&self.home));
            let target = match key {
                Some(k) if k.place == KeyPlace::Query => set_param(&url, &k.name, &k.value),
                _ => url.split('#').next().unwrap_or(&url).to_string(),
            };
            let mut req = self.agent.get(target.as_str());
            if let Some(k) = key
                && k.place == KeyPlace::Header
            {
                req = req.header(k.name.as_str(), k.value.as_str());
            }
            let res = req
                .call()
                .map_err(|e| anyhow::anyhow!("Could not reach {host}: {e}"))?;
            if res.status().is_redirection()
                && let Some(to) = res.headers().get("location").and_then(|v| v.to_str().ok())
            {
                url = resolve(&url, to);
                continue;
            }
            return Ok(Reply { res, host, url });
        }
        anyhow::bail!(
            "{} sent Tenun round in circles (more than 5 redirects).",
            self.home.0
        )
    }

    /// Sends the request and checks the answer. Returns what the answer says about itself, and its
    /// body, which has not been read yet. `page` is for the words of an error.
    fn open(&self, url: &str, page: usize) -> anyhow::Result<(Opened, ureq::Body)> {
        let reply = self.get(url)?;
        let status = reply.res.status();
        if !status.is_success() {
            let (code, reason) = (status.as_u16(), status.canonical_reason().unwrap_or(""));
            let tail = if page > 1 {
                format!(" That was page {page}.")
            } else {
                String::new()
            };
            if code == 401 || code == 403 {
                anyhow::bail!(
                    "{} answered {code} {reason}. The link needs a key, or the key is wrong.{tail}",
                    reply.host
                );
            }
            anyhow::bail!(
                "{} answered {code} {reason}. Check that the link is still shared.{tail}",
                reply.host
            );
        }
        let (parts, body) = reply.res.into_parts();
        let size = parts
            .headers
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let ctype = parts
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let opened = Opened {
            next: link_next(&parts.headers, &reply.url),
            url: reply.url,
            size,
            ctype,
        };
        Ok((opened, body))
    }
}

/// What an answer says about itself, before its body is read.
struct Opened {
    ctype: String,
    /// The `Content-Length`, when the server sent one.
    size: Option<u64>,
    /// The `rel="next"` link of the `Link` header.
    next: Option<String>,
    /// The link that answered, after redirects.
    url: String,
}

/// What the first bytes of an answer say it is.
enum Kind {
    Csv,
    Json { declared: bool, lines: bool },
}

/// Reads the first bytes and decides what the answer is. A web page is refused.
fn sniff(ctype: &str, reader: &mut impl Read) -> anyhow::Result<(Vec<u8>, Kind)> {
    let mut head = vec![0u8; 512];
    let n = read_up_to(reader, &mut head).map_err(|e| {
        if e.to_string().contains("limit") {
            anyhow::anyhow!("That file is too big. Tenun reads links up to 50 MB.")
        } else {
            anyhow::anyhow!("The download stopped: {e}")
        }
    })?;
    head.truncate(n);
    if ctype.starts_with("text/html") || looks_like_page(&head) {
        anyhow::bail!(NOT_DATA);
    }
    let lines = ["ndjson", "jsonl", "x-json-stream"]
        .iter()
        .any(|t| ctype.contains(t));
    let declared = lines || ctype.contains("json");
    let kind = if declared || jsonrec::looks_like_json(&head) {
        Kind::Json { declared, lines }
    } else {
        Kind::Csv
    };
    Ok((head, kind))
}

enum Cut {
    TooBig,
    Broken(String),
}

fn read_all(reader: &mut impl Read, buf: &mut Vec<u8>) -> Result<(), Cut> {
    let mut chunk = [0u8; 64 * 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.to_string().contains("limit") => return Err(Cut::TooBig),
            Err(e) => return Err(Cut::Broken(e.to_string())),
        }
    }
}

fn read_up_to(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        let k = r.read(&mut buf[n..])?;
        if k == 0 {
            break;
        }
        n += k;
    }
    Ok(n)
}

fn looks_like_page(head: &[u8]) -> bool {
    let t = String::from_utf8_lossy(head)
        .trim_start()
        .to_ascii_lowercase();
    t.starts_with("<!doctype html") || t.starts_with("<html")
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

// ---- download -----------------------------------------------------------------------------

/// Downloads the link to `dest` as CSV, and returns notes for the table (for example that the
/// link had more pages than were read). The file is written next to `dest` first and renamed, so
/// a failed download never damages the copy that is there.
pub fn download(spec: &Spec, dest: &Path) -> anyhow::Result<Vec<String>> {
    let client = Client::new(spec)?;
    download_with(&client, spec, dest).map_err(|e| anyhow::anyhow!(client.scrub(format!("{e:#}"))))
}

fn download_with(client: &Client, spec: &Spec, dest: &Path) -> anyhow::Result<Vec<String>> {
    let mut url = spec.url.trim().to_string();
    // A page number starts at the number in the link, or at 1.
    let mut number = 1u64;
    if let Some(Paging::Page { param: p }) = spec.pages {
        number = param(&url, p)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(1);
        url = set_param(&url, p, &number.to_string());
    }
    let (first, body) = client.open(&url, 1)?;
    let too_big = || anyhow::anyhow!("That file is too big. Tenun reads links up to 50 MB.");
    if first.size.is_some_and(|n| n > MAX_BYTES) {
        return Err(too_big());
    }
    let mut reader = body.into_with_config().limit(MAX_BYTES).reader();
    let (head, kind) = sniff(&first.ctype, &mut reader)?;
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = dest.with_extension("part");
    let result = (|| -> anyhow::Result<Vec<String>> {
        let mut notes = Vec::new();
        match kind {
            Kind::Csv => {
                let mut out = std::fs::File::create(&tmp)?;
                out.write_all(&head)?;
                let mut buf = [0u8; 64 * 1024];
                loop {
                    let k = reader.read(&mut buf).map_err(|e| {
                        if e.to_string().contains("limit") {
                            too_big()
                        } else {
                            anyhow::anyhow!("The download stopped: {e}")
                        }
                    })?;
                    if k == 0 {
                        break;
                    }
                    out.write_all(&buf[..k])?;
                }
                out.flush()?;
            }
            Kind::Json { declared, lines } => {
                let mut bytes = head;
                match read_all(&mut reader, &mut bytes) {
                    Ok(()) => {}
                    Err(Cut::TooBig) => return Err(too_big()),
                    Err(Cut::Broken(e)) => anyhow::bail!("The download stopped: {e}"),
                }
                match jsonrec::parse(&bytes, lines) {
                    Ok(val) => {
                        // The text is not needed any more: keep only the parsed answer in memory.
                        let size = bytes.len() as u64;
                        drop(bytes);
                        let (csv, n) =
                            collect(client, spec, &first, val, size, url.clone(), number)?;
                        std::fs::write(&tmp, csv)?;
                        notes = n;
                    }
                    // Text that only looked like JSON is read as CSV.
                    Err(_) if !declared => std::fs::write(&tmp, &bytes)?,
                    Err(e) => anyhow::bail!(
                        "That link says it gives JSON, but Tenun could not read it ({e})."
                    ),
                }
            }
        }
        Ok(notes)
    })();
    match result {
        Ok(notes) => {
            std::fs::rename(&tmp, dest)?;
            Ok(notes)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Reads the records of the first page and then the pages that follow it. Returns the CSV text and
/// the notes. Stops at: no next page, a page with no records, a link seen before, 200 pages or
/// 50 MB. The last two keep what was read and say so in a note.
fn collect(
    client: &Client,
    spec: &Spec,
    first: &Opened,
    mut val: Val,
    first_bytes: u64,
    first_url: String,
    first_number: u64,
) -> anyhow::Result<(String, Vec<String>)> {
    let mut flat = Flat::default();
    let mut notes = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(first_url);
    let mut used = first_bytes;
    let mut here = first.url.clone();
    let mut link = first.next.clone();
    let mut number = first_number;
    let mut path = String::new();
    let mut page = 1usize;
    loop {
        let records: &[Val] = if page == 1 {
            let picked = jsonrec::pick(&val, spec.records).map_err(|e| anyhow::anyhow!(e))?;
            path = picked.path;
            picked.records
        } else {
            match jsonrec::get(&val, &path) {
                Some(Val::Arr(items)) => items,
                _ => &[],
            }
        };
        if records.is_empty() {
            if page == 1 {
                anyhow::bail!(
                    "The list at {} is empty, so there is nothing to read yet.",
                    if path == "." {
                        "the top of the answer"
                    } else {
                        &path
                    }
                );
            }
            break;
        }
        flat.add(records);
        let next = match spec.pages {
            None => None,
            Some(Paging::Link) => link.clone(),
            Some(Paging::Field { path: p }) => {
                jsonrec::text_at(&val, p).map(|t| resolve(&here, &t))
            }
            Some(Paging::Cursor { path: p, param: q }) => {
                jsonrec::text_at(&val, p).map(|t| set_param(&here, q, &t))
            }
            Some(Paging::Page { param: q }) => {
                if jsonrec::total_pages(&val).is_some_and(|t| number >= t) {
                    None
                } else {
                    Some(set_param(&here, q, &(number + 1).to_string()))
                }
            }
        };
        let Some(next) = next else { break };
        let next = next.split('#').next().unwrap_or(&next).to_string();
        if !seen.insert(next.clone()) {
            break;
        }
        if page >= MAX_PAGES {
            notes.push(format!(
                "Read the first {MAX_PAGES} pages ({} rows). The link has more.",
                thousands(flat.rows())
            ));
            break;
        }
        let (mut opened, body) = client.open(&next, page + 1)?;
        let room = MAX_BYTES - used;
        let stop_at_size = |notes: &mut Vec<String>, rows: usize| {
            notes.push(format!(
                "Read the first {} ({} rows) and stopped at 50 MB. The link has more.",
                if page == 1 {
                    "page".to_string()
                } else {
                    format!("{page} pages")
                },
                thousands(rows)
            ));
        };
        if opened.size.is_some_and(|n| n > room) {
            stop_at_size(&mut notes, flat.rows());
            break;
        }
        let mut reader = body.into_with_config().limit(room).reader();
        let mut bytes = Vec::new();
        match read_all(&mut reader, &mut bytes) {
            Ok(()) => {}
            Err(Cut::TooBig) => {
                stop_at_size(&mut notes, flat.rows());
                break;
            }
            Err(Cut::Broken(e)) => anyhow::bail!("The download stopped on page {}: {e}", page + 1),
        }
        let lines = ["ndjson", "jsonl"].iter().any(|t| opened.ctype.contains(t));
        val = jsonrec::parse(&bytes, lines).map_err(|e| {
            anyhow::anyhow!("Page {} is not JSON that Tenun can read ({e}).", page + 1)
        })?;
        used += bytes.len() as u64;
        here = opened.url.clone();
        link = opened.next.take();
        number += 1;
        page += 1;
    }
    if flat.columns().is_empty() {
        anyhow::bail!("The records have no fields to read.");
    }
    if flat.left_out() > 0 {
        notes.push(format!(
            "Left out {} columns. Tenun reads up to {} columns.",
            thousands(flat.left_out()),
            thousands(jsonrec::MAX_COLUMNS)
        ));
    }
    let (csv, hidden) = client.hide(flat.to_csv());
    if hidden {
        notes.push("The key was in the data. Tenun hid it.".into());
    }
    Ok((csv, notes))
}

// ---- preview ------------------------------------------------------------------------------

/// Looks at the first page only. Nothing is written and the key is not stored.
pub fn preview(spec: &Spec) -> anyhow::Result<Preview> {
    let client = Client::new(spec)?;
    preview_with(&client, spec).map_err(|e| anyhow::anyhow!(client.scrub(format!("{e:#}"))))
}

fn preview_with(client: &Client, spec: &Spec) -> anyhow::Result<Preview> {
    let url = spec.url.trim();
    let (opened, body) = client.open(url, 1)?;
    if opened.size.is_some_and(|n| n > MAX_BYTES) {
        anyhow::bail!("That file is too big. Tenun reads links up to 50 MB.");
    }
    let mut reader = body.into_with_config().limit(MAX_BYTES).reader();
    let (head, kind) = sniff(&opened.ctype, &mut reader)?;
    let mut bytes = head;
    let csv_preview = |bytes: &[u8]| -> anyhow::Result<Preview> {
        let cut = &bytes[..bytes.len().min(PREVIEW_BYTES as usize)];
        let mut text = String::from_utf8_lossy(cut).into_owned();
        if bytes.len() > cut.len()
            && let Some(i) = text.rfind('\n')
        {
            text.truncate(i + 1);
        }
        let table = paste::parse(&text)
            .ok_or_else(|| anyhow::anyhow!("Tenun could not read a table from that link."))?;
        let p = paste::preview(&table, true, 5);
        Ok(Preview {
            format: "csv",
            lists: Vec::new(),
            records: String::new(),
            columns: p.columns,
            rows: p.rows,
            total: p.total,
            pages_hint: String::new(),
        })
    };
    match kind {
        Kind::Csv => {
            let mut chunk = vec![0u8; (PREVIEW_BYTES as usize).saturating_sub(bytes.len())];
            let n = read_up_to(&mut reader, &mut chunk).unwrap_or(0);
            bytes.extend_from_slice(&chunk[..n]);
            csv_preview(&bytes)
        }
        Kind::Json { declared, lines } => {
            match read_all(&mut reader, &mut bytes) {
                Ok(()) => {}
                Err(Cut::TooBig) => {
                    anyhow::bail!("That file is too big. Tenun reads links up to 50 MB.")
                }
                Err(Cut::Broken(e)) => anyhow::bail!("The download stopped: {e}"),
            }
            let val = match jsonrec::parse(&bytes, lines) {
                Ok(v) => v,
                Err(_) if !declared => return csv_preview(&bytes),
                Err(e) => anyhow::bail!(
                    "That link says it gives JSON, but Tenun could not read it ({e})."
                ),
            };
            let picked = jsonrec::pick(&val, spec.records).map_err(|e| anyhow::anyhow!(e))?;
            let mut flat = Flat::default();
            flat.add(picked.records);
            let shown = |t: String| client.hide(t).0;
            Ok(Preview {
                format: "json",
                lists: jsonrec::lists(&val),
                records: picked.path,
                columns: flat.columns().into_iter().map(shown).collect(),
                rows: flat
                    .head(5)
                    .into_iter()
                    .map(|r| r.into_iter().map(shown).collect())
                    .collect(),
                total: flat.rows(),
                pages_hint: jsonrec::pages_hint(
                    &val,
                    opened.next.is_some(),
                    param(url, "page").is_some(),
                ),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_links_pass() {
        assert!(check("https://docs.google.com/spreadsheets/d/e/x/pub?output=csv").is_ok());
        assert!(check("http://127.0.0.1:8080/a.csv").is_ok());
        assert!(check("http://example.com/a.csv").is_err());
        assert!(check("ftp://example.com/a.csv").is_err());
        assert!(check("not a link").is_err());
        assert!(check("/tmp/a.csv").is_err());
    }

    #[test]
    fn a_parameter_is_replaced_in_place_or_added() {
        assert_eq!(
            set_param("https://h/a?x=1&api_key=old&y=2", "api_key", "n w"),
            "https://h/a?x=1&api_key=n%20w&y=2"
        );
        assert_eq!(set_param("https://h/a", "page", "2"), "https://h/a?page=2");
        assert_eq!(
            set_param("https://h/a?x=1#top", "page", "2"),
            "https://h/a?x=1&page=2"
        );
        assert_eq!(
            param("https://h/a?x=1&page=4&page=5", "page").as_deref(),
            Some("4")
        );
        assert_eq!(param("https://h/a?x=1", "page"), None);
        assert_eq!(
            param("https://h/a?a%20b=c+d", "a b").as_deref(),
            Some("c d")
        );
    }

    #[test]
    fn links_in_an_answer_are_made_complete() {
        let base = "https://h.example/api/v1/items?page=1";
        assert_eq!(resolve(base, "https://o.example/x"), "https://o.example/x");
        assert_eq!(
            resolve(base, "/api/v2/items?page=2"),
            "https://h.example/api/v2/items?page=2"
        );
        assert_eq!(
            resolve(base, "?page=2"),
            "https://h.example/api/v1/items?page=2"
        );
        assert_eq!(
            resolve(base, "more?page=2"),
            "https://h.example/api/v1/more?page=2"
        );
        assert_eq!(resolve(base, "../v0/x"), "https://h.example/api/v0/x");
        assert_eq!(resolve(base, "//cdn.example/x"), "https://cdn.example/x");
        assert_eq!(
            resolve("http://127.0.0.1:9/a/b", "/c"),
            "http://127.0.0.1:9/c"
        );
    }

    #[test]
    fn the_link_header_gives_the_next_page() {
        let mut h = HeaderMap::new();
        h.append(
            "link",
            HeaderValue::from_static(
                "<https://h/x?page=1>; rel=\"prev\", <https://h/x?page=3>; rel=\"next\", <https://h/x?page=9>; rel=\"last\"",
            ),
        );
        assert_eq!(
            link_next(&h, "https://h/x").as_deref(),
            Some("https://h/x?page=3")
        );
        let mut h = HeaderMap::new();
        h.append("link", HeaderValue::from_static("</x?page=2>; rel=next"));
        assert_eq!(
            link_next(&h, "https://h/x").as_deref(),
            Some("https://h/x?page=2")
        );
        let mut h = HeaderMap::new();
        h.append(
            "link",
            HeaderValue::from_static("<https://h/x?page=1>; rel=\"prev\""),
        );
        assert_eq!(link_next(&h, "https://h/x"), None);
    }

    #[test]
    fn a_key_is_taken_out_of_a_text() {
        let spec = Spec {
            url: "https://h.example/a",
            records: None,
            pages: None,
            key: Some(KeyArg {
                place: KeyPlace::Query,
                name: "api_key",
                value: "s3cret key",
            }),
        };
        let c = Client::new(&spec).unwrap();
        let t = c.scrub("failed: https://h.example/a?api_key=s3cret%20key and s3cret key".into());
        assert_eq!(
            t,
            format!("failed: https://h.example/a?api_key={HIDDEN} and {HIDDEN}")
        );
    }

    fn client_for(url: &'static str, value: &'static str) -> Client {
        Client::new(&Spec {
            url,
            records: None,
            pages: None,
            key: Some(KeyArg {
                place: KeyPlace::Header,
                name: "X-Api-Key",
                value,
            }),
        })
        .unwrap()
    }

    #[test]
    fn the_key_goes_by_host_and_port_as_parsed_not_by_the_start_of_the_text() {
        let home = origin_of("https://api.example.com/v1/items").unwrap();
        assert_eq!(home, ("api.example.com".to_string(), 443));
        // The same place, written another way: the key may go.
        for same in [
            "https://API.Example.COM/other",
            "https://api.example.com:443/x?y=1",
            "https://user:pw@api.example.com/x",
        ] {
            assert_eq!(origin_of(same), Some(home.clone()), "{same}");
        }
        // Another place, whatever the link starts with: no key.
        for other in [
            "https://api.example.com.evil.com/x",
            "https://api.example.com@evil.com/x",
            "https://evil.com#@api.example.com/x",
            "https://api.example.com:8443/x",
            "http://api.example.com/x",
            "https://api.example.com./x",
            "https://xapi.example.com/x",
            "//evil.com/x",
        ] {
            assert_ne!(origin_of(other), Some(home.clone()), "{other}");
        }
        assert_eq!(
            origin_of("http://[::1]:8080/a"),
            Some(("[::1]".to_string(), 8080))
        );
        // A link that a page names is made complete first, so a host trick stays a host trick.
        assert_ne!(
            origin_of(&resolve(
                "https://api.example.com/v1/a",
                "//api.example.com.evil.com/x"
            )),
            Some(home.clone())
        );
        assert_eq!(
            origin_of(&resolve("https://api.example.com/v1/a", "/@evil.com/x")),
            Some(home)
        );
    }

    #[test]
    fn a_public_link_cannot_send_tenun_to_this_computer() {
        let public = client_for("https://api.example.com/items", "s3cret-key");
        for to in [
            "http://127.0.0.1:4700/api/workspace",
            "http://localhost:4700/",
            "https://localhost/",
            "https://LOCALHOST:8443/",
            "https://localhost./",
            "https://app.localhost/",
            "https://127.0.0.2/",
            "https://0.0.0.0/",
            "http://[::1]:4700/",
            "https://[::1]/",
            "https://[::ffff:127.0.0.1]/",
        ] {
            assert!(public.allowed(to).is_err(), "{to} must be refused");
        }
        assert!(public.allowed("https://api.example.com/page2").is_ok());
        assert!(public.allowed("https://docs.example.org/x").is_ok());
        // A link to this computer may stay on it, and may leave it.
        let local = client_for("http://127.0.0.1:4707/items", "s3cret-key");
        assert!(local.allowed("http://localhost:4707/page2").is_ok());
        assert!(local.allowed("https://api.example.com/x").is_ok());
        // Another computer on the network is not this computer.
        assert!(public.allowed("https://192.0.2.7/x").is_ok());
    }

    #[test]
    fn a_key_in_the_data_is_hidden_unless_it_is_too_short_to_tell_apart() {
        let c = client_for("https://api.example.com/items", "s3cret key");
        let (t, hit) = c.hide("a,b\nhttps://h/x?api_key=s3cret%20key&p=2,s3cret key\n".into());
        assert!(hit);
        assert_eq!(
            t,
            format!("a,b\nhttps://h/x?api_key={HIDDEN}&p=2,{HIDDEN}\n")
        );
        let (t, hit) = c.hide("nothing here\n".into());
        assert!(!hit && t == "nothing here\n");
        // A one-letter key would hit every cell with that letter.
        let short = client_for("https://api.example.com/items", "x");
        let (t, hit) = short.hide("xyz\n".into());
        assert!(!hit && t == "xyz\n");
    }

    #[test]
    fn numbers_get_commas() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(24_000), "24,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
