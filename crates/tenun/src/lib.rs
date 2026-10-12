//! Tenun's local HTTP server: the JSON API, live events and the embedded UI.
//! The desktop app and `tenun serve` both run this same server.

mod api;
pub mod cloud;
mod export;
mod fsbrowse;
mod guard;
pub mod sample;
pub mod settings;
pub mod xlsx;

use axum::Router;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tenun_core::Engine;
use tenun_core::secrets::Secrets;
use tenun_core::watch::FileWatcher;
use tokio::sync::broadcast;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A self-contained HTML file for a snapshot (viewer bundle + data).
pub fn export_html(payload: &serde_json::Value) -> String {
    export::html(payload)
}

/// Puts the map shapes a snapshot's map tiles draw into it, so its exported file works offline (build 06).
pub fn add_maps(payload: &mut serde_json::Value, geo_dir: &std::path::Path) {
    export::with_maps(payload, geo_dir)
}

pub struct Options {
    pub host: IpAddr,
    /// 0 picks a free port.
    pub port: u16,
    pub workspace: Option<PathBuf>,
    /// Native file dialogs, when a desktop shell provides them.
    pub picker: Option<Picker>,
}

/// What the UI asks a native dialog for.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PickRequest {
    DataFiles,
    /// A folder of data files.
    Folder,
    /// A PNG, JPEG or WebP picture for an image tile.
    Image,
    OpenWorkspace,
    SaveWorkspace {
        name: String,
    },
    SaveHtml {
        name: String,
    },
    SaveXlsx {
        name: String,
    },
}

/// Shows a native dialog and returns the chosen paths (empty = cancelled).
pub type Picker = Arc<dyn Fn(PickRequest) -> Vec<PathBuf> + Send + Sync>;

pub struct AppState {
    pub engine: Engine,
    pub events: broadcast::Sender<String>,
    watcher: Mutex<Option<FileWatcher>>,
    /// Required when listening beyond this computer.
    pub token: Option<String>,
    pub picker: Option<Picker>,
    /// This server's own URL, for "Open in browser".
    pub url: Mutex<String>,
    pub secrets: Secrets,
    /// Where this computer's settings are kept.
    pub settings_path: PathBuf,
    pub cloud_base: String,
    pub schedule: Mutex<cloud::Schedule>,
}

pub struct Running {
    pub addr: SocketAddr,
    /// The URL to open, including the access token in LAN mode.
    pub url: String,
    pub state: Arc<AppState>,
}

impl AppState {
    /// Points the watcher at the current file and folder sources.
    pub fn rewatch(&self) {
        let files: Vec<PathBuf> = self
            .engine
            .watched_files()
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        let folders: Vec<PathBuf> = self
            .engine
            .watched_folders()
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        if let Some(w) = self.watcher.lock().unwrap().as_mut() {
            w.set(&files, &folders);
        }
    }

    pub fn emit(&self, value: serde_json::Value) {
        let _ = self.events.send(value.to_string());
    }
}

fn random_token() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("system randomness");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Starts the server in the background and returns once it listens.
pub async fn start(opts: Options) -> anyhow::Result<Running> {
    let engine = Engine::new(Secrets::new(Secrets::default_path()))?;
    if let Some(p) = &opts.workspace {
        if p.exists() {
            engine.open(p)?;
        } else {
            // A new file: start empty and save there on the first change.
            engine.new_workspace();
            engine.save(Some(p.clone()))?;
        }
    }
    let token = (!opts.host.is_loopback()).then(random_token);
    let (events, _) = broadcast::channel(64);
    let state = Arc::new(AppState {
        engine,
        events,
        watcher: Mutex::new(None),
        token: token.clone(),
        picker: opts.picker.clone(),
        url: Mutex::new(String::new()),
        secrets: Secrets::new(Secrets::default_path()),
        settings_path: settings::Settings::default_path(),
        cloud_base: cloud::base_url(),
        schedule: Mutex::new(cloud::Schedule::default()),
    });

    // A check of live database links finishes after the load that began it: tell the windows.
    {
        let events = state.events.clone();
        state.engine.on_links_checked(move || {
            let _ = events.send(serde_json::json!({ "type": "workspace-changed" }).to_string());
        });
    }

    // File changes reload the source and tell every open window.
    let weak = Arc::downgrade(&state);
    let handle = tokio::runtime::Handle::current();
    let watcher = FileWatcher::new(move |paths| {
        let Some(state) = weak.upgrade() else { return };
        handle.spawn_blocking(move || {
            let changed = state.engine.sources_for(&paths);
            if changed.is_empty() {
                return;
            }
            let mut tables = Vec::new();
            for id in &changed {
                tables.extend(state.engine.load(id));
            }
            // A folder that was missing may be back, and a new one needs watching too.
            state.rewatch();
            state.schedule.lock().unwrap().mark_changed(&state.engine);
            state.emit(serde_json::json!({
                "type": "data-changed", "sources": changed, "tables": tables
            }));
        });
    })?;
    *state.watcher.lock().unwrap() = Some(watcher);
    state.rewatch();

    // Published dashboards that keep themselves updated: checked once a minute.
    let sched = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            tick.tick().await;
            let st = sched.clone();
            let _ = tokio::task::spawn_blocking(move || {
                // Calculated columns with `TODAY()` are built again when the day has changed.
                if st.engine.refresh_dates() {
                    st.schedule.lock().unwrap().mark_changed(&st.engine);
                    st.emit(serde_json::json!({ "type": "data-changed" }));
                }
                // CSV links whose time has come are fetched again.
                let urls = st.engine.refresh_due_urls();
                if !urls.is_empty() {
                    let tables: Vec<String> = st
                        .engine
                        .catalog()
                        .tables
                        .iter()
                        .filter(|t| urls.contains(&t.source_id))
                        .map(|t| t.id.clone())
                        .collect();
                    st.schedule.lock().unwrap().mark_changed(&st.engine);
                    st.emit(serde_json::json!({
                        "type": "data-changed", "sources": urls, "tables": tables
                    }));
                }
                let mut schedule = st.schedule.lock().unwrap();
                let (ok, failed) =
                    cloud::run_due(&st.engine, &st.secrets, &st.cloud_base, &mut schedule);
                if !ok.is_empty() || !failed.is_empty() {
                    st.emit(serde_json::json!({ "type": "published", "ok": ok, "failed": failed }));
                }
            })
            .await;
        }
    });

    let app = router(state.clone());
    let listener = tokio::net::TcpListener::bind((opts.host, opts.port)).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("server stopped: {e}");
        }
    });
    let shown_host = if opts.host.is_loopback() || opts.host.is_unspecified() {
        "127.0.0.1".to_string()
    } else {
        opts.host.to_string()
    };
    let mut url = format!("http://{shown_host}:{}/", addr.port());
    if let Some(t) = &token {
        url.push_str(&format!("?token={t}"));
    }
    *state.url.lock().unwrap() = url.clone();
    Ok(Running { addr, url, state })
}

/// The biggest request body any route takes. Image tiles hold up to 1 MB each as a data URL.
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api", api::routes())
        .fallback(api::static_file)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            guard::guard,
        ))
        .layer(tower_http::compression::CompressionLayer::new())
        // A dashboard can hold several pictures as data URLs. The default 2 MB would refuse the save.
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}
