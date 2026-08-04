use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageType {
    #[default]
    Sqlite,
    Postgres,
    Memory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    #[serde(rename = "gzip")]
    Gzip,
    #[serde(rename = "brotli")]
    Brotli,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompressionConfig {
    pub enabled: bool,
    pub algorithm: Option<CompressionAlgorithm>,
    pub threshold: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutoVacuum {
    #[default]
    None,
    Full,
    Incremental,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SqliteStorageConfig {
    pub db_path: String,
    #[serde(default)]
    pub enable_wal: bool,
    #[serde(default)]
    pub enable_logging: bool,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub file_must_exist: bool,
    #[serde(default)]
    pub timeout: i64,
    #[serde(default)]
    pub auto_vacuum: AutoVacuum,
    pub journal_size_limit: Option<i64>,
    pub page_size: Option<i64>,
    pub maintenance_interval_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostgresStorageConfig {
    pub host: String,
    #[serde(default = "default_pg_port")]
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    #[serde(default)]
    pub ssl: bool,
    pub pool_size: Option<u32>,
    pub min_connections: Option<u32>,
    pub idle_timeout: Option<i64>,
    pub connection_timeout: Option<i64>,
    pub max_uses: Option<i64>,
}

fn default_pg_port() -> u16 {
    5432
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageConfig {
    #[serde(rename = "type")]
    pub storage_type: StorageType,
    pub sqlite: Option<SqliteStorageConfig>,
    pub postgres: Option<PostgresStorageConfig>,
    /// Application name used for default db path derivation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_type: StorageType::Memory,
            sqlite: None,
            postgres: None,
            app_name: None,
        }
    }
}
