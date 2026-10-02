//! Downloads small portable engine packs (Pandoc, Ghostscript, 7-Zip) on first use.
//!
//! The packs are zip files built by `.github/workflows/engine-packs.yml` and listed in a
//! manifest you host (by default a GitHub release). Every pack is SHA-256 verified.
//!
//! manifest.json:
//! { "engines": { "pandoc": { "macos-aarch64": { "url": "...", "sha256": "...", "size": 123 } } } }

use convert_core::EngineId;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

/// Override at build time: FILEFORGE_ENGINES_URL=https://.../manifest.json cargo tauri build
pub const MANIFEST_URL: &str = match option_env!("FILEFORGE_ENGINES_URL") {
    Some(u) => u,
    None => "https://github.com/YOUR_GITHUB_USER/fileforge/releases/download/engines/manifest.json",
};

#[derive(Deserialize)]
struct Manifest {
    engines: HashMap<String, HashMap<String, Pack>>,
}

#[derive(Deserialize, Clone)]
struct Pack {
    url: String,
    sha256: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Serialize, Clone)]
pub struct InstallProgress {
    pub id: EngineId,
    pub stage: &'static str,
    pub progress: f32,
}

pub fn platform_key() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

pub async fn install(app: &AppHandle, http: &reqwest::Client, downloads_dir: &Path, id: EngineId) -> Result<PathBuf, String> {
    let emit = |stage: &'static str, progress: f32| {
        let _ = app.emit("engine-progress", InstallProgress { id, stage, progress });
    };
    emit("Fetching engine list", 0.0);
    let manifest: Manifest = http
        .get(MANIFEST_URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Couldn't reach the engine list: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Engine list is malformed: {e}"))?;
    let pack = manifest
        .engines
        .get(id.key())
        .and_then(|m| m.get(&platform_key()))
        .cloned()
        .ok_or_else(|| format!("No {} download for {}", id.name(), platform_key()))?;

    std::fs::create_dir_all(downloads_dir).map_err(|e| e.to_string())?;
    let zip_path = downloads_dir.join(format!("{}.zip.part", id.key()));
    let resp = http
        .get(&pack.url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = resp.content_length().or(pack.size).unwrap_or(0);
    let mut file = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut got: u64 = 0;
    let mut stream = resp.bytes_stream();
    let mut last_emit = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        got += chunk.len() as u64;
        if total > 0 && got - last_emit > total / 100 {
            last_emit = got;
            emit("Downloading", got as f32 / total as f32 * 0.9);
        }
    }
    drop(file);
    let digest: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if !digest.eq_ignore_ascii_case(pack.sha256.trim()) {
        let _ = std::fs::remove_file(&zip_path);
        return Err("Download was corrupted (checksum mismatch). Please try again.".into());
    }

    emit("Unpacking", 0.92);
    let target = downloads_dir.join(id.key());
    let staging = downloads_dir.join(format!("{}.new", id.key()));
    let _ = std::fs::remove_dir_all(&staging);
    let zp = zip_path.clone();
    let st = staging.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let f = std::fs::File::open(&zp).map_err(|e| e.to_string())?;
        let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
        z.extract(&st).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = std::fs::remove_file(&zip_path);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&staging, &target).map_err(|e| e.to_string())?;

    let exe = convert_core::engines::find_in_tree(&target, id.binary_names(), 4)
        .ok_or_else(|| format!("The {} pack didn't contain an executable", id.name()))?;
    #[cfg(unix)]
    make_tree_executable(&target);
    emit("Installed", 1.0);
    Ok(exe)
}

/// Zip extraction may drop exec bits; restore them for anything in a bin folder or at the root.
#[cfg(unix)]
fn make_tree_executable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            make_tree_executable(&p);
        } else if p.extension().is_none() || p.extension().map(|x| x == "dylib" || x == "so").unwrap_or(false) {
            if let Ok(m) = std::fs::metadata(&p) {
                let mut perm = m.permissions();
                perm.set_mode(perm.mode() | 0o755);
                let _ = std::fs::set_permissions(&p, perm);
            }
        }
    }
}
