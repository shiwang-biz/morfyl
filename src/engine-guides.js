// What each engine is for, and how to set it up by hand on macOS and Windows.
//
// Step format: a string (may contain <b>, <i>, and <a data-url="..."> links), or
// { text, cmd } where `cmd` is shown as a copyable command.

const HOMEBREW_STEPS = [
  "Open <b>Terminal</b>: press <b>⌘ Space</b>, type <i>Terminal</i> and press <b>Return</b>.",
  {
    text: "If you don't have <a data-url='https://brew.sh'>Homebrew</a> yet (a free, widely used installer for Mac tools), paste this and press Return. It asks for your Mac password (nothing shows while you type, that's normal) and takes a few minutes:",
    cmd: '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"',
  },
  {
    text: "On Apple Silicon Macs, when Homebrew finishes, run this once so Terminal can find it (Homebrew also shows it under <i>Next steps</i>):",
    cmd: 'eval "$(/opt/homebrew/bin/brew shellenv)"',
  },
];

const MAC_CHIP =
  "Not sure which Mac you have? Click the <b>Apple menu  → About This Mac</b>. <b>Chip: Apple M1/M2/M3/M4…</b> means <i>Apple Silicon</i>; <b>Processor: Intel…</b> means <i>Intel</i>.";

const FINISH = "Come back to Morfyl and click <b>Check again</b> at the top of this page. The card turns green when Morfyl finds it.";

/** "I want to convert…" → engine, shown at the top of the Engines page. */
export const WHICH_ENGINE = [
  { what: "Photos and images: JPG, PNG, iPhone HEIC, WebP, camera RAW, icons", engine: "imagemagick" },
  { what: "Video and audio: MP4, MOV, MKV, MP3, WAV, GIFs from video", engine: "ffmpeg" },
  { what: "Word, Excel, PowerPoint, OpenDocument, CSV, and Office files to PDF", engine: "libreoffice" },
  { what: "Markdown, HTML, LaTeX and notes to Word, web pages or EPUB", engine: "pandoc" },
  { what: "PDF to images or text, smaller PDFs, grayscale PDFs", engine: "ghostscript" },
  { what: "Ebooks: EPUB, MOBI, Kindle AZW3, FB2", engine: "calibre" },
  { what: "Archives: ZIP, RAR, 7Z, TAR, GZ", engine: "sevenzip" },
];

export const GUIDES = {
  imagemagick: {
    tagline: "Converts and resizes photos and images.",
    uses: [
      ["iPhone photo (HEIC)", "JPG", "so anyone can open it"],
      ["Large photo (JPG)", "WebP or smaller JPG", "for websites and WhatsApp"],
      ["Logo (PNG)", "ICO", "a website or app icon"],
      ["Camera RAW (CR2, NEF, ARW, DNG)", "JPG", "share straight from your camera"],
      ["Any image", "PDF", "to email or print"],
    ],
    tip: "Use <b>Options → Remove metadata</b> to strip location (GPS) data before sharing photos.",
    builtIn: true,
  },
  ffmpeg: {
    tagline: "Converts and shrinks video and audio.",
    uses: [
      ["iPhone video (MOV)", "MP4", "plays on every phone, PC and TV"],
      ["Big video", "smaller MP4", "choose 720p or Small file in Options"],
      ["Video", "MP3", "keep only the sound"],
      ["Screen recording", "animated GIF", "for chats and docs"],
      ["WAV / FLAC / M4A", "MP3", "music for any player"],
    ],
    builtIn: true,
  },
  libreoffice: {
    tagline: "Converts Office documents, spreadsheets and presentations.",
    uses: [
      ["Word (DOCX, DOC)", "PDF", "send a document that looks the same everywhere"],
      ["Excel (XLSX)", "PDF or CSV", "share or import into other tools"],
      ["PowerPoint (PPTX)", "PDF", "a handout anyone can open"],
      ["Old DOC / XLS / PPT", "DOCX / XLSX / PPTX", "modern Office formats"],
      ["CSV", "Excel (XLSX)", "open data as a spreadsheet"],
    ],
    tip: "Also needed for <b>Markdown → PDF</b> (together with Pandoc).",
    size: "about 300 MB",
    setup: {
      macos: [
        MAC_CHIP,
        "Open <a data-url='https://www.libreoffice.org/download/download-libreoffice/'>libreoffice.org/download</a>. Under the big download button, pick <b>macOS (Apple Silicon)</b> or <b>macOS (Intel x86-64)</b> to match your Mac, then click <b>Download</b>.",
        "Open the downloaded <b>.dmg</b> file from your Downloads folder.",
        "In the window that opens, drag the <b>LibreOffice</b> icon onto the <b>Applications</b> folder.",
        "Open <b>LibreOffice</b> once from Applications. If macOS asks <i>\"Are you sure you want to open it?\"</i>, click <b>Open</b>. Then you can quit it.",
        FINISH,
      ],
      windows: [
        "Open <a data-url='https://www.libreoffice.org/download/download-libreoffice/'>libreoffice.org/download</a>. Make sure it says <b>Windows (64-bit)</b> or <b>Windows x86-64</b>, then click <b>Download</b>.",
        "Open the downloaded <b>.msi</b> file and click <b>Next</b>, choose <b>Typical</b>, then <b>Install</b>. Click <b>Yes</b> if Windows asks for permission.",
        "Click <b>Finish</b> when it's done.",
        { text: "Prefer a command? Open <b>PowerShell</b> and run:", cmd: "winget install --id TheDocumentFoundation.LibreOffice -e" },
        FINISH,
      ],
    },
    paths: {
      macos: ["/Applications/LibreOffice.app", "~/Applications/LibreOffice.app"],
      windows: ["C:\\Program Files\\LibreOffice\\program\\soffice.exe"],
    },
    locate: {
      macos: "choose <b>LibreOffice</b> in your Applications folder.",
      windows: "choose <b>soffice.exe</b> inside the LibreOffice\\program folder.",
    },
  },
  pandoc: {
    tagline: "Converts text documents between Markdown, HTML, Word, LaTeX and EPUB.",
    uses: [
      ["Markdown notes (MD)", "Word (DOCX)", "turn notes into a proper document"],
      ["Markdown or HTML", "web page (HTML)", "a clean, standalone page"],
      ["Word (DOCX)", "Markdown", "for websites, blogs and GitHub"],
      ["Markdown", "EPUB", "make your own ebook"],
      ["LaTeX (TEX)", "Word (DOCX)", "share papers with non-LaTeX users"],
    ],
    tip: "Markdown → PDF also needs LibreOffice.",
    size: "about 30 MB",
    setup: {
      macos: [
        MAC_CHIP,
        "Open the <a data-url='https://github.com/jgm/pandoc/releases/latest'>Pandoc download page</a> and scroll to <b>Assets</b>.",
        "Download the file ending in <b>arm64-macOS.pkg</b> (Apple Silicon) or <b>x86_64-macOS.pkg</b> (Intel).",
        "Open the <b>.pkg</b> file and click <b>Continue → Install</b>. Enter your Mac password when asked.",
        { text: "Or, if you use Homebrew, run this in Terminal instead:", cmd: "brew install pandoc" },
        FINISH,
      ],
      windows: [
        "Open the <a data-url='https://github.com/jgm/pandoc/releases/latest'>Pandoc download page</a> and scroll to <b>Assets</b>.",
        "Download the file ending in <b>windows-x86_64.msi</b>.",
        "Open it and click <b>Next → Install → Finish</b>. It installs just for you, so no admin password is needed.",
        { text: "Prefer a command? Open <b>PowerShell</b> and run:", cmd: "winget install --id JohnMacFarlane.Pandoc -e" },
        FINISH,
      ],
    },
    paths: {
      macos: ["/usr/local/bin/pandoc", "/opt/homebrew/bin/pandoc"],
      windows: ["%LOCALAPPDATA%\\Pandoc\\pandoc.exe", "anywhere on your PATH"],
    },
    locate: {
      macos: "choose the <b>pandoc</b> file (usually in /usr/local/bin; press <b>⌘ Shift G</b> in the file picker to type a path).",
      windows: "choose <b>pandoc.exe</b>.",
    },
  },
  ghostscript: {
    tagline: "Works with PDF files: turns pages into images, shrinks and converts PDFs.",
    uses: [
      ["PDF", "PNG or JPG", "one image per page"],
      ["Large PDF", "smaller PDF", "fits email size limits"],
      ["Color PDF", "grayscale PDF", "for black-and-white printing"],
      ["PDF", "text (TXT)", "copy the words out of a PDF"],
      ["PDF", "TIFF", "one multi-page image for archiving or fax"],
    ],
    size: "about 50 MB",
    setup: {
      macos: [
        "Ghostscript has no Mac installer of its own; the easy way is Homebrew.",
        ...HOMEBREW_STEPS,
        { text: "Then install Ghostscript:", cmd: "brew install ghostscript" },
        FINISH,
      ],
      windows: [
        "Open <a data-url='https://ghostscript.com/releases/gsdnld.html'>ghostscript.com/releases</a>.",
        "In the table, find <b>Ghostscript AGPL Release</b> for <b>Windows (64 bit)</b> and click it to download.",
        "Open the downloaded <b>.exe</b>, click <b>Yes</b> if Windows asks for permission, then <b>Next → I Agree → Install</b> with the default options, and <b>Finish</b>.",
        { text: "Prefer a command? Open <b>PowerShell</b> and run:", cmd: "winget install --id ArtifexSoftware.GhostScript -e" },
        FINISH,
      ],
    },
    paths: {
      macos: ["/opt/homebrew/bin/gs", "/usr/local/bin/gs"],
      windows: ["C:\\Program Files\\gs\\gs<version>\\bin\\gswin64c.exe"],
    },
    locate: {
      macos: "choose the <b>gs</b> file (in /opt/homebrew/bin; press <b>⌘ Shift G</b> in the file picker to type a path).",
      windows: "choose <b>gswin64c.exe</b> in the Ghostscript <b>bin</b> folder.",
    },
  },
  calibre: {
    tagline: "Converts ebooks between EPUB, Kindle, PDF and more.",
    uses: [
      ["EPUB", "Kindle (AZW3)", "read on a Kindle"],
      ["MOBI / AZW3", "EPUB", "for Apple Books, Google Play Books, Kobo"],
      ["Ebook", "PDF", "print it or read on any device"],
      ["Ebook", "Word (DOCX)", "edit the text"],
      ["FB2 (FictionBook)", "EPUB", "open it in any ebook app"],
    ],
    size: "about 250–350 MB",
    setup: {
      macos: [
        "Open <a data-url='https://calibre-ebook.com/download_osx'>calibre-ebook.com/download_osx</a> and click <b>Download calibre</b>. The same file works on Apple Silicon and Intel Macs.",
        "Open the downloaded <b>.dmg</b> file. If it shows a licence, click <b>Agree</b>.",
        "Drag the <b>calibre</b> icon onto the <b>Applications</b> folder.",
        "Open <b>calibre</b> once from Applications (click <b>Open</b> if macOS asks), then you can quit it. You don't need to set up a library for Morfyl.",
        FINISH,
      ],
      windows: [
        "Open <a data-url='https://calibre-ebook.com/download_windows'>calibre-ebook.com/download_windows</a> and click <b>Download calibre 64bit</b>.",
        "Open the downloaded <b>.msi</b>, tick <b>I accept</b>, click <b>Install</b> (and <b>Yes</b> if Windows asks), then <b>Finish</b>.",
        { text: "Prefer a command? Open <b>PowerShell</b> and run:", cmd: "winget install --id calibre.calibre -e" },
        FINISH,
      ],
    },
    paths: {
      macos: ["/Applications/calibre.app", "~/Applications/calibre.app"],
      windows: ["C:\\Program Files\\Calibre2\\ebook-convert.exe"],
    },
    locate: {
      macos: "choose <b>calibre</b> in your Applications folder.",
      windows: "choose <b>ebook-convert.exe</b> inside the Calibre2 folder.",
    },
  },
  sevenzip: {
    tagline: "Opens and creates ZIP, RAR, 7Z and TAR archives.",
    uses: [
      ["RAR or 7Z you received", "folder", "get the files out"],
      ["TAR.GZ / TGZ", "folder", "unpack downloads from Linux and developers"],
      ["Any archive", "ZIP", "re-pack in the format everyone can open"],
      ["ZIP", "7Z", "usually a smaller file"],
    ],
    size: "about 2 MB",
    setup: {
      macos: [
        "The easiest way on a Mac is Homebrew.",
        ...HOMEBREW_STEPS,
        { text: "Then install 7-Zip:", cmd: "brew install sevenzip" },
        FINISH,
      ],
      windows: [
        "Open <a data-url='https://www.7-zip.org/download.html'>7-zip.org/download</a>.",
        "In the first row, click <b>Download</b> next to <b>64-bit Windows x64</b>.",
        "Open the downloaded <b>.exe</b>, click <b>Yes</b> if Windows asks for permission, then <b>Install</b> and <b>Close</b>.",
        { text: "Prefer a command? Open <b>PowerShell</b> and run:", cmd: "winget install --id 7zip.7zip -e" },
        FINISH,
      ],
    },
    paths: {
      macos: ["/opt/homebrew/bin/7zz", "/usr/local/bin/7zz"],
      windows: ["C:\\Program Files\\7-Zip\\7z.exe"],
    },
    locate: {
      macos: "choose the <b>7zz</b> file (in /opt/homebrew/bin; press <b>⌘ Shift G</b> in the file picker to type a path).",
      windows: "choose <b>7z.exe</b> in C:\\Program Files\\7-Zip.",
    },
  },
};
