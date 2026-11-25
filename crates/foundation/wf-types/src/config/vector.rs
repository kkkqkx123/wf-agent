//! Serializable configuration for the RAG vector backend and embedding provider.

use serde::{Deserialize, Serialize};

/// Vector storage backend selector. Memory is the development default;
/// embedded simvec and remote Qdrant are explicit opt-ins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VectorBackend {
    #[default]
    Memory,
    Simvec,
    Qdrant,
}

/// Distance function declared for newly created collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VectorDistance {
    #[default]
    Cosine,
    Dot,
    Euclid,
}

/// Vector backend configuration. Collection names and dimensions come from
/// callers; this type only decides where points live and how collections are
/// created.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VectorIndexConfig {
    pub backend: VectorBackend,
    pub simvec_path: Option<String>,
    pub qdrant_url: Option<String>,
    pub qdrant_api_key: Option<String>,
    pub qdrant_timeout_secs: u64,
    pub qdrant_batch_size: usize,
    pub qdrant_exact_count: bool,
    pub distance: VectorDistance,
}

impl Default for VectorIndexConfig {
    fn default() -> Self {
        Self {
            backend: VectorBackend::Memory,
            simvec_path: None,
            qdrant_url: None,
            qdrant_api_key: None,
            qdrant_timeout_secs: 10,
            qdrant_batch_size: 128,
            qdrant_exact_count: true,
            distance: VectorDistance::Cosine,
        }
    }
}

impl VectorIndexConfig {
    /// Validates fields required by the selected backend.
    pub fn validate(&self) -> Result<(), String> {
        match self.backend {
            VectorBackend::Memory => Ok(()),
            VectorBackend::Simvec
                if self
                    .simvec_path
                    .as_deref()
                    .is_some_and(|path| !path.trim().is_empty()) =>
            {
                Ok(())
            }
            VectorBackend::Simvec => Err("vector.simvec_path is required for simvec".to_string()),
            VectorBackend::Qdrant
                if self
                    .qdrant_url
                    .as_deref()
                    .is_some_and(|url| !url.trim().is_empty()) =>
            {
                Ok(())
            }
            VectorBackend::Qdrant => Err("vector.qdrant_url is required for qdrant".to_string()),
        }
    }
}

/// Embedding provider configuration reused by every RAG surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RagEmbeddingConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub dimension: usize,
    pub request_dimensions: Option<usize>,
    pub timeout_secs: u64,
}

impl Default for RagEmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            model: String::new(),
            api_key: None,
            dimension: 0,
            request_dimensions: None,
            timeout_secs: 30,
        }
    }
}

impl RagEmbeddingConfig {
    /// Validates the provider when the master switch is on.
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        if self.base_url.trim().is_empty() || self.model.trim().is_empty() {
            return Err("rag embedding requires base_url and model".to_string());
        }
        if self.dimension == 0 {
            return Err("rag embedding dimension must be positive".to_string());
        }
        Ok(())
    }
}
