//! JSON routes under `/api`, the event socket, and the embedded UI files.

use crate::settings::Settings;
use crate::{AppState, VERSION, cloud, export, fsbrowse, sample, xlsx};
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
use tenun_core::engine::{AddSource, KeyEdit, KeyIn, LinkPreview};
use tenun_core::formula::CheckRequest;
use tenun_core::paste;
use tenun_core::query::{Filter, SemanticQuery};
use tenun_core::secrets::Secrets;
use tenun_core::snapshot::{self, DashFilter, Tile, run_tile, run_tile_public};
use tenun_core::workspace::{DerivedSpec, Every, FieldRef, Locale, Paging, TableSetup};

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
        .route("/workspace/measures", put(put_measures))
        .route("/sample", post(open_sample))
        .route("/sources", post(add_source))
        .route("/sources/{id}", delete(remove_source))
        .route("/sources/{id}/reload", post(reload_source))
        .route("/sources/{id}/every", put(source_every))
        .route("/sources/{id}/link", put(source_link))
        .route("/sources/{id}/derived", put(source_derived))
        .route("/derived/preview", post(derived_preview))
        .route("/geo/places", get(geo_places))
        .route("/geo/{country}/{level}", get(geo_boundaries))
        .route("/link/preview", post(link_preview))
        .route("/paste/preview", post(paste_preview))
        .route("/paste", post(paste_add))
        .route("/tables/preview", get(preview))
        .route("/tables/setup", put(table_setup))
        .route("/profile", get(profile))
        .route("/prep/unpivot", get(unpivot_detect))
        .route("/prep/unpivot/preview", post(unpivot_preview))
        .route("/relationships", post(add_rel))
        .route("/relationships/{id}", delete(remove_rel).patch(rename_rel))
        .route("/relationships/names", put(rename_rels))
        .route("/relationships/check", post(check_links))
        .route("/relationships/suggest", get(suggest))
        .route("/dashboards", put(put_dashboards))
        .route("/layout", put(put_layout))
        .route("/query", post(query))
        .route("/formula/check", post(formula_check))
        .route("/fs", get(fs))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/viewer", get(viewer))
        .route("/tile", post(tile))
        .route("/rows", post(rows))
        .route("/cloud", get(cloud_status))
        .route("/cloud/connect", post(cloud_connect))
        .route("/cloud/disconnect", post(cloud_disconnect))
        .route("/cloud/publish", post(cloud_publish))
        .route("/cloud/unpublish", post(cloud_unpublish))
        .route("/snapshot", post(snapshot))
        .route("/export-html", post(export_html))
        .route("/export-file", post(export_file))
        .route("/export-xlsx", post(export_xlsx))
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
struct DerivedBody {
    spec: DerivedSpec,
}

/// Changes how a new table is made (build 04). Its name, links and tiles stay.
async fn source_derived(
    State(s): S,
    Path(id): Path<String>,
    Json(b): Json<DerivedBody>,
) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            let tables = st.engine.set_derived(&id, b.spec)?;
            st.emit(json!({ "type": "data-changed", "sources": [id], "tables": tables }));
            Ok(serde_json::to_value(st.engine.snapshot())?)
        })
        .await?,
    ))
}

/// The columns, first rows, number of rows and notes of a new table before it is made.
async fn derived_preview(State(s): S, Json(b): Json<DerivedBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            Ok(serde_json::to_value(st.engine.preview_derived(&b.spec)?)?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct PlacesQuery {
    level: tenun_core::geo::Level,
    #[serde(default)]
    country: String,
}

/// The standard names and codes of one level of one country (build 05): the Place names sheet and its CSV.
async fn geo_places(Query(q): Query<PlacesQuery>) -> Json<Value> {
    let places = tenun_core::geo::list(q.level, &q.country);
    Json(json!({ "level": q.level, "country": q.country.to_ascii_uppercase(), "places": places }))
}

/// The boundaries of a level of a country that does not ship with Tenun (V4-D19): downloaded from geoBoundaries
/// once, kept in `<config dir>/tenun/geo/`, then served from there. GeoJSON.
async fn geo_boundaries(
    State(s): S,
    Path((country, level)): Path<(String, String)>,
) -> R<Response> {
    let level = match level.as_str() {
        "province" => tenun_core::geo::Level::Province,
        "regency" => tenun_core::geo::Level::Regency,
        _ => return Err(anyhow::anyhow!("A map level is province or regency.").into()),
    };
    let body = blocking(&s, move |_| {
        let dir = Secrets::default_path().with_file_name("geo");
        let file =
            tenun_core::geo::boundaries(&country, level, &dir, tenun_core::geo::GEOBOUNDARIES)?;
        Ok(std::fs::read(file)?)
    })
    .await?;
    Ok(([(header::CONTENT_TYPE, "application/geo+json")], body).into_response())
}

#[derive(Deserialize)]
struct EveryBody {
    every: Every,
}

/// How often a web link is fetched again.
async fn source_every(
    State(s): S,
    Path(id): Path<String>,
    Json(b): Json<EveryBody>,
) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            st.engine.set_url_every(&id, b.every)?;
            st.emit(json!({ "type": "workspace-changed" }));
            Ok(serde_json::to_value(st.engine.snapshot())?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct LinkBody {
    /// Where the list of records sits in a JSON answer; empty lets Tenun pick.
    #[serde(default)]
    records: Option<String>,
    #[serde(default)]
    pages: Option<Paging>,
    /// Left out: the key stays as it is. `null`: the key and its stored value are removed. An
    /// object: the key is set; with no value the stored value stays.
    #[serde(default, deserialize_with = "set_or_null")]
    key: Option<Option<KeyIn>>,
}

/// Tells a field that is `null` from a field that is not there.
fn set_or_null<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Changes where a web link finds its records, how it follows pages and which key it sends, then
/// fetches it again. The stored key value is never sent back.
async fn source_link(
    State(s): S,
    Path(id): Path<String>,
    Json(b): Json<LinkBody>,
) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            let key = match b.key {
                None => KeyEdit::Keep,
                Some(None) => KeyEdit::Remove,
                Some(Some(k)) => KeyEdit::Set(k),
            };
            let tables = st.engine.set_link(&id, b.records, b.pages, key)?;
            st.emit(json!({ "type": "data-changed", "sources": [id], "tables": tables }));
            Ok(serde_json::to_value(st.engine.snapshot())?)
        })
        .await?,
    ))
}

/// A look at the first page of a web link. Nothing is saved, not the data and not the key.
async fn link_preview(State(s): S, Json(b): Json<LinkPreview>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            Ok(serde_json::to_value(st.engine.link_preview(b)?)?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct PasteBody {
    text: String,
    #[serde(default = "yes")]
    header: bool,
    #[serde(default)]
    name: String,
}

fn yes() -> bool {
    true
}

/// The name, the first rows and the number of rows of a pasted table. An error says that the
/// text is not a table.
async fn paste_preview(Json(b): Json<PasteBody>) -> R<Json<Value>> {
    let p = paste::parse(&b.text)
        .ok_or_else(|| anyhow::anyhow!("There is no table in what you pasted."))?;
    Ok(Json(serde_json::to_value(paste::preview(&p, b.header, 5))?))
}

/// Writes a pasted table as a CSV file in the data folder and adds it as a source.
async fn paste_add(State(s): S, Json(b): Json<PasteBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            let id = st.engine.add_pasted(&b.name, &b.text, b.header)?;
            let mut v = changed(st)?;
            v["added"] = json!(id);
            Ok(v)
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
struct SetupBody {
    table: String,
    /// How the table is prepared; null clears it.
    setup: Option<TableSetup>,
}

/// Sets the header row, the column overrides, the unpivot and the calculated columns of one
/// table. The source loads again when the data changes, and the tiles ask again.
///
/// A calculated column that cannot be saved is a normal answer (V4-D35): `{ ok: false, calc, name,
/// error: { message, at, len } }`, the same error as `POST /api/formula/check`, with `calc` the
/// index of the column in the list that was sent. Nothing is saved then.
async fn table_setup(State(s): S, Json(b): Json<SetupBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            let table = b.table.clone();
            match st.engine.set_table_setup(&b.table, b.setup) {
                Ok(true) => st.emit(json!({ "type": "data-changed", "tables": [table] })),
                Ok(false) => st.emit(json!({ "type": "workspace-changed" })),
                Err(e) => {
                    return match e.downcast_ref::<tenun_core::calc::CalcRefused>() {
                        Some(refused) => Ok(refused.reply()),
                        None => Err(e),
                    };
                }
            }
            Ok(serde_json::to_value(st.engine.snapshot())?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct MeasuresBody {
    /// The whole list. A new measure has an empty `id`.
    measures: Vec<tenun_core::workspace::NamedMeasure>,
    /// Yes to deleting measures that tiles use.
    #[serde(default)]
    confirm: bool,
}

/// Saves the named measures (the whole list). Every new or changed formula is checked by build 01.
///
/// Two normal answers are not errors (V4-D35): `{ ok: false, measure, name, error: { message, at,
/// len } }` for a formula that cannot be saved, and `{ ok: false, used: [...] }` when the save would
/// delete a measure that tiles use and `confirm` is not set. Nothing is saved then. A good save
/// answers the snapshot.
async fn put_measures(State(s): S, Json(b): Json<MeasuresBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            match st.engine.set_measures(b.measures, b.confirm) {
                Ok(()) => st.emit(json!({ "type": "data-changed", "tables": [] })),
                Err(e) => {
                    if let Some(refused) = e.downcast_ref::<tenun_core::engine::MeasureRefused>() {
                        return Ok(refused.reply());
                    }
                    if let Some(used) = e.downcast_ref::<tenun_core::engine::MeasureInUse>() {
                        return Ok(used.reply());
                    }
                    return Err(e);
                }
            }
            Ok(serde_json::to_value(st.engine.snapshot())?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct ProfileQ {
    table: String,
    column: String,
}

async fn profile(State(s): S, Query(q): Query<ProfileQ>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            Ok(serde_json::to_value(
                st.engine.profile(&q.table, &q.column)?,
            )?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct TableQ {
    table: String,
}

/// The wide-sheet columns of a table (months across the top), or null.
async fn unpivot_detect(State(s): S, Query(q): Query<TableQ>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            Ok(serde_json::to_value(st.engine.detect_unpivot(&q.table))?)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct UnpivotBody {
    table: String,
    keep: Vec<String>,
    name: String,
    value: String,
}

/// Five rows of the table before, and five rows after its columns turn into rows.
async fn unpivot_preview(State(s): S, Json(b): Json<UnpivotBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            let before = st.engine.preview(&b.table, 5)?;
            let after = st
                .engine
                .unpivot_preview(&b.table, b.keep, b.name, b.value, 5)?;
            Ok(json!({ "before": before, "after": after }))
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

/// Names a link, or clears its name with an empty one (V4-D28). A body with no `name` is refused:
/// it is not a request to clear the name.
async fn rename_rel(State(s): S, Path(id): Path<String>, Json(b): Json<Value>) -> R<Json<Value>> {
    let Some(name) = b.get("name").and_then(Value::as_str).map(str::to_string) else {
        return Err(
            anyhow::anyhow!("Send the name of the link as text. An empty name clears it.").into(),
        );
    };
    Ok(Json(
        blocking(&s, move |st| {
            st.engine.rename_link(&id, Some(name))?;
            changed(st)
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct RenameRelsBody {
    /// Link id → its new name. They are checked together, so two links can swap their names.
    names: std::collections::BTreeMap<String, String>,
}

/// Names several links at once, as one set (V4-D46).
async fn rename_rels(State(s): S, Json(b): Json<RenameRelsBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            let names = b.names.into_iter().map(|(k, v)| (k, Some(v))).collect();
            st.engine.rename_links(names)?;
            changed(st)
        })
        .await?,
    ))
}

/// Reads the links of live database tables again, because a dashboard refreshed (V4-D51). The
/// windows are told when an answer changed.
async fn check_links(State(s): S) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, |st| {
            let mut changed = st.engine.recheck_live_links();
            // A calculated column with `TODAY()` is built again when the day has changed (V4-D34).
            let dated = st.engine.refresh_dates();
            if dated {
                st.emit(json!({ "type": "data-changed" }));
            } else if changed {
                st.emit(json!({ "type": "workspace-changed" }));
            }
            changed |= dated;
            Ok(json!({ "changed": changed }))
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

/// The body of `POST /api/formula/check`: the formula, and, for a calculated column that is being
/// written, which column it is and whether it is SQL.
#[derive(Deserialize)]
struct CheckBody {
    #[serde(flatten)]
    check: CheckRequest,
    /// The calculated column being edited: the columns of the table from it on are not there.
    #[serde(default)]
    before: Option<String>,
    /// The formula is raw DuckDB SQL (Advanced, a column only).
    #[serde(default)]
    sql: bool,
}

/// Checks a formula. A wrong formula is a normal answer (`ok: false`, a plain message and the
/// place to underline), not an error. A good one comes with its type and a preview. The answer
/// never holds SQL.
async fn formula_check(State(s): S, Json(b): Json<CheckBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            Ok(serde_json::to_value(st.engine.check_calc(
                &b.check,
                b.before.as_deref(),
                b.sql,
            )?)?)
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
struct RowsBody {
    tile: Value,
    #[serde(default)]
    filters: Vec<DashFilter>,
    /// The filter a click on the mark makes (the same one the cross-filter uses). Without it the
    /// rows are those of the whole tile.
    #[serde(default)]
    click: Option<Filter>,
}

/// The raw rows behind a tile or one of its marks, at most 500, with the total count.
async fn rows(State(s): S, Json(b): Json<RowsBody>) -> R<Json<Value>> {
    Ok(Json(
        blocking(&s, move |st| {
            let t: Tile = serde_json::from_value(b.tile)?;
            let r = tenun_core::filters::rows_behind(&st.engine, &t, &b.filters, b.click)?;
            Ok(serde_json::to_value(r)?)
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

#[derive(Deserialize)]
struct XlsxBody {
    tile: Value,
    #[serde(default)]
    filters: Vec<DashFilter>,
    /// The column names the app shows. Without them the engine's own names are used.
    #[serde(default)]
    headers: Vec<String>,
    #[serde(default)]
    title: String,
    /// Desktop app: write the file here instead of returning it.
    #[serde(default)]
    path: Option<String>,
}

/// The numbers one tile shows, as an Excel file (header row, number formats, dates, total row).
async fn export_xlsx(State(s): S, Json(b): Json<XlsxBody>) -> R<Response> {
    let (bytes, path) = blocking(&s, move |st| {
        let t: Tile = serde_json::from_value(b.tile.clone())?;
        if snapshot::is_static(&t.kind) || t.kind == "filter" {
            anyhow::bail!("This tile has no numbers to download.");
        }
        // A file that leaves the computer carries the notes with no key value and no file name.
        let res = run_tile_public(&st.engine, &t, &b.filters)?;
        Ok((xlsx::build(&b.tile, &res, &b.headers, &b.title)?, b.path))
    })
    .await?;
    if let Some(p) = path {
        tokio::task::spawn_blocking(move || {
            let mut p = PathBuf::from(p);
            if p.extension().is_none_or(|e| e != "xlsx") {
                p.set_extension("xlsx");
            }
            std::fs::write(p, bytes)
        })
        .await??;
        return Ok(Json(json!({ "ok": true })).into_response());
    }
    Ok((
        [(
            header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )],
        bytes,
    )
        .into_response())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_change_tells_a_missing_key_from_a_null_key() {
        // Left out: the key stays. Null: the key goes. An object: the key is set.
        let keep: LinkBody = serde_json::from_str(r#"{"records":"data.items"}"#).unwrap();
        assert!(keep.key.is_none());
        let remove: LinkBody = serde_json::from_str(r#"{"records":null,"key":null}"#).unwrap();
        assert!(matches!(remove.key, Some(None)));
        let set: LinkBody =
            serde_json::from_str(r#"{"key":{"place":"header","name":"X-Api-Key"}}"#).unwrap();
        let Some(Some(k)) = set.key else {
            panic!("a key");
        };
        // No value means: keep the value that is stored.
        assert_eq!((k.name.as_str(), k.value.as_str()), ("X-Api-Key", ""));
    }
}
