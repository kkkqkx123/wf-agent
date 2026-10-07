//! Generative rerank provider scoring through a chat prompt.
//!
//! Sends a cross-encoder style prompt to an OpenAI-compatible chat endpoint
//! and parses the returned JSON array `[{id, score, reasoning}]`. Score
//! validation, fusion, and ordering reuse the shared [`crate::provider`]
//! helpers. The chat call is a minimal inline POST on purpose: depending on
//! `llm-chat-basic` would couple the two leaf crates.

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::{RerankError, Result};
use crate::provider::{
    assemble_reranked, limit_candidates, truncate_content, validate_request, RerankProvider,
    RerankRequest, RerankResult, ScoredCandidate,
};

/// Chat endpoint used for generative scoring.
#[derive(Debug, Clone)]
pub struct GenerativeChatEndpoint {
    /// Full chat-completions URL (e.g. `https://api.example.com/v1/chat/completions`).
    pub base_url: String,
    /// Bearer API key; absent for keyless local servers.
    pub api_key: Option<String>,
    /// Chat model used for scoring.
    pub model: String,
}

impl GenerativeChatEndpoint {
    /// Creates an endpoint for a chat-completions URL and model.
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: None,
            model: model.into(),
        }
    }

    /// Sets the bearer API key.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }
}

/// Rerank provider scoring candidates with a chat model.
pub struct GenerativeRerankProvider {
    endpoint: GenerativeChatEndpoint,
    client: reqwest::Client,
}

impl GenerativeRerankProvider {
    /// Creates a provider for a chat endpoint and HTTP timeout.
    pub fn new(endpoint: GenerativeChatEndpoint, timeout_secs: u64) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs.max(1)))
            .build()
            .map_err(|err| RerankError::Transport(err.to_string()))?;
        Ok(Self { endpoint, client })
    }

    /// Returns the chat endpoint configuration.
    pub fn endpoint(&self) -> &GenerativeChatEndpoint {
        &self.endpoint
    }

    /// Builds the cross-encoder scoring prompt for limited candidates.
    fn build_prompt(&self, request: &RerankRequest) -> String {
        let mut prompt = format!(
            "You are a code search relevance evaluator. Given a query and multiple code snippets, \
             evaluate the relevance of each snippet to the query on a scale of 0.0 to 1.0.\n\n\
             Query: {}\n\n\
             Code Snippets:\n",
            request.query
        );
        for (index, candidate) in request.candidates.iter().enumerate() {
            prompt.push_str(&format!(
                "[{index}] ID: {}\nFile: {}\nType: {}\nContent:\n{}\n\n",
                candidate.id,
                candidate.file_path,
                candidate.entity_type.as_deref().unwrap_or("unknown"),
                truncate_content(&candidate.content, 500)
            ));
        }
        prompt.push_str(
            "Please output a JSON array with the following structure for each candidate:\n\
             [{\"id\": \"...\", \"score\": 0.0-1.0, \"reasoning\": \"...\"}]\n\
             Sort by score in descending order.",
        );
        prompt
    }

    /// Parses the chat content into scored candidates.
    fn parse_rerank_response(&self, response: &str) -> Result<Vec<ScoredCandidate>> {
        #[derive(Debug, Deserialize)]
        struct ScoredItem {
            id: String,
            score: f32,
            #[serde(default)]
            reasoning: String,
        }

        let json = extract_json_array(response);
        let items: Vec<ScoredItem> = serde_json::from_str(&json).map_err(|err| {
            let preview: String = response.chars().take(200).collect();
            RerankError::Decode(format!(
                "failed to parse rerank response: {err}. Preview: {preview}"
            ))
        })?;
        Ok(items
            .into_iter()
            .map(|item| ScoredCandidate {
                id: item.id,
                score: item.score,
                reasoning: Some(item.reasoning),
            })
            .collect())
    }
}

/// Extracts the first JSON array from surrounding chat prose.
fn extract_json_array(response: &str) -> String {
    if let Some(start) = response.find('[') {
        if let Some(end) = response.rfind(']') {
            if end > start {
                return response[start..=end].to_string();
            }
        }
    }
    response.to_string()
}

#[async_trait]
impl RerankProvider for GenerativeRerankProvider {
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult> {
        validate_request(request)?;
        let limited = limit_candidates(request);
        let prompt = self.build_prompt(&limited);
        let body = serde_json::json!({
            "model": self.endpoint.model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": limited.config.temperature,
            "max_tokens": 2000,
        });

        let start = std::time::Instant::now();
        let call = async {
            let mut outgoing = self.client.post(&self.endpoint.base_url).json(&body);
            if let Some(api_key) = &self.endpoint.api_key {
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
            let decoded: serde_json::Value = response
                .json()
                .await
                .map_err(|err| RerankError::Decode(err.to_string()))?;
            let content = decoded
                .pointer("/choices/0/message/content")
                .and_then(|value| value.as_str())
                .ok_or_else(|| RerankError::Decode("chat response contains no choices".into()))?;
            let prompt_tokens = decoded
                .pointer("/usage/prompt_tokens")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let total_tokens = decoded
                .pointer("/usage/total_tokens")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            Ok((content.to_string(), prompt_tokens, total_tokens))
        };
        let timeout = std::time::Duration::from_millis(limited.config.timeout_ms.max(1));
        let (content, prompt_tokens, total_tokens) = tokio::time::timeout(timeout, call).await??;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let reranked_candidates =
            assemble_reranked(&limited, self.parse_rerank_response(&content)?)?;

        Ok(RerankResult {
            reranked_candidates,
            prompt_tokens,
            total_tokens,
            elapsed_ms,
        })
    }

    fn provider_name(&self) -> &str {
        "generative-llm"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::tests::test_request;

    fn test_provider() -> GenerativeRerankProvider {
        GenerativeRerankProvider::new(
            GenerativeChatEndpoint::new("https://api.example.com/v1/chat/completions", "scorer"),
            30,
        )
        .expect("provider builds")
    }

    #[test]
    fn prompt_lists_every_candidate() {
        let prompt = test_provider().build_prompt(&test_request());
        assert!(prompt.contains("how to start the app"));
        assert!(prompt.contains("c1"));
        assert!(prompt.contains("c2"));
        assert!(prompt.contains("fn main()"));
    }

    #[test]
    fn json_extraction_tolerates_surrounding_prose() {
        let fenced = "Here is the result: [{\"id\": \"c1\", \"score\": 0.9}] done";
        assert_eq!(
            extract_json_array(fenced),
            "[{\"id\": \"c1\", \"score\": 0.9}]"
        );
        assert_eq!(extract_json_array("no array here"), "no array here");
    }

    #[test]
    fn successful_response_parses_and_orders() {
        let content = r#"[{"id":"c2","score":0.9,"reasoning":"matches"},{"id":"c1","score":0.4,"reasoning":"weak"}]"#;
        let mut request = test_request();
        request.config.return_reasoning = true;
        let scores = test_provider()
            .parse_rerank_response(content)
            .expect("parses");
        let reranked = assemble_reranked(&request, scores).expect("assembles");
        assert_eq!(reranked[0].id, "c2");
        assert_eq!(reranked[0].reasoning.as_deref(), Some("matches"));
        assert_eq!(reranked[1].id, "c1");
    }

    #[test]
    fn malformed_responses_are_rejected() {
        let request = test_request();
        let provider = test_provider();

        let short = r#"[{"id":"c1","score":0.9}]"#;
        let scores = provider
            .parse_rerank_response(short)
            .expect("indexes parse");
        assert!(assemble_reranked(&request, scores).is_err());

        let duplicate = r#"[{"id":"c1","score":0.9},{"id":"c1","score":0.8}]"#;
        let scores = provider
            .parse_rerank_response(duplicate)
            .expect("indexes parse");
        assert!(assemble_reranked(&request, scores).is_err());

        let unknown = r#"[{"id":"c1","score":0.9},{"id":"nope","score":0.8}]"#;
        let scores = provider
            .parse_rerank_response(unknown)
            .expect("indexes parse");
        assert!(assemble_reranked(&request, scores).is_err());

        assert!(provider.parse_rerank_response("not json").is_err());
    }
}
