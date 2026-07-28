use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LegacyStorageConfig {
    pub storage_type: String,
    pub connection_string: Option<String>,
    pub max_connections: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LegacyCompressionConfig {
    pub enabled: bool,
    pub algorithm: Option<String>,
    pub level: Option<u32>,
}
