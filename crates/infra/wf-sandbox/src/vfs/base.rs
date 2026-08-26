use std::path::Path;

use async_trait::async_trait;

use crate::resolver::VfsProvider;

pub struct HostFs {
    root: String,
}

impl HostFs {
    pub fn new(root: String) -> Self {
        Self { root }
    }
}

#[async_trait]
impl VfsProvider for HostFs {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>, std::io::Error> {
        let full_path = Path::new(&self.root).join(path.trim_start_matches('/'));
        tokio::fs::read(full_path).await
    }

    async fn write_file(&self, path: &str, data: Vec<u8>) -> Result<(), std::io::Error> {
        let full_path = Path::new(&self.root).join(path.trim_start_matches('/'));
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(full_path, data).await
    }

    async fn exists(&self, path: &str) -> bool {
        let full_path = Path::new(&self.root).join(path.trim_start_matches('/'));
        tokio::fs::try_exists(full_path).await.unwrap_or(false)
    }

    async fn check_read(&self, _path: &str) -> Result<(), std::io::Error> {
        Ok(())
    }

    async fn check_write(&self, _path: &str) -> Result<(), std::io::Error> {
        Ok(())
    }

    fn path_policy(&self) -> Option<wf_types::script::sandbox::PathPolicy> {
        None
    }
}
