//! Cohere-compatible rerank provider over a dedicated `/rerank` endpoint.
//!
//! Sends `{model, query, documents, top_n, return_documents}` and reads
//! `{results: [{index, relevance_score}]}`. Candidate identity is recovered
//! positionally, so documents are plain strings (object entries are rejected
//! by compatible servers such as SiliconFlow).

use async_trait::async_trait;
use serde::Deserialize;

use crate::config::RerankConfig;
use crate::error::{RerankError, Result};
use crate::provider::{
    assemble_reranked, limit_candidates, truncate_content, validate_request, RerankProvider,
    RerankRequest, RerankResult, ScoredCandidate,
};

/// Rerank provider backed by a Cohere-compatible `/rerank` endpoint.
pub struct CohereRerankProvider {
    config: RerankConfig,
    client: reqwest::Client,
}

impl CohereRerankProvider {
    /// Creates a provider; `config.base_url` is the full `/rerank` URL.
    pub fn new(config: RerankConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs.max(1)))
            .build()
            .map_err(|err| RerankError::Transport(err.to_string()))?;
        Ok(Self { config, client })
    }

    /// Returns the provider configuration.
    pub fn config(&self) -> &RerankConfig {
        &self.config
    }

    /// Builds the Cohere-compatible request body for limited candidates.
    fn build_request_body(&self, request: &RerankRequest) -> serde_json::Value {
        let documents: Vec<String> = request
            .candidates
            .iter()
            .map(|candidate| truncate_content(&candidate.content, 500))
            .collect();
        serde_json::json!({
            "model": self.config.model,
            "query": request.query,
            "documents": documents,
            "top_n": request.candidates.len(),
            "return_documents": false,
        })
    }

    /// Parses a Cohere-compatible response into scored candidates.
    fn parse_rerank_response(
        &self,
        response: &str,
        request: &RerankRequest,
    ) -> Result<Vec<ScoredCandidate>> {
        #[derive(Debug, Deserialize)]
        struct CohereResponse {
            results: Vec<CohereItem>,
        }

        #[derive(Debug, Deserialize)]
        struct CohereItem {
            index: usize,
            relevance_score: f32,
        }

        let parsed: CohereResponse = serde_json::from_str(response).map_err(|err| {
            let preview: String = response.chars().take(200).collect();
            RerankError::Decode(format!(
                "failed to parse rerank response: {err}. Preview: {preview}"
            ))
        })?;

        if parsed.results.len() != request.candidates.len() {
            return Err(RerankError::Decode(format!(
                "rerank count mismatch: expected {}, received {}",
                request.candidates.len(),
                parsed.results.len()
            )));
        }

        let mut seen = std::collections::HashSet::with_capacity(parsed.results.len());
        let mut scores = Vec::with_capacity(parsed.results.len());
        for item in &parsed.results {
            if !seen.insert(item.index) {
                return Err(RerankError::Decode(format!(
                    "rerank response contains duplicate candidate index '{}'",
                    item.index
                )));
            }
            let candidate = request.candidates.get(item.index).ok_or_else(|| {
                RerankError::Decode(format!(
                    "rerank response contains out-of-range candidate index '{}'",
                    item.index
                ))
            })?;
            scores.push(ScoredCandidate {
                id: candidate.id.clone(),
                score: item.relevance_score,
                reasoning: Some(format!(
                    "cross-encoder relevance score {:.3}",
                    item.relevance_score
                )),
            });
        }
        Ok(scores)
    }
}

#[async_trait]
impl RerankProvider for CohereRerankProvider {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult> {
        validate_request(request)?;
        let limited = limit_candidates(request);
        let body = self.build_request_body(&limited);

        let start = std::time::Instant::now();
        let call = async {
            let mut outgoing = self.client.post(&self.config.base_url).json(&body);
            if let Some(api_key) = &self.config.api_key {
                outgoing = outgoing.bearer_auth(api_key);
            }
            let response = outgoing.send().await?;
            let status = response.status();
            if !status.is_success() {
                let text = response.text().await.unwrap_or_default();
                return Err(RerankError::Provider {
                    status: status.as_u16(),
                    message: text,
                });
            }
            let text = response
                .text()
                .await
                .map_err(|err| RerankError::Transport(err.to_string()))?;
            Ok(text)
        };
        let timeout = std::time::Duration::from_millis(limited.config.timeout_ms.max(1));
        let response = tokio::time::timeout(timeout, call).await??;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let total_tokens = serde_json::from_str::<serde_json::Value>(&response)
            .ok()
            .and_then(|value| value.get("usage")?.get("total_tokens")?.as_u64())
            .unwrap_or(0);

        let reranked_candidates =
            assemble_reranked(&limited, self.parse_rerank_response(&response, &limited)?)?;

        Ok(RerankResult {
            reranked_candidates,
            prompt_tokens: 0,
            total_tokens,
            elapsed_ms,
        })
    }

    fn provider_name(&self) -> &str {
        "cross-encoder"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::tests::test_request;

    fn test_provider() -> CohereRerankProvider {
        CohereRerankProvider::new(RerankConfig::new(
            "https://api.example.com/v1/rerank",
            "BAAI/bge-reranker-v2-m3",
        ))
        .expect("provider builds")
    }

    #[test]
    fn request_body_uses_plain_string_documents() {
        let body = test_provider().build_request_body(&test_request());
        assert_eq!(body["model"], "BAAI/bge-reranker-v2-m3");
        assert_eq!(body["query"], "how to start the app");
        assert_eq!(body["top_n"], 2);
        let documents = body["documents"].as_array().expect("documents array");
        assert_eq!(documents.len(), 2);
        assert!(documents.iter().all(|document| document.is_string()));
        assert!(documents[0].as_str().expect("string").contains("fn main()"));
    }

    #[test]
    fn successful_response_parses_and_orders() {
        let response =
            r#"{"results":[{"index":1,"relevance_score":0.9},{"index":0,"relevance_score":0.6}]}"#;
        let request = test_request();
        let scores = test_provider()
            .parse_rerank_response(response, &request)
            .expect("parses");
        assert_eq!(scores.len(), 2);
        let reranked = assemble_reranked(&request, scores).expect("assembles");
        assert_eq!(reranked[0].id, "c2");
        assert_eq!(reranked[0].rank_change, 1);
        assert_eq!(reranked[1].id, "c1");
        assert_eq!(reranked[1].rank_change, -1);
    }

    #[test]
    fn malformed_responses_are_rejected() {
        let request = test_request();
        let provider = test_provider();

        let short = r#"{"results":[{"index":0,"relevance_score":0.9}]}"#;
        assert!(provider.parse_rerank_response(short, &request).is_err());

        let out_of_range =
            r#"{"results":[{"index":0,"relevance_score":0.9},{"index":5,"relevance_score":0.8}]}"#;
        assert!(provider
            .parse_rerank_response(out_of_range, &request)
            .is_err());

        let duplicate =
            r#"{"results":[{"index":0,"relevance_score":0.9},{"index":0,"relevance_score":0.8}]}"#;
        assert!(provider.parse_rerank_response(duplicate, &request).is_err());

        let bad_score =
            r#"{"results":[{"index":0,"relevance_score":2.5},{"index":1,"relevance_score":0.8}]}"#;
        let scores = provider
            .parse_rerank_response(bad_score, &request)
            .expect("indexes parse");
        assert!(assemble_reranked(&request, scores).is_err());
    }
}
