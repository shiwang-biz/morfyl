//! Path helpers.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static RESERVED: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

/// `dir/stem.ext`, or `dir/stem (1).ext`, `dir/stem (2).ext` ... if taken.
/// Names are reserved for the life of the process so parallel jobs never collide.
/// `ext = None` produces a folder name.
pub fn unique_path(dir: &Path, stem: &str, ext: Option<&str>) -> PathBuf {
    let mut guard = RESERVED.lock().unwrap_or_else(|e| e.into_inner());
    let reserved = guard.get_or_insert_with(HashSet::new);
    let make = |n: usize| {
        let name = match (n, ext) {
            (0, Some(e)) => format!("{stem}.{e}"),
            (0, None) => stem.to_string(),
            (n, Some(e)) => format!("{stem} ({n}).{e}"),
            (n, None) => format!("{stem} ({n})"),
        };
        dir.join(name)
    };
    let mut n = 0;
    loop {
        let p = make(n);
        if !p.exists() && !reserved.contains(&p) {
            reserved.insert(p.clone());
            return p;
        }
        n += 1;
    }
}

/// File name without extension; `archive.tar.gz` -> `archive`.
pub fn stem(path: &Path) -> String {
    let s = path.file_stem().and_then(|s| s.to_str()).unwrap_or("output").to_string();
    s.strip_suffix(".tar").map(str::to_string).unwrap_or(s)
}

/// `file:///...` URL for LibreOffice's -env:UserInstallation.
pub fn file_url(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let mut enc = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            ' ' => enc.push_str("%20"),
            '%' => enc.push_str("%25"),
            '#' => enc.push_str("%23"),
            '?' => enc.push_str("%3F"),
            c => enc.push(c),
        }
    }
    if enc.starts_with('/') {
        format!("file://{enc}")
    } else {
        format!("file:///{enc}")
    }
}

/// Move a file or folder; falls back to copy + delete across volumes.
pub fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        copy_dir(from, to)?;
        std::fs::remove_dir_all(from)
    } else {
        std::fs::copy(from, to)?;
        std::fs::remove_file(from)
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems() {
        assert_eq!(stem(Path::new("/a/b.tar.gz")), "b");
        assert_eq!(stem(Path::new("photo.jpeg")), "photo");
    }

    #[test]
    fn urls() {
        assert_eq!(file_url(Path::new("/tmp/a b")), "file:///tmp/a%20b");
        assert_eq!(file_url(Path::new(r"C:\Users\x")), "file:///C:/Users/x");
    }

    #[test]
    fn unique_reserves() {
        let d = std::env::temp_dir();
        let a = unique_path(&d, "ff-unique-test", Some("x"));
        let b = unique_path(&d, "ff-unique-test", Some("x"));
        assert_ne!(a, b);
    }
}
