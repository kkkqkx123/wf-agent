//! Per-file serialization for filesystem mutations.
//!
//! Concurrent edits to the same file otherwise interleave read-modify-write
//! cycles and lose updates. Operations for different files still run in
//! parallel; only the same normalized path serializes. Normalization
//! prefers the canonical path and falls back to the absolute path when the
//! file does not exist yet, mirroring realpath-with-fallback resolution.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use dashmap::DashMap;

fn locks() -> &'static DashMap<PathBuf, Arc<Mutex<()>>> {
    static LOCKS: OnceLock<DashMap<PathBuf, Arc<Mutex<()>>>> = OnceLock::new();
    LOCKS.get_or_init(DashMap::new)
}

/// Normalize a path to its lock key. Canonical paths collapse symlinks and
/// `./` segments so aliased spellings share one queue; missing files fall
/// back to the absolute spelling.
fn lock_key(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Acquire the serialization lock for one file path. Bind the result to
/// a named guard so the lock lives across the read-modify-write cycle:
/// `let _guard = lock_for_file(&path).lock()`.
pub fn lock_for_file(path: &Path) -> Arc<Mutex<()>> {
    let key = lock_key(path);
    let entry = locks()
        .entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(())));
    let lock = entry.clone();
    drop(entry);
    lock
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_key_on_absolute_spelling() {
        let missing = std::env::temp_dir().join("wf-missing-queue-probe.txt");
        let _ = std::fs::remove_file(&missing);
        assert_eq!(lock_key(&missing), missing);
    }

    #[test]
    fn same_file_shares_one_lock() {
        let dir = std::env::temp_dir().join(format!("wf-queue-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("shared.txt");
        let _ = std::fs::write(&file, "x");
        let canonical = file.canonicalize().expect("probe file exists");
        assert_eq!(lock_key(&file), canonical);
        let file_lock = lock_for_file(&file);
        let _guard = file_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
