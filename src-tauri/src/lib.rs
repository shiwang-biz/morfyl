//! Tauri shell: exposes the conversion core to the UI.

mod installer;

use convert_core::{engines, formats, Caps, EngineId, Job, Locator, Source, Toolbox};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::watch;

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    /// engine key -> executable chosen by the user
    #[serde(default)]
    engine_paths: HashMap<String, PathBuf>,
}

struct AppState {
    locator: Mutex<Locator>,
    /// Probed capabilities, keyed by executable path so a changed engine is re-probed.
    caps: tokio::sync::Mutex<HashMap<PathBuf, Caps>>,
    jobs: Mutex<HashMap<String, watch::Sender<bool>>>,
    installs: Mutex<HashMap<EngineId, installer::CancelFlag>>,
    settings_path: PathBuf,
    downloads_dir: PathBuf,
    scratch: PathBuf,
    http: reqwest::Client,
}

impl AppState {
    fn save_overrides(&self) -> Result<(), String> {
        let loc = self.locator.lock().unwrap();
        let s = Settings {
            engine_paths: loc.overrides.iter().map(|(k, v)| (k.key().to_string(), v.clone())).collect(),
        };
        if let Some(p) = self.settings_path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        std::fs::write(&self.settings_path, serde_json::to_vec_pretty(&s).unwrap()).map_err(|e| e.to_string())
    }

    async fn caps_for(&self, id: EngineId, path: &Path) -> Caps {
        let mut cache = self.caps.lock().await;
        if let Some(c) = cache.get(path) {
            return c.clone();
        }
        let c = engines::probe(id, path).await;
        cache.insert(path.to_path_buf(), c.clone());
        c
    }

    async fn toolbox(&self) -> Toolbox {
        let found: Vec<(EngineId, PathBuf)> = {
            let loc = self.locator.lock().unwrap();
            EngineId::ALL.iter().filter_map(|id| loc.find(*id).map(|l| (*id, l.path))).collect()
        };
        let mut tb = Toolbox::default();
        for (id, path) in found {
            let caps = self.caps_for(id, &path).await;
            tb.engines.insert(id, (path, caps));
        }
        tb
    }
}

fn engine_from_key(key: &str) -> Result<EngineId, String> {
    EngineId::ALL.into_iter().find(|e| e.key() == key).ok_or_else(|| format!("Unknown engine {key}"))
}

// ------------------------------------------------------------------ engines

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineInfo {
    id: EngineId,
    name: &'static str,
    purpose: &'static str,
    license: &'static str,
    delivery: convert_core::Delivery,
    download_page: &'static str,
    size_hint: &'static str,
    /// Installed by Morfyl into its own folder (so it can also be removed from the app).
    managed: bool,
    installing: bool,
    path: Option<PathBuf>,
    source: Option<Source>,
    version: Option<String>,
}

#[tauri::command]
async fn list_engines(state: State<'_, AppState>) -> Result<Vec<EngineInfo>, String> {
    let located: Vec<_> = {
        let loc = state.locator.lock().unwrap();
        EngineId::ALL.iter().map(|id| (*id, loc.find(*id))).collect()
    };
    let mut out = Vec::new();
    let installing: Vec<EngineId> = state.installs.lock().unwrap().keys().copied().collect();
    for (id, l) in located {
        let version = match &l {
            Some(l) => state.caps_for(id, &l.path).await.version,
            None => None,
        };
        out.push(EngineInfo {
            id,
            name: id.name(),
            purpose: id.purpose(),
            license: id.license(),
            delivery: id.delivery(),
            download_page: id.download_page(),
            size_hint: id.size_hint(),
            managed: matches!(l.as_ref().map(|l| l.source), Some(Source::Downloaded)),
            installing: installing.contains(&id),
            path: l.as_ref().map(|l| l.path.clone()),
            source: l.map(|l| l.source),
            version,
        });
    }
    Ok(out)
}

#[tauri::command]
async fn set_engine_path(state: State<'_, AppState>, id: String, path: Option<String>) -> Result<(), String> {
    let id = engine_from_key(&id)?;
    {
        let mut loc = state.locator.lock().unwrap();
        match path {
            Some(p) => {
                let p = PathBuf::from(p);
                if !p.is_file() {
                    return Err("That isn't a file".into());
                }
                loc.overrides.insert(id, p);
            }
            None => {
                loc.overrides.remove(&id);
            }
        }
    }
    state.save_overrides()
}

#[tauri::command]
async fn install_engine(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let id = engine_from_key(&id)?;
    let cancel = {
        let mut installs = state.installs.lock().unwrap();
        if installs.contains_key(&id) {
            return Err(format!("{} is already being installed", id.name()));
        }
        let flag = installer::CancelFlag::default();
        installs.insert(id, flag.clone());
        flag
    };
    let result = installer::install(&app, &state.http, &state.downloads_dir, id, cancel).await;
    state.installs.lock().unwrap().remove(&id);
    // The engine may have changed: forget cached capabilities so it is probed again.
    state.caps.lock().await.clear();
    result.map(|_| ())
}

#[tauri::command]
fn cancel_install(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let id = engine_from_key(&id)?;
    if let Some(flag) = state.installs.lock().unwrap().get(&id) {
        flag.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
async fn uninstall_engine(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let id = engine_from_key(&id)?;
    installer::uninstall(&state.downloads_dir, id)?;
    state.caps.lock().await.clear();
    Ok(())
}

/// Download sizes and versions for the installable engines (needs internet).
#[tauri::command]
async fn engine_offers(state: State<'_, AppState>) -> Result<HashMap<String, installer::Offer>, String> {
    Ok(installer::offers(&state.http).await)
}

#[tauri::command]
fn platform() -> &'static str {
    std::env::consts::OS
}

// -------------------------------------------------------------------- files

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputInfo {
    format: &'static str,
    label: &'static str,
    available: bool,
    /// Engine keys still needed, or a reason the installed build can't do it.
    missing: Vec<EngineId>,
    note: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileInfo {
    path: PathBuf,
    name: String,
    size: u64,
    category: Option<formats::Category>,
    category_label: Option<&'static str>,
    outputs: Vec<OutputInfo>,
}

/// Expand dropped folders (up to a few levels) and describe every file.
#[tauri::command]
async fn inspect(state: State<'_, AppState>, paths: Vec<PathBuf>) -> Result<Vec<FileInfo>, String> {
    let mut files = Vec::new();
    for p in paths {
        collect_files(&p, 4, &mut files);
    }
    let tb = state.toolbox().await;
    Ok(files
        .into_iter()
        .map(|path| {
            let cat = formats::detect(&path);
            let outputs = formats::outputs_for(&path)
                .into_iter()
                .map(|o| {
                    let missing: Vec<EngineId> = o.engines.iter().copied().filter(|e| !tb.engines.contains_key(e)).collect();
                    let mut note = None;
                    if missing.is_empty() && o.engines.contains(&EngineId::ImageMagick) {
                        if let Some((_, caps)) = tb.engines.get(&EngineId::ImageMagick) {
                            if !caps.can_write(o.format) {
                                note = Some("Not supported by the installed ImageMagick".to_string());
                            }
                        }
                    }
                    OutputInfo { format: o.format, label: o.label, available: missing.is_empty() && note.is_none(), missing, note }
                })
                .collect();
            FileInfo {
                name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                size: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                category: cat,
                category_label: cat.map(|c| c.label()),
                outputs,
                path,
            }
        })
        .collect())
}

fn collect_files(p: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if p.is_file() {
        out.push(p.to_path_buf());
    } else if p.is_dir() && depth > 0 {
        let Ok(rd) = std::fs::read_dir(p) else { return };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for e in entries {
            let hidden = e.file_name().map(|n| n.to_string_lossy().starts_with('.')).unwrap_or(false);
            if !hidden {
                collect_files(&e, depth - 1, out);
            }
        }
    }
}

// --------------------------------------------------------------- converting

#[derive(Serialize, Clone)]
struct JobProgress {
    id: String,
    progress: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConvertResult {
    outputs: Vec<PathBuf>,
    commands: Vec<String>,
}

#[tauri::command]
async fn convert(app: AppHandle, state: State<'_, AppState>, id: String, job: Job) -> Result<ConvertResult, String> {
    let tb = state.toolbox().await;
    std::fs::create_dir_all(&state.scratch).map_err(|e| e.to_string())?;
    let plan = convert_core::plan(&job, &tb, &state.scratch).map_err(|e| e.to_string())?;
    let commands = plan.command_lines();

    let (tx, rx) = watch::channel(false);
    state.jobs.lock().unwrap().insert(id.clone(), tx);
    let (app2, id2) = (app.clone(), id.clone());
    let result = convert_core::run(
        &plan,
        move |convert_core::Event::Progress(p)| {
            let _ = app2.emit("job-progress", JobProgress { id: id2.clone(), progress: p });
        },
        rx,
    )
    .await;
    state.jobs.lock().unwrap().remove(&id);
    result.map(|outputs| ConvertResult { outputs, commands }).map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel(state: State<'_, AppState>, id: String) {
    if let Some(tx) = state.jobs.lock().unwrap().get(&id) {
        let _ = tx.send(true);
    }
}

#[tauri::command]
fn cancel_all(state: State<'_, AppState>) {
    for tx in state.jobs.lock().unwrap().values() {
        let _ = tx.send(true);
    }
}

#[tauri::command]
fn reveal(path: PathBuf) -> Result<(), String> {
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

// --------------------------------------------------------------------- main

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_local_data_dir()?;
            let settings_path = config_dir.join("settings.json");
            let settings: Settings = std::fs::read(&settings_path)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();

            // Bundled engines: sidecars sit next to the app executable; a resources/engines
            // folder is also searched for builds that ship engines with their libraries.
            let mut bundled_dirs = Vec::new();
            if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
                bundled_dirs.push(dir);
            }
            if let Ok(res) = app.path().resource_dir() {
                bundled_dirs.push(res.join("engines"));
            }

            let downloads_dir = data_dir.join("engines");
            let locator = Locator {
                bundled_dirs,
                downloads_dir: Some(downloads_dir.clone()),
                overrides: settings
                    .engine_paths
                    .into_iter()
                    .filter_map(|(k, v)| engine_from_key(&k).ok().map(|id| (id, v)))
                    .collect(),
            };
            let scratch = std::env::temp_dir().join("morfyl");
            let _ = std::fs::remove_dir_all(&scratch); // leftovers from a crash
            app.manage(AppState {
                locator: Mutex::new(locator),
                caps: Default::default(),
                jobs: Default::default(),
                installs: Default::default(),
                settings_path,
                downloads_dir,
                scratch,
                http: reqwest::Client::builder()
                    .user_agent(concat!("Morfyl/", env!("CARGO_PKG_VERSION")))
                    .build()?,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_engines,
            set_engine_path,
            install_engine,
            cancel_install,
            uninstall_engine,
            engine_offers,
            platform,
            inspect,
            convert,
            cancel,
            cancel_all,
            reveal
        ])
        .run(tauri::generate_context!())
        .expect("error while running Morfyl");
}
