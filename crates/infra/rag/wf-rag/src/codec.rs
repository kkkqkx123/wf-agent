//! Host-side protocol codecs for the embedding and rerank model ports.
//!
//! This module mirrors the chat-LLM codec design: a codec only describes how
//! to encode a request and decode a response for one wire protocol. The host
//! keeps HTTP transport, credentials, retries and timeouts, and translates
//! the decoded values into its own model types. Plugins contribute a codec
//! through [`PluginEmbeddingCodec`]/[`PluginRerankCodec`] and are adapted here;
//! they never own a model call or an API key.
//!
//! Hosts hold codec instances directly; no process-wide registry is kept.
//! The default provider construction path stays host-owned and unchanged.

use std::sync::Arc;
use serde_json::Value;
use wf_plugin_sdk::{CodecHttpRequest, PluginEmbeddingCodec, PluginRerankCodec};

use crate::error::{RagError, Result};

/// Host-side embedding wire protocol codec.
///
/// Splits a batch of texts into a request description and decodes the response
/// body into ordered dense vectors. No HTTP client, no credentials: the host
/// materializes [`CodecHttpRequest`] and performs the call.
pub trait EmbeddingCodec: Send + Sync {
    /// Stable protocol name used for registry lookup and diagnostics.
    fn name(&self) -> &str;
    /// Describe the HTTP request for a batch of texts.
    fn build_request(&self, texts: &[String], config: &Value) -> Result<CodecHttpRequest>;
    /// Decode a response body into dense vectors.
    fn parse_response(&self, body: &str) -> Result<Vec<Vec<f32>>>;
}

/// Host-side rerank wire protocol codec.
pub trait RerankCodec: Send + Sync {
    /// Stable protocol name used for registry lookup and diagnostics.
    fn name(&self) -> &str;
    /// Describe the HTTP request for a query and candidate documents.
    fn build_request(
        &self,
        query: &str,
        documents: &[String],
        config: &Value,
    ) -> Result<CodecHttpRequest>;
    /// Decode a response body into scores aligned with the documents.
    fn parse_response(&self, body: &str) -> Result<Vec<f64>>;
}

fn plugin_error(error: wf_plugin_sdk::PluginError) -> RagError {
    RagError::Embedding(error.to_string())
}

fn encode_vectors(value: Value) -> Result<Vec<Vec<f32>>> {
    let vectors = value
        .get("vectors")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            RagError::Embedding("embedding codec response misses a 'vectors' array".to_string())
        })?;
    vectors
        .iter()
        .map(|row| {
            let values = row.as_array().ok_or_else(|| {
                RagError::Embedding("embedding codec vector is not an array".to_string())
            })?;
            values
                .iter()
                .map(|v| {
                    v.as_f64().map(|f| f as f32).ok_or_else(|| {
                        RagError::Embedding("embedding codec vector has a non-number".to_string())
                    })
                })
                .collect()
        })
        .collect()
}

fn encode_scores(value: Value) -> Result<Vec<f64>> {
    let scores = value
        .get("scores")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            RagError::Rerank("rerank codec response misses a 'scores' array".to_string())
        })?;
    scores
        .iter()
        .map(|v| {
            v.as_f64()
                .ok_or_else(|| RagError::Rerank("rerank codec score is not a number".to_string()))
        })
        .collect()
}

/// Adapts a plugin [`PluginEmbeddingCodec`] to the host [`EmbeddingCodec`].
///
/// Structured values cross the boundary as JSON. The plugin only answers with
/// a request description and a decoded response; the host builds the request.
pub struct PluginEmbeddingCodecAdapter {
    name: String,
    codec: Arc<dyn PluginEmbeddingCodec>,
}

impl PluginEmbeddingCodecAdapter {
    /// Wraps a plugin codec under a stable registry name.
    pub fn new(name: impl Into<String>, codec: Arc<dyn PluginEmbeddingCodec>) -> Self {
        Self {
            name: name.into(),
            codec,
        }
    }

    /// Returns the underlying plugin codec.
    pub fn codec(&self) -> &Arc<dyn PluginEmbeddingCodec> {
        &self.codec
    }
}

impl EmbeddingCodec for PluginEmbeddingCodecAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn build_request(&self, texts: &[String], config: &Value) -> Result<CodecHttpRequest> {
        let texts = serde_json::to_value(texts).map_err(|e| RagError::Embedding(e.to_string()))?;
        self.codec
            .build_request(texts, config.clone())
            .map_err(plugin_error)
    }

    fn parse_response(&self, body: &str) -> Result<Vec<Vec<f32>>> {
        let value = self.codec.parse_response(body).map_err(plugin_error)?;
        encode_vectors(value)
    }
}

/// Adapts a plugin [`PluginRerankCodec`] to the host [`RerankCodec`].
pub struct PluginRerankCodecAdapter {
    name: String,
    codec: Arc<dyn PluginRerankCodec>,
}

impl PluginRerankCodecAdapter {
    /// Wraps a plugin codec under a stable registry name.
    pub fn new(name: impl Into<String>, codec: Arc<dyn PluginRerankCodec>) -> Self {
        Self {
            name: name.into(),
            codec,
        }
    }

    /// Returns the underlying plugin codec.
    pub fn codec(&self) -> &Arc<dyn PluginRerankCodec> {
        &self.codec
    }
}

impl RerankCodec for PluginRerankCodecAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn build_request(
        &self,
        query: &str,
        documents: &[String],
        config: &Value,
    ) -> Result<CodecHttpRequest> {
        let documents =
            serde_json::to_value(documents).map_err(|e| RagError::Rerank(e.to_string()))?;
        self.codec
            .build_request(query.to_string(), documents, config.clone())
            .map_err(|e| RagError::Rerank(e.to_string()))
    }

    fn parse_response(&self, body: &str) -> Result<Vec<f64>> {
        let value = self
            .codec
            .parse_response(body)
            .map_err(|e| RagError::Rerank(e.to_string()))?;
        encode_scores(value)
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    struct EchoEmbeddingCodec;

    impl EmbeddingCodec for EchoEmbeddingCodec {
        fn name(&self) -> &str {
            "echo"
        }
        fn build_request(&self, texts: &[String], _config: &Value) -> Result<CodecHttpRequest> {
            Ok(CodecHttpRequest {
                method: "POST".to_string(),
                url: "https://example.test/embeddings".to_string(),
                headers: std::collections::BTreeMap::new(),
                body: serde_json::to_value(texts)
                    .map_err(|e| RagError::Embedding(e.to_string()))?,
            })
        }
        fn parse_response(&self, body: &str) -> Result<Vec<Vec<f32>>> {
            let value: Value =
                serde_json::from_str(body).map_err(|e| RagError::Embedding(e.to_string()))?;
            encode_vectors(value)
        }
    }

    struct EchoRerankCodec;

    impl RerankCodec for EchoRerankCodec {
        fn name(&self) -> &str {
            "echo-rank"
        }
        fn build_request(
            &self,
            query: &str,
            documents: &[String],
            _config: &Value,
        ) -> Result<CodecHttpRequest> {
            Ok(CodecHttpRequest {
                method: "POST".to_string(),
                url: "https://example.test/rerank".to_string(),
                headers: std::collections::BTreeMap::new(),
                body: serde_json::json!({"query": query, "documents": documents}),
            })
        }
        fn parse_response(&self, body: &str) -> Result<Vec<f64>> {
            let value: Value =
                serde_json::from_str(body).map_err(|e| RagError::Rerank(e.to_string()))?;
            encode_scores(value)
        }
    }

    #[test]
    fn embedding_codec_registry_round_trip() {
        let codec = EchoEmbeddingCodec;
        assert_eq!(codec.name(), "echo");
        let request = codec
            .build_request(&["hello".to_string()], &serde_json::json!({}))
            .expect("request");
        assert_eq!(request.method, "POST");
        let vectors = codec
            .parse_response(r#"{"vectors":[[0.1,0.2],[0.3,0.4]]}"#)
            .expect("vectors");
        assert_eq!(vectors.len(), 2);
        assert_eq!(vectors[0], vec![0.1_f32, 0.2]);
    }

    #[test]
    fn rerank_codec_registry_round_trip() {
        let codec = EchoRerankCodec;
        assert_eq!(codec.name(), "echo-rank");
        let request = codec
            .build_request(
                "q",
                &["a".to_string(), "b".to_string()],
                &serde_json::json!({}),
            )
            .expect("request");
        assert_eq!(request.method, "POST");
        let scores = codec
            .parse_response(r#"{"scores":[0.9,0.1]}"#)
            .expect("scores");
        assert_eq!(scores, vec![0.9, 0.1]);
    }

    #[test]
    fn malformed_responses_are_rejected() {
        let codec = EchoEmbeddingCodec;
        assert!(codec.parse_response("{}").is_err());
        let rank = EchoRerankCodec;
        assert!(rank.parse_response("{}").is_err());
    }
}
