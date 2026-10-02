//! Turns "convert this file to that format with these options" into concrete engine commands.

use crate::engines::{Caps, EngineId};
use crate::formats::{self, Category};
use crate::util::{file_url, stem, unique_path};
use crate::Error;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    High,
    #[default]
    Medium,
    Low,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    /// None = next to the source file.
    pub output_dir: Option<PathBuf>,
    /// 1-100, for JPG/WebP/AVIF/HEIC/JXL and PDF->JPG.
    pub image_quality: u8,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub strip_metadata: bool,
    pub video_quality: Quality,
    /// Downscale video to at most this height (e.g. 1080, 720).
    pub video_max_height: Option<u32>,
    /// kbps for lossy audio.
    pub audio_bitrate: u32,
    /// Resolution for PDF -> image.
    pub pdf_dpi: u32,
    pub gif_fps: u32,
    pub gif_width: u32,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            output_dir: None,
            image_quality: 85,
            max_width: None,
            max_height: None,
            strip_metadata: false,
            video_quality: Quality::Medium,
            video_max_height: None,
            audio_bitrate: 192,
            pdf_dpi: 150,
            gif_fps: 12,
            gif_width: 480,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressKind {
    None,
    /// `-progress pipe:1` output plus the `Duration:` line on stderr.
    Ffmpeg,
    /// Lines that start with `NN%` (7-Zip -bsp1, Calibre).
    Percent,
}

#[derive(Debug, Clone)]
pub enum Step {
    Exec {
        engine: EngineId,
        program: PathBuf,
        args: Vec<OsString>,
        cwd: Option<PathBuf>,
        progress: ProgressKind,
    },
    Mkdir(PathBuf),
    /// Move an engine's output into place. Missing `from` means the engine silently failed.
    MoveFile { from: PathBuf, to: PathBuf, engine: EngineId },
    /// If `dir` holds a single `.tar` (from .tar.gz etc.), extract it in place.
    UnwrapTar { dir: PathBuf, sevenzip: PathBuf },
    /// Move everything in `from` into a new folder `to`.
    MoveDir { from: PathBuf, to: PathBuf },
    /// Page images were written into `dir`; if there is only one page, replace the folder with `single`.
    CollapsePages { dir: PathBuf, single: PathBuf },
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub steps: Vec<Step>,
    /// Final files/folders, best guess before running (CollapsePages may change it).
    pub outputs: Vec<PathBuf>,
    /// Scratch folder deleted after the run, success or not.
    pub temp_dir: Option<PathBuf>,
}

impl Plan {
    /// Human-readable command lines, for logs and the "show command" button.
    pub fn command_lines(&self) -> Vec<String> {
        self.steps
            .iter()
            .filter_map(|s| match s {
                Step::Exec { program, args, .. } => {
                    let mut line = quote(&program.to_string_lossy());
                    for a in args {
                        line.push(' ');
                        line.push_str(&quote(&a.to_string_lossy()));
                    }
                    Some(line)
                }
                _ => None,
            })
            .collect()
    }
}

fn quote(s: &str) -> String {
    if s.chars().any(|c| c.is_whitespace() || "\"'()[];&|<>*?$".contains(c)) {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// The engines available right now, with what each can do.
#[derive(Debug, Clone, Default)]
pub struct Toolbox {
    pub engines: HashMap<EngineId, (PathBuf, Caps)>,
}

impl Toolbox {
    fn get(&self, id: EngineId) -> Result<(&Path, &Caps), Error> {
        self.engines
            .get(&id)
            .map(|(p, c)| (p.as_path(), c))
            .ok_or(Error::MissingEngine(id))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub input: PathBuf,
    pub format: String,
    #[serde(default)]
    pub options: Options,
}

fn os<S: Into<OsString>>(s: S) -> OsString {
    s.into()
}

macro_rules! args {
    ($($e:expr),* $(,)?) => { vec![$(os($e)),*] };
}

pub fn plan(job: &Job, tools: &Toolbox, scratch_root: &Path) -> Result<Plan, Error> {
    let input = &job.input;
    if !input.is_file() {
        return Err(Error::Unsupported(format!("{} is not a file", input.display())));
    }
    let cat = formats::detect(input).ok_or_else(|| Error::Unsupported("Unknown file type".into()))?;
    let choice = formats::outputs_for(input)
        .into_iter()
        .find(|c| c.format == job.format)
        .ok_or_else(|| Error::Unsupported(format!("{} -> {} is not supported", cat.label(), job.format)))?;
    for e in &choice.engines {
        tools.get(*e)?;
    }
    let out_dir = match &job.options.output_dir {
        Some(d) => d.clone(),
        None => input.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")),
    };
    std::fs::create_dir_all(&out_dir)?;
    let ctx = Ctx {
        input,
        ext: formats::extension(input),
        stem: stem(input),
        out_dir: &out_dir,
        opt: &job.options,
        fmt: job.format.as_str(),
        tools,
        scratch_root,
    };
    match cat {
        Category::Image => ctx.image(),
        Category::Video | Category::Audio => ctx.media(cat),
        Category::Pdf => ctx.pdf(),
        Category::Document | Category::Spreadsheet | Category::Presentation => ctx.office(),
        Category::Markup => ctx.markup(),
        Category::Ebook => ctx.ebook(),
        Category::Archive => ctx.archive(),
    }
}

struct Ctx<'a> {
    input: &'a Path,
    ext: String,
    stem: String,
    out_dir: &'a Path,
    opt: &'a Options,
    fmt: &'a str,
    tools: &'a Toolbox,
    scratch_root: &'a Path,
}

impl Ctx<'_> {
    fn out(&self, ext: &str) -> PathBuf {
        unique_path(self.out_dir, &self.stem, Some(ext))
    }

    fn out_named(&self, suffix: &str, ext: &str) -> PathBuf {
        unique_path(self.out_dir, &format!("{} {suffix}", self.stem), Some(ext))
    }

    fn scratch(&self) -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        unique_path(self.scratch_root, &format!("job-{n}"), None)
    }

    fn exec(&self, engine: EngineId, args: Vec<OsString>, progress: ProgressKind) -> Result<Step, Error> {
        let (program, _) = self.tools.get(engine)?;
        Ok(Step::Exec { engine, program: program.to_path_buf(), args, cwd: None, progress })
    }

    // ---------------------------------------------------------------- images

    fn image(&self) -> Result<Plan, Error> {
        let (_, caps) = self.tools.get(EngineId::ImageMagick)?;
        if !caps.can_write(self.fmt) {
            return Err(Error::NotAvailable(format!(
                "This ImageMagick build can't write {}. Install a full ImageMagick and select it under Engines.",
                self.fmt.to_uppercase()
            )));
        }
        let o = self.opt;
        let out = self.out(self.fmt);
        let mut a: Vec<OsString> = Vec::new();
        if self.ext == "svg" {
            a.extend(args!["-background", "none", "-density", "192"]);
        }
        // Formats that keep every frame/page; everything else takes the first frame
        // (animated GIF -> PNG, layered PSD -> flattened composite).
        let keeps_frames = matches!(self.fmt, "gif" | "webp" | "tiff" | "pdf");
        let multi_in = matches!(self.ext.as_str(), "gif" | "tif" | "tiff" | "ico" | "psd" | "webp");
        let mut input = self.input.as_os_str().to_owned();
        if multi_in && (!keeps_frames || self.ext == "psd") {
            input.push("[0]");
        }
        a.push(input);
        a.push(os("-auto-orient"));
        if self.ext == "gif" && keeps_frames {
            a.push(os("-coalesce"));
        }
        if o.strip_metadata {
            a.push(os("-strip"));
        }
        if o.max_width.is_some() || o.max_height.is_some() {
            let w = o.max_width.map(|v| v.to_string()).unwrap_or_default();
            let h = o.max_height.map(|v| v.to_string()).unwrap_or_default();
            a.extend(args!["-resize", format!("{w}x{h}>")]);
        }
        match self.fmt {
            "jpg" => a.extend(args!["-background", "white", "-alpha", "remove", "-alpha", "off", "-quality", o.image_quality.to_string()]),
            "webp" | "avif" | "heic" | "jxl" => a.extend(args!["-quality", o.image_quality.to_string()]),
            "ico" => a.extend(args!["-resize", "256x256>", "-define", "icon:auto-resize=256,128,64,48,32,16"]),
            "tiff" => a.extend(args!["-compress", "lzw"]),
            "pdf" => a.extend(args!["-compress", "jpeg", "-quality", o.image_quality.to_string()]),
            _ => {}
        }
        a.push(out.clone().into());
        Ok(Plan {
            steps: vec![self.exec(EngineId::ImageMagick, a, ProgressKind::None)?],
            outputs: vec![out],
            temp_dir: None,
        })
    }

    // ---------------------------------------------------------- video/audio

    fn media(&self, cat: Category) -> Result<Plan, Error> {
        let (_, caps) = self.tools.get(EngineId::FFmpeg)?;
        let o = self.opt;
        let out = self.out(self.fmt);
        let mut a: Vec<OsString> = args!["-hide_banner", "-nostdin", "-y", "-progress", "pipe:1", "-nostats", "-i", self.input];
        let abr = format!("{}k", o.audio_bitrate.clamp(32, 512));
        let scale = o.video_max_height.map(|h| format!("scale=-2:'min({h},ih)'"));

        let audio_only = |a: &mut Vec<OsString>, fmt: &str| -> Result<(), Error> {
            a.push(os("-vn"));
            match fmt {
                "mp3" => {
                    need(caps, "libmp3lame", "MP3")?;
                    a.extend(args!["-c:a", "libmp3lame", "-b:a", &abr]);
                }
                "m4a" => a.extend(args!["-c:a", "aac", "-b:a", &abr]),
                "wav" => a.extend(args!["-c:a", "pcm_s16le"]),
                "aiff" => a.extend(args!["-c:a", "pcm_s16be"]),
                "flac" => a.extend(args!["-c:a", "flac"]),
                "ogg" => {
                    if caps.has_encoder("libvorbis") {
                        a.extend(args!["-c:a", "libvorbis", "-b:a", &abr]);
                    } else {
                        a.extend(args!["-c:a", "vorbis", "-strict", "-2", "-ac", "2"]);
                    }
                }
                "opus" => {
                    if caps.has_encoder("libopus") {
                        a.extend(args!["-c:a", "libopus", "-b:a", &abr]);
                    } else {
                        a.extend(args!["-c:a", "opus", "-strict", "-2", "-ar", "48000", "-b:a", &abr]);
                    }
                }
                other => return Err(Error::Unsupported(format!("audio format {other}"))),
            }
            Ok(())
        };

        match (cat, self.fmt) {
            (_, "mp3" | "m4a" | "wav" | "aiff" | "flac" | "ogg" | "opus") => audio_only(&mut a, self.fmt)?,
            (Category::Video, "mp4" | "mov" | "mkv") => {
                if let Some(s) = &scale {
                    a.extend(args!["-vf", s]);
                }
                a.extend(h264_args(caps, o)?);
                a.extend(args!["-c:a", "aac", "-b:a", &abr]);
                if self.fmt != "mkv" {
                    a.extend(args!["-movflags", "+faststart"]);
                }
            }
            (Category::Video, "webm") => {
                if let Some(s) = &scale {
                    a.extend(args!["-vf", s]);
                }
                let crf = match o.video_quality { Quality::High => "24", Quality::Medium => "32", Quality::Low => "40" };
                if caps.has_encoder("libvpx-vp9") {
                    a.extend(args!["-c:v", "libvpx-vp9", "-crf", crf, "-b:v", "0", "-row-mt", "1", "-deadline", "good", "-cpu-used", "4"]);
                } else if caps.has_encoder("libsvtav1") {
                    a.extend(args!["-c:v", "libsvtav1", "-crf", crf, "-preset", "8"]);
                } else {
                    return Err(Error::NotAvailable("This FFmpeg build has no VP9 or AV1 encoder".into()));
                }
                if caps.has_encoder("libopus") {
                    a.extend(args!["-c:a", "libopus", "-b:a", &abr]);
                } else {
                    a.extend(args!["-c:a", "opus", "-strict", "-2", "-ar", "48000"]);
                }
            }
            (Category::Video, "avi") => {
                if let Some(s) = &scale {
                    a.extend(args!["-vf", s]);
                }
                let q = match o.video_quality { Quality::High => "3", Quality::Medium => "5", Quality::Low => "8" };
                a.extend(args!["-c:v", "mpeg4", "-q:v", q, "-tag:v", "XVID"]);
                if caps.has_encoder("libmp3lame") {
                    a.extend(args!["-c:a", "libmp3lame", "-b:a", &abr]);
                } else {
                    a.extend(args!["-c:a", "ac3", "-b:a", &abr]);
                }
            }
            (Category::Video, "gif") => {
                let vf = format!(
                    "fps={},scale={}:-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4",
                    o.gif_fps.clamp(1, 50),
                    o.gif_width.clamp(64, 1920)
                );
                a.extend(args!["-vf", vf, "-loop", "0", "-an"]);
            }
            (_, f) => return Err(Error::Unsupported(format!("{} -> {f}", cat.label()))),
        }
        a.push(out.clone().into());
        Ok(Plan {
            steps: vec![self.exec(EngineId::FFmpeg, a, ProgressKind::Ffmpeg)?],
            outputs: vec![out],
            temp_dir: None,
        })
    }

    // ------------------------------------------------------------------ PDF

    fn pdf(&self) -> Result<Plan, Error> {
        let o = self.opt;
        let mut a: Vec<OsString> = args!["-dSAFER", "-dBATCH", "-dNOPAUSE", "-dQUIET"];
        let res = format!("-r{}", o.pdf_dpi.clamp(36, 1200));
        let mut steps = Vec::new();
        let outputs;
        match self.fmt {
            "png" | "jpg" => {
                let dir = unique_path(self.out_dir, &format!("{} pages", self.stem), None);
                if self.fmt == "png" {
                    a.extend(args!["-sDEVICE=png16m", "-dTextAlphaBits=4", "-dGraphicsAlphaBits=4", &res]);
                } else {
                    a.extend(args!["-sDEVICE=jpeg", format!("-dJPEGQ={}", o.image_quality.clamp(1, 100)), "-dTextAlphaBits=4", "-dGraphicsAlphaBits=4", &res]);
                }
                a.push(os("-o"));
                a.push(dir.join(format!("page-%03d.{}", self.fmt)).into());
                a.push(self.input.into());
                let single = self.out(self.fmt);
                steps.push(Step::Mkdir(dir.clone()));
                steps.push(self.exec(EngineId::Ghostscript, a, ProgressKind::None)?);
                steps.push(Step::CollapsePages { dir: dir.clone(), single });
                outputs = vec![dir];
            }
            "tiff" => {
                let out = self.out("tiff");
                a.extend(args!["-sDEVICE=tiff24nc", "-sCompression=lzw", &res, "-o", &out, self.input]);
                steps.push(self.exec(EngineId::Ghostscript, a, ProgressKind::None)?);
                outputs = vec![out];
            }
            "txt" => {
                let out = self.out("txt");
                a.extend(args!["-sDEVICE=txtwrite", "-o", &out, self.input]);
                steps.push(self.exec(EngineId::Ghostscript, a, ProgressKind::None)?);
                outputs = vec![out];
            }
            "pdf-compressed" | "pdf-grayscale" => {
                let gray = self.fmt == "pdf-grayscale";
                let out = self.out_named(if gray { "(grayscale)" } else { "(compressed)" }, "pdf");
                a.extend(args!["-sDEVICE=pdfwrite", "-dCompatibilityLevel=1.5", "-dDetectDuplicateImages=true"]);
                if gray {
                    a.extend(args!["-sColorConversionStrategy=Gray", "-dProcessColorModel=/DeviceGray"]);
                } else {
                    a.extend(args!["-dPDFSETTINGS=/ebook"]);
                }
                a.extend(args!["-o", &out, self.input]);
                steps.push(self.exec(EngineId::Ghostscript, a, ProgressKind::None)?);
                outputs = vec![out];
            }
            f => return Err(Error::Unsupported(format!("PDF -> {f}"))),
        }
        Ok(Plan { steps, outputs, temp_dir: None })
    }

    // --------------------------------------------------------------- office

    /// LibreOffice step that writes `<stem>.<ext>` into `out_dir`, using a private profile
    /// so it works even while the user has LibreOffice open.
    fn soffice(&self, input: &Path, filter: &str, scratch: &Path, out_dir: &Path) -> Result<Step, Error> {
        let profile = scratch.join("lo-profile");
        let a: Vec<OsString> = args![
            format!("-env:UserInstallation={}", file_url(&profile)),
            "--headless",
            "--norestore",
            "--nolockcheck",
            "--nodefault",
            "--convert-to",
            filter,
            "--outdir",
            out_dir,
            input,
        ];
        self.exec(EngineId::LibreOffice, a, ProgressKind::None)
    }

    fn office(&self) -> Result<Plan, Error> {
        if matches!(self.fmt, "md" | "epub") {
            return self.office_via_pandoc();
        }
        let filter = match self.fmt {
            "pdf" => "pdf",
            "docx" => "docx:MS Word 2007 XML",
            "doc" => "doc:MS Word 97",
            "odt" => "odt",
            "rtf" => "rtf",
            "txt" => "txt:Text (encoded):UTF8",
            "html" => "html",
            "xlsx" => "xlsx:Calc MS Excel 2007 XML",
            "xls" => "xls:MS Excel 97",
            "ods" => "ods",
            "csv" => "csv:Text - txt - csv (StarCalc):44,34,76,1",
            "pptx" => "pptx:Impress MS PowerPoint 2007 XML",
            "ppt" => "ppt:MS PowerPoint 97",
            "odp" => "odp",
            f => return Err(Error::Unsupported(format!("office -> {f}"))),
        };
        let scratch = self.scratch();
        let lo_out = scratch.join("out");
        let produced = lo_out.join(format!("{}.{}", self.input.file_stem().unwrap_or_default().to_string_lossy(), self.fmt));
        let out = self.out(self.fmt);
        Ok(Plan {
            steps: vec![
                Step::Mkdir(lo_out.clone()),
                self.soffice(self.input, filter, &scratch, &lo_out)?,
                Step::MoveFile { from: produced, to: out.clone(), engine: EngineId::LibreOffice },
            ],
            outputs: vec![out],
            temp_dir: Some(scratch),
        })
    }

    fn office_via_pandoc(&self) -> Result<Plan, Error> {
        let mut steps = Vec::new();
        let mut temp_dir = None;
        let source: PathBuf = if formats::pandoc_reads(&self.ext) {
            self.input.to_path_buf()
        } else {
            let scratch = self.scratch();
            let lo_out = scratch.join("out");
            steps.push(Step::Mkdir(lo_out.clone()));
            steps.push(self.soffice(self.input, "docx:MS Word 2007 XML", &scratch, &lo_out)?);
            let docx = lo_out.join(format!("{}.docx", self.input.file_stem().unwrap_or_default().to_string_lossy()));
            temp_dir = Some(scratch);
            docx
        };
        let out = self.out(self.fmt);
        let mut a: Vec<OsString> = args![&source, "-o", &out];
        if self.fmt == "md" {
            let media = unique_path(self.out_dir, &format!("{}_media", self.stem), None);
            a.extend(args!["-t", "gfm", "--extract-media", media]);
        } else {
            a.extend(args!["--metadata", format!("title={}", self.stem)]);
        }
        steps.push(self.exec(EngineId::Pandoc, a, ProgressKind::None)?);
        Ok(Plan { steps, outputs: vec![out], temp_dir })
    }

    // --------------------------------------------------------------- markup

    fn markup(&self) -> Result<Plan, Error> {
        let mut a: Vec<OsString> = args![self.input];
        match self.fmt {
            "pdf" => {
                // Pandoc's own PDF route needs a LaTeX install; go via DOCX + LibreOffice instead.
                let scratch = self.scratch();
                let lo_out = scratch.join("out");
                let docx = scratch.join(format!("{}.docx", self.stem));
                a.extend(args!["-s", "-o", &docx]);
                let produced = lo_out.join(format!("{}.pdf", self.stem));
                let out = self.out("pdf");
                return Ok(Plan {
                    steps: vec![
                        Step::Mkdir(lo_out.clone()),
                        self.exec(EngineId::Pandoc, a, ProgressKind::None)?,
                        self.soffice(&docx, "pdf", &scratch, &lo_out)?,
                        Step::MoveFile { from: produced, to: out.clone(), engine: EngineId::LibreOffice },
                    ],
                    outputs: vec![out],
                    temp_dir: Some(scratch),
                });
            }
            "html" => a.extend(args!["-s", "--metadata", format!("pagetitle={}", self.stem)]),
            "md" => a.extend(args!["-t", "gfm"]),
            "txt" => a.extend(args!["-t", "plain"]),
            "epub" => a.extend(args!["--metadata", format!("title={}", self.stem)]),
            "docx" | "odt" | "rst" | "tex" | "rtf" => a.push(os("-s")),
            f => return Err(Error::Unsupported(format!("markup -> {f}"))),
        }
        let out = self.out(self.fmt);
        a.extend(args!["-o", &out]);
        Ok(Plan {
            steps: vec![self.exec(EngineId::Pandoc, a, ProgressKind::None)?],
            outputs: vec![out],
            temp_dir: None,
        })
    }

    // ---------------------------------------------------------------- ebook

    fn ebook(&self) -> Result<Plan, Error> {
        let out = self.out(self.fmt);
        let a: Vec<OsString> = args![self.input, &out];
        Ok(Plan {
            steps: vec![self.exec(EngineId::Calibre, a, ProgressKind::Percent)?],
            outputs: vec![out],
            temp_dir: None,
        })
    }

    // -------------------------------------------------------------- archive

    fn archive(&self) -> Result<Plan, Error> {
        let (sz, _) = self.tools.get(EngineId::SevenZip)?;
        let sz = sz.to_path_buf();
        // Work inside the output folder so the final move is a cheap rename.
        let work = unique_path(self.out_dir, &format!(".fileforge-{}", self.stem), None);
        let extracted = work.join("x");
        let mut steps = vec![
            Step::Mkdir(extracted.clone()),
            self.exec(
                EngineId::SevenZip,
                args!["x", "-y", "-bsp1", "-bso0", format!("-o{}", extracted.display()), self.input],
                ProgressKind::Percent,
            )?,
            Step::UnwrapTar { dir: extracted.clone(), sevenzip: sz.clone() },
        ];
        let out;
        if self.fmt == "folder" {
            out = unique_path(self.out_dir, &self.stem, None);
            steps.push(Step::MoveDir { from: extracted, to: out.clone() });
        } else {
            let t = match self.fmt {
                "zip" => "zip",
                "7z" => "7z",
                "tar" => "tar",
                f => return Err(Error::Unsupported(format!("archive -> {f}"))),
            };
            out = self.out(self.fmt);
            steps.push(Step::Exec {
                engine: EngineId::SevenZip,
                program: sz,
                args: args!["a", format!("-t{t}"), "-y", "-bsp1", "-bso0", &out, "*"],
                cwd: Some(extracted),
                progress: ProgressKind::Percent,
            });
        }
        Ok(Plan { steps, outputs: vec![out], temp_dir: Some(work) })
    }
}

fn need(caps: &Caps, encoder: &str, what: &str) -> Result<(), Error> {
    if caps.encoders.is_none() || caps.has_encoder(encoder) {
        Ok(())
    } else {
        Err(Error::NotAvailable(format!("This FFmpeg build has no {what} encoder ({encoder})")))
    }
}

/// Best available H.264 encoder: x264 (GPL builds) > Apple VideoToolbox > Windows Media Foundation > OpenH264.
fn h264_args(caps: &Caps, o: &Options) -> Result<Vec<OsString>, Error> {
    let q = o.video_quality;
    // Hardware encoders take a bitrate; scale it with the target height.
    let base: f32 = match q { Quality::High => 8.0, Quality::Medium => 5.0, Quality::Low => 2.5 };
    let factor = match o.video_max_height { Some(h) if h <= 480 => 0.3, Some(h) if h <= 720 => 0.5, _ => 1.0 };
    let br = format!("{:.1}M", base * factor);
    if caps.encoders.is_none() || caps.has_encoder("libx264") {
        let crf = match q { Quality::High => "18", Quality::Medium => "23", Quality::Low => "28" };
        Ok(args!["-c:v", "libx264", "-preset", "medium", "-crf", crf, "-pix_fmt", "yuv420p"])
    } else if caps.has_encoder("h264_videotoolbox") {
        Ok(args!["-c:v", "h264_videotoolbox", "-b:v", br, "-allow_sw", "1", "-pix_fmt", "yuv420p"])
    } else if caps.has_encoder("h264_mf") {
        Ok(args!["-c:v", "h264_mf", "-b:v", br, "-rate_control", "cbr", "-pix_fmt", "nv12"])
    } else if caps.has_encoder("libopenh264") {
        Ok(args!["-c:v", "libopenh264", "-b:v", br, "-pix_fmt", "yuv420p"])
    } else {
        Err(Error::NotAvailable("This FFmpeg build has no H.264 encoder".into()))
    }
}
