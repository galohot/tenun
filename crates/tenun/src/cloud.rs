//! Publishing to tenun.cloud: device sign-in, the publish API, and the
//! schedule that keeps published dashboards updated. Used by the app's server
//! and by the headless `tenun agent`.
//!
//! Push, never pull (V2-D1): the snapshot is built here, next to the data, and
//! only the finished results travel. Database passwords never leave this machine.

use flate2::Compression;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Write;
use std::time::Duration;
use tenun_core::Engine;
use tenun_core::secrets::Secrets;

pub const DEFAULT_BASE: &str = "https://tenun.cloud";

/// The service address: `TENUN_CLOUD_URL` overrides it (local development, tests).
pub fn base_url() -> String {
    std::env::var("TENUN_CLOUD_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn token_key(base: &str) -> String {
    format!("cloud-token:{base}")
}

fn email_key(base: &str) -> String {
    format!("cloud-email:{base}")
}

/// This computer's name, shown on hosted dashboards as "Updated … from <name>".
pub fn source_label() -> String {
    gethostname::gethostname()
        .to_string_lossy()
        .chars()
        .take(60)
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("Connect Tenun to tenun.cloud first.")]
    NotConnected,
    #[error("tenun.cloud no longer accepts this connection. Connect again.")]
    Unauthorized,
    #[error("The free plan allows fewer updates. Tenun will try again in {0} minutes.")]
    RateLimited(u64),
    #[error("This dashboard is larger than the free plan's {0} MB limit.")]
    TooLarge(u64),
    #[error("{0}")]
    Refused(String),
    #[error("tenun.cloud could not be reached: {0}")]
    Network(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub interval: u64,
    pub expires_in: u64,
}

pub enum Poll {
    Pending,
    Done { token: String, email: String },
    Expired,
}

pub struct Client {
    pub base: String,
    agent: ureq::Agent,
}

/// A response, read whatever its status (ureq treats 4xx as errors by default).
struct Reply {
    status: u16,
    body: Value,
    retry_after: Option<u64>,
}

impl Client {
    pub fn new(base: String) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .user_agent(format!("tenun/{}", crate::VERSION))
            .build()
            .into();
        Self { base, agent }
    }

    fn send(
        &self,
        req: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<Reply, CloudError> {
        let mut res = req.map_err(|e| CloudError::Network(e.to_string()))?;
        let status = res.status().as_u16();
        let retry_after = res
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        let text = res.body_mut().read_to_string().unwrap_or_default();
        Ok(Reply {
            status,
            body: serde_json::from_str(&text).unwrap_or(Value::Null),
            retry_after,
        })
    }

    fn check(&self, r: Reply) -> Result<Value, CloudError> {
        match r.status {
            200..=299 => Ok(r.body),
            401 => Err(CloudError::Unauthorized),
            413 => Err(CloudError::TooLarge(10)),
            429 => Err(CloudError::RateLimited(
                r.retry_after.unwrap_or(3600).div_ceil(60),
            )),
            _ => Err(CloudError::Refused(
                r.body
                    .get("message")
                    .or_else(|| r.body.get("error"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("tenun.cloud refused the request")
                    .to_string(),
            )),
        }
    }

    fn post(&self, path: &str, token: Option<&str>, body: &Value) -> Result<Reply, CloudError> {
        let mut req = self.agent.post(format!("{}/api/v1{path}", self.base));
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        self.send(
            req.header("content-type", "application/json")
                .send(body.to_string()),
        )
    }

    pub fn start_device(&self, label: &str) -> Result<DeviceStart, CloudError> {
        let v = self.check(self.post("/devices", None, &json!({ "label": label }))?)?;
        serde_json::from_value(v).map_err(|e| CloudError::Refused(e.to_string()))
    }

    pub fn poll_device(&self, device_code: &str) -> Result<Poll, CloudError> {
        let r = self.post(
            "/devices/token",
            None,
            &json!({ "device_code": device_code }),
        )?;
        if r.status == 200 {
            return Ok(Poll::Done {
                token: r.body["token"].as_str().unwrap_or_default().to_string(),
                email: r.body["email"].as_str().unwrap_or_default().to_string(),
            });
        }
        Ok(match r.body["error"].as_str() {
            Some("authorization_pending") => Poll::Pending,
            _ => Poll::Expired,
        })
    }

    /// The account: email, published dashboards and the plan's limits.
    pub fn me(&self, token: &str) -> Result<Value, CloudError> {
        let req = self
            .agent
            .get(format!("{}/api/v1/me", self.base))
            .header("authorization", format!("Bearer {token}"))
            .call();
        self.check(self.send(req)?)
    }

    pub fn create_dashboard(&self, token: &str, title: &str) -> Result<String, CloudError> {
        self.create(KIND_DASHBOARD, token, title)
    }

    /// A new dashboard or site (`kind` is the API's path: `dashboards`, `sites`).
    pub fn create(&self, kind: &str, token: &str, title: &str) -> Result<String, CloudError> {
        let v =
            self.check(self.post(&format!("/{kind}"), Some(token), &json!({ "title": title }))?)?;
        Ok(v["id"].as_str().unwrap_or_default().to_string())
    }

    pub fn patch_dashboard(&self, token: &str, id: &str, body: &Value) -> Result<(), CloudError> {
        self.patch(KIND_DASHBOARD, token, id, body)
    }

    pub fn patch(&self, kind: &str, token: &str, id: &str, body: &Value) -> Result<(), CloudError> {
        let req = self
            .agent
            .patch(format!("{}/api/v1/{kind}/{id}", self.base))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .send(body.to_string());
        self.check(self.send(req)?).map(|_| ())
    }

    pub fn delete_dashboard(&self, token: &str, id: &str) -> Result<(), CloudError> {
        self.delete(KIND_DASHBOARD, token, id)
    }

    pub fn delete(&self, kind: &str, token: &str, id: &str) -> Result<(), CloudError> {
        let req = self
            .agent
            .delete(format!("{}/api/v1/{kind}/{id}", self.base))
            .header("authorization", format!("Bearer {token}"))
            .call();
        let r = self.send(req)?;
        if r.status == 404 {
            return Ok(()); // already gone
        }
        self.check(r).map(|_| ())
    }

    /// Uploads a gzip snapshot. `Ok(None)` = the cloud dashboard no longer exists.
    pub fn put_snapshot(
        &self,
        token: &str,
        id: &str,
        gz: Vec<u8>,
        meta: &SnapMeta,
    ) -> Result<Option<Value>, CloudError> {
        self.put(KIND_DASHBOARD, token, id, gz, meta)
    }

    /// Uploads a gzip snapshot of a dashboard or a site. `Ok(None)` = it no longer exists there.
    pub fn put(
        &self,
        kind: &str,
        token: &str,
        id: &str,
        gz: Vec<u8>,
        meta: &SnapMeta,
    ) -> Result<Option<Value>, CloudError> {
        let mut req = self
            .agent
            .put(format!("{}/api/v1/{kind}/{id}/snapshot", self.base))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/gzip")
            .header("x-tenun-generated", &meta.generated)
            .header("x-tenun-source", &meta.source)
            .header("x-tenun-title", percent(&meta.title));
        if let Some(m) = meta.every_minutes {
            req = req.header("x-tenun-every", m.to_string());
        }
        if let Some(p) = meta.pages {
            req = req.header("x-tenun-pages", p.to_string());
        }
        let r = self.send(req.send(&gz[..]))?;
        if r.status == 404 {
            return Ok(None);
        }
        self.check(r).map(Some)
    }
}

/// Header values must be ASCII; titles are sent percent-encoded.
fn percent(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b" -_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub struct SnapMeta {
    pub generated: String,
    pub source: String,
    pub title: String,
    pub every_minutes: Option<u32>,
    /// A site's pages (build 14): tenun.cloud holds up to 10.
    pub pages: Option<usize>,
}

/// The API paths of what can be published.
pub const KIND_DASHBOARD: &str = "dashboards";
pub const KIND_SITE: &str = "sites";
/// The schedule's name for the site (build 14): no dashboard id looks like it.
pub const SITE_ID: &str = "\u{0}site";
/// The pages a site on tenun.cloud holds at most (the Worker's `LIMITS.sitePages`).
pub const SITE_PAGES: usize = 10;

// ---- credentials -------------------------------------------------------------

pub fn connection(secrets: &Secrets, base: &str) -> Option<(String, String)> {
    let token = secrets.get(&token_key(base))?;
    if token.is_empty() {
        return None;
    }
    Some((token, secrets.get(&email_key(base)).unwrap_or_default()))
}

pub fn save_connection(
    secrets: &Secrets,
    base: &str,
    token: &str,
    email: &str,
) -> anyhow::Result<()> {
    secrets.set(&token_key(base), token)?;
    secrets.set(&email_key(base), email)
}

pub fn forget_connection(secrets: &Secrets, base: &str) -> anyhow::Result<()> {
    secrets.set(&token_key(base), "")?;
    secrets.set(&email_key(base), "")
}

// ---- publish settings (stored on the dashboard, in the .tenun file) -----------

/// How often a dashboard is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Every {
    #[default]
    Manual,
    Change,
    Hour,
    Day,
}

impl Every {
    pub fn minutes(self) -> Option<u32> {
        match self {
            Every::Manual => None,
            Every::Change | Every::Hour => Some(60),
            Every::Day => Some(1440),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishSettings {
    /// The cloud dashboard's id; empty until the first publish.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub url: String,
    /// `link`, `password` (v5 build 11) or `invited`.
    #[serde(default = "link")]
    pub access: String,
    #[serde(default)]
    pub invites: Vec<String>,
    /// Other sites may show it in a frame (v5 build 11, V5-D14). Never sent for `invited`.
    #[serde(default)]
    pub embed: bool,
    /// The entry of the password in `Secrets` (`publish:<key>`): a random key, never the password (v5 build 11).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pw_key: String,
    /// tenun.cloud holds the password of that entry, so a publish does not send it again.
    #[serde(default)]
    pub pw_sent: bool,
    #[serde(default)]
    pub every: Every,
    /// Overrides this computer's name on the hosted page.
    #[serde(default)]
    pub source: String,
    /// When it was last published (RFC 3339).
    #[serde(default)]
    pub last: String,
    /// The last error, shown in the app; cleared by a success.
    #[serde(default)]
    pub error: String,
}

fn link() -> String {
    "link".into()
}

impl Default for PublishSettings {
    /// New dashboards are shared by link and published by hand until a schedule is chosen.
    fn default() -> Self {
        Self {
            id: String::new(),
            url: String::new(),
            access: link(),
            invites: vec![],
            embed: false,
            pw_key: String::new(),
            pw_sent: false,
            every: Every::Manual,
            source: String::new(),
            last: String::new(),
            error: String::new(),
        }
    }
}

pub fn settings(dashboard: &Value) -> Option<PublishSettings> {
    dashboard
        .get("publish")
        .and_then(|p| serde_json::from_value(p.clone()).ok())
}

// ---- password links (v5 build 11, V5-D13) --------------------------------------

/// A password has 4 to 200 characters, as tenun.cloud asks.
pub const MIN_PASSWORD: usize = 4;
pub const MAX_PASSWORD: usize = 200;

fn password_entry(key: &str) -> String {
    format!("publish:{key}")
}

/// Keeps a new password in `Secrets` and marks it to go with the next publish. The settings (and so the `.tenun` file)
/// keep only the random key of its entry.
pub fn set_password(
    secrets: &Secrets,
    ps: &mut PublishSettings,
    password: &str,
) -> Result<(), CloudError> {
    let n = password.chars().count();
    if !(MIN_PASSWORD..=MAX_PASSWORD).contains(&n) {
        return Err(CloudError::Refused(format!(
            "A password has {MIN_PASSWORD} to {MAX_PASSWORD} characters."
        )));
    }
    if ps.pw_key.is_empty() {
        ps.pw_key = crate::random_token();
    }
    secrets
        .set(&password_entry(&ps.pw_key), password)
        .map_err(|e| CloudError::Refused(e.to_string()))?;
    ps.access = "password".into();
    ps.pw_sent = false;
    Ok(())
}

/// Sets the password of a dashboard, or of the site for `SITE_ID`, and saves its settings.
pub fn set_item_password(
    engine: &Engine,
    secrets: &Secrets,
    id: &str,
    password: &str,
) -> Result<(), CloudError> {
    if id == SITE_ID {
        let mut ps = site_settings(engine).unwrap_or_default();
        set_password(secrets, &mut ps, password)?;
        save_site_settings(engine, Some(&ps));
        return Ok(());
    }
    let dash = engine
        .dashboard(id)
        .ok_or_else(|| CloudError::Refused(format!("no dashboard {id}")))?;
    let mut ps = settings(&dash).unwrap_or_default();
    set_password(secrets, &mut ps, password)?;
    engine.update_dashboard(id, |d| {
        d["publish"] = serde_json::to_value(&ps).unwrap_or(Value::Null);
    });
    Ok(())
}

/// What a publish sends with its settings: the title, who can see it, and the password only when tenun.cloud does not
/// hold it yet (`fresh`: the cloud item was made by this publish). The second value says whether the password went.
fn settings_body(
    secrets: &Secrets,
    title: &str,
    ps: &PublishSettings,
    fresh: bool,
) -> (Value, bool) {
    let mut body = json!({ "title": title, "access": ps.access, "invites": ps.invites,
        "embed": ps.embed && ps.access != "invited" });
    if ps.access == "password"
        && (fresh || !ps.pw_sent)
        && !ps.pw_key.is_empty()
        && let Some(pw) = secrets
            .get(&password_entry(&ps.pw_key))
            .filter(|p| !p.is_empty())
    {
        body["password"] = json!(pw);
        return (body, true);
    }
    (body, false)
}

/// After a publish: tenun.cloud holds the password when it was sent. Any other access drops it there, so a later
/// password access sends it again.
fn after_publish(ps: &mut PublishSettings, sent: bool) {
    if ps.access != "password" {
        ps.pw_sent = false;
    } else if sent {
        ps.pw_sent = true;
    }
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut e = GzEncoder::new(Vec::new(), Compression::new(6));
    e.write_all(bytes).expect("in-memory write");
    e.finish().expect("in-memory gzip")
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Publishes one dashboard now. Creates the cloud dashboard on first use, sends
/// the current settings, builds the snapshot here and uploads it.
pub fn publish(
    engine: &Engine,
    secrets: &Secrets,
    base: &str,
    dashboard_id: &str,
) -> Result<Value, CloudError> {
    let (token, _) = connection(secrets, base).ok_or(CloudError::NotConnected)?;
    let client = Client::new(base.to_string());
    let dash = engine
        .dashboard(dashboard_id)
        .ok_or_else(|| CloudError::Refused(format!("no dashboard {dashboard_id}")))?;
    let mut ps = settings(&dash).unwrap_or_default();
    let title = dash["title"].as_str().unwrap_or("Dashboard").to_string();
    let mut sent = false;

    let result = (|| {
        for attempt in 0..2 {
            let fresh = ps.id.is_empty();
            if fresh {
                ps.id = client.create_dashboard(&token, &title)?;
            }
            let (body, with_password) = settings_body(secrets, &title, &ps, fresh);
            let applied = client
                .patch_dashboard(&token, &ps.id, &body)
                .map(|_| true)
                .or_else(|e| match e {
                    // Deleted on the website: start again with a new cloud dashboard.
                    CloudError::Refused(ref m) if m == "not_found" && attempt == 0 => Ok(false),
                    other => Err(other),
                })?;
            sent |= applied && with_password;
            let mut snap = tenun_core::snapshot::build(engine, dashboard_id, &[])
                .map_err(|e| CloudError::Refused(e.to_string()))?;
            // The map shapes go with it, as in an exported file, so tenun.cloud draws its maps (build 14).
            crate::export::with_maps(&mut snap, &engine.geo_dir());
            let meta = SnapMeta {
                pages: None,
                generated: snap["generated"].as_str().unwrap_or_default().to_string(),
                source: if ps.source.is_empty() {
                    source_label()
                } else {
                    ps.source.clone()
                },
                title: title.clone(),
                every_minutes: ps.every.minutes(),
            };
            match client.put_snapshot(&token, &ps.id, gzip(snap.to_string().as_bytes()), &meta)? {
                Some(v) => return Ok(v),
                None if attempt == 0 => ps.id.clear(),
                None => break,
            }
        }
        Err(CloudError::Refused(
            "tenun.cloud did not accept the dashboard".into(),
        ))
    })();

    let saved = ps.clone();
    engine.update_dashboard(dashboard_id, |d| {
        let mut p = saved.clone();
        match &result {
            Ok(v) => {
                p.url = v["url"].as_str().unwrap_or_default().to_string();
                p.last = now_rfc3339();
                p.error.clear();
            }
            Err(e) => p.error = e.to_string(),
        }
        // The settings went before the upload, so they hold even when the upload failed.
        after_publish(&mut p, sent);
        d["publish"] = serde_json::to_value(p).unwrap_or(Value::Null);
    });
    result
}

/// Removes a dashboard from tenun.cloud and clears its publish settings.
pub fn unpublish(
    engine: &Engine,
    secrets: &Secrets,
    base: &str,
    dashboard_id: &str,
) -> Result<(), CloudError> {
    let dash = engine
        .dashboard(dashboard_id)
        .ok_or_else(|| CloudError::Refused(format!("no dashboard {dashboard_id}")))?;
    if let Some(ps) = settings(&dash)
        && !ps.id.is_empty()
    {
        let (token, _) = connection(secrets, base).ok_or(CloudError::NotConnected)?;
        Client::new(base.to_string()).delete_dashboard(&token, &ps.id)?;
    }
    engine.update_dashboard(dashboard_id, |d| {
        if let Some(o) = d.as_object_mut() {
            o.remove("publish");
        }
    });
    Ok(())
}

/// The site's publish settings, kept in the workspace's site settings (`site.publish`).
pub fn site_settings(engine: &Engine) -> Option<PublishSettings> {
    engine
        .site()
        .and_then(|s| s.get("publish").cloned())
        .and_then(|p| serde_json::from_value(p).ok())
}

fn save_site_settings(engine: &Engine, ps: Option<&PublishSettings>) {
    engine.update_site(|site| match ps {
        Some(p) => site["publish"] = serde_json::to_value(p).unwrap_or(Value::Null),
        None => {
            if let Some(o) = site.as_object_mut() {
                o.remove("publish");
            }
        }
    });
}

/// Publishes the whole site now (build 14): every page, the pages its drill-throughs open and the maps, in one
/// payload, as the one-file export holds it. Creates the cloud site on first use.
pub fn publish_site(engine: &Engine, secrets: &Secrets, base: &str) -> Result<Value, CloudError> {
    let (token, _) = connection(secrets, base).ok_or(CloudError::NotConnected)?;
    let client = Client::new(base.to_string());
    let mut ps = site_settings(engine).unwrap_or_default();
    let mut sent = false;
    let result = (|| {
        let mut site = tenun_core::site::build_site(engine, |_, _| {})
            .map_err(|e| CloudError::Refused(e.to_string()))?;
        let pages = site["order"].as_array().map_or(0, Vec::len);
        if pages > SITE_PAGES {
            return Err(CloudError::Refused(format!(
                "A site on tenun.cloud holds up to {SITE_PAGES} pages; this one has {pages}. Leave some pages out of the workspace, or export the site instead."
            )));
        }
        crate::export::site_with_maps(&mut site, &engine.geo_dir());
        let title = site["title"].as_str().unwrap_or("Site").to_string();
        for attempt in 0..2 {
            let fresh = ps.id.is_empty();
            if fresh {
                ps.id = client.create(KIND_SITE, &token, &title)?;
            }
            let (body, with_password) = settings_body(secrets, &title, &ps, fresh);
            let applied = client
                .patch(KIND_SITE, &token, &ps.id, &body)
                .map(|_| true)
                .or_else(|e| match e {
                    // Deleted on the website: start again with a new cloud site.
                    CloudError::Refused(ref m) if m == "not_found" && attempt == 0 => Ok(false),
                    other => Err(other),
                })?;
            sent |= applied && with_password;
            let meta = SnapMeta {
                generated: site["generated"].as_str().unwrap_or_default().to_string(),
                source: if ps.source.is_empty() {
                    source_label()
                } else {
                    ps.source.clone()
                },
                title: title.clone(),
                every_minutes: ps.every.minutes(),
                pages: Some(pages),
            };
            match client.put(
                KIND_SITE,
                &token,
                &ps.id,
                gzip(site.to_string().as_bytes()),
                &meta,
            )? {
                Some(v) => return Ok(v),
                None if attempt == 0 => ps.id.clear(),
                None => break,
            }
        }
        Err(CloudError::Refused(
            "tenun.cloud did not accept the site".into(),
        ))
    })();
    match &result {
        Ok(v) => {
            ps.url = v["url"].as_str().unwrap_or_default().to_string();
            ps.last = now_rfc3339();
            ps.error.clear();
        }
        Err(e) => ps.error = e.to_string(),
    }
    after_publish(&mut ps, sent);
    save_site_settings(engine, Some(&ps));
    result
}

/// The site's publish settings when it goes with the timed updates and `tenun publish`: it is switched on and has been
/// set to publish.
pub fn site_publishes(engine: &Engine) -> Option<PublishSettings> {
    engine.site().filter(|s| s["on"].as_bool() == Some(true))?;
    site_settings(engine)
}

/// Publishes a dashboard, or the site for `SITE_ID`.
pub fn publish_item(
    engine: &Engine,
    secrets: &Secrets,
    base: &str,
    id: &str,
) -> Result<Value, CloudError> {
    if id == SITE_ID {
        publish_site(engine, secrets, base)
    } else {
        publish(engine, secrets, base, id)
    }
}

/// How a log line names what was published.
pub fn item_name(id: &str) -> &str {
    if id == SITE_ID { "the site" } else { id }
}

/// Removes the site from tenun.cloud and forgets its publish settings.
pub fn unpublish_site(engine: &Engine, secrets: &Secrets, base: &str) -> Result<(), CloudError> {
    if let Some(ps) = site_settings(engine)
        && !ps.id.is_empty()
    {
        let (token, _) = connection(secrets, base).ok_or(CloudError::NotConnected)?;
        Client::new(base.to_string()).delete(KIND_SITE, &token, &ps.id)?;
    }
    save_site_settings(engine, None);
    Ok(())
}

// ---- the schedule -------------------------------------------------------------

/// Decides which dashboards are due. One per process (app server or agent).
#[derive(Default)]
pub struct Schedule {
    /// Dashboards whose data changed since their last publish ("on change").
    dirty: std::collections::HashSet<String>,
    /// After a failure, no new attempt before this time (back-off).
    hold_until: HashMap<String, chrono::DateTime<chrono::Utc>>,
    failures: HashMap<String, u32>,
}

impl Schedule {
    pub fn mark_changed(&mut self, engine: &Engine) {
        for d in engine.dashboards() {
            if let (Some(id), Some(ps)) = (d["id"].as_str(), settings(&d))
                && ps.every == Every::Change
            {
                self.dirty.insert(id.to_string());
            }
        }
        if site_publishes(engine).is_some_and(|p| p.every == Every::Change) {
            self.dirty.insert(SITE_ID.to_string());
        }
    }

    /// Dashboards to publish now; the site too (`SITE_ID`) when it is published on a schedule (build 14).
    pub fn due(&self, engine: &Engine, now: chrono::DateTime<chrono::Utc>) -> Vec<String> {
        let mut out = Vec::new();
        let site = site_publishes(engine).map(|p| (SITE_ID.to_string(), p));
        let items = engine
            .dashboards()
            .into_iter()
            .filter_map(|d| Some((d["id"].as_str()?.to_string(), settings(&d)?)))
            .chain(site);
        for (id, ps) in items {
            let id = id.as_str();
            if self.hold_until.get(id).is_some_and(|t| *t > now) {
                continue;
            }
            let last = chrono::DateTime::parse_from_rfc3339(&ps.last)
                .ok()
                .map(|t| t.with_timezone(&chrono::Utc));
            let since = |mins: i64| last.is_none_or(|l| now - l >= chrono::Duration::minutes(mins));
            let due = match ps.every {
                Every::Manual => false,
                Every::Hour => since(60),
                // A daily schedule may run a few minutes early so it does not drift later each day.
                Every::Day => since(1440 - 10),
                Every::Change => self.dirty.contains(id) && since(60),
            };
            if due {
                out.push(id.to_string());
            }
        }
        out
    }

    pub fn record(
        &mut self,
        id: &str,
        ok: bool,
        err: Option<&CloudError>,
        now: chrono::DateTime<chrono::Utc>,
    ) {
        if ok {
            self.dirty.remove(id);
            self.failures.remove(id);
            self.hold_until.remove(id);
            return;
        }
        let n = self.failures.entry(id.to_string()).or_insert(0);
        *n += 1;
        // 5, 10, 20, 40 … minutes, at most 6 hours; a rate limit says exactly how long.
        let wait = match err {
            Some(CloudError::RateLimited(m)) => *m as i64,
            Some(CloudError::NotConnected | CloudError::Unauthorized) => 360,
            _ => (5_i64 << (*n - 1).min(7)).min(360),
        };
        self.hold_until
            .insert(id.to_string(), now + chrono::Duration::minutes(wait));
    }
}

/// Runs everything that is due, once. Returns (published, failed) dashboard ids.
pub fn run_due(
    engine: &Engine,
    secrets: &Secrets,
    base: &str,
    schedule: &mut Schedule,
) -> (Vec<String>, Vec<(String, String)>) {
    let now = chrono::Utc::now();
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    for id in schedule.due(engine, now) {
        match publish_item(engine, secrets, base, &id) {
            Ok(_) => {
                schedule.record(&id, true, None, now);
                ok.push(id);
            }
            Err(e) => {
                schedule.record(&id, false, Some(&e), now);
                failed.push((id, e.to_string()));
            }
        }
    }
    (ok, failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encodes_titles() {
        assert_eq!(percent("Kopi & Teh"), "Kopi %26 Teh");
        assert_eq!(percent("Ringkasan Penjualan"), "Ringkasan Penjualan");
        assert_eq!(percent("Café"), "Caf%C3%A9");
    }

    #[test]
    fn new_dashboards_are_shared_by_link() {
        assert_eq!(PublishSettings::default().access, "link");
        let parsed: PublishSettings = serde_json::from_value(json!({})).unwrap();
        assert_eq!(parsed.access, "link");
    }

    #[test]
    fn every_minutes() {
        assert_eq!(Every::Manual.minutes(), None);
        assert_eq!(Every::Day.minutes(), Some(1440));
    }
}
