//! Serializable configuration for the RAG vector backend and embedding provider.

use serde::{Deserialize, Serialize};

/// Default embedding request timeout in seconds. Also used as the sentinel
/// that lets an llm profile fill the timeout when no explicit inline value
/// was supplied.
pub const DEFAULT_EMBEDDING_TIMEOUT_SECS: u64 = 30;

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
    /// Optional llm-domain profile id. When set, the host resolves endpoint,
    /// model and credentials from the llm profile/provider/auth domain instead
    /// of the inline fields below. Inline fields remain a fallback and keep
    /// existing configurations working unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_ref: Option<String>,
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
            profile_ref: None,
            base_url: String::new(),
            model: String::new(),
            api_key: None,
            dimension: 0,
            request_dimensions: None,
            timeout_secs: DEFAULT_EMBEDDING_TIMEOUT_SECS,
        }
    }
}

impl RagEmbeddingConfig {
    /// Resolves connection details from an llm-domain profile.
    ///
    /// When `profile_ref` matches `profile.id`, endpoint/model/credentials/
    /// timeout that are still unset on this config are filled from the
    /// profile. Explicit inline values always win, so existing configurations
    /// keep working unchanged and the llm domain stays the source of truth
    /// only for configs that opt in via `profile_ref`. RAG-specific fields
    /// (`dimension`, `request_dimensions`) are never taken from the profile.
    /// Returns `None` when `profile_ref` is set but no matching profile is
    /// supplied, so the caller can report a clear configuration error.
    pub fn resolve_with_profile(
        &self,
        profile: Option<&llm_types::llm::LlmProfile>,
    ) -> Result<Self, String> {
        let mut resolved = self.clone();
        let Some(profile) = profile else {
            if self
                .profile_ref
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty())
            {
                return Err(format!(
                    "rag embedding profile '{}' was not found in the llm domain",
                    self.profile_ref.as_deref().unwrap_or_default()
                ));
            }
            return Ok(resolved);
        };
        if resolved.base_url.trim().is_empty() {
            if let Some(url) = profile.base_url.as_deref() {
                resolved.base_url = url.to_string();
            }
        }
        if resolved.model.trim().is_empty() {
            resolved.model = profile.model.clone();
        }
        if resolved.api_key.is_none() {
            resolved.api_key = profile.api_key.clone();
        }
        if resolved.timeout_secs == DEFAULT_EMBEDDING_TIMEOUT_SECS {
            if let Some(timeout) = profile.timeout {
                resolved.timeout_secs = timeout;
            }
        }
        Ok(resolved)
    }

    /// Validates the provider when the master switch is on.
    ///
    /// When `profile_ref` is set, endpoint/model/credentials come from the
    /// llm domain and inline base_url/model are not required. RAG-specific
    /// dimension bounds still apply in both modes.
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        let uses_profile = self
            .profile_ref
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty());
        if !uses_profile && (self.base_url.trim().is_empty() || self.model.trim().is_empty()) {
            return Err("rag embedding requires base_url and model".to_string());
        }
        if self.dimension == 0 {
            return Err("rag embedding dimension must be positive".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str) -> llm_types::llm::LlmProfile {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": id,
            "format": "openai",
            "model": "profile-model",
            "base_url": "https://profile.example/v1",
            "api_key": "profile-key",
            "timeout": 77
        }))
        .expect("valid profile")
    }

    #[test]
    fn profile_fills_only_unset_inline_fields() {
        let config = RagEmbeddingConfig {
            enabled: true,
            profile_ref: Some("emb".to_string()),
            model: String::new(),
            base_url: String::new(),
            api_key: None,
            dimension: 768,
            request_dimensions: None,
            timeout_secs: 30,
        };
        let resolved = config
            .resolve_with_profile(Some(&profile("emb")))
            .expect("resolve");
        assert_eq!(resolved.base_url, "https://profile.example/v1");
        assert_eq!(resolved.model, "profile-model");
        assert_eq!(resolved.api_key.as_deref(), Some("profile-key"));
        assert_eq!(resolved.timeout_secs, 77);
        assert_eq!(resolved.dimension, 768);
    }

    #[test]
    fn explicit_inline_fields_win_over_profile() {
        let config = RagEmbeddingConfig {
            enabled: true,
            profile_ref: Some("emb".to_string()),
            base_url: "https://inline.example/v1".to_string(),
            model: "inline-model".to_string(),
            api_key: Some("inline-key".to_string()),
            dimension: 1536,
            request_dimensions: Some(512),
            timeout_secs: 12,
        };
        let resolved = config
            .resolve_with_profile(Some(&profile("emb")))
            .expect("resolve");
        assert_eq!(resolved.base_url, "https://inline.example/v1");
        assert_eq!(resolved.model, "inline-model");
        assert_eq!(resolved.api_key.as_deref(), Some("inline-key"));
        assert_eq!(resolved.timeout_secs, 12);
        assert_eq!(resolved.request_dimensions, Some(512));
    }

    #[test]
    fn missing_profile_is_reported() {
        let config = RagEmbeddingConfig {
            enabled: true,
            profile_ref: Some("absent".to_string()),
            dimension: 768,
            ..Default::default()
        };
        assert!(config.resolve_with_profile(None).is_err());
    }

    #[test]
    fn no_profile_ref_is_a_noop() {
        let config = RagEmbeddingConfig {
            enabled: true,
            base_url: "https://inline.example/v1".to_string(),
            model: "inline-model".to_string(),
            dimension: 768,
            ..Default::default()
        };
        let resolved = config.resolve_with_profile(None).expect("resolve");
        assert_eq!(resolved.base_url, "https://inline.example/v1");
    }
}
