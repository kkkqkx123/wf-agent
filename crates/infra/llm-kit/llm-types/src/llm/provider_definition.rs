use serde::{Deserialize, Serialize};

/// Declarative connection template for a class of LLM endpoints.
///
/// A provider definition carries connection defaults shared by multiple
/// profiles: base URL, authentication, default headers and model discovery.
/// It references one wire protocol format by name; profiles reference a
/// definition through `LlmProfile::provider_id` and override any default
/// with explicit fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmProviderDefinition {
    /// Provider id referenced by `LlmProfile::provider_id`.
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Default base URL; a profile-level `base_url` wins when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Default auth type (`native` / `bearer` / `x-api-key` /
    /// `x-goog-api-key`); a profile-level `auth_type` wins when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<String>,
    /// Default headers; profile `custom_headers` override per key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_headers: Option<crate::Metadata>,
    /// Wire protocol format resolved through `CodecRegistry`
    /// (built-in variant or registered custom name).
    pub format: super::LlmFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_discovery: Option<super::ModelDiscovery>,
    /// Version segment used when joining the default models endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
}
