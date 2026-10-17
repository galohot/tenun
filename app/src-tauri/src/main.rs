// The desktop shell: runs the Tenun server inside the app and shows it in a
// native window. Everything else is the same web UI as `tenun serve`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{DragDropEvent, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_updater::UpdaterExt;
use tenun::PickRequest;
use tenun::update::{UpdateInfo, Updater};
use tenun_core::engine::AddSource;

fn main() {
    let workspace = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .filter(|p| p.extension().is_some_and(|e| e == "tenun"));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let handle = app.handle().clone();
            let picker: tenun::Picker = Arc::new(move |req: PickRequest| pick(&handle, req));
            let running = tauri::async_runtime::block_on(tenun::start(tenun::Options {
                host: IpAddr::from([127, 0, 0, 1]),
                port: 0,
                workspace: workspace.clone(),
                picker: Some(picker),
            }))?;
            let url = running.url.parse()?;
            app.manage(running.state.clone());
            running.state.set_updater(Arc::new(AppUpdater {
                app: app.handle().clone(),
                found: Mutex::new(None),
            }));
            // Keep this server in the list `tenun mcp` reads for as long as the app runs (v5 build 14).
            if let Some(r) = running.registration {
                app.manage(r);
            }
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("Tenun")
                .inner_size(1360.0, 860.0)
                .min_inner_size(960.0, 620.0)
                .build()?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Files dropped on the window become sources, with their real paths.
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                let state = window.state::<Arc<tenun::AppState>>().inner().clone();
                let paths = paths.clone();
                std::thread::spawn(move || {
                    for p in paths {
                        let path = p.to_string_lossy().into_owned();
                        let result = if path.ends_with(".tenun") {
                            state.engine.open(&p).map(|_| ())
                        } else if p.is_dir() {
                            state
                                .engine
                                .add_source(AddSource::Folder {
                                    path,
                                    pattern: None,
                                })
                                .map(|_| ())
                        } else {
                            state
                                .engine
                                .add_source(AddSource::File { path })
                                .map(|_| ())
                        };
                        if let Err(e) = result {
                            state.emit(
                                serde_json::json!({ "type": "error", "message": format!("{e:#}") }),
                            );
                        }
                    }
                    state.rewatch();
                    state.emit(serde_json::json!({ "type": "workspace-changed" }));
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("Tenun failed to start");
}

/// New versions from the GitHub release's `latest.json` (0.7 build 01). The signature of what it
/// downloads is checked against the public key in `tauri.conf.json` before anything is installed.
struct AppUpdater {
    app: tauri::AppHandle,
    found: Mutex<Option<tauri_plugin_updater::Update>>,
}

impl Updater for AppUpdater {
    fn check(&self) -> anyhow::Result<Option<UpdateInfo>> {
        let found = tauri::async_runtime::block_on(self.app.updater()?.check())?;
        let info = found.as_ref().map(|u| UpdateInfo {
            version: u.version.clone(),
            notes: u.body.clone().unwrap_or_default(),
        });
        *self.found.lock().unwrap() = found;
        Ok(info)
    }

    fn install(&self) -> anyhow::Result<()> {
        let update =
            self.found.lock().unwrap().take().ok_or_else(|| {
                anyhow::anyhow!("There is no new version to install. Check again.")
            })?;
        tauri::async_runtime::block_on(update.download_and_install(|_, _| {}, || {}))?;
        self.app.restart();
    }
}

fn pick(app: &tauri::AppHandle, req: PickRequest) -> Vec<PathBuf> {
    let d = app.dialog().file();
    let to_path = |f: tauri_plugin_dialog::FilePath| f.into_path().ok();
    match req {
        PickRequest::DataFiles => d
            .add_filter(
                "Data",
                &[
                    "xlsx", "xlsm", "xlsb", "xls", "ods", "csv", "tsv", "txt", "parquet", "sqlite",
                    "sqlite3", "db", "duckdb",
                ],
            )
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .filter_map(to_path)
            .collect(),
        PickRequest::Folder => d
            .blocking_pick_folder()
            .and_then(to_path)
            .into_iter()
            .collect(),
        PickRequest::Image => d
            .add_filter("Picture", &["png", "jpg", "jpeg", "webp"])
            .blocking_pick_file()
            .and_then(to_path)
            .into_iter()
            .collect(),
        PickRequest::OpenWorkspace => d
            .add_filter("Tenun workspace", &["tenun"])
            .blocking_pick_file()
            .and_then(to_path)
            .into_iter()
            .collect(),
        PickRequest::SaveHtml { name } => d
            .add_filter("Web page", &["html"])
            .set_file_name(format!("{name}.html"))
            .blocking_save_file()
            .and_then(to_path)
            .into_iter()
            .collect(),
        PickRequest::SaveZip { name } => d
            .add_filter("ZIP archive", &["zip"])
            .set_file_name(format!("{name}.zip"))
            .blocking_save_file()
            .and_then(to_path)
            .into_iter()
            .collect(),
        PickRequest::SaveXlsx { name } => d
            .add_filter("Excel workbook", &["xlsx"])
            .set_file_name(format!("{name}.xlsx"))
            .blocking_save_file()
            .and_then(to_path)
            .into_iter()
            .collect(),
        PickRequest::SaveWorkspace { name } => d
            .add_filter("Tenun workspace", &["tenun"])
            .set_file_name(format!("{name}.tenun"))
            .blocking_save_file()
            .and_then(to_path)
            .into_iter()
            .collect(),
    }
}
