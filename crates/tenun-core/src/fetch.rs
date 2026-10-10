//! Fetches a CSV link into a cache file. The link is https only (a local address may use http,
//! so a test server and a file served on this computer work). No cookies are kept or sent, and a
//! download over 50 MB is refused.

use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

/// The biggest download: 50 MB.
pub const MAX_BYTES: u64 = 50 * 1024 * 1024;

/// The host of a link, or an error that says what is wrong in words.
pub fn check(url: &str) -> anyhow::Result<String> {
    let url = url.trim();
    let uri: ureq::http::Uri = url
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

/// Downloads `url` to `dest`. The file is written next to `dest` first and renamed, so a failed
/// download never damages the copy that is there.
pub fn download(url: &str, dest: &Path) -> anyhow::Result<()> {
    let url = url.trim();
    let host = check(url)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(!is_local(&host))
        .max_redirects(5)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(120)))
        .http_status_as_error(false)
        .user_agent(format!("tenun/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut res = agent
        .get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("Could not reach {host}: {e}"))?;
    let status = res.status();
    if !status.is_success() {
        anyhow::bail!(
            "{host} answered {} {}. Check that the link is still shared.",
            status.as_u16(),
            status.canonical_reason().unwrap_or("")
        );
    }
    let too_big = || anyhow::anyhow!("That file is too big. Tenun reads links up to 50 MB.");
    if res
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|n| n > MAX_BYTES)
    {
        return Err(too_big());
    }
    let is_page = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.to_ascii_lowercase().starts_with("text/html"));
    let mut reader = res.body_mut().with_config().limit(MAX_BYTES).reader();
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = dest.with_extension("part");
    let result = (|| -> anyhow::Result<()> {
        let mut out = std::fs::File::create(&tmp)?;
        let mut head = [0u8; 512];
        let n = read_up_to(&mut reader, &mut head).map_err(|_| too_big())?;
        if is_page || looks_like_page(&head[..n]) {
            anyhow::bail!(
                "That link gives a web page, not CSV data. In Google Sheets, publish the sheet as CSV and use that link."
            );
        }
        out.write_all(&head[..n])?;
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
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
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
}
