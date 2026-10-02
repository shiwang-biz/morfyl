//! File categories, input detection and the output formats each category offers.

use crate::engines::EngineId;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Image,
    Video,
    Audio,
    Document,
    Spreadsheet,
    Presentation,
    Markup,
    Ebook,
    Pdf,
    Archive,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Image => "Image",
            Category::Video => "Video",
            Category::Audio => "Audio",
            Category::Document => "Document",
            Category::Spreadsheet => "Spreadsheet",
            Category::Presentation => "Presentation",
            Category::Markup => "Text / Markup",
            Category::Ebook => "Ebook",
            Category::Pdf => "PDF",
            Category::Archive => "Archive",
        }
    }
}

const IMAGE_IN: &[&str] = &[
    "jpg", "jpeg", "jfif", "png", "gif", "webp", "bmp", "tif", "tiff", "ico", "avif", "heic", "heif",
    "jxl", "jp2", "j2k", "psd", "tga", "svg", "hdr", "dds", "pcx", "ppm", "pgm", "pbm", "pnm",
    "qoi", "xcf", // camera RAW (read only, needs libraw)
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "dng", "orf", "raf", "rw2", "pef", "srw",
];
const VIDEO_IN: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "avi", "webm", "flv", "wmv", "mpeg", "mpg", "3gp", "3g2", "ts", "mts",
    "m2ts", "vob", "ogv",
];
const AUDIO_IN: &[&str] = &[
    "mp3", "wav", "aac", "m4a", "flac", "ogg", "oga", "opus", "wma", "aiff", "aif", "alac", "amr", "ac3",
    "mka",
];
const DOC_IN: &[&str] = &["doc", "docx", "odt", "rtf", "txt", "wpd", "pages"];
const SHEET_IN: &[&str] = &["xls", "xlsx", "ods", "csv", "tsv", "numbers"];
const SLIDES_IN: &[&str] = &["ppt", "pptx", "odp", "key"];
const MARKUP_IN: &[&str] = &["md", "markdown", "html", "htm", "tex", "rst", "org", "textile", "ipynb"];
const EBOOK_IN: &[&str] = &["epub", "mobi", "azw", "azw3", "fb2", "lit", "pdb"];
const ARCHIVE_IN: &[&str] = &["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz", "zst", "iso", "cab"];

/// Lowercased extension, treating `.tar.gz` style double extensions as their compressed form.
pub fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn detect(path: &Path) -> Option<Category> {
    let ext = extension(path);
    let ext = ext.as_str();
    let in_list = |list: &[&str]| list.contains(&ext);
    Some(if ext == "pdf" {
        Category::Pdf
    } else if in_list(IMAGE_IN) {
        Category::Image
    } else if in_list(VIDEO_IN) {
        Category::Video
    } else if in_list(AUDIO_IN) {
        Category::Audio
    } else if in_list(DOC_IN) {
        Category::Document
    } else if in_list(SHEET_IN) {
        Category::Spreadsheet
    } else if in_list(SLIDES_IN) {
        Category::Presentation
    } else if in_list(MARKUP_IN) {
        Category::Markup
    } else if in_list(EBOOK_IN) {
        Category::Ebook
    } else if in_list(ARCHIVE_IN) {
        Category::Archive
    } else {
        return None;
    })
}

/// One choice in the "convert to" menu.
#[derive(Debug, Clone, Serialize)]
pub struct OutputChoice {
    /// Extension-like id ("png", "mp4", "folder" for archive extraction, "pdf-compressed" ...).
    pub format: &'static str,
    pub label: &'static str,
    /// Engines that must all be available for this conversion (in pipeline order).
    pub engines: Vec<EngineId>,
}

/// Document formats Pandoc can read without help.
pub fn pandoc_reads(ext: &str) -> bool {
    matches!(ext, "docx" | "odt" | "rtf")
}

fn c(format: &'static str, label: &'static str, engines: &[EngineId]) -> OutputChoice {
    OutputChoice { format, label, engines: engines.to_vec() }
}

/// Every output this app knows how to produce for a given input file.
/// The caller filters by which engines are installed and what they can write.
pub fn outputs_for(path: &Path) -> Vec<OutputChoice> {
    use EngineId::*;
    let ext = extension(path);
    let Some(cat) = detect(path) else { return vec![] };
    let mut v = match cat {
        Category::Image => vec![
            c("png", "PNG", &[ImageMagick]),
            c("jpg", "JPG", &[ImageMagick]),
            c("webp", "WebP", &[ImageMagick]),
            c("avif", "AVIF", &[ImageMagick]),
            c("heic", "HEIC", &[ImageMagick]),
            c("jxl", "JPEG XL", &[ImageMagick]),
            c("gif", "GIF", &[ImageMagick]),
            c("tiff", "TIFF", &[ImageMagick]),
            c("bmp", "BMP", &[ImageMagick]),
            c("ico", "ICO (icon)", &[ImageMagick]),
            c("pdf", "PDF", &[ImageMagick]),
        ],
        Category::Video => vec![
            c("mp4", "MP4 (H.264)", &[FFmpeg]),
            c("mov", "MOV", &[FFmpeg]),
            c("mkv", "MKV", &[FFmpeg]),
            c("webm", "WebM (VP9)", &[FFmpeg]),
            c("avi", "AVI", &[FFmpeg]),
            c("gif", "Animated GIF", &[FFmpeg]),
            c("mp3", "MP3 (audio only)", &[FFmpeg]),
            c("m4a", "M4A / AAC (audio only)", &[FFmpeg]),
            c("wav", "WAV (audio only)", &[FFmpeg]),
            c("flac", "FLAC (audio only)", &[FFmpeg]),
            c("ogg", "OGG Vorbis (audio only)", &[FFmpeg]),
            c("opus", "Opus (audio only)", &[FFmpeg]),
        ],
        Category::Audio => vec![
            c("mp3", "MP3", &[FFmpeg]),
            c("m4a", "M4A / AAC", &[FFmpeg]),
            c("wav", "WAV", &[FFmpeg]),
            c("flac", "FLAC", &[FFmpeg]),
            c("ogg", "OGG Vorbis", &[FFmpeg]),
            c("opus", "Opus", &[FFmpeg]),
            c("aiff", "AIFF", &[FFmpeg]),
        ],
        Category::Document => vec![
            c("pdf", "PDF", &[LibreOffice]),
            c("docx", "Word (DOCX)", &[LibreOffice]),
            c("doc", "Word 97 (DOC)", &[LibreOffice]),
            c("odt", "OpenDocument (ODT)", &[LibreOffice]),
            c("rtf", "RTF", &[LibreOffice]),
            c("txt", "Plain text", &[LibreOffice]),
            c("html", "HTML", &[LibreOffice]),
            // Pandoc reads DOCX/ODT/RTF directly; older formats go through LibreOffice first.
            c("md", "Markdown", if pandoc_reads(&ext) { &[Pandoc] } else { &[LibreOffice, Pandoc] }),
            c("epub", "EPUB", if pandoc_reads(&ext) { &[Pandoc] } else { &[LibreOffice, Pandoc] }),
        ],
        Category::Spreadsheet => vec![
            c("xlsx", "Excel (XLSX)", &[LibreOffice]),
            c("xls", "Excel 97 (XLS)", &[LibreOffice]),
            c("ods", "OpenDocument (ODS)", &[LibreOffice]),
            c("csv", "CSV", &[LibreOffice]),
            c("pdf", "PDF", &[LibreOffice]),
            c("html", "HTML", &[LibreOffice]),
        ],
        Category::Presentation => vec![
            c("pptx", "PowerPoint (PPTX)", &[LibreOffice]),
            c("ppt", "PowerPoint 97 (PPT)", &[LibreOffice]),
            c("odp", "OpenDocument (ODP)", &[LibreOffice]),
            c("pdf", "PDF", &[LibreOffice]),
        ],
        Category::Markup => vec![
            c("html", "HTML", &[Pandoc]),
            c("docx", "Word (DOCX)", &[Pandoc]),
            c("odt", "OpenDocument (ODT)", &[Pandoc]),
            c("pdf", "PDF", &[Pandoc, LibreOffice]),
            c("epub", "EPUB", &[Pandoc]),
            c("md", "Markdown", &[Pandoc]),
            c("rst", "reStructuredText", &[Pandoc]),
            c("tex", "LaTeX", &[Pandoc]),
            c("rtf", "RTF", &[Pandoc]),
            c("txt", "Plain text", &[Pandoc]),
        ],
        Category::Ebook => vec![
            c("epub", "EPUB", &[Calibre]),
            c("mobi", "MOBI", &[Calibre]),
            c("azw3", "Kindle (AZW3)", &[Calibre]),
            c("pdf", "PDF", &[Calibre]),
            c("docx", "Word (DOCX)", &[Calibre]),
            c("txt", "Plain text", &[Calibre]),
            c("fb2", "FB2", &[Calibre]),
        ],
        Category::Pdf => vec![
            c("png", "PNG (one per page)", &[Ghostscript]),
            c("jpg", "JPG (one per page)", &[Ghostscript]),
            c("tiff", "TIFF (multi-page)", &[Ghostscript]),
            c("txt", "Plain text", &[Ghostscript]),
            c("pdf-compressed", "Smaller PDF", &[Ghostscript]),
            c("pdf-grayscale", "Grayscale PDF", &[Ghostscript]),
        ],
        Category::Archive => vec![
            c("folder", "Extract to folder", &[SevenZip]),
            c("zip", "ZIP", &[SevenZip]),
            c("7z", "7Z", &[SevenZip]),
            c("tar", "TAR", &[SevenZip]),
        ],
    };
    // Never offer a same-format conversion, except where it means something (re-encode/compress).
    let same = |f: &str| f == ext || (f == "jpg" && ext == "jpeg") || (f == "tiff" && ext == "tif") || (f == "md" && ext == "markdown") || (f == "html" && ext == "htm");
    // (MP4/JPG/WebP -> same format is kept: it means "re-encode smaller / resize".)
    v.retain(|o| !same(o.format) || matches!(o.format, "mp4" | "jpg" | "webp"));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_categories() {
        assert_eq!(detect(Path::new("a/B.JPG")), Some(Category::Image));
        assert_eq!(detect(Path::new("clip.MOV")), Some(Category::Video));
        assert_eq!(detect(Path::new("x.pdf")), Some(Category::Pdf));
        assert_eq!(detect(Path::new("x.tar.gz")), Some(Category::Archive));
        assert_eq!(detect(Path::new("noext")), None);
    }

    #[test]
    fn excludes_same_format() {
        let outs = outputs_for(Path::new("a.png"));
        assert!(!outs.iter().any(|o| o.format == "png"));
        assert!(outs.iter().any(|o| o.format == "webp"));
        let outs = outputs_for(Path::new("a.mp4"));
        assert!(outs.iter().any(|o| o.format == "mp4"), "mp4 re-encode is allowed");
    }
}
