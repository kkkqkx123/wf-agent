use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Write `data` to `path` atomically: the bytes land in a temporary sibling
/// file first and the sibling is renamed over the target, so a crash or a
/// full disk can leave a stale temporary file but never a truncated target.
/// The temporary file lives in the same directory so the rename stays on one
/// filesystem. Failures (including no-space errors) are returned to the
/// caller instead of being swallowed.
pub fn write_atomic_sync(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = sibling_tmp_path(path);
    match std::fs::write(&tmp, data) {
        Ok(()) => {}
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

fn sibling_tmp_path(path: &Path) -> std::path::PathBuf {
    let id = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config".to_string());
    path.with_extension(format!("tmp.{}.{}.{}", std::process::id(), id, file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_round_trips_and_overwrites() {
        let dir = std::env::temp_dir().join(format!("wf-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("settings.json");

        write_atomic_sync(&path, b"{\"v\":1}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"v\":1}");

        write_atomic_sync(&path, b"{\"v\":2}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"v\":2}");

        let leftovers: Vec<_> = std::fs::read_dir(dir.join("nested"))
            .unwrap()
            .filter_map(|entry| entry.ok().map(|e| e.file_name()))
            .collect();
        assert_eq!(
            leftovers.len(),
            1,
            "no tmp sibling left behind: {leftovers:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_missing_parent_is_created() {
        let dir = std::env::temp_dir().join(format!("wf-atomic-mkdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("a").join("b").join("c.json");

        write_atomic_sync(&path, b"{}").unwrap();
        assert!(path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
