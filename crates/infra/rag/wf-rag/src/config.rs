//! Serializable ingest and query configuration.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use vector_contracts::{VectorKind, VectorSpaceSpec};

use crate::chunker::ChunkerConfig;
use crate::cleaner::CleanerConfig;

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
    /// Cleaning rules applied before chunking.
    #[serde(default)]
    pub cleaning: CleanerConfig,
    /// Plugin metadata extractor enriching documents before chunking;
    /// unset keeps caller-provided metadata unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_extractor: Option<String>,
    /// Plugin chunk splitter replacing the built-in splitter;
    /// unset keeps built-in chunking. Conflicts with `hierarchical`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_splitter: Option<String>,
    /// Retrieval kind of the collection; dense keeps the embedding path.
    #[serde(default)]
    pub vector_kind: VectorKind,
    /// Vector encoder used for writes. Required for sparse and multi kinds;
    /// unset on dense keeps the embedding provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    /// Sparse vocabulary bound; unset derives the bound from encoded data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse_max_index: Option<u32>,
    /// Per-point sub-vector bound for multi collections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_max_vectors: Option<usize>,
    /// Named vector spaces written by the configured named encoder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named_spaces: Option<BTreeMap<String, VectorSpaceSpec>>,
}

/// Named chunking behavior. A preset only selects separators and
/// pre/post-processing; explicit size parameters still win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkPreset {
    /// Generic recursive separators.
    #[default]
    Default,
    /// Markdown headings and lists define section boundaries.
    Markdown,
    /// Sentence boundaries are preferred.
    Sentence,
    /// Blank lines and code block boundaries are preferred.
    Code,
    /// Markdown tables and fenced blocks stay intact.
    Table,
}

/// Chunking parameters in serializable form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkerConfigData {
    /// Named behavior applied before explicit size overrides.
    #[serde(default)]
    pub preset: ChunkPreset,
    /// Soft target length of a leaf chunk in characters.
    pub target_chars: usize,
    /// Overlap between consecutive leaf chunks in characters.
    pub overlap_chars: usize,
    /// Optional token budget per leaf chunk (whitespace tokens, unset disables).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<usize>,
}

fn default_chunker_config() -> ChunkerConfigData {
    ChunkerConfigData {
        preset: ChunkPreset::Default,
        target_chars: 800,
        overlap_chars: 100,
        token_budget: None,
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
            cleaning: CleanerConfig::default(),
            metadata_extractor: None,
            chunk_splitter: None,
            vector_kind: VectorKind::Dense,
            encoder: None,
            sparse_max_index: None,
            multi_max_vectors: None,
            named_spaces: None,
        }
    }

    /// Builds an ingest config for a named-layout collection.
    ///
    /// `encoder` must emit the named encoding schema with one vector per
    /// declared space.
    pub fn with_named_spaces(
        mut self,
        spaces: BTreeMap<String, VectorSpaceSpec>,
        encoder: impl Into<String>,
    ) -> Self {
        self.named_spaces = Some(spaces);
        self.encoder = Some(encoder.into());
        self.vector_kind = VectorKind::Dense;
        self
    }

    /// Builds an ingest config for one retrieval kind.
    pub fn with_kind(mut self, kind: VectorKind, encoder: impl Into<String>) -> Self {
        self.vector_kind = kind;
        self.encoder = Some(encoder.into());
        self
    }

    /// Converts to the runtime chunker config, applying the named preset
    /// first and the explicit size parameters afterwards.
    pub fn chunker_config(&self) -> ChunkerConfig {
        let mut config = match self.chunking.preset {
            ChunkPreset::Default => ChunkerConfig::default(),
            ChunkPreset::Markdown => ChunkerConfig::markdown(),
            ChunkPreset::Sentence => ChunkerConfig::sentence(),
            ChunkPreset::Code => ChunkerConfig::code(),
            ChunkPreset::Table => ChunkerConfig::table(),
        };
        config.target_chars = self.chunking.target_chars;
        config.overlap_chars = self.chunking.overlap_chars;
        config.token_budget = self.chunking.token_budget;
        config
    }
}

/// One additional collection queried with its own kind and encoder.
/// Its hits enter the same fusion as the primary collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindQueryConfig {
    /// Collection to query.
    pub collection: String,
    /// Retrieval kind of that collection.
    pub kind: VectorKind,
    /// Registered encoder used to encode the query text.
    pub encoder: String,
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
    /// Structured metadata conditions merged with the namespace scope.
    #[serde(default)]
    pub conditions: Vec<vector_contracts::FieldFilter>,
    /// Sentence-window radius applied around each hit.
    #[serde(default)]
    pub window_radius: usize,
    /// Promote stored parent sections for hits carrying a parent link.
    #[serde(default)]
    pub auto_merge: bool,
    /// Rerank options; unset disables reranking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank: Option<RerankOptions>,
    /// How multi-list results are fused.
    #[serde(default)]
    pub fusion: FusionConfig,
    /// Keyword candidates blended with dense hits (0 disables).
    #[serde(default)]
    pub keyword_candidates: usize,
    /// Weight of the keyword score when blending with dense scores.
    #[serde(default = "default_keyword_weight")]
    pub keyword_weight: f32,
    /// Reorder hits for long-context prompts (strongest at both ends).
    #[serde(default)]
    pub context_reorder: bool,
    /// Drop weakest hits beyond this content budget in chars (unset disables).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_budget_chars: Option<usize>,
    /// Registered encoder used on the query path; unset embeds dense queries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector_encoding: Option<String>,
    /// Retrieval kind of the primary collection; dense keeps the embedding path.
    #[serde(default)]
    pub vector_kind: VectorKind,
    /// Candidate count for sparse recall; unset uses the normal candidate limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse_candidates: Option<usize>,
    /// Candidate count for multi-vector recall; unset uses the normal limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_budget: Option<usize>,
    /// Allows weighted fusion across candidate lists of different kinds.
    #[serde(default)]
    pub weighted_comparable: bool,
    /// Additional kind-specific collections recalled and fused with the primary one.
    #[serde(default)]
    pub additional_kinds: Vec<KindQueryConfig>,
    /// Named-space declarations used by the named encoder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named_spaces: Option<BTreeMap<String, VectorSpaceSpec>>,
    /// Named space queried on the primary collection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_space: Option<String>,
    /// Additional named spaces recalled from the same collection and fused.
    #[serde(default)]
    pub additional_named_spaces: Vec<String>,
    /// Plugin post-fusion shaping step; unset keeps fused order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postprocessor: Option<String>,
}

/// Multi-list fusion strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FusionMode {
    /// Reciprocal rank fusion (default).
    #[default]
    Rrf,
    /// Concatenate lists keeping first-seen order, then truncate.
    Concat,
    /// Score-weighted blend preserving each list's normalized scores.
    Weighted,
}

impl FusionMode {
    /// Stable name used in retrieval metrics.
    pub fn as_name(self) -> &'static str {
        match self {
            Self::Rrf => "rrf",
            Self::Concat => "concat",
            Self::Weighted => "weighted",
        }
    }
}

/// Fusion parameters shared by multi-query and hybrid retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionConfig {
    /// Fusion strategy.
    #[serde(default)]
    pub mode: FusionMode,
    /// RRF depth constant.
    #[serde(default = "default_rrf_k")]
    pub rrf_k: u32,
    /// Dense-list weight for weighted blending.
    #[serde(default = "default_dense_weight")]
    pub dense_weight: f32,
    /// Keyword/other-list weight for weighted blending.
    #[serde(default = "default_other_weight")]
    pub other_weight: f32,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self {
            mode: FusionMode::Rrf,
            rrf_k: default_rrf_k(),
            dense_weight: default_dense_weight(),
            other_weight: default_other_weight(),
        }
    }
}

fn default_rrf_k() -> u32 {
    60
}

fn default_dense_weight() -> f32 {
    0.7
}

fn default_other_weight() -> f32 {
    0.3
}

fn default_keyword_weight() -> f32 {
    0.3
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
            fusion: FusionConfig::default(),
            keyword_candidates: 0,
            keyword_weight: default_keyword_weight(),
            context_reorder: false,
            context_budget_chars: None,
            vector_encoding: None,
            vector_kind: VectorKind::Dense,
            sparse_candidates: None,
            multi_budget: None,
            weighted_comparable: false,
            additional_kinds: Vec::new(),
            named_spaces: None,
            target_space: None,
            additional_named_spaces: Vec::new(),
            postprocessor: None,
            conditions: Vec::new(),
        }
    }
}

/// Stable fingerprint of the configured vector encoding.
/// Changing the kind, encoder or shape parameters invalidates stored vectors
/// and requires an explicit reindex.
pub fn vector_encoding_fingerprint(index: &RagIndexConfig) -> Option<String> {
    let encoder = index.encoder.as_deref()?.trim();
    if encoder.is_empty() {
        return None;
    }
    let value = serde_json::json!({
        "encoder": encoder,
        "kind": index.vector_kind,
        "sparse_max_index": index.sparse_max_index,
        "multi_max_vectors": index.multi_max_vectors,
        "named_spaces": index.named_spaces,
    });
    Some(crate::sync::content_hash(&value.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_selects_separators_and_explicit_sizes_win() {
        let index = RagIndexConfig {
            chunking: ChunkerConfigData {
                preset: ChunkPreset::Code,
                target_chars: 120,
                overlap_chars: 12,
                token_budget: None,
            },
            ..RagIndexConfig::new("kb")
        };
        let runtime = index.chunker_config();
        assert_eq!(runtime.target_chars, 120);
        assert_eq!(runtime.overlap_chars, 12);
        assert!(runtime
            .separators
            .iter()
            .any(|separator| separator == "}\n"));
    }

    #[test]
    fn encoding_fingerprint_tracks_kind_and_parameters() {
        let first = RagIndexConfig::new("kb").with_kind(VectorKind::Sparse, "sparse-encoder");
        let fingerprint = vector_encoding_fingerprint(&first).expect("fingerprint");
        assert_eq!(
            fingerprint,
            vector_encoding_fingerprint(&first).expect("stable")
        );
        let changed = RagIndexConfig {
            sparse_max_index: Some(100),
            ..first
        };
        assert_ne!(
            fingerprint,
            vector_encoding_fingerprint(&changed).expect("changed")
        );
        assert!(vector_encoding_fingerprint(&RagIndexConfig::new("kb")).is_none());
    }

    #[test]
    fn named_encoding_fingerprint_tracks_every_space_parameter() {
        let spaces = BTreeMap::from([(
            "title".to_string(),
            VectorSpaceSpec::new(
                VectorKind::Dense,
                8,
                serde_json::from_value(serde_json::json!("cosine")).expect("distance"),
            ),
        )]);
        let first = RagIndexConfig::new("kb").with_named_spaces(spaces, "named-encoder");
        let fingerprint = vector_encoding_fingerprint(&first).expect("fingerprint");
        let changed = RagIndexConfig::new("kb").with_named_spaces(
            BTreeMap::from([(
                "title".to_string(),
                VectorSpaceSpec::new(
                    VectorKind::Dense,
                    16,
                    serde_json::from_value(serde_json::json!("cosine")).expect("distance"),
                ),
            )]),
            "named-encoder",
        );
        assert_ne!(
            fingerprint,
            vector_encoding_fingerprint(&changed).expect("changed")
        );
    }
}
