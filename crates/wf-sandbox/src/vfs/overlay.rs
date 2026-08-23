use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use wf_types::script::sandbox::PathPolicy;

use crate::resolver::VfsProvider;
use crate::vfs::whiteout::WhiteoutCache;

pub struct OverlayVFS {
    base: PathBuf,
    /// Pending writes, keyed by the path as passed to `write_file`. A plain
    /// mutex is sufficient: guards are never held across await points, and a
    /// synchronous drain (`take_delta`) must be able to lock it.
    delta: Arc<std::sync::Mutex<HashMap<PathBuf, Vec<u8>>>>,
    /// Paths explicitly deleted through `delete_file`. Like writes these stay
    /// in-memory until `flush` commits them onto the base directory.
    whiteouts: Arc<std::sync::Mutex<WhiteoutCache>>,
    path_policy: PathPolicy,
}

impl OverlayVFS {
    pub fn new(base: PathBuf, path_policy: PathPolicy) -> Self {
        Self {
            base,
            delta: Arc::new(std::sync::Mutex::new(HashMap::new())),
            whiteouts: Arc::new(std::sync::Mutex::new(WhiteoutCache::new())),
            path_policy,
        }
    }

    fn is_whiteouted(&self, path: &Path) -> bool {
        self.whiteouts
            .lock()
            .map(|w| w.is_whiteout(&path.to_path_buf()))
            .unwrap_or(false)
    }

    pub async fn read_file(&self, path: &Path) -> Result<Vec<u8>, std::io::Error> {
        let path_str = path.to_string_lossy().to_string();
        if !self
            .path_policy
            .allowed_read
            .iter()
            .any(|p| path_str.starts_with(p))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Read not allowed",
            ));
        }

        if self.is_whiteouted(path) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File deleted in overlay: {}", path.display()),
            ));
        }

        if let Ok(delta) = self.delta.lock() {
            if let Some(data) = delta.get(path) {
                return Ok(data.clone());
            }
        }

        let full_path = self.base.join(path);
        tokio::fs::read(full_path).await
    }

    pub async fn write_file(&self, path: &Path, data: Vec<u8>) -> Result<(), std::io::Error> {
        self.check_write(&path.to_string_lossy()).await?;

        // A write after an explicit delete resurrects the path: drop the
        // whiteout so reads and existence checks see the pending content.
        if let Ok(mut whiteouts) = self.whiteouts.lock() {
            whiteouts.remove_whiteout(&path.to_path_buf());
        }
        if let Ok(mut delta) = self.delta.lock() {
            delta.insert(path.to_path_buf(), data);
        }
        Ok(())
    }

    /// Record an explicit deletion (whiteout) for `path`.
    ///
    /// Deletions are tracked in-memory like writes and only reach the base
    /// directory on [`Self::flush`]. Any pending write for the same path is
    /// discarded because the whiteout supersedes it; reads and existence
    /// checks treat the path as missing immediately. This keeps "file
    /// emptied" (an empty write) distinguishable from "file deleted".
    pub async fn delete_file(&self, path: &Path) -> Result<(), std::io::Error> {
        self.check_write(&path.to_string_lossy()).await?;

        if let Ok(mut delta) = self.delta.lock() {
            delta.remove(path);
        }
        if let Ok(mut whiteouts) = self.whiteouts.lock() {
            whiteouts.whiteout(path.to_path_buf());
        }
        Ok(())
    }

    /// Commit the overlay state onto the base directory.
    ///
    /// Pending writes are written back with content addressing: base files
    /// whose bytes already match the delta entry are skipped. Whiteouted
    /// paths are removed from the base directory (missing files are
    /// tolerated). On success the delta and whiteout sets are cleared, making
    /// `flush` the commit point between the sandbox's in-memory view and the
    /// real filesystem.
    pub async fn flush(&self) -> Result<(), std::io::Error> {
        let writes: Vec<(PathBuf, Vec<u8>)> = self
            .delta
            .lock()
            .map(|delta| {
                delta
                    .iter()
                    .map(|(path, data)| (path.clone(), data.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let deletions: Vec<PathBuf> = self
            .whiteouts
            .lock()
            .map(|whiteouts| whiteouts.paths())
            .unwrap_or_default();

        // Deletions run first so a path that was deleted and rewritten ends
        // up with the fresh write.
        for rel_path in &deletions {
            let full_path = self.base.join(rel_path);
            match tokio::fs::remove_file(&full_path).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }

        for (rel_path, data) in &writes {
            let full_path = self.base.join(rel_path);
            if let Ok(existing) = tokio::fs::read(&full_path).await {
                if existing == *data {
                    continue;
                }
            }
            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&full_path, data).await?;
        }

        if let Ok(mut delta) = self.delta.lock() {
            delta.clear();
        }
        if let Ok(mut whiteouts) = self.whiteouts.lock() {
            whiteouts.clear();
        }
        Ok(())
    }

    fn check_path(
        &self,
        path: &str,
        allowed: &[String],
        action: &str,
    ) -> Result<(), std::io::Error> {
        if allowed.iter().any(|p| path.starts_with(p)) {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("{action} not allowed"),
            ))
        }
    }

    pub async fn exists(&self, path: &Path) -> bool {
        if self.is_whiteouted(path) {
            return false;
        }

        if let Ok(delta) = self.delta.lock() {
            if delta.contains_key(path) {
                return true;
            }
        }

        let full_path = self.base.join(path);
        tokio::fs::try_exists(full_path).await.unwrap_or(false)
    }
}

#[async_trait::async_trait]
impl VfsProvider for OverlayVFS {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>, std::io::Error> {
        self.read_file(Path::new(path)).await
    }

    async fn write_file(&self, path: &str, data: Vec<u8>) -> Result<(), std::io::Error> {
        self.write_file(Path::new(path), data).await
    }

    async fn exists(&self, path: &str) -> bool {
        self.exists(Path::new(path)).await
    }

    async fn check_read(&self, path: &str) -> Result<(), std::io::Error> {
        self.check_path(path, &self.path_policy.allowed_read, "Read")
    }

    async fn check_write(&self, path: &str) -> Result<(), std::io::Error> {
        self.check_path(path, &self.path_policy.allowed_write, "Write")
    }

    fn path_policy(&self) -> Option<wf_types::script::sandbox::PathPolicy> {
        Some(self.path_policy.clone())
    }

    fn take_delta(&self) -> HashMap<PathBuf, Vec<u8>> {
        self.delta
            .lock()
            .map(|mut delta| delta.drain().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy_allowing(prefix: &str) -> PathPolicy {
        PathPolicy {
            allowed_read: vec![prefix.to_string()],
            allowed_write: vec![prefix.to_string()],
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let dir = std::env::temp_dir().join("vfs-test-overlay");
        let _ = tokio::fs::create_dir_all(&dir).await;

        let policy = PathPolicy {
            allowed_read: vec!["/tmp".to_string()],
            allowed_write: vec!["/tmp".to_string()],
        };
        let vfs = OverlayVFS::new(dir.clone(), policy);

        let test_path = Path::new("/tmp/test.txt");
        vfs.write_file(test_path, b"hello world".to_vec())
            .await
            .unwrap();

        let data = vfs.read_file(test_path).await.unwrap();
        assert_eq!(data, b"hello world");

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn test_write_denied_permission() {
        let dir = std::env::temp_dir().join("vfs-test-deny");
        let _ = tokio::fs::create_dir_all(&dir).await;

        let policy = PathPolicy {
            allowed_read: vec![],
            allowed_write: vec![],
        };
        let vfs = OverlayVFS::new(dir.clone(), policy);

        let result = vfs.write_file(Path::new("/etc/passwd"), vec![]).await;
        assert!(result.is_err());

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn test_take_delta_drains_pending_writes() {
        let dir = temp_dir("vfs-test-take-delta");

        let vfs = OverlayVFS::new(dir.clone(), policy_allowing("data"));
        vfs.write_file(Path::new("data/a.txt"), b"A".to_vec())
            .await
            .unwrap();
        vfs.write_file(Path::new("data/b.txt"), b"B".to_vec())
            .await
            .unwrap();

        let delta = vfs.take_delta();
        assert_eq!(delta.len(), 2);
        assert_eq!(
            delta.get(Path::new("data/a.txt")).map(|v| v.as_slice()),
            Some(b"A".as_slice())
        );
        assert_eq!(
            delta.get(Path::new("data/b.txt")).map(|v| v.as_slice()),
            Some(b"B".as_slice())
        );

        assert!(
            vfs.take_delta().is_empty(),
            "drain must clear pending writes so repeated calls do not re-report"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn test_delete_records_whiteout_and_hides_path() {
        let dir = temp_dir("vfs-test-whiteout-hide");

        let vfs = OverlayVFS::new(dir.clone(), policy_allowing("data"));
        vfs.write_file(Path::new("data/c.txt"), b"C".to_vec())
            .await
            .unwrap();
        vfs.delete_file(Path::new("data/c.txt")).await.unwrap();

        // The whiteout supersedes the pending write: nothing left to drain.
        assert!(
            vfs.take_delta().is_empty(),
            "deleted path must not appear in the write delta"
        );
        assert!(
            !vfs.exists(Path::new("data/c.txt")).await,
            "whiteouted path must report as missing"
        );
        let err = vfs.read_file(Path::new("data/c.txt")).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(
            !tokio::fs::try_exists(dir.join("data").join("c.txt"))
                .await
                .unwrap_or(true),
            "delete must not touch the base directory before flush"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn test_write_after_delete_resurrects_path() {
        let dir = temp_dir("vfs-test-resurrect");

        let vfs = OverlayVFS::new(dir.clone(), policy_allowing("data"));
        vfs.delete_file(Path::new("data/d.txt")).await.unwrap();
        vfs.write_file(Path::new("data/d.txt"), b"D".to_vec())
            .await
            .unwrap();

        assert!(
            vfs.exists(Path::new("data/d.txt")).await,
            "a write after delete must resurrect the path"
        );
        assert_eq!(
            vfs.read_file(Path::new("data/d.txt")).await.unwrap(),
            b"D"
        );

        vfs.flush().await.unwrap();
        let flushed = tokio::fs::read(dir.join("data").join("d.txt")).await;
        assert_eq!(flushed.unwrap(), b"D");

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn test_flush_commits_writes_and_deletions_to_base() {
        let dir = temp_dir("vfs-test-flush-commit");
        let data_dir = dir.join("data");
        tokio::fs::create_dir_all(&data_dir).await.unwrap();
        tokio::fs::write(data_dir.join("gone.txt"), b"OLD").await.unwrap();
        tokio::fs::write(data_dir.join("same.txt"), b"SAME").await
            .unwrap();

        let vfs = OverlayVFS::new(dir.clone(), policy_allowing("data"));
        vfs.write_file(Path::new("data/made.txt"), b"made".to_vec())
            .await
            .unwrap();
        vfs.write_file(Path::new("data/nested/deep.txt"), b"deep".to_vec())
            .await
            .unwrap();
        // Identical to the base copy: flush must skip rewriting it.
        vfs.write_file(Path::new("data/same.txt"), b"SAME".to_vec())
            .await
            .unwrap();
        vfs.delete_file(Path::new("data/gone.txt")).await.unwrap();

        vfs.flush().await.unwrap();

        assert_eq!(
            tokio::fs::read(data_dir.join("made.txt")).await.unwrap(),
            b"made"
        );
        assert_eq!(
            tokio::fs::read(data_dir.join("nested").join("deep.txt"))
                .await
                .unwrap(),
            b"deep",
            "flush must create missing parent directories"
        );
        assert_eq!(
            tokio::fs::read(data_dir.join("same.txt")).await.unwrap(),
            b"SAME"
        );
        assert!(
            !tokio::fs::try_exists(data_dir.join("gone.txt"))
                .await
                .unwrap_or(true),
            "whiteouted path must be removed from the base directory"
        );

        // Commit clears the state: drained delta is empty and reads fall
        // through to the base directory.
        assert!(vfs.take_delta().is_empty());
        assert_eq!(
            vfs.read_file(Path::new("data/made.txt")).await.unwrap(),
            b"made"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn test_flush_tolerates_missing_whiteouted_base_file() {
        let dir = temp_dir("vfs-test-flush-missing");

        let vfs = OverlayVFS::new(dir.clone(), policy_allowing("data"));
        vfs.delete_file(Path::new("data/never-existed.txt"))
            .await
            .unwrap();

        vfs.flush()
            .await
            .expect("deleting a non-existent base file must not fail flush");

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
