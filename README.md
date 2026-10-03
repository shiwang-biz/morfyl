# Morfyl – File Converter

by **[Craziest Coders](https://craziestcoders.com)**

A desktop file converter for **macOS and Windows**, built with Tauri 2.
Drop in images, video, audio, documents, PDFs, ebooks or archives, pick an output format, convert.

| Input | Engine | Outputs |
|---|---|---|
| Images: JPG, PNG, WebP, AVIF, HEIC, GIF, TIFF, BMP, ICO, PSD, SVG, JP2, camera RAW (CR2/CR3/NEF/ARW/DNG…) | ImageMagick (built in) | JPG, PNG, WebP, AVIF, GIF, TIFF, BMP, ICO, PDF (+ HEIC/JXL if the build supports them) |
| Video: MP4, MOV, MKV, AVI, WebM, FLV, WMV, MPEG, 3GP, TS… | FFmpeg (built in) | MP4, MOV, MKV, WebM, AVI, animated GIF, or audio only |
| Audio: MP3, WAV, AAC/M4A, FLAC, OGG, Opus, WMA, AIFF… | FFmpeg (built in) | MP3, M4A, WAV, FLAC, OGG, Opus, AIFF |
| PDF | Ghostscript (downloads on first use) | PNG/JPG per page, multi-page TIFF, text, smaller PDF, grayscale PDF |
| Markdown, HTML, LaTeX, RST, Org, Jupyter | Pandoc (downloads on first use) | HTML, DOCX, ODT, EPUB, PDF, Markdown, RST, LaTeX, RTF, text |
| Archives: ZIP, 7Z, RAR, TAR, GZ, BZ2, XZ, ISO | 7-Zip (downloads on first use) | Extract to folder, ZIP, 7Z, TAR |
| Word, Excel, PowerPoint, OpenDocument, RTF, CSV | LibreOffice (user installs) | PDF, DOCX, XLSX, PPTX, ODT/ODS/ODP, CSV, HTML, text (+ Markdown/EPUB via Pandoc) |
| Ebooks: EPUB, MOBI, AZW3, FB2 | Calibre (user installs) | EPUB, MOBI, AZW3, PDF, DOCX, text |

Features: drag and drop files or whole folders, batch queue with parallel jobs, live progress for video/audio/archives/ebooks,
cancel, per-file or "convert all to" format, output folder choice, image quality/resize/strip-metadata, video quality and max
resolution, audio bitrate, GIF settings, PDF DPI, light and dark mode. Existing files are never overwritten (`photo (1).jpg`).

## How the size is kept small

* **Tauri** instead of Electron: the app shell is a few MB because it uses the system web view.
* **Custom engine builds.** `scripts/engines/build-ffmpeg.sh` and `build-imagemagick.sh` compile static, size-optimised
  binaries with only what Morfyl uses (FFmpeg keeps every decoder so it can open anything, but only the encoders,
  muxers and filters it needs; no network, devices or ffprobe). They are verified to depend only on OS libraries.
* **OS video encoders.** H.264 uses Apple VideoToolbox / Windows Media Foundation, so the default FFmpeg is LGPL and
  carries no x264. Set `GPL=1` to add x264 instead.
* **External engines are installed by the user.** Pandoc, Ghostscript, 7-Zip, LibreOffice and Calibre are installed
  with their own official installers. The Engines tab explains what each engine is for and has a step-by-step setup
  guide for Mac and Windows (`src/engine-guides.js`). Morfyl finds them automatically in Applications, Program Files,
  Homebrew folders and PATH; **Locate…** points it at any other copy.

Check the real sizes in the CI logs (each build script prints them); expect roughly 30–45 MB installers.

## Branding

* App name: **Morfyl – File Converter**; publisher **Craziest Coders**; bundle ID `com.craziestcoders.morfyl`
  (set in `src-tauri/tauri.conf.json`).
* The app shows a faint diagonal **craziestcoders.com** watermark over the window and a
  "by Craziest Coders" link in the top bar that opens craziestcoders.com.
  To change its strength, edit `.watermark span { opacity: … }` in `src/styles.css`;
  the text is in `index.html` (`class="watermark"`).
* Converted files are **not** watermarked.

## Project layout

```
crates/convert-core/    Rust library: format detection, engine discovery, command building, runner (no UI; fully tested)
src-tauri/              Tauri app: commands, events, engine downloader
src/ + index.html       UI (plain JS + CSS, no framework)
scripts/engines/        Slim ImageMagick + FFmpeg builds (versions pinned in versions.env)
.github/workflows/      ci.yml (tests), release.yml (installers)
```

## Run it locally

Prerequisites: [Rust](https://rustup.rs), Node 20+, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
(Xcode Command Line Tools on macOS, WebView2 + MSVC Build Tools on Windows).

```bash
npm install
npm run app:dev
```

In development the app uses engines already on your machine, so install a few:

* macOS: `brew install imagemagick ffmpeg pandoc ghostscript sevenzip` (+ LibreOffice / Calibre apps if wanted)
* Windows: `winget install ImageMagick.ImageMagick Gyan.FFmpeg JohnMacFarlane.Pandoc ArtifexSoftware.GhostScript 7zip.7zip`

Engine search order: path chosen with **Locate…** → bundled with the app → downloaded packs → standard install locations
(Homebrew, Program Files, /Applications) → PATH.

Tests (real conversions with whatever engines you have installed; missing ones are skipped):

```bash
cargo test -p convert-core -- --nocapture
```

## Build the installers (GitHub Actions)

1. Push this project to a GitHub repository.
3. **Tag a release:** `git tag v0.1.0 && git push --tags`. `release.yml` compiles ImageMagick + FFmpeg for
   macOS Apple Silicon, macOS Intel and Windows x64 (cached after the first run, which takes ~30–60 min),
   then builds `.dmg`, `.msi` and `-setup.exe` into a **draft release**.

You can also build the engines locally: on macOS run `bash scripts/engines/build-imagemagick.sh && bash scripts/engines/build-ffmpeg.sh`
(needs `brew install nasm meson ninja cmake autoconf automake libtool pkg-config`), on Windows run the same in an MSYS2 UCRT64 shell
with `TARGET_TRIPLE=x86_64-pc-windows-msvc`. Then `npm run app:build:release`.

### Code signing (recommended)

Without a Developer ID, the Mac build is ad-hoc signed (`"signingIdentity": "-"` in `tauri.conf.json`), so macOS asks
the user to confirm under **System Settings → Privacy & Security → Open Anyway**. (An app with no signature at all shows
"is damaged and can't be opened"; `xattr -cr /Applications/Morfyl.app` clears that.) Windows SmartScreen also warns on first launch.
When the `APPLE_SIGNING_IDENTITY` secret is set, it replaces the ad-hoc identity.

* **macOS:** add repository secrets `APPLE_CERTIFICATE` (base64 .p12 of a *Developer ID Application* cert),
  `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, and for notarization `APPLE_ID`, `APPLE_PASSWORD`
  (app-specific password), `APPLE_TEAM_ID`. The workflow picks them up automatically; the bundled engines are signed too.
* **Windows:** see https://v2.tauri.app/distribute/sign/windows/ (Azure Trusted Signing or an OV/EV certificate).

## Licences

Morfyl's own code: MIT. Engines run as separate programs; their licences are listed in `LICENSES/README.txt`
and their licence texts are copied into the app at build time. Points to know before selling a closed-source version:

* FFmpeg default build is **LGPL** (fine for closed source when shipped as a separate program, as here). `GPL=1` makes it GPL.
* **Ghostscript is AGPL.** Distributing it with a closed-source commercial product needs a licence from Artifex,
  or swap it for another PDF renderer.
* Pandoc (GPL) and 7-Zip (LGPL + unRAR rule) are downloaded as separate programs.

## Notes and known limits

* The engine build scripts pin exact versions in `scripts/engines/versions.env`. If a download URL moves,
  update the version there. The first CI run may need small fixes for new upstream releases.
* HEIC **writing** needs x265 (GPL), so the slim build reads HEIC but doesn't write it; the app hides formats
  the installed build can't produce. Choosing a full ImageMagick via **Locate…** unlocks them.
* GitHub's Intel macOS runner label is `macos-15-intel`; if GitHub retires it, drop that matrix row or cross-compile.
* PDF → Word isn't offered: good PDF-to-DOCX needs dedicated software.
