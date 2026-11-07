use serde::{Deserialize, Serialize};

/// Tokenizer selection for keyword indexes. The default keeps the
/// English-only behavior so existing deployments observe zero change;
/// mixed enables dictionary segmentation for Chinese with English
/// identifier splitting kept intact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeywordTokenizer {
    /// Lowercase alphanumeric splitting, Chinese degrades to whole phrases.
    #[default]
    English,
    /// Dictionary segmentation for Chinese plus identifier splitting.
    Mixed,
}

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
    /// Elasticsearch per-request deadline in seconds.
    #[serde(default = "default_elastic_timeout_secs")]
    pub elastic_timeout_secs: u64,
    /// Elasticsearch documents per bulk request.
    #[serde(default = "default_elastic_batch_size")]
    pub elastic_batch_size: usize,
    /// Elasticsearch count uses the exact `_count` API; `false` falls back
    /// to the search total so huge collections trade accuracy for speed.
    #[serde(default = "default_elastic_exact_count")]
    pub elastic_exact_count: bool,
    /// Tokenizer selection. English preserves the legacy behavior;
    /// mixed enables Chinese dictionary segmentation on every backend.
    #[serde(default)]
    pub tokenizer: KeywordTokenizer,
    /// Plugin analyzer name replacing the built-in mixed segmentation.
    /// `None` keeps the built-in path; `Some` requires the named
    /// analyzer to be registered by the host before first use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analyzer_name: Option<String>,
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

fn default_elastic_timeout_secs() -> u64 {
    10
}

fn default_elastic_batch_size() -> usize {
    256
}

fn default_elastic_exact_count() -> bool {
    true
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
            elastic_timeout_secs: default_elastic_timeout_secs(),
            elastic_batch_size: default_elastic_batch_size(),
            elastic_exact_count: default_elastic_exact_count(),
            tokenizer: KeywordTokenizer::default(),
            analyzer_name: None,
        }
    }
}
