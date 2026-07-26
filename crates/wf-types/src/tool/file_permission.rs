use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilePermissionSettings {
    pub allowed_paths: Vec<String>,
    pub allow_read: bool,
    pub allow_write: bool,
    pub allow_delete: bool,
    pub max_file_size: Option<u64>,
}
