//! Serializable ingest and query configuration.

use serde::{Deserialize, Serialize};

use crate::chunker::ChunkerConfig;

/// Write-side policy when a source is already indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    /// Keep existing points, index only new sources.
    #[default]
    Skip,
    /// Delete the source scope first, then index.
    Overwrite,
    /// Fail when the source scope is non-empty.
    Fail,
}

/// Ingest configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagIndexConfig {
    /// Knowledge-base namespace written into every point payload.
    pub namespace: String,
    /// Chunking parameters.
    #[serde(default = "default_chunker_config")]
    pub chunking: ChunkerConfigData,
    /// Embedding texts per provider call.
    #[serde(default = "default_batch_size")]
    pub embedding_batch_size: usize,
    /// Behavior on already-indexed sources.
    #[serde(default)]
    pub duplicate_policy: DuplicatePolicy,
    /// Store hierarchical parent sections alongside leaf chunks.
    #[serde(default)]
    pub hierarchical: bool,
}

/// Chunking parameters in serializable form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkerConfigData {
    /// Soft target length of a leaf chunk in characters.
    pub target_chars: usize,
    /// Overlap between consecutive leaf chunks in characters.
    pub overlap_chars: usize,
}

fn default_chunker_config() -> ChunkerConfigData {
    ChunkerConfigData {
        target_chars: 800,
        overlap_chars: 100,
    }
}

fn default_batch_size() -> usize {
    32
}

impl RagIndexConfig {
    /// Minimal config indexing into one namespace with defaults.
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            chunking: default_chunker_config(),
            embedding_batch_size: default_batch_size(),
            duplicate_policy: DuplicatePolicy::Skip,
            hierarchical: false,
        }
    }

    /// Converts to the runtime chunker config with default separators.
    pub fn chunker_config(&self) -> ChunkerConfig {
        ChunkerConfig {
            target_chars: self.chunking.target_chars,
            overlap_chars: self.chunking.overlap_chars,
            ..ChunkerConfig::default()
        }
    }
}

/// Query-side parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagQueryConfig {
    /// Maximum hits to return.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Optional lower bound; hits below it are dropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_threshold: Option<f32>,
    /// Extra metadata constraints merged with the namespace scope.
    #[serde(default)]
    pub fields: std::collections::HashMap<String, String>,
    /// Sentence-window radius applied around each hit.
    #[serde(default)]
    pub window_radius: usize,
    /// Promote stored parent sections for hits carrying a parent link.
    #[serde(default)]
    pub auto_merge: bool,
    /// Rerank options; unset disables reranking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank: Option<RerankOptions>,
}

fn default_top_k() -> usize {
    5
}

impl Default for RagQueryConfig {
    fn default() -> Self {
        Self {
            top_k: default_top_k(),
            score_threshold: None,
            fields: std::collections::HashMap::new(),
            window_radius: 0,
            auto_merge: false,
            rerank: None,
        }
    }
}

/// Rerank options for queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankOptions {
    /// Maximum hits kept after reranking.
    pub top_n: usize,
    /// Candidate content truncation in characters.
    #[serde(default = "default_truncate_chars")]
    pub truncate_chars: usize,
}

fn default_truncate_chars() -> usize {
    500
}
