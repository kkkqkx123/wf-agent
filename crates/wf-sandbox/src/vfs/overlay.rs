use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::Mutex;
use wf_types::script::sandbox::PathPolicy;

use crate::resolver::VfsProvider;

pub struct OverlayVFS {
    base: PathBuf,
    delta: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
    path_policy: PathPolicy,
}

impl OverlayVFS {
    pub fn new(base: PathBuf, path_policy: PathPolicy) -> Self {
        Self {
            base,
            delta: Arc::new(Mutex::new(HashMap::new())),
            path_policy,
        }
    }

    pub async fn read_file(&self, path: &Path) -> Result<Vec<u8>, std::io::Error> {
        {
            let delta = self.delta.lock().await;
            if let Some(data) = delta.get(path) {
                return Ok(data.clone());
            }
        }

        let full_path = self.base.join(path);
        tokio::fs::read(full_path).await
    }

    pub async fn write_file(&self, path: &Path, data: Vec<u8>) -> Result<(), std::io::Error> {
        let path_str = path.to_string_lossy().to_string();
        if !self
            .path_policy
            .allowed_write
            .iter()
            .any(|p| path_str.starts_with(p))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Write not allowed",
            ));
        }

        let mut delta = self.delta.lock().await;
        delta.insert(path.to_path_buf(), data);
        Ok(())
    }

    pub async fn exists(&self, path: &Path) -> bool {
        {
            let delta = self.delta.lock().await;
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
