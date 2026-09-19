use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// FileNode - File Benchmarking
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileNode {
    /// File path (relative to repository root)
    pub file_path: PathBuf,
    /// Blake3 hash of file contents
    pub base_hash: [u8; 32],
}

impl FileNode {
    pub fn new(file_path: PathBuf, content: &[u8]) -> Self {
        let hash = blake3::hash(content);
        FileNode {
            file_path,
            base_hash: *hash.as_bytes(),
        }
    }

    /// Returns a string representation of the file path
    pub fn path_str(&self) -> &str {
        self.file_path.to_str().unwrap_or("")
    }
}
