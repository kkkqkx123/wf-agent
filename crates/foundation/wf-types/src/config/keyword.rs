use serde::{Deserialize, Serialize};

/// Keyword index backend selector. The default keeps the built-in SQLite
/// FTS index so existing deployments observe zero behavior change;
/// tantivy and elastic are explicit opt-ins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeywordBackend {
    /// Built-in SQLite FTS5 index.
    #[default]
    Sqlite,
    /// Embedded tantivy index (requires `rag-core/tantivy`).
    Tantivy,
    /// Remote Elasticsearch index (requires `rag-core/elastic`).
    Elastic,
}

/// Keyword index configuration. The index is enabled by default and
/// resolves its placement from the storage configuration; an explicit
/// setting wins, and disabling keeps hybrid retrieval off while dense
/// retrieval keeps working.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeywordIndexConfig {
    /// Master switch. `false` leaves the keyword slot empty.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Backend selector. Non-sqlite values are explicit opt-ins.
    #[serde(default)]
    pub backend: KeywordBackend,
    /// Explicit SQLite database path. `None` derives a sibling of the
    /// storage database from the storage configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db_path: Option<String>,
    /// Explicit tantivy index directory. `None` derives a sibling of the
    /// storage database; memory storage uses an in-RAM index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tantivy_path: Option<String>,
    /// Elasticsearch base URL, for example `http://127.0.0.1:9200`.
    /// Required when the backend is elastic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elastic_url: Option<String>,
    /// Elasticsearch API key. `None` sends no credentials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elastic_api_key: Option<String>,
    /// Elasticsearch index carrying every collection.
    #[serde(
        default = "default_elastic_index",
        skip_serializing_if = "is_default_elastic_index"
    )]
    pub elastic_index: String,
}

fn default_enabled() -> bool {
    true
}

fn default_elastic_index() -> String {
    "rag-keyword".to_string()
}

fn is_default_elastic_index(value: &String) -> bool {
    value == "rag-keyword"
}

impl Default for KeywordIndexConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            backend: KeywordBackend::default(),
            db_path: None,
            tantivy_path: None,
            elastic_url: None,
            elastic_api_key: None,
            elastic_index: default_elastic_index(),
        }
    }
}
