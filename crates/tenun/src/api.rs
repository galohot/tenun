//! JSON routes under `/api`, the event socket, and the embedded UI files.

use crate::{AppState, VERSION, export, fsbrowse, sample};
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
use tenun_core::workspace::FieldRef;

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
        .route("/viewer", get(viewer))
        .route("/export-file", post(export_file))
        .route("/events", get(events))
        .route("/pick", post(pick))
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

async fn open_browser(State(s): S) -> R<Json<Value>> {
    let url = s.url.lock().unwrap().clone();
    webbrowser::open(&url)?;
    Ok(Json(json!({ "ok": true })))
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

async fn ws_new(State(s): S) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, |st| {
            st.engine.new_workspace();
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
