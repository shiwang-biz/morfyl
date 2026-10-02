//! Executes a [`Plan`], streaming progress and honouring cancellation.

use crate::engines::{command, EngineId};
use crate::plan::{Plan, ProgressKind, Step};
use crate::util::move_path;
use crate::Error;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub enum Event {
    /// 0.0..=1.0 for the current step; steps are weighted equally.
    Progress(f32),
}

pub async fn run(
    plan: &Plan,
    on_event: impl Fn(Event) + Send + Sync + 'static,
    mut cancel: watch::Receiver<bool>,
) -> Result<Vec<PathBuf>, Error> {
    let on_event = Arc::new(on_event);
    let result = run_steps(plan, on_event, &mut cancel).await;
    if let Some(t) = &plan.temp_dir {
        let _ = std::fs::remove_dir_all(t);
    }
    if result.is_err() {
        for o in &plan.outputs {
            if o.is_dir() {
                let _ = std::fs::remove_dir_all(o);
            } else {
                let _ = std::fs::remove_file(o);
            }
        }
    }
    result
}

async fn run_steps(
    plan: &Plan,
    on_event: Arc<dyn Fn(Event) + Send + Sync>,
    cancel: &mut watch::Receiver<bool>,
) -> Result<Vec<PathBuf>, Error> {
    let exec_count = plan.steps.iter().filter(|s| matches!(s, Step::Exec { .. })).count().max(1) as f32;
    let mut exec_done = 0f32;
    let mut outputs = plan.outputs.clone();

    for step in &plan.steps {
        if *cancel.borrow() {
            return Err(Error::Cancelled);
        }
        match step {
            Step::Mkdir(d) => std::fs::create_dir_all(d)?,
            Step::Exec { engine, program, args, cwd, progress } => {
                let base = exec_done;
                let ev = on_event.clone();
                let report = move |p: f32| ev(Event::Progress(((base + p.clamp(0.0, 1.0)) / exec_count).min(1.0)));
                exec(*engine, program, args, cwd.as_deref(), *progress, report, cancel).await?;
                exec_done += 1.0;
                on_event(Event::Progress(exec_done / exec_count));
            }
            Step::MoveFile { from, to, engine } => {
                if !from.is_file() {
                    return Err(Error::Failed {
                        engine: *engine,
                        message: format!("{} finished without producing a file. The input may be damaged or password-protected.", engine.name()),
                    });
                }
                move_path(from, to)?;
            }
            Step::UnwrapTar { dir, sevenzip } => {
                let entries: Vec<PathBuf> = std::fs::read_dir(dir)?.flatten().map(|e| e.path()).collect();
                if let [only] = entries.as_slice() {
                    if only.extension().map(|e| e.eq_ignore_ascii_case("tar")).unwrap_or(false) {
                        let args = vec!["x".into(), "-y".into(), "-bso0".into(), format!("-o{}", dir.display()).into(), only.clone().into_os_string()];
                        exec(EngineId::SevenZip, sevenzip, &args, None, ProgressKind::None, |_| {}, cancel).await?;
                        std::fs::remove_file(only)?;
                    }
                }
            }
            Step::MoveDir { from, to } => move_path(from, to)?,
            Step::CollapsePages { dir, single } => {
                let pages: Vec<PathBuf> = std::fs::read_dir(dir)?.flatten().map(|e| e.path()).collect();
                if pages.is_empty() {
                    return Err(Error::Failed { engine: EngineId::Ghostscript, message: "No pages were rendered".into() });
                }
                if let [only] = pages.as_slice() {
                    move_path(only, single)?;
                    std::fs::remove_dir(dir)?;
                    outputs = vec![single.clone()];
                }
            }
        }
    }
    Ok(outputs)
}

/// Run one engine process. Progress comes from stdout/stderr depending on `kind`.
pub async fn exec(
    engine: EngineId,
    program: &Path,
    args: &[std::ffi::OsString],
    cwd: Option<&Path>,
    kind: ProgressKind,
    report: impl Fn(f32) + Send + Sync + 'static,
    cancel: &mut watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut cmd = command(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    let mut child = cmd.spawn().map_err(|e| Error::Failed {
        engine,
        message: format!("Could not start {}: {e}", program.display()),
    })?;

    let tail = Arc::new(Mutex::new(VecDeque::<String>::with_capacity(16)));
    let duration = Arc::new(Mutex::new(None::<f64>));
    let report = Arc::new(report);

    let out_task = tokio::spawn(read_lines(child.stdout.take(), {
        let (tail, duration, report) = (tail.clone(), duration.clone(), report.clone());
        move |line: &str| {
            handle_line(line, kind, &duration, &*report);
            if kind != ProgressKind::Ffmpeg {
                push_tail(&tail, line);
            }
        }
    }));
    let err_task = tokio::spawn(read_lines(child.stderr.take(), {
        let (tail, duration, report) = (tail.clone(), duration.clone(), report.clone());
        move |line: &str| {
            handle_line(line, kind, &duration, &*report);
            push_tail(&tail, line);
        }
    }));

    let status = tokio::select! {
        s = child.wait() => s?,
        _ = wait_cancel(cancel) => {
            let _ = child.kill().await;
            return Err(Error::Cancelled);
        }
    };
    let _ = out_task.await;
    let _ = err_task.await;

    if status.success() {
        Ok(())
    } else {
        let lines: Vec<String> = tail.lock().unwrap().iter().cloned().collect();
        let message = if lines.is_empty() {
            format!("{} exited with {status}", engine.name())
        } else {
            lines.join("\n")
        };
        Err(Error::Failed { engine, message })
    }
}

async fn wait_cancel(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow() {
            return;
        }
        if rx.changed().await.is_err() {
            // Sender dropped: can never be cancelled.
            std::future::pending::<()>().await;
        }
    }
}

async fn read_lines<R: AsyncRead + Unpin>(r: Option<R>, mut f: impl FnMut(&str)) {
    let Some(r) = r else { return };
    // Split on \r as well: 7-Zip and Calibre redraw progress in place.
    let mut reader = BufReader::new(r);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let s = String::from_utf8_lossy(&buf);
                for part in s.split(['\r', '\n', '\u{8}']) {
                    let t = part.trim();
                    if !t.is_empty() {
                        f(t);
                    }
                }
            }
        }
    }
}

fn push_tail(tail: &Mutex<VecDeque<String>>, line: &str) {
    let mut t = tail.lock().unwrap();
    if t.len() == 12 {
        t.pop_front();
    }
    t.push_back(line.chars().take(400).collect());
}

fn handle_line(line: &str, kind: ProgressKind, duration: &Mutex<Option<f64>>, report: &(dyn Fn(f32) + Send + Sync)) {
    match kind {
        ProgressKind::None => {}
        ProgressKind::Percent => {
            if let Some(p) = parse_percent(line) {
                report(p);
            }
        }
        ProgressKind::Ffmpeg => {
            if let Some(rest) = line.strip_prefix("Duration:") {
                let d = rest.split(',').next().and_then(parse_hms);
                let mut g = duration.lock().unwrap();
                if g.is_none() {
                    *g = d;
                }
            } else if let Some(us) = line.strip_prefix("out_time_us=").or_else(|| line.strip_prefix("out_time_ms=")) {
                if let (Ok(us), Some(total)) = (us.trim().parse::<f64>(), *duration.lock().unwrap()) {
                    if total > 0.0 {
                        report((us / 1_000_000.0 / total) as f32);
                    }
                }
            }
        }
    }
}

pub fn parse_percent(line: &str) -> Option<f32> {
    let digits: String = line.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 3 {
        return None;
    }
    let rest = &line.trim_start()[digits.len()..];
    if rest.starts_with('%') {
        digits.parse::<f32>().ok().map(|v| v / 100.0)
    } else {
        None
    }
}

/// "00:01:02.50" -> 62.5
pub fn parse_hms(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: f64 = parts[0].parse().ok()?;
    let m: f64 = parts[1].parse().ok()?;
    let sec: f64 = parts[2].parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percents() {
        assert_eq!(parse_percent(" 45% 12 - file.txt"), Some(0.45));
        assert_eq!(parse_percent("12% Converting input"), Some(0.12));
        assert_eq!(parse_percent("2024 files"), None);
    }

    #[test]
    fn hms() {
        assert_eq!(parse_hms(" 00:01:02.50"), Some(62.5));
        assert_eq!(parse_hms("N/A"), None);
    }
}
