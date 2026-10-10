//! JSON routes under `/api`, the event socket, and the embedded UI files.

use crate::settings::Settings;
use crate::{AppState, VERSION, cloud, export, fsbrowse, sample};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use tenun_core::engine::AddSource;
use tenun_core::query::SemanticQuery;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile};
use tenun_core::workspace::{FieldRef, Locale};

type S = State<Arc<AppState>>;

pub struct ApiError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError(e.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("{:#}", self.0) })),
        )
            .into_response()
    }
}

type R<T> = Result<T, ApiError>;

/// Runs blocking engine work off the async threads.
async fn blocking<T: Send + 'static>(
    state: &Arc<AppState>,
    f: impl FnOnce(&AppState) -> anyhow::Result<T> + Send + 'static,
) -> R<T> {
    let st = state.clone();
    Ok(tokio::task::spawn_blocking(move || f(&st)).await??)
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health))
        .route("/workspace", get(workspace))
        .route("/workspace/new", post(ws_new))
        .route("/workspace/open", post(ws_open))
        .route("/workspace/save", post(ws_save))
        .route("/workspace/rename", post(ws_rename))
        .route("/workspace/locale", put(ws_locale))
        .route("/sample", post(open_sample))
        .route("/sources", post(add_source))
        .route("/sources/{id}", delete(remove_source))
        .route("/sources/{id}/reload", post(reload_source))
        .route("/tables/preview", get(preview))
        .route("/relationships", post(add_rel))
        .route("/relationships/{id}", delete(remove_rel))
        .route("/relationships/suggest", get(suggest))
        .route("/dashboards", put(put_dashboards))
        .route("/layout", put(put_layout))
        .route("/query", post(query))
        .route("/fs", get(fs))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/viewer", get(viewer))
        .route("/tile", post(tile))
        .route("/cloud", get(cloud_status))
        .route("/cloud/connect", post(cloud_connect))
        .route("/cloud/disconnect", post(cloud_disconnect))
        .route("/cloud/publish", post(cloud_publish))
        .route("/cloud/unpublish", post(cloud_unpublish))
        .route("/snapshot", post(snapshot))
        .route("/export-html", post(export_html))
        .route("/export-file", post(export_file))
        .route("/events", get(events))
        .route("/pick", post(pick))
        .route("/image/read", post(read_image))
        .route("/open-browser", post(open_browser))
}

async fn health(State(s): S) -> Json<Value> {
    Json(json!({
        "ok": true,
        "version": VERSION,
        "duckdb": s.engine.duckdb_version(),
        "native": s.picker.is_some(),
    }))
}

/// Native dialogs (desktop app only). Browser mode uses the built-in browser.
async fn pick(State(s): S, Json(req): Json<crate::PickRequest>) -> R<Json<Value>> {
    let picker = s
        .picker
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no native dialogs in browser mode"))?;
    let paths = tokio::task::spawn_blocking(move || picker(req)).await?;
    Ok(Json(json!({
        "paths": paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
struct ImagePath {
    path: String,
}

/// The bytes of a picture the native dialog chose, for an image tile. PNG, JPEG or WebP, 1 MB at most.
async fn read_image(Json(b): Json<ImagePath>) -> R<Response> {
    let (kind, bytes) =
        tokio::task::spawn_blocking(move || fsbrowse::read_image(&b.path)).await??;
    Ok(([(header::CONTENT_TYPE, kind)], bytes).into_response())
}

async fn open_browser(State(s): S) -> R<Json<Value>> {
    let url = s.url.lock().unwrap().clone();
    webbrowser::open(&url)?;
    Ok(Json(json!({ "ok": true })))
}

/// This computer's settings. Read from disk each time, so every window agrees.
async fn get_settings(State(s): S) -> Json<Settings> {
    Json(Settings::load(&s.settings_path))
}

async fn put_settings(State(s): S, Json(b): Json<Settings>) -> R<Json<Settings>> {
    b.save(&s.settings_path)?;
    Ok(Json(b))
}

async fn workspace(State(s): S) -> R<Json<Value>> {
    let snap = blocking(&s, |st| Ok(st.engine.snapshot())).await?;
    Ok(Json(serde_json::to_value(snap)?))
}

/// After any change to the source list: re-watch files, tell other windows,
/// and return the new snapshot.
fn changed(st: &AppState) -> anyhow::Result<Value> {
    st.rewatch();
    st.emit(json!({ "type": "workspace-changed" }));
    Ok(serde_json::to_value(st.engine.snapshot())?)
}

#[derive(Deserialize, Default)]
struct NewBody {
    /// The language of the browser ("id-ID"). Without it the computer's language is used.
    #[serde(default)]
    lang: Option<String>,
}

async fn ws_new(State(s): S, body: Option<Json<NewBody>>) -> R<Json<Value>> {
    let lang = body.and_then(|Json(b)| b.lang);
    Ok(Json(
        blocking(&s, move |st| {
            match lang {
                Some(l) => st.engine.new_workspace_in(Locale::guess(&l)),
                None => st.engine.new_workspace(),
            }
            changed(st)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct PathBody {
    path: Option<String>,
}

async fn ws_open(State(s): S, Json(b): Json<PathBody>) -> R<Json<Value>> {
    let path = b
        .path
        .ok_or_else(|| anyhow::anyhow!("choose a .tenun file"))?;
    Ok(Json(
        blocking(&s, move |st| {
            st.engine.open(&PathBuf::from(path))?;
            changed(st)
        })
        .await?,
    ))
}

async fn ws_save(State(s): S, Json(b): Json<PathBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            let p = b.path.map(|p| {
                let mut p = PathBuf::from(p);
                if p.extension().is_none_or(|e| e != "tenun") {
                    p.set_extension("tenun");
                }
                p
            });
            st.engine.save(p)?;
            // Relative paths may have moved with the file.
            st.rewatch();
            Ok(serde_json::to_value(st.engine.snapshot())?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct NameBody {
    name: String,
}

async fn ws_rename(State(s): S, Json(b): Json<NameBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            st.engine.rename(b.name);
            Ok(serde_json::to_value(st.engine.snapshot())?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct LocaleBody {
    locale: Locale,
}

/// Changes how the open workspace reads numbers and dates. Tiles ask again, so the
/// period names the engine writes follow it.
async fn ws_locale(State(s): S, Json(b): Json<LocaleBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            st.engine.set_locale(b.locale);
            st.emit(json!({ "type": "workspace-changed" }));
            Ok(serde_json::to_value(st.engine.snapshot())?)
        })
        .await?,
    ))
}

async fn open_sample(State(s): S) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, |st| {
            sample::open(&st.engine)?;
            changed(st)
        })
        .await?,
    ))
}

async fn add_source(State(s): S, Json(req): Json<AddSource>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            let id = st.engine.add_source(req)?;
            let mut v = changed(st)?;
            v["added"] = json!(id);
            Ok(v)
        })
        .await?,
    ))
}

async fn remove_source(State(s): S, Path(id): Path<String>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            st.engine.remove_source(&id);
            changed(st)
        })
        .await?,
    ))
}

async fn reload_source(State(s): S, Path(id): Path<String>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            let tables = st.engine.load(&id);
            st.emit(json!({ "type": "data-changed", "sources": [id], "tables": tables }));
            Ok(serde_json::to_value(st.engine.snapshot())?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct PreviewQ {
    id: String,
    limit: Option<u32>,
}

async fn preview(State(s): S, Query(q): Query<PreviewQ>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            Ok(serde_json::to_value(
                st.engine.preview(&q.id, q.limit.unwrap_or(200))?,
            )?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct RelBody {
    from: FieldRef,
    to: FieldRef,
}

async fn add_rel(State(s): S, Json(b): Json<RelBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            st.engine.add_relationship(b.from, b.to)?;
            changed(st)
        })
        .await?,
    ))
}

async fn remove_rel(State(s): S, Path(id): Path<String>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            st.engine.remove_relationship(&id);
            changed(st)
        })
        .await?,
    ))
}

async fn suggest(State(s): S) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, |st| Ok(serde_json::to_value(st.engine.suggest()?)?)).await?,
    ))
}

async fn put_dashboards(State(s): S, Json(b): Json<Vec<Value>>) -> R<Json<Value>> {
    blocking(&s, move |st| {
        st.engine.set_dashboards(b);
        st.emit(json!({ "type": "dashboards-changed" }));
        Ok(())
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn put_layout(State(s): S, Json(b): Json<Value>) -> R<Json<Value>> {
    blocking(&s, move |st| {
        st.engine.set_layout(b);
        Ok(())
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn query(State(s): S, Json(q): Json<SemanticQuery>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            Ok(serde_json::to_value(st.engine.query(&q)?)?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct FsQ {
    path: Option<String>,
}

async fn fs(Query(q): Query<FsQ>) -> R<Json<Value>> {
    let listing = tokio::task::spawn_blocking(move || fsbrowse::list(q.path.as_deref())).await??;
    Ok(Json(serde_json::to_value(listing)?))
}

#[derive(Deserialize)]
struct TileBody {
    tile: Value,
    #[serde(default)]
    filters: Vec<DashFilter>,
}

/// One live tile, computed by the same engine as snapshots.
async fn tile(State(s): S, Json(b): Json<TileBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            let t: Tile = serde_json::from_value(b.tile)?;
            Ok(serde_json::to_value(run_tile(&st.engine, &t, &b.filters)?)?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct SnapBody {
    dashboard: String,
    #[serde(default)]
    filters: Vec<DashFilter>,
    /// Export only: write the HTML file here (desktop app) instead of returning it.
    #[serde(default)]
    path: Option<String>,
}

async fn snapshot(State(s): S, Json(b): Json<SnapBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            snapshot::build(&st.engine, &b.dashboard, &b.filters)
        })
        .await?,
    ))
}

async fn export_html(State(s): S, Json(b): Json<SnapBody>) -> R<Response> {
    let (html, path) = blocking(&s, move |st| {
        let payload = snapshot::build(&st.engine, &b.dashboard, &b.filters)?;
        Ok((export::html(&payload), b.path))
    })
    .await?;
    if let Some(p) = path {
        tokio::task::spawn_blocking(move || export::write(export::WriteFile { path: p, html }))
            .await??;
        return Ok(Json(json!({ "ok": true })).into_response());
    }
    Ok(([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response())
}

// ---- tenun.cloud --------------------------------------------------------------

async fn cloud_status(State(s): S) -> R<Json<Value>> {
    let v = blocking(&s, |st| {
        let conn = cloud::connection(&st.secrets, &st.cloud_base);
        // How much of the free plan is used; the app works without it when offline.
        let usage = conn.as_ref().and_then(|(token, _)| {
            let me = cloud::Client::new(st.cloud_base.clone()).me(token).ok()?;
            Some(json!({
                "dashboards": me["dashboards"].as_array().map(|a| a.len()).unwrap_or(0),
                "limit": me["limits"]["dashboards"],
            }))
        });
        Ok(json!({
            "base": st.cloud_base,
            "connected": conn.is_some(),
            "email": conn.map(|c| c.1),
            "source": cloud::source_label(),
            "usage": usage,
        }))
    })
    .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
struct ConnectBody {
    /// Open the approval page in the default browser (desktop app).
    #[serde(default)]
    open: bool,
}

/// Starts device sign-in and waits for approval in the background.
async fn cloud_connect(State(s): S, Json(b): Json<ConnectBody>) -> R<Json<Value>> {
    let st = s.clone();
    let start = blocking(&s, |st| {
        let client = cloud::Client::new(st.cloud_base.clone());
        Ok(client.start_device(&format!("Tenun on {}", cloud::source_label()))?)
    })
    .await?;
    if b.open {
        let _ = webbrowser::open(&start.verification_uri_complete);
    }
    let device = start.clone();
    std::thread::spawn(move || {
        let client = cloud::Client::new(st.cloud_base.clone());
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(device.expires_in);
        while std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_secs(device.interval.max(2)));
            match client.poll_device(&device.device_code) {
                Ok(cloud::Poll::Pending) => continue,
                Ok(cloud::Poll::Done { token, email }) => {
                    let ok =
                        cloud::save_connection(&st.secrets, &st.cloud_base, &token, &email).is_ok();
                    st.emit(json!({ "type": "cloud-connected", "ok": ok, "email": email }));
                    return;
                }
                Ok(cloud::Poll::Expired) | Err(_) => break,
            }
        }
        st.emit(json!({ "type": "cloud-connected", "ok": false }));
    });
    Ok(Json(json!({
        "user_code": start.user_code,
        "url": start.verification_uri_complete,
        "expires_in": start.expires_in,
    })))
}

async fn cloud_disconnect(State(s): S) -> R<Json<Value>> {
    blocking(&s, |st| {
        cloud::forget_connection(&st.secrets, &st.cloud_base)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct DashBody {
    dashboard: String,
}

async fn cloud_publish(State(s): S, Json(b): Json<DashBody>) -> R<Json<Value>> {
    let (result, snap) = blocking(&s, move |st| {
        let r = cloud::publish(&st.engine, &st.secrets, &st.cloud_base, &b.dashboard);
        st.schedule.lock().unwrap().record(
            &b.dashboard,
            r.is_ok(),
            r.as_ref().err(),
            chrono::Utc::now(),
        );
        Ok((
            r.map_err(|e| e.to_string()),
            serde_json::to_value(st.engine.snapshot())?,
        ))
    })
    .await?;
    match result {
        Ok(v) => Ok(Json(
            json!({ "ok": true, "url": v["url"], "version": v["version"], "snapshot": snap }),
        )),
        // The snapshot still matters: it carries the error on the dashboard.
        Err(e) => Err(anyhow::anyhow!(e).into()),
    }
}

async fn cloud_unpublish(State(s): S, Json(b): Json<DashBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            cloud::unpublish(&st.engine, &st.secrets, &st.cloud_base, &b.dashboard)?;
            Ok(serde_json::to_value(st.engine.snapshot())?)
        })
        .await?,
    ))
}

async fn viewer() -> Json<Value> {
    Json(export::assets())
}

async fn export_file(Json(b): Json<export::WriteFile>) -> R<Json<Value>> {
    tokio::task::spawn_blocking(move || export::write(b)).await??;
    Ok(Json(json!({ "ok": true })))
}

async fn events(State(s): S, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| pump(socket, s))
}

async fn pump(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.events.subscribe();
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(text) => {
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            }
        }
    }
}

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../ui/dist"]
struct Ui;

pub async fn static_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let (file, path) = match Ui::get(path) {
        Some(f) if !path.is_empty() => (f, path.to_string()),
        // Unknown paths get the app; it routes on the client.
        _ => match Ui::get("index.html") {
            Some(f) => (f, "index.html".to_string()),
            None => return (StatusCode::NOT_FOUND, "UI not built").into_response(),
        },
    };
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (header::CONTENT_TYPE, mime.as_ref().to_string()),
            (header::CACHE_CONTROL, cache.to_string()),
        ],
        file.data.into_owned(),
    )
        .into_response()
}
