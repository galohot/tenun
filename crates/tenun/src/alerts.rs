//! Alerts (v5 build 12, V5-D15): a number tile's value against a threshold, told to Slack, Microsoft Teams, Discord,
//! Telegram or any web address. They run where the data is: the app after each reload and once a minute, and
//! `tenun agent` on each cycle. tenun.cloud never holds an address or a value.
//!
//! The value is the tile's value as its page opens: the dashboard's saved filters, its filter tiles, its default view.
//! Never a reader's clicks. One message when a rule starts to hold; one more when it stops, if the rule asks. A send
//! that fails is tried again at the next cycle, three tries in all, then the rule shows the error.
//!
//! The address of each rule (a webhook, or a Telegram bot token and chat) is kept in this computer's `Secrets` under
//! `alert:<rule id>`, never in the `.tenun` file. The state of each rule is kept beside it in `alerts.json`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tenun_core::Engine;
use tenun_core::numtext::{self, NumFormat};
use tenun_core::secrets::Secrets;
use tenun_core::workspace::{AlertOp, AlertRule, ChannelKind, Locale};

/// A message is tried this many times, once a cycle, before the rule shows the error and stops trying.
pub const TRIES: u32 = 3;

/// Telegram's address; `TENUN_TELEGRAM_URL` replaces it (tests).
pub fn telegram_base() -> String {
    std::env::var("TENUN_TELEGRAM_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.telegram.org".into())
        .trim_end_matches('/')
        .to_string()
}

// ---- where a rule sends to ------------------------------------------------------

/// What a rule sends to, kept in `Secrets`: a web address, or a Telegram bot token and chat.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Address {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chat: String,
}

pub fn secret_key(rule_id: &str) -> String {
    format!("alert:{rule_id}")
}

/// The address of a rule on this computer, if it has one.
pub fn address(secrets: &Secrets, rule_id: &str) -> Option<Address> {
    secrets
        .get(&secret_key(rule_id))
        .and_then(|s| serde_json::from_str::<Address>(&s).ok())
        .filter(|a| !a.url.is_empty() || !a.token.is_empty())
}

/// Checks an address for a channel and keeps it. An empty one is refused in words.
pub fn set_address(
    secrets: &Secrets,
    rule_id: &str,
    kind: ChannelKind,
    a: &Address,
) -> anyhow::Result<()> {
    let a = Address {
        url: a.url.trim().to_string(),
        token: a.token.trim().to_string(),
        chat: a.chat.trim().to_string(),
    };
    match kind {
        ChannelKind::Telegram => {
            // A bot token is "<digits>:<letters, digits, - and _>".
            let ok = a.token.split_once(':').is_some_and(|(n, k)| {
                !n.is_empty()
                    && n.bytes().all(|b| b.is_ascii_digit())
                    && k.len() >= 20
                    && k.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            });
            if !ok {
                anyhow::bail!(
                    "That is not a Telegram bot token. It looks like 123456789:AAE… — ask @BotFather."
                );
            }
            if a.chat.is_empty() || a.chat.len() > 64 || a.chat.contains(char::is_whitespace) {
                anyhow::bail!(
                    "Type the chat id: a number such as -1001234567890, or @channelname."
                );
            }
        }
        _ => {
            let lower = a.url.to_ascii_lowercase();
            if !(lower.starts_with("https://") || lower.starts_with("http://"))
                || a.url.len() > 2000
            {
                anyhow::bail!(
                    "Paste the web address that {} gave you. It starts with https://.",
                    kind_name(kind)
                );
            }
        }
    }
    secrets.set(&secret_key(rule_id), &serde_json::to_string(&a)?)
}

pub fn kind_name(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Slack => "Slack",
        ChannelKind::Teams => "Microsoft Teams",
        ChannelKind::Discord => "Discord",
        ChannelKind::Telegram => "Telegram",
        ChannelKind::Webhook => "the web address",
    }
}

// ---- reading a rule ---------------------------------------------------------------

/// One reading of a rule's value.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub value: f64,
    /// "Oct 1–14, 2026, so far"; empty for a value over all the data.
    pub period: String,
    /// What the message calls the value: the tile's title, or the name the app gave it.
    pub name: String,
    pub dashboard: String,
    /// The dashboard's tenun.cloud link when it is published.
    pub link: Option<String>,
    pub locale: Locale,
    pub format: NumFormat,
}

/// The format a tile writes its value `i` in: its named measure's, with the tile's own over it, field by field.
fn format_of(engine: &Engine, tile: &Value, i: usize) -> NumFormat {
    let mut f = serde_json::Map::new();
    if let Some(named) = tile
        .pointer(&format!("/measures/{i}/named"))
        .and_then(Value::as_str)
        && let Some(base) = engine
            .measures()
            .iter()
            .find(|m| m.id == named)
            .and_then(|m| m.format.clone())
            .and_then(|v| v.as_object().cloned())
    {
        f.extend(base);
    }
    if let Some(own) = tile
        .pointer(&format!("/formats/{i}"))
        .and_then(Value::as_object)
    {
        f.extend(own.clone());
    }
    serde_json::from_value(Value::Object(f)).unwrap_or_default()
}

/// Reads a rule's value now. `Ok(None)`: the tile has no value (no data in its period).
pub fn read(engine: &Engine, rule: &AlertRule) -> anyhow::Result<Option<Reading>> {
    let dash = engine
        .dashboard(&rule.dashboard)
        .ok_or_else(|| anyhow::anyhow!("Its dashboard was removed."))?;
    let raw = dash
        .get("tiles")
        .and_then(Value::as_array)
        .and_then(|ts| ts.iter().find(|t| t["id"] == rule.tile.as_str()))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Its tile was removed."))?;
    let res = tenun_core::snapshot::tile_as_opened(engine, &rule.dashboard, &rule.tile)?;
    let kpi = res
        .kpi
        .ok_or_else(|| anyhow::anyhow!("Its tile is no longer a number tile."))?;
    let i = rule.measure.unwrap_or(0);
    let value = if i == 0 {
        kpi.value
    } else {
        kpi.extras.get(i - 1).copied().flatten()
    };
    let Some(value) = value.filter(|v| v.is_finite()) else {
        return Ok(None);
    };
    let title = raw["title"].as_str().unwrap_or_default().trim().to_string();
    let name = if !title.is_empty() && i == 0 {
        title
    } else if !rule.name.trim().is_empty() {
        rule.name.trim().to_string()
    } else {
        "The value".into()
    };
    // A value over all the data has no period to name.
    let mut period = if kpi.period == "All data" {
        String::new()
    } else {
        kpi.period.clone()
    };
    if kpi.partial && !period.is_empty() {
        period.push_str(", so far");
    }
    let link = crate::cloud::settings(&dash)
        .map(|p| p.url)
        .filter(|u| !u.is_empty());
    Ok(Some(Reading {
        value,
        period,
        name,
        dashboard: dash["title"].as_str().unwrap_or("Untitled").to_string(),
        link,
        locale: engine.locale(),
        format: format_of(engine, &raw, i),
    }))
}

/// Whether the rule holds for a value.
pub fn holds(rule: &AlertRule, v: f64) -> bool {
    match rule.op {
        AlertOp::Below => v < rule.value,
        AlertOp::Above => v > rule.value,
        AlertOp::Outside => v < rule.value || v > rule.to.unwrap_or(rule.value),
    }
}

/// What a message is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Event {
    /// The rule started to hold.
    Start,
    /// It no longer holds.
    Back,
    /// "Send a test" in the app.
    Test,
}

/// The message in words: one sentence, the dashboard, and its link when it is published.
pub fn message(event: Event, rule: &AlertRule, r: &Reading) -> String {
    let n = |v: f64| numtext::short(v, r.locale, &r.format);
    let what = if r.period.is_empty() {
        r.name.clone()
    } else {
        format!("{} ({})", r.name, r.period)
    };
    let range = || format!("{} to {}", n(rule.value), n(rule.to.unwrap_or(rule.value)));
    let line = match (event, rule.op) {
        (Event::Test, _) => format!(
            "Test from Tenun: alerts for {} will come here. It is {} now.",
            r.name,
            n(r.value)
        ),
        (Event::Start, AlertOp::Below) => {
            format!("{what} is {}, below {}.", n(r.value), n(rule.value))
        }
        (Event::Start, AlertOp::Above) => {
            format!("{what} is {}, above {}.", n(r.value), n(rule.value))
        }
        (Event::Start, AlertOp::Outside) => {
            format!("{what} is {}, outside {}.", n(r.value), range())
        }
        (Event::Back, AlertOp::Below) => {
            format!(
                "{what} is {}, no longer below {}.",
                n(r.value),
                n(rule.value)
            )
        }
        (Event::Back, AlertOp::Above) => {
            format!(
                "{what} is {}, no longer above {}.",
                n(r.value),
                n(rule.value)
            )
        }
        (Event::Back, AlertOp::Outside) => {
            format!("{what} is {}, back within {}.", n(r.value), range())
        }
    };
    let mut text = format!("{line} — {}", r.dashboard);
    if let Some(l) = &r.link {
        text.push('\n');
        text.push_str(l);
    }
    text
}

/// What a generic web address receives: the message and its parts.
pub fn webhook_body(event: Event, rule: &AlertRule, r: &Reading, at: &str) -> Value {
    json!({
        "rule": rule.id,
        "event": event,
        "name": r.name,
        "value": r.value,
        "op": rule.op,
        "threshold": rule.value,
        "to": rule.to,
        "period": r.period,
        "dashboard": r.dashboard,
        "link": r.link,
        "at": at,
        "text": message(event, rule, r),
    })
}

// ---- sending ----------------------------------------------------------------------

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .user_agent(format!("tenun/{}", crate::VERSION))
        .build()
        .into()
}

/// The host of an address, for an error: the rest of a webhook address is its secret.
fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// What a Teams Workflows webhook ("Post to a channel when a webhook request is received") reads: a message with one
/// Adaptive Card that holds the text. The template posts each card of `attachments`; a bare `text` is not read.
pub fn teams_body(text: &str) -> Value {
    json!({
        "type": "message",
        "attachments": [{
            "contentType": "application/vnd.microsoft.card.adaptive",
            "contentUrl": null,
            "content": {
                "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
                "type": "AdaptiveCard",
                "version": "1.4",
                "body": [{ "type": "TextBlock", "text": text, "wrap": true }]
            }
        }]
    })
}

/// Sends one message. An error says what went wrong in words and never holds the address or a token.
pub fn send(
    kind: ChannelKind,
    a: &Address,
    text: &str,
    webhook: &Value,
    telegram: &str,
) -> Result<(), String> {
    let (url, body, who) = match kind {
        ChannelKind::Slack => (a.url.clone(), json!({ "text": text }), host_of(&a.url)),
        ChannelKind::Teams => (a.url.clone(), teams_body(text), host_of(&a.url)),
        ChannelKind::Discord => (a.url.clone(), json!({ "content": text }), host_of(&a.url)),
        ChannelKind::Webhook => (a.url.clone(), webhook.clone(), host_of(&a.url)),
        ChannelKind::Telegram => (
            format!("{telegram}/bot{}/sendMessage", a.token),
            json!({ "chat_id": a.chat, "text": text, "disable_web_page_preview": true }),
            "Telegram".to_string(),
        ),
    };
    let res = agent()
        .post(&url)
        .header("content-type", "application/json")
        .send(body.to_string());
    match res {
        Ok(r) if r.status().is_success() => Ok(()),
        Ok(r) => Err(format!("{who} answered {}.", r.status().as_u16())),
        Err(_) => Err(format!("{who} could not be reached.")),
    }
}

// ---- the state on this computer ---------------------------------------------------

/// The state of one rule on this computer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RuleState {
    /// Since when the rule holds (RFC 3339). None while it does not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holding_since: Option<String>,
    /// The message that it started to hold went out, so a "back" message may follow.
    #[serde(default)]
    pub told: bool,
    /// A message still to send, and how many tries it had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<Event>,
    #[serde(default)]
    pub tries: u32,
    /// The last error of a send, shown on the rule. A send that works, or a new start, clears it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    /// Why the value cannot be read now (its tile was removed…). A good reading clears it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub problem: String,
    /// The last value read, its period, and when.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub period: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub checked: String,
}

/// `alerts.json`, beside the secrets: rule id → state. Rules of every workspace this computer runs.
pub fn state_path(secrets: &Secrets) -> PathBuf {
    secrets.path().with_file_name("alerts.json")
}

pub fn load_states(path: &Path) -> BTreeMap<String, RuleState> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_states(path: &Path, states: &BTreeMap<String, RuleState>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    if let Ok(text) = serde_json::to_string_pretty(states)
        && std::fs::write(&tmp, text).is_ok()
    {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// What one check did.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outcome {
    /// (rule id, the message that went).
    pub sent: Vec<(String, String)>,
    /// (rule id, the error).
    pub failed: Vec<(String, String)>,
}

/// A state is forgotten when its rule was not checked for this long (a rule of a workspace no longer used).
const FORGET_DAYS: i64 = 90;

/// Checks every rule of the open workspace that is on, once, and sends what is due.
pub fn check(engine: &Engine, secrets: &Secrets, now: chrono::DateTime<chrono::Utc>) -> Outcome {
    check_with(engine, secrets, &state_path(secrets), now, &telegram_base())
}

pub fn check_with(
    engine: &Engine,
    secrets: &Secrets,
    path: &Path,
    now: chrono::DateTime<chrono::Utc>,
    telegram: &str,
) -> Outcome {
    let rules = engine.alerts();
    // A workspace with no alert reads and writes nothing.
    if rules.is_empty() {
        return Outcome::default();
    }
    let at = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut states = load_states(path);
    let mut out = Outcome::default();
    for rule in rules {
        if !rule.on {
            // A rule switched off starts again from nothing when it is switched on.
            states.remove(&rule.id);
            continue;
        }
        let st = states.entry(rule.id.clone()).or_default();
        st.checked = at.clone();
        let reading = match read(engine, &rule) {
            Ok(Some(r)) => r,
            Ok(None) => {
                // No value (no data in the period): nothing changes.
                st.problem.clear();
                st.value = None;
                continue;
            }
            Err(e) => {
                st.problem = e.to_string();
                continue;
            }
        };
        st.problem.clear();
        st.value = Some(reading.value);
        st.period = reading.period.clone();
        let now_holds = holds(&rule, reading.value);
        match (now_holds, st.holding_since.is_some()) {
            (true, false) => {
                st.holding_since = Some(at.clone());
                st.told = false;
                st.pending = Some(Event::Start);
                st.tries = 0;
                st.error.clear();
            }
            (false, true) => {
                st.holding_since = None;
                // "Back to normal" follows only a message that went.
                st.pending = (rule.back && st.told).then_some(Event::Back);
                st.told = false;
                st.tries = 0;
            }
            _ => {}
        }
        let Some(event) = st.pending else { continue };
        let Some(addr) = address(secrets, &rule.id) else {
            // The address is on another computer; this one keeps the message until it has one.
            st.error =
                "This computer has no address for this alert. Set it in the app on this computer."
                    .into();
            continue;
        };
        let text = message(event, &rule, &reading);
        match send(
            rule.channel.kind,
            &addr,
            &text,
            &webhook_body(event, &rule, &reading, &at),
            telegram,
        ) {
            Ok(()) => {
                st.pending = None;
                st.tries = 0;
                st.error.clear();
                st.told |= event == Event::Start;
                out.sent.push((rule.id.clone(), text));
            }
            Err(e) => {
                st.tries += 1;
                if st.tries >= TRIES {
                    st.pending = None;
                    st.error = format!("{e} Tried {TRIES} times; Tenun stopped trying.");
                } else {
                    st.error = e.clone();
                }
                out.failed.push((rule.id.clone(), e));
            }
        }
    }
    states.retain(|_, s| {
        chrono::DateTime::parse_from_rfc3339(&s.checked)
            .is_ok_and(|t| (now - t.with_timezone(&chrono::Utc)).num_days() < FORGET_DAYS)
    });
    save_states(path, &states);
    out
}

/// "Send a test": a message to the rule's address now, with the value it reads.
pub fn test(engine: &Engine, secrets: &Secrets, rule_id: &str) -> anyhow::Result<String> {
    let rule = engine
        .alerts()
        .into_iter()
        .find(|r| r.id == rule_id)
        .ok_or_else(|| anyhow::anyhow!("Save the alert first."))?;
    let addr =
        address(secrets, rule_id).ok_or_else(|| anyhow::anyhow!("Set where to send it first."))?;
    let reading = read(engine, &rule)?.ok_or_else(|| {
        anyhow::anyhow!("The tile has no value now, so there is nothing to send.")
    })?;
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let text = message(Event::Test, &rule, &reading);
    send(
        rule.channel.kind,
        &addr,
        &text,
        &webhook_body(Event::Test, &rule, &reading, &at),
        &telegram_base(),
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    Ok(text)
}
