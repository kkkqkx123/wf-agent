//! Endpoint configuration for rerank providers.

use serde::{Deserialize, Serialize};

/// Configuration shared by rerank providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankConfig {
    /// Full endpoint URL: the dedicated `/rerank` URL for Cohere-style
    /// providers, the chat-completions base URL for generative providers is
    /// configured separately (see [`crate::generative::GenerativeChatEndpoint`]).
    pub base_url: String,
    /// Bearer API key; absent for keyless local servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Rerank model name (e.g. `BAAI/bge-reranker-v2-m3`).
    pub model: String,
    /// HTTP request timeout in seconds.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    30
}

impl RerankConfig {
    /// Creates a config for an explicit endpoint URL and model.
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: None,
            model: model.into(),
            timeout_secs: default_timeout_secs(),
        }
    }

    /// Sets the bearer API key.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Sets the HTTP request timeout in seconds.
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_endpoint_and_defaults_timeout() {
        let config = RerankConfig::new("https://api.example.com/v1/rerank/", "reranker");
        assert_eq!(config.base_url, "https://api.example.com/v1/rerank");
        assert_eq!(config.timeout_secs, 30);
        assert!(config.api_key.is_none());
    }
}
