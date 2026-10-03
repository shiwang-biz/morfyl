//! One-click engine installs.
//!
//! Two kinds of downloads, both listed in a manifest published by
//! `.github/workflows/engine-packs.yml` (a GitHub release tagged "engines"):
//!
//! * **packs**: small zips we build ourselves (Pandoc, Ghostscript, 7-Zip). SHA-256 verified.
//! * **installers**: the official LibreOffice / Calibre installers (.dmg on macOS, .msi on
//!   Windows), found by `scripts/engines/resolve-installers.py`. They are unpacked into
//!   Morfyl's own data folder, so no admin password is needed and nothing is added to the
//!   system: removing that folder removes them.
//!
//! manifest.json:
//! { "engines":    { "pandoc":      { "macos-aarch64": { "url", "sha256", "size" } } },
//!   "installers": { "libreoffice": { "macos-aarch64": { "url", "kind": "dmg", "version", "size", "sha256"? } } } }

use convert_core::EngineId;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Override at build time: MORFYL_ENGINES_URL=https://.../manifest.json cargo tauri build
pub const MANIFEST_URL: &str = match option_env!("MORFYL_ENGINES_URL") {
    Some(u) => u,
    None => "https://github.com/shiwang-biz/morfyl/releases/download/engines/manifest.json",
};

#[derive(Deserialize, Default)]
struct Manifest {
    #[serde(default)]
    engines: HashMap<String, HashMap<String, Pack>>,
    #[serde(default)]
    installers: HashMap<String, HashMap<String, Installer>>,
}

#[derive(Deserialize, Clone)]
struct Pack {
    url: String,
    sha256: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Deserialize, Clone)]
struct Installer {
    url: String,
    kind: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    sha256: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct InstallProgress {
    pub id: EngineId,
    pub stage: String,
    /// 0..1, or negative while the step has no measurable progress.
    pub progress: f32,
}

/// What the Engines tab shows before installing: how big the download is.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub available: bool,
    pub size: Option<u64>,
    pub version: Option<String>,
}

pub fn platform_key() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Cancels in-flight installs (the "Cancel" button, or app shutdown).
#[derive(Clone, Default)]
pub struct CancelFlag(pub Arc<AtomicBool>);

impl CancelFlag {
    fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::Relaxed) {
            Err("Cancelled".into())
        } else {
            Ok(())
        }
    }
}

async fn fetch_manifest(http: &reqwest::Client) -> Result<Manifest, String> {
    http.get(MANIFEST_URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Couldn't reach the engine list. Check your internet connection. ({e})"))?
        .json()
        .await
        .map_err(|e| format!("The engine list is malformed: {e}"))
}

/// Download sizes for every engine that can be installed on this computer.
pub async fn offers(http: &reqwest::Client) -> HashMap<String, Offer> {
    let Ok(m) = fetch_manifest(http).await else { return HashMap::new() };
    let pk = platform_key();
    let mut out = HashMap::new();
    for (k, per) in &m.engines {
        if let Some(p) = per.get(&pk) {
            out.insert(k.clone(), Offer { available: true, size: p.size, version: None });
        }
    }
    for (k, per) in &m.installers {
        if let Some(i) = per.get(&pk) {
            out.insert(k.clone(), Offer { available: true, size: i.size, version: i.version.clone() });
        }
    }
    out
}

pub async fn install(
    app: &AppHandle,
    http: &reqwest::Client,
    downloads_dir: &Path,
    id: EngineId,
    cancel: CancelFlag,
) -> Result<PathBuf, String> {
    let emit = |stage: &str, progress: f32| {
        let _ = app.emit("engine-progress", InstallProgress { id, stage: stage.to_string(), progress });
    };
    emit("Getting download details", -1.0);
    let manifest = fetch_manifest(http).await?;
    let pk = platform_key();
    std::fs::create_dir_all(downloads_dir).map_err(|e| e.to_string())?;

    let result = if let Some(pack) = manifest.engines.get(id.key()).and_then(|m| m.get(&pk)).cloned() {
        install_pack(&emit, http, downloads_dir, id, pack, &cancel).await
    } else if let Some(inst) = manifest.installers.get(id.key()).and_then(|m| m.get(&pk)).cloned() {
        install_official(&emit, http, downloads_dir, id, inst, &cancel).await
    } else {
        Err(format!(
            "Automatic install of {} isn't available for this computer yet. Use the install guide instead.",
            id.name()
        ))
    };
    // Never leave half-finished downloads behind.
    let _ = std::fs::remove_file(downloads_dir.join(format!("{}.download", id.key())));
    let _ = std::fs::remove_dir_all(downloads_dir.join(format!("{}.new", id.key())));
    if result.is_ok() {
        emit("Installed", 1.0);
    }
    result
}

/// Streams `url` to `dest`, reporting 0..`share` of the overall progress. Returns the SHA-256.
async fn download(
    emit: &impl Fn(&str, f32),
    http: &reqwest::Client,
    url: &str,
    expected_size: Option<u64>,
    dest: &Path,
    share: f32,
    cancel: &CancelFlag,
) -> Result<String, String> {
    let resp = http
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = resp.content_length().or(expected_size).filter(|t| *t > 0);
    let mut file = std::fs::File::create(dest).map_err(|e| format!("Can't save the download: {e}"))?;
    let mut hasher = Sha256::new();
    let mut got: u64 = 0;
    let mut last = 0u64;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        cancel.check()?;
        let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| format!("Can't save the download (is the disk full?): {e}"))?;
        got += chunk.len() as u64;
        if got - last > 512 * 1024 {
            last = got;
            match total {
                Some(t) => emit(
                    &format!("Downloading {} of {}", mb(got), mb(t)),
                    (got as f32 / t as f32).min(1.0) * share,
                ),
                None => emit(&format!("Downloading {}", mb(got)), -1.0),
            }
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn mb(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / 1_048_576.0)
}

// ------------------------------------------------------------------ packs

async fn install_pack(
    emit: &impl Fn(&str, f32),
    http: &reqwest::Client,
    downloads_dir: &Path,
    id: EngineId,
    pack: Pack,
    cancel: &CancelFlag,
) -> Result<PathBuf, String> {
    let zip_path = downloads_dir.join(format!("{}.download", id.key()));
    let digest = download(emit, http, &pack.url, pack.size, &zip_path, 0.9, cancel).await?;
    if !digest.eq_ignore_ascii_case(pack.sha256.trim()) {
        return Err("The download was corrupted (checksum mismatch). Please try again.".into());
    }
    emit("Unpacking", 0.92);
    let staging = downloads_dir.join(format!("{}.new", id.key()));
    let _ = std::fs::remove_dir_all(&staging);
    let (zp, st) = (zip_path.clone(), staging.clone());
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let f = std::fs::File::open(&zp).map_err(|e| e.to_string())?;
        let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
        z.extract(&st).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    #[cfg(unix)]
    make_tree_executable(&staging);
    finish(downloads_dir, id, &staging)
}

/// Swap the freshly unpacked folder into place and confirm it contains the program.
fn finish(downloads_dir: &Path, id: EngineId, staging: &Path) -> Result<PathBuf, String> {
    let target = downloads_dir.join(id.key());
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(staging, &target).map_err(|e| e.to_string())?;
    convert_core::engines::find_in_tree(&target, id.binary_names(), 6)
        .ok_or_else(|| format!("{} was installed but its program wasn't found", id.name()))
}

/// Zip extraction may drop exec bits; restore them on files with no extension (Unix binaries).
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

// -------------------------------------------------------- official installers

async fn install_official(
    emit: &impl Fn(&str, f32),
    http: &reqwest::Client,
    downloads_dir: &Path,
    id: EngineId,
    inst: Installer,
    cancel: &CancelFlag,
) -> Result<PathBuf, String> {
    let file = downloads_dir.join(format!("{}.download", id.key()));
    let digest = download(emit, http, &inst.url, inst.size, &file, 0.8, cancel).await?;
    if let Some(expected) = &inst.sha256 {
        if !digest.eq_ignore_ascii_case(expected.trim()) {
            return Err("The download was corrupted (checksum mismatch). Please try again.".into());
        }
    }
    cancel.check()?;
    let staging = downloads_dir.join(format!("{}.new", id.key()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    emit(&format!("Installing {} (this can take a minute)", id.name()), 0.85);
    match inst.kind.as_str() {
        "dmg" => install_dmg(&file, &staging).await?,
        "msi" => install_msi(&file, &staging).await?,
        other => return Err(format!("Unknown installer type {other}")),
    }
    finish(downloads_dir, id, &staging)
}

/// macOS: mount the disk image read-only, copy the .app out with `ditto` (keeps its code
/// signature intact), unmount.
#[cfg(target_os = "macos")]
async fn install_dmg(dmg: &Path, dest: &Path) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let mount = dest.join(".mount");
    std::fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
    let mut attach = tokio::process::Command::new("/usr/bin/hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"])
        .arg(&mount)
        .arg(dmg)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't open the disk image: {e}"))?;
    // Some disk images show a licence; agree to it so attach doesn't wait forever.
    if let Some(mut stdin) = attach.stdin.take() {
        let _ = stdin.write_all(b"Y\n").await;
    }
    let out = attach.wait_with_output().await.map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("Couldn't open the disk image: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let copy = (|| -> Result<(), String> {
        let app = std::fs::read_dir(&mount)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().map(|x| x == "app").unwrap_or(false) && !p.is_symlink())
            .ok_or("The disk image doesn't contain an app")?;
        let status = std::process::Command::new("/usr/bin/ditto")
            .arg(&app)
            .arg(dest.join(app.file_name().unwrap()))
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("Copying the app failed. Is the disk full?".into())
        }
    })();
    let _ = tokio::process::Command::new("/usr/bin/hdiutil")
        .args(["detach", "-force"])
        .arg(&mount)
        .output()
        .await;
    let _ = std::fs::remove_dir(&mount);
    copy
}

/// Windows: an "administrative install" (`msiexec /a`) unpacks the program files into a folder
/// without registering anything or asking for admin rights.
#[cfg(windows)]
async fn install_msi(msi: &Path, dest: &Path) -> Result<(), String> {
    // msiexec only accepts PROPERTY="value" when the quotes surround the value, so pass it raw.
    let mut cmd = tokio::process::Command::new("msiexec.exe");
    cmd.arg("/a").arg(msi).arg("/qn").raw_arg(format!("TARGETDIR=\"{}\"", dest.display()));
    cmd.creation_flags(0x0800_0000); // no console window
    let status = cmd.status().await.map_err(|e| format!("Couldn't start Windows Installer: {e}"))?;
    match status.code() {
        Some(0) | Some(3010) => Ok(()),
        Some(1618) => Err("Another installation is running. Wait for it to finish, then try again.".into()),
        Some(code) => Err(format!("Windows Installer failed (code {code}). Try the manual install guide.")),
        None => Err("Windows Installer was stopped".into()),
    }
}

#[cfg(not(target_os = "macos"))]
async fn install_dmg(_: &Path, _: &Path) -> Result<(), String> {
    Err("Disk images can only be installed on macOS".into())
}

#[cfg(not(windows))]
async fn install_msi(_: &Path, _: &Path) -> Result<(), String> {
    Err("MSI installers can only be installed on Windows".into())
}

/// Delete an engine Morfyl installed (only ever touches Morfyl's own folder).
pub fn uninstall(downloads_dir: &Path, id: EngineId) -> Result<(), String> {
    let dir = downloads_dir.join(id.key());
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}
