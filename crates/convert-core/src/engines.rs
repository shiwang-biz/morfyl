//! Conversion engines: what they are, where to find them, what they can do.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineId {
    ImageMagick,
    FFmpeg,
    LibreOffice,
    Pandoc,
    Calibre,
    Ghostscript,
    SevenZip,
}

/// How an engine reaches the user's machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Delivery {
    /// Shipped inside the app installer.
    Bundled,
    /// Small portable pack the app downloads on first use.
    OnDemand,
    /// Too large to ship; the user installs it (we detect it) or points us at it.
    System,
}

impl EngineId {
    pub const ALL: [EngineId; 7] = [
        EngineId::ImageMagick,
        EngineId::FFmpeg,
        EngineId::LibreOffice,
        EngineId::Pandoc,
        EngineId::Calibre,
        EngineId::Ghostscript,
        EngineId::SevenZip,
    ];

    pub fn key(self) -> &'static str {
        match self {
            EngineId::ImageMagick => "imagemagick",
            EngineId::FFmpeg => "ffmpeg",
            EngineId::LibreOffice => "libreoffice",
            EngineId::Pandoc => "pandoc",
            EngineId::Calibre => "calibre",
            EngineId::Ghostscript => "ghostscript",
            EngineId::SevenZip => "sevenzip",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EngineId::ImageMagick => "ImageMagick",
            EngineId::FFmpeg => "FFmpeg",
            EngineId::LibreOffice => "LibreOffice",
            EngineId::Pandoc => "Pandoc",
            EngineId::Calibre => "Calibre",
            EngineId::Ghostscript => "Ghostscript",
            EngineId::SevenZip => "7-Zip",
        }
    }

    pub fn purpose(self) -> &'static str {
        match self {
            EngineId::ImageMagick => "Images: JPG, PNG, WebP, AVIF, HEIC, RAW, PSD and 100+ more",
            EngineId::FFmpeg => "Video and audio: MP4, MOV, MKV, WebM, GIF, MP3, WAV, FLAC",
            EngineId::LibreOffice => "Office files: Word, Excel, PowerPoint, OpenDocument to and from PDF",
            EngineId::Pandoc => "Markdown, HTML, LaTeX, reStructuredText, DOCX, EPUB",
            EngineId::Calibre => "Ebooks: EPUB, MOBI, AZW3, FB2",
            EngineId::Ghostscript => "PDF: to images, to text, compress, grayscale",
            EngineId::SevenZip => "Archives: ZIP, 7Z, RAR, TAR, GZ, XZ",
        }
    }

    pub fn license(self) -> &'static str {
        match self {
            EngineId::ImageMagick => "ImageMagick License (Apache-2.0 style)",
            EngineId::FFmpeg => "LGPL-2.1+ (GPL if built with x264/x265)",
            EngineId::LibreOffice => "MPL-2.0",
            EngineId::Pandoc => "GPL-2.0+",
            EngineId::Calibre => "GPL-3.0",
            EngineId::Ghostscript => "AGPL-3.0",
            EngineId::SevenZip => "LGPL-2.1 + unRAR restriction",
        }
    }

    pub fn delivery(self) -> Delivery {
        match self {
            EngineId::ImageMagick | EngineId::FFmpeg => Delivery::Bundled,
            EngineId::Pandoc | EngineId::Ghostscript | EngineId::SevenZip => Delivery::OnDemand,
            EngineId::LibreOffice | EngineId::Calibre => Delivery::System,
        }
    }

    pub fn download_page(self) -> &'static str {
        match self {
            EngineId::ImageMagick => "https://imagemagick.org/script/download.php",
            EngineId::FFmpeg => "https://ffmpeg.org/download.html",
            EngineId::LibreOffice => "https://www.libreoffice.org/download/download-libreoffice/",
            EngineId::Pandoc => "https://pandoc.org/installing.html",
            EngineId::Calibre => "https://calibre-ebook.com/download",
            EngineId::Ghostscript => "https://ghostscript.com/releases/gsdnld.html",
            EngineId::SevenZip => "https://www.7-zip.org/download.html",
        }
    }

    /// Executable names to look for, most preferred first (without `.exe`).
    pub fn binary_names(self) -> &'static [&'static str] {
        match self {
            // `convert` is ImageMagick 6. Never use it on Windows: it is a built-in disk tool there.
            #[cfg(windows)]
            EngineId::ImageMagick => &["magick"],
            #[cfg(not(windows))]
            EngineId::ImageMagick => &["magick", "convert"],
            EngineId::FFmpeg => &["ffmpeg"],
            EngineId::LibreOffice => &["soffice", "libreoffice"],
            EngineId::Pandoc => &["pandoc"],
            EngineId::Calibre => &["ebook-convert"],
            #[cfg(windows)]
            EngineId::Ghostscript => &["gswin64c", "gswin32c", "gs"],
            #[cfg(not(windows))]
            EngineId::Ghostscript => &["gs"],
            EngineId::SevenZip => &["7zz", "7z", "7za"],
        }
    }

    /// Well-known install locations. GUI apps on macOS do not inherit the shell PATH,
    /// so Homebrew and app-bundle paths must be checked explicitly.
    pub fn known_paths(self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        #[cfg(target_os = "macos")]
        {
            match self {
                EngineId::LibreOffice => out.push("/Applications/LibreOffice.app/Contents/MacOS/soffice".into()),
                EngineId::Calibre => out.push("/Applications/calibre.app/Contents/MacOS/ebook-convert".into()),
                _ => {}
            }
            for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"] {
                for n in self.binary_names() {
                    out.push(Path::new(dir).join(n));
                }
            }
        }
        #[cfg(windows)]
        {
            let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
            let pf86 = std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into());
            let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
            let pf = Path::new(&pf);
            match self {
                EngineId::LibreOffice => {
                    out.push(pf.join(r"LibreOffice\program\soffice.exe"));
                    out.push(Path::new(&pf86).join(r"LibreOffice\program\soffice.exe"));
                }
                EngineId::Calibre => {
                    out.push(pf.join(r"Calibre2\ebook-convert.exe"));
                    out.push(Path::new(&pf86).join(r"Calibre2\ebook-convert.exe"));
                }
                EngineId::SevenZip => out.push(pf.join(r"7-Zip\7z.exe")),
                EngineId::Pandoc => out.push(Path::new(&local).join(r"Pandoc\pandoc.exe")),
                EngineId::Ghostscript => out.extend(versioned_dirs(&pf.join("gs"), r"bin\gswin64c.exe")),
                EngineId::ImageMagick => out.extend(prefixed_dirs(pf, "ImageMagick", "magick.exe")),
                EngineId::FFmpeg => {}
            }
        }
        #[cfg(target_os = "linux")]
        {
            for dir in ["/usr/bin", "/usr/local/bin", "/snap/bin"] {
                for n in self.binary_names() {
                    out.push(Path::new(dir).join(n));
                }
            }
        }
        out
    }
}

#[cfg(windows)]
fn versioned_dirs(parent: &Path, rel: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(parent)
        .map(|rd| rd.flatten().map(|e| e.path().join(rel)).collect())
        .unwrap_or_default();
    v.sort();
    v.reverse(); // newest version first
    v
}

#[cfg(windows)]
fn prefixed_dirs(parent: &Path, prefix: &str, exe: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(parent)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
                .map(|e| e.path().join(exe))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.reverse();
    v
}

pub fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Custom,
    Bundled,
    Downloaded,
    System,
}

#[derive(Debug, Clone, Serialize)]
pub struct Located {
    pub path: PathBuf,
    pub source: Source,
}

/// Finds engine executables. Search order: user override, bundled with the app,
/// downloaded engine packs, well-known install locations, PATH.
#[derive(Debug, Clone, Default)]
pub struct Locator {
    pub bundled_dirs: Vec<PathBuf>,
    pub downloads_dir: Option<PathBuf>,
    pub overrides: HashMap<EngineId, PathBuf>,
}

impl Locator {
    pub fn find(&self, id: EngineId) -> Option<Located> {
        if let Some(p) = self.overrides.get(&id) {
            if p.is_file() {
                return Some(Located { path: p.clone(), source: Source::Custom });
            }
        }
        for dir in &self.bundled_dirs {
            for n in id.binary_names() {
                let p = dir.join(exe_name(n));
                if p.is_file() {
                    return Some(Located { path: p, source: Source::Bundled });
                }
                // Bundled engines may live in their own sub-folder (engines/ffmpeg/ffmpeg).
                let p = dir.join(id.key()).join(exe_name(n));
                if p.is_file() {
                    return Some(Located { path: p, source: Source::Bundled });
                }
            }
        }
        if let Some(dl) = &self.downloads_dir {
            if let Some(p) = find_in_tree(&dl.join(id.key()), id.binary_names(), 4) {
                return Some(Located { path: p, source: Source::Downloaded });
            }
        }
        for p in id.known_paths() {
            if p.is_file() {
                return Some(Located { path: p, source: Source::System });
            }
        }
        for n in id.binary_names() {
            if let Ok(p) = which::which(n) {
                return Some(Located { path: p, source: Source::System });
            }
        }
        None
    }
}

/// Depth-limited search for the first matching executable inside an extracted pack.
pub fn find_in_tree(root: &Path, names: &[&str], depth: usize) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }
    for n in names {
        let p = root.join(exe_name(n));
        if p.is_file() {
            return Some(p);
        }
    }
    if depth == 0 {
        return None;
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // Prefer "bin" folders.
    dirs.sort_by_key(|p| p.file_name().map(|n| n != "bin").unwrap_or(true));
    dirs.into_iter().find_map(|d| find_in_tree(&d, names, depth - 1))
}

/// What a located engine can actually do (builds differ: slim vs full, LGPL vs GPL).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Caps {
    pub version: Option<String>,
    /// ImageMagick: upper-case format names it can write.
    pub writable: Option<HashSet<String>>,
    /// FFmpeg: encoder names (libx264, h264_videotoolbox, aac ...).
    pub encoders: Option<HashSet<String>>,
}

impl Caps {
    pub fn has_encoder(&self, name: &str) -> bool {
        self.encoders.as_ref().map(|e| e.contains(name)).unwrap_or(false)
    }

    /// For ImageMagick output formats. Unknown capability lists allow everything.
    pub fn can_write(&self, format: &str) -> bool {
        let Some(w) = &self.writable else { return true };
        let f = match format {
            "jpg" => "JPEG",
            "tiff" => "TIFF",
            other => return w.contains(&other.to_ascii_uppercase()),
        };
        w.contains(f)
    }
}

pub fn command(program: &Path) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(program);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: don't flash a console for every conversion.
        c.creation_flags(0x0800_0000);
    }
    c.kill_on_drop(true);
    c
}

async fn output_of(program: &Path, args: &[&str]) -> Option<String> {
    let out = command(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    Some(s)
}

/// Ask an engine for its version and capabilities. Cheap enough to run once per launch.
pub async fn probe(id: EngineId, path: &Path) -> Caps {
    let mut caps = Caps::default();
    let version_args: &[&str] = match id {
        EngineId::ImageMagick | EngineId::FFmpeg => &["-version"],
        EngineId::Ghostscript => &["--version"],
        EngineId::SevenZip => &[],
        _ => &["--version"],
    };
    // LibreOffice --version starts a full office instance on some platforms; skip it.
    if id != EngineId::LibreOffice {
        if let Some(out) = output_of(path, version_args).await {
            caps.version = out.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.chars().take(120).collect());
        }
    }
    match id {
        EngineId::ImageMagick => {
            if let Some(out) = output_of(path, &["-list", "format"]).await {
                caps.writable = Some(parse_magick_formats(&out));
            }
        }
        EngineId::FFmpeg => {
            if let Some(out) = output_of(path, &["-hide_banner", "-encoders"]).await {
                caps.encoders = Some(parse_ffmpeg_encoders(&out));
            }
        }
        _ => {}
    }
    caps
}

/// Parses `magick -list format` rows like `     AVIF* HEIC      rw+   AV1 Image File Format`.
pub fn parse_magick_formats(out: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in out.lines() {
        let mut it = line.split_whitespace();
        let (Some(name), Some(_module), Some(mode)) = (it.next(), it.next(), it.next()) else { continue };
        if !(mode.len() == 3 && mode.chars().all(|c| matches!(c, 'r' | 'w' | '+' | '-'))) {
            continue;
        }
        if mode.contains('w') {
            set.insert(name.trim_end_matches('*').to_ascii_uppercase());
        }
    }
    set
}

/// Parses `ffmpeg -encoders` rows like ` V....D libx264   libx264 H.264 ...`.
pub fn parse_ffmpeg_encoders(out: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    let mut started = false;
    for line in out.lines() {
        if line.trim_start().starts_with("------") {
            started = true;
            continue;
        }
        if !started {
            continue;
        }
        let mut it = line.split_whitespace();
        if let (Some(flags), Some(name)) = (it.next(), it.next()) {
            if flags.len() == 6 {
                set.insert(name.to_string());
            }
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_magick_list() {
        let out = "   Format  Module    Mode  Description\n-----\n      AVIF  HEIC      rw+   AV1 Image\n      HEIC  HEIC      r--   HEIC\n      JPEG* JPEG      rw-   Joint\n";
        let s = parse_magick_formats(out);
        assert!(s.contains("AVIF") && s.contains("JPEG") && !s.contains("HEIC"));
    }

    #[test]
    fn parses_ffmpeg_encoders() {
        let out = "Encoders:\n V..... = Video\n ------\n V....D libx264              libx264 H.264\n A....D aac                  AAC\n";
        let s = parse_ffmpeg_encoders(out);
        assert!(s.contains("libx264") && s.contains("aac"));
    }
}
