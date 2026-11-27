//! Backend-neutral keyword storage contracts for retrieval features.
//!
//! Hosts inject any [`KeywordIndex`] implementation; RAG pipelines map the
//! shared scored points into their own chunk model. The crate reuses the
//! scored point type from `vector-contracts`, while its own scope describes
//! only the fields every keyword backend can filter natively. Generic payload
//! conditions stay in the caller so hybrid retrieval can recheck them on
//! returned hits. Capabilities such as highlighting, snippets and paging stay
//! out until a consumer needs them.

pub mod error;
pub mod sqlite_index;
pub mod store;
pub mod tokenizer;
pub mod types;
#[cfg(feature = "elastic")]
pub mod elastic_index;
#[cfg(feature = "tantivy")]
pub mod tantivy_index;

pub use error::{KeywordError, Result};
pub use sqlite_index::SqliteKeywordIndex;
pub use store::KeywordIndex;
#[cfg(feature = "elastic")]
pub use elastic_index::ElasticKeywordIndex;
#[cfg(feature = "tantivy")]
pub use tantivy_index::TantivyKeywordIndex;
pub use tokenizer::{
    clear_analyzers, ensure_analyzer_registered, find_analyzer, is_cjk, mixed_terms, pretokenize,
    register_analyzer, split_identifier, terms, unregister_analyzer, CustomAnalyzer, MixedToken,
    MAX_CJK_RUN_CHARS, MAX_TOKENIZE_CHARS,
};
pub use types::{KeywordDocument, KeywordFormat, KeywordQuery, KeywordScope};

/// Connection parameters of the remote Elasticsearch keyword backend.
///
/// Plain data shared by every crate: hosts build it without enabling the
/// `elastic` feature, and the feature-gated backend consumes it.
#[derive(Debug, Clone)]
pub struct ElasticKeywordConfig {
    /// Base URL of the REST API, for example `http://127.0.0.1:9200`.
    pub base_url: String,
    /// Optional API key sent as an `ApiKey` authorization header.
    pub api_key: Option<String>,
    /// Index carrying every collection.
    pub index: String,
    /// Per-request deadline in seconds.
    pub timeout_secs: u64,
    /// Documents per bulk request.
    pub batch_size: usize,
    /// `true` counts through the exact `_count` API; `false` reads the
    /// search total so huge collections trade accuracy for speed.
    pub exact_count: bool,
    /// Tokenizer selection; English preserves legacy behavior.
    pub tokenizer: wf_types::config::keyword::KeywordTokenizer,
    /// Plugin analyzer name replacing built-in segmentation.
    pub analyzer_name: Option<String>,
    /// Behavior on an existing index with a different format stamp.
    pub format_policy: wf_types::config::keyword::KeywordFormatPolicy,
}

impl ElasticKeywordConfig {
    /// Builds a config with a ten-second deadline and the default index.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: None,
            index: "rag-keyword".to_string(),
            timeout_secs: 10,
            batch_size: 256,
            exact_count: true,
            tokenizer: wf_types::config::keyword::KeywordTokenizer::English,
            analyzer_name: None,
            format_policy: wf_types::config::keyword::KeywordFormatPolicy::default(),
        }
    }
}

/// Opens an in-memory SQLite keyword index with an explicit selection.
pub fn open_sqlite_ram_keyword_with(
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    let index =
        sqlite_index::SqliteKeywordIndex::open_in_memory_with(tokenizer, analyzer_name)?;
    Ok(std::sync::Arc::new(index))
}

/// Opens a file SQLite keyword index with an explicit selection.
pub fn open_sqlite_file_keyword_with(
    path: &str,
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    let index = sqlite_index::SqliteKeywordIndex::open_with(path, tokenizer, analyzer_name)?;
    Ok(std::sync::Arc::new(index))
}

/// Opens a file SQLite keyword index with an explicit format stamp.
pub fn open_sqlite_file_keyword_format(path: &str, format: KeywordFormat) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    let index = sqlite_index::SqliteKeywordIndex::open_with_format(path, format)?;
    Ok(std::sync::Arc::new(index))
}

/// Opens an in-RAM tantivy keyword index. Fails when the feature is off.
#[cfg(feature = "tantivy")]
pub fn open_tantivy_ram_keyword_with(
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    let index = tantivy_index::TantivyKeywordIndex::open_in_memory_with(tokenizer, analyzer_name)?;
    Ok(std::sync::Arc::new(index))
}

/// Opens a file tantivy keyword index. Fails when the feature is off.
#[cfg(feature = "tantivy")]
pub fn open_tantivy_dir_keyword_with(
    path: &str,
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    let index = tantivy_index::TantivyKeywordIndex::open_with(path, tokenizer, analyzer_name)?;
    Ok(std::sync::Arc::new(index))
}

/// Opens a file tantivy keyword index with a format stamp and heap budget.
#[cfg(feature = "tantivy")]
pub fn open_tantivy_dir_keyword_format(
    path: &str,
    format: KeywordFormat,
    writer_heap_bytes: Option<usize>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    let index = tantivy_index::TantivyKeywordIndex::open_with_format(path, format, writer_heap_bytes)?;
    Ok(std::sync::Arc::new(index))
}

/// Opens a remote Elasticsearch keyword index. Fails when the feature is off.
#[cfg(feature = "elastic")]
pub fn open_elastic_keyword(
    config: &ElasticKeywordConfig,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    let index = elastic_index::ElasticKeywordIndex::new(config)?;
    Ok(std::sync::Arc::new(index))
}
