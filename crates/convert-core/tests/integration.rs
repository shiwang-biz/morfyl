//! End-to-end conversions against whatever engines are installed on this machine.
//! Engines that aren't found are skipped, so this is safe to run anywhere:
//!   cargo test -p convert-core --test integration -- --nocapture

use convert_core::*;
use std::path::{Path, PathBuf};
use tokio::sync::watch;

async fn toolbox() -> Toolbox {
    let loc = Locator::default();
    let mut tb = Toolbox::default();
    for id in EngineId::ALL {
        if let Some(l) = loc.find(id) {
            let caps = engines::probe(id, &l.path).await;
            eprintln!("engine {:<12} {} ({})", id.key(), l.path.display(), caps.version.clone().unwrap_or_default());
            tb.engines.insert(id, (l.path, caps));
        }
    }
    tb
}

fn workdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("morfyl-it").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

async fn convert(tb: &Toolbox, input: &Path, format: &str) -> Result<Vec<PathBuf>, Error> {
    let job = Job { input: input.to_path_buf(), format: format.into(), options: Options::default() };
    let scratch = std::env::temp_dir().join("morfyl-it-scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let p = plan(&job, tb, &scratch)?;
    for l in p.command_lines() {
        eprintln!("  $ {l}");
    }
    let (_tx, rx) = watch::channel(false);
    let last = std::sync::Arc::new(std::sync::Mutex::new(0f32));
    let l2 = last.clone();
    let out = run(&p, move |Event::Progress(v)| *l2.lock().unwrap() = v, rx).await?;
    eprintln!("  -> {:?} (progress {:.2})", out, *last.lock().unwrap());
    for o in &out {
        assert!(o.exists(), "missing output {o:?}");
        if o.is_file() {
            assert!(std::fs::metadata(o).unwrap().len() > 0, "empty output {o:?}");
        }
    }
    Ok(out)
}

fn sh(program: &Path, args: &[&str]) {
    let st = std::process::Command::new(program).args(args).output().unwrap();
    assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stderr));
}

#[tokio::test(flavor = "multi_thread")]
async fn end_to_end() {
    let tb = toolbox().await;
    let dir = workdir("e2e");
    let mut ran = 0;

    if let Some((ff, _)) = tb.engines.get(&EngineId::FFmpeg) {
        let video = dir.join("clip.mov");
        sh(ff, &["-y", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=2", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-shortest", "-c:v", "mpeg4", "-c:a", "aac", video.to_str().unwrap()]);
        for f in ["mp4", "webm", "mkv", "gif", "mp3", "wav", "flac", "ogg", "opus", "m4a"] {
            match convert(&tb, &video, f).await {
                Ok(_) => ran += 1,
                Err(Error::NotAvailable(m)) => eprintln!("  (skipped {f}: {m})"),
                Err(e) => panic!("video -> {f}: {e}"),
            }
        }
        let audio = dir.join("tone.wav");
        sh(ff, &["-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", audio.to_str().unwrap()]);
        convert(&tb, &audio, "flac").await.unwrap();
        ran += 1;
    }

    if let Some((im, _)) = tb.engines.get(&EngineId::ImageMagick) {
        let img = dir.join("photo.png");
        sh(im, &["-size", "300x200", "gradient:red-blue", img.to_str().unwrap()]);
        for f in ["jpg", "webp", "gif", "bmp", "tiff", "ico", "pdf", "avif"] {
            match convert(&tb, &img, f).await {
                Ok(_) => ran += 1,
                Err(Error::NotAvailable(m)) => eprintln!("  (skipped {f}: {m})"),
                Err(e) => panic!("image -> {f}: {e}"),
            }
        }
        // Resize + same-name collision handling
        let job = Job {
            input: img.clone(),
            format: "jpg".into(),
            options: Options { max_width: Some(100), ..Options::default() },
        };
        let p = plan(&job, &tb, &std::env::temp_dir()).unwrap();
        assert!(p.outputs[0].to_string_lossy().contains("photo (1).jpg"));
        let (_tx, rx) = watch::channel(false);
        run(&p, |_| {}, rx).await.unwrap();
    }

    if tb.engines.contains_key(&EngineId::Pandoc) {
        let md = dir.join("notes.md");
        std::fs::write(&md, "# Hello\n\nSome *markdown* with a [link](https://example.com).\n\n- one\n- two\n").unwrap();
        for f in ["html", "docx", "odt", "epub", "rst", "txt", "tex"] {
            convert(&tb, &md, f).await.unwrap();
            ran += 1;
        }
        if tb.engines.contains_key(&EngineId::LibreOffice) {
            let pdf = convert(&tb, &md, "pdf").await.unwrap();
            ran += 1;
            let docx = dir.join("notes.docx");
            for f in ["pdf", "odt", "txt", "md"] {
                convert(&tb, &docx, f).await.unwrap();
                ran += 1;
            }
            if tb.engines.contains_key(&EngineId::Ghostscript) {
                for f in ["png", "jpg", "txt", "pdf-compressed"] {
                    convert(&tb, &pdf[0], f).await.unwrap();
                    ran += 1;
                }
            }
        }
    }

    if tb.engines.contains_key(&EngineId::LibreOffice) {
        let csv = dir.join("table.csv");
        std::fs::write(&csv, "name,qty\napple,3\npear,5\n").unwrap();
        for f in ["xlsx", "ods", "pdf"] {
            convert(&tb, &csv, f).await.unwrap();
            ran += 1;
        }
    }

    if tb.engines.contains_key(&EngineId::SevenZip) {
        let src = dir.join("bundle");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), "hello").unwrap();
        std::fs::write(src.join("sub/b.txt"), "world").unwrap();
        let tgz = dir.join("bundle.tar.gz");
        sh(Path::new("tar"), &["-czf", tgz.to_str().unwrap(), "-C", dir.to_str().unwrap(), "bundle"]);
        let folder = convert(&tb, &tgz, "folder").await.unwrap();
        assert!(folder[0].join("bundle/sub/b.txt").is_file(), "tar.gz should be fully unwrapped");
        let zip = convert(&tb, &tgz, "zip").await.unwrap();
        for f in ["7z", "tar", "folder"] {
            let out = convert(&tb, &zip[0], f).await.unwrap();
            if f == "folder" {
                assert!(out[0].join("bundle/a.txt").is_file());
            }
        }
        ran += 5;
    }

    eprintln!("ran {ran} conversions");
    assert!(ran > 0 || tb.engines.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_kills_process() {
    let tb = toolbox().await;
    let Some((ff, _)) = tb.engines.get(&EngineId::FFmpeg) else { return };
    let dir = workdir("cancel");
    let video = dir.join("long.mkv");
    sh(ff, &["-y", "-f", "lavfi", "-i", "testsrc=size=1280x720:rate=30:duration=20", "-c:v", "mpeg4", "-q:v", "2", video.to_str().unwrap()]);
    let job = Job { input: video, format: "webm".into(), options: Options::default() };
    let Ok(p) = plan(&job, &tb, &std::env::temp_dir()) else { return };
    let (tx, rx) = watch::channel(false);
    let h = tokio::spawn({
        let p = p.clone();
        async move { run(&p, |_| {}, rx).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    tx.send(true).unwrap();
    let res = tokio::time::timeout(std::time::Duration::from_secs(5), h).await.expect("cancel hung").unwrap();
    assert!(matches!(res, Err(Error::Cancelled)), "{res:?}");
    assert!(!p.outputs[0].exists(), "partial output should be removed");
}
