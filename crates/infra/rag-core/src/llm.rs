//! Shared model-call port for synthesis and query transforms.
//!
//! The crate never opens its own model call path. Hosts implement this
//! port with the model gateway; synthesis and transforms only assemble
//! prompts and parse answers.

use async_trait::async_trait;

use crate::error::{RagError, Result};

/// Minimal text completion used by synthesis and query transforms.
#[async_trait]
pub trait CompletionLlm: Send + Sync {
    /// Completes one prompt into answer text. Empty answers are rejected
    /// by the callers that require model output.
    async fn complete(&self, prompt: &str) -> Result<String>;
}

/// Maps a port failure into a crate error carrying only the message.
pub fn map_failure(operation: &str, message: impl Into<String>) -> RagError {
    RagError::Llm(format!("{} failed: {}", operation, message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingLlm;

    #[async_trait]
    impl CompletionLlm for FailingLlm {
        async fn complete(&self, _prompt: &str) -> Result<String> {
            Err(RagError::Llm("gateway down".to_string()))
        }
    }

    struct EchoLlm;

    #[async_trait]
    impl CompletionLlm for EchoLlm {
        async fn complete(&self, prompt: &str) -> Result<String> {
            Ok(format!("echo {prompt}"))
        }
    }

    #[tokio::test]
    async fn echo_returns_prompt_content() {
        let answer = EchoLlm.complete("hello").await.expect("complete");
        assert!(answer.contains("hello"));
    }

    #[tokio::test]
    async fn failure_maps_to_model_error() {
        let err = FailingLlm.complete("hello").await.expect_err("fails");
        assert!(matches!(err, RagError::Llm(_)));
        assert_eq!(
            map_failure("synthesis", "down").to_string(),
            "model error: synthesis failed: down"
        );
    }
}
