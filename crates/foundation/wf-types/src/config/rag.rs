//! Serializable configuration for optional RAG enhancement capabilities.
//!
//! Every capability starts disabled. The runtime only builds a service when
//! a host supplies the concrete storage, embedding and transport handles.

use serde::{Deserialize, Serialize};

/// Named chunking behavior exposed by host configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RagChunkPreset {
    #[default]
    Default,
    Markdown,
    Sentence,
    Code,
    Table,
}

/// Change detection basis exposed by host configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RagSyncBasis {
    #[default]
    ContentHash,
    HttpValidation,
}

/// Multi-list fusion mode exposed by host configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RagFusionMode {
    #[default]
    Rrf,
    Concat,
    Weighted,
}

/// Opt-in URL ingestion settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RagIngestCapabilityConfig {
    pub enabled: bool,
    pub default_collection: String,
    pub default_namespace: String,
    pub max_content_chars: usize,
    pub allowed_domains: Vec<String>,
    /// Filesystem roots available to the `file` source reader.
    #[serde(default)]
    pub source_roots: Vec<String>,
}

impl Default for RagIngestCapabilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_collection: "kb".to_string(),
            default_namespace: "kb".to_string(),
            max_content_chars: 2_000_000,
            allowed_domains: Vec::new(),
            source_roots: Vec::new(),
        }
    }
}

/// Named chunking settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RagChunkingCapabilityConfig {
    pub preset: RagChunkPreset,
    pub target_chars: usize,
    pub overlap_chars: usize,
    pub hierarchical: bool,
}

impl Default for RagChunkingCapabilityConfig {
    fn default() -> Self {
        Self {
            preset: RagChunkPreset::Default,
            target_chars: 800,
            overlap_chars: 100,
            hierarchical: false,
        }
    }
}

/// Incremental sync settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RagSyncCapabilityConfig {
    pub basis: RagSyncBasis,
}

impl Default for RagSyncCapabilityConfig {
    fn default() -> Self {
        Self {
            basis: RagSyncBasis::ContentHash,
        }
    }
}

/// Retrieval enhancement switches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RagRetrievalCapabilityConfig {
    pub hybrid: bool,
    pub keyword_candidates: usize,
    pub fusion_mode: RagFusionMode,
    pub rerank: bool,
    pub rerank_top_n: usize,
    pub window_radius: usize,
    pub context_budget_chars: Option<usize>,
}

impl Default for RagRetrievalCapabilityConfig {
    fn default() -> Self {
        Self {
            hybrid: false,
            keyword_candidates: 0,
            fusion_mode: RagFusionMode::Rrf,
            rerank: false,
            rerank_top_n: 5,
            window_radius: 0,
            context_budget_chars: None,
        }
    }
}

/// Citation presentation settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RagCitationCapabilityConfig {
    pub excerpt_chars: usize,
    pub include_score: bool,
    pub no_hits_template: Option<String>,
}

impl Default for RagCitationCapabilityConfig {
    fn default() -> Self {
        Self {
            excerpt_chars: 200,
            include_score: false,
            no_hits_template: None,
        }
    }
}

/// Offline evaluation settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RagEvalCapabilityConfig {
    pub enabled: bool,
    pub max_cases: usize,
}

impl Default for RagEvalCapabilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_cases: 100,
        }
    }
}

/// Replaceable prompt templates for synthesis and query transforms.
///
/// Every field is a plain template string with named `{placeholder}` slots;
/// empty strings fall back to the facade defaults. Kept dependency-free so
/// the foundation config layer does not depend on the RAG facade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RagPromptTemplatesConfig {
    pub system: String,
    pub synthesis: String,
    pub passage: String,
    pub refine: String,
    pub compress: String,
    pub decompose: String,
    pub hypothetical: String,
    pub feedback: String,
    pub rewrite: String,
    pub variants: String,
}

impl Default for RagPromptTemplatesConfig {
    fn default() -> Self {
        Self {
            system: "Use only the supplied passages when answering; cite numbered passages."
                .to_string(),
            synthesis: "Answer the question using the numbered passages.\n\nQuestion: {query}\n\nPassages:\n{context}"
                .to_string(),
            passage: "Answer the question using this passage numbered {index} from {source}.\n\nQuestion: {query}\n\nPassage {index}: {excerpt}"
                .to_string(),
            refine: "Refine the draft answer using this passage numbered {index} from {source}.\n\nQuestion: {query}\n\nDraft:\n{draft}\n\nPassage {index}: {excerpt}"
                .to_string(),
            compress: "Compress the draft answer while keeping its citations.\n\nDraft:\n{draft}"
                .to_string(),
            decompose: "Break the question below into independent sub-questions that together cover it. List each sub-question on its own line starting with '- '. Keep them short.\n\nQuestion: {query}"
                .to_string(),
            hypothetical: "Write a short factual passage that answers the question below. Output only the passage.\n\nQuestion: {query}"
                .to_string(),
            feedback: "Rewrite the question below into a sharper retrieval query using the passages as context. Output only the rewritten question.\n\nQuestion: {query}\n\nPassages:\n{context}"
                .to_string(),
            rewrite: "Rewrite the follow-up question into a standalone question using the conversation history. Output only the rewritten question.\n\nHistory:\n{history}\n\nFollow-up: {query}"
                .to_string(),
            variants: "Rewrite the question below into {count} diverse search variants covering the same need. List each variant on its own line starting with '- '.\n\nQuestion: {query}"
                .to_string(),
        }
    }
}

/// Question-answering capability built on top of retrieval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RagAnswerCapabilityConfig {
    /// Enables the synthesis and answer flow; needs a completion profile.
    pub enabled: bool,
    /// Model profile id used for synthesis and query transforms.
    pub profile_id: String,
    /// Maximum passages fed into one synthesis call.
    pub max_passages: usize,
    /// Replaceable prompt templates.
    pub prompts: RagPromptTemplatesConfig,
}

impl Default for RagAnswerCapabilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            profile_id: "DEFAULT".to_string(),
            max_passages: 6,
            prompts: RagPromptTemplatesConfig::default(),
        }
    }
}

/// Complete RAG enhancement configuration; disabled as a whole by default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RagCapabilitiesConfig {
    pub enabled: bool,
    pub default_collection: String,
    pub default_namespace: String,
    pub ingest: RagIngestCapabilityConfig,
    pub chunking: RagChunkingCapabilityConfig,
    pub sync: RagSyncCapabilityConfig,
    pub retrieval: RagRetrievalCapabilityConfig,
    pub citation: RagCitationCapabilityConfig,
    pub eval: RagEvalCapabilityConfig,
    pub answer: RagAnswerCapabilityConfig,
}

impl Default for RagCapabilitiesConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_collection: "kb".to_string(),
            default_namespace: "kb".to_string(),
            ingest: RagIngestCapabilityConfig::default(),
            chunking: RagChunkingCapabilityConfig::default(),
            sync: RagSyncCapabilityConfig::default(),
            retrieval: RagRetrievalCapabilityConfig::default(),
            citation: RagCitationCapabilityConfig::default(),
            eval: RagEvalCapabilityConfig::default(),
            answer: RagAnswerCapabilityConfig::default(),
        }
    }
}

impl RagCapabilitiesConfig {
    /// Whether the host should attach the retrieval service. Read-only
    /// search is part of the main switch; enhanced tools keep their own.
    pub fn any_enabled(&self) -> bool {
        self.enabled
    }

    /// Validates numeric bounds before the config reaches a pipeline.
    pub fn validate(&self) -> Result<(), String> {
        if self.default_collection.trim().is_empty() || self.default_namespace.trim().is_empty() {
            return Err("rag default collection and namespace must not be empty".to_string());
        }
        if self.ingest.max_content_chars == 0 {
            return Err("rag ingest max_content_chars must be positive".to_string());
        }
        if self.chunking.target_chars == 0
            || self.chunking.overlap_chars >= self.chunking.target_chars
        {
            return Err("rag chunking requires positive target and smaller overlap".to_string());
        }
        if self.retrieval.rerank && self.retrieval.rerank_top_n == 0 {
            return Err("rag rerank_top_n must be positive".to_string());
        }
        if self.retrieval.context_budget_chars == Some(0) {
            return Err("rag context_budget_chars must be positive".to_string());
        }
        if self.eval.max_cases == 0 {
            return Err("rag eval max_cases must be positive".to_string());
        }
        if self.citation.excerpt_chars == 0 {
            return Err("rag citation excerpt_chars must be positive".to_string());
        }
        if self.answer.enabled && self.answer.profile_id.trim().is_empty() {
            return Err("rag answer profile_id must not be empty when enabled".to_string());
        }
        if self.answer.max_passages == 0 {
            return Err("rag answer max_passages must be positive".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_every_capability_closed() {
        let config = RagCapabilitiesConfig::default();
        assert!(!config.enabled);
        assert!(!config.any_enabled());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn read_only_retrieval_can_be_enabled_alone() {
        let config = RagCapabilitiesConfig {
            enabled: true,
            ..RagCapabilitiesConfig::default()
        };
        assert!(config.any_enabled());
        assert!(!config.ingest.enabled);
        assert!(!config.eval.enabled);
    }

    #[test]
    fn validation_rejects_invalid_budgets() {
        let mut config = RagCapabilitiesConfig::default();
        config.chunking.overlap_chars = config.chunking.target_chars;
        assert!(config.validate().is_err());
    }
}
