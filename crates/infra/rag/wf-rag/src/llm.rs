//! Shared model-call port for synthesis and query transforms.
//!
//! The crate never opens its own model call path. Hosts implement this
//! port with the model gateway; synthesis and transforms only assemble
//! prompts and parse answers.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{RagError, Result};

/// Structured completion request with separated system and user prompts.
///
/// Hosts may ignore the separation and concatenate, but separating roles
/// lets gateways apply role-aware templates and token budgets.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Optional system instruction; unset keeps single-prompt behavior.
    pub system: Option<String>,
    /// User prompt carrying the task content.
    pub user: String,
    /// Optional output truncation in characters; unset keeps full output.
    pub max_output_chars: Option<usize>,
}

impl CompletionRequest {
    /// Single-prompt request preserving the legacy shape.
    pub fn user(user: impl Into<String>) -> Self {
        Self {
            system: None,
            user: user.into(),
            max_output_chars: None,
        }
    }

    /// Role-separated request with an explicit system instruction.
    pub fn system_user(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            system: Some(system.into()),
            user: user.into(),
            max_output_chars: None,
        }
    }

    /// Caps the returned text to at most `max` characters.
    pub fn with_max_output_chars(mut self, max: usize) -> Self {
        self.max_output_chars = Some(max);
        self
    }

    /// Legacy concatenated prompt for gateways without role support.
    pub fn concatenated(&self) -> String {
        match self
            .system
            .as_deref()
            .filter(|item| !item.trim().is_empty())
        {
            Some(system) => format!("{system}\n\n{}", self.user),
            None => self.user.clone(),
        }
    }

    /// Truncates model output on character boundaries.
    pub fn truncate_output(&self, text: String) -> String {
        let Some(max) = self.max_output_chars.filter(|max| *max > 0) else {
            return text;
        };
        if text.chars().count() <= max {
            return text;
        }
        text.chars().take(max).collect()
    }
}

/// Replaceable prompt templates shared by synthesis and query transforms.
///
/// Templates use `{name}` placeholders and stay plain text so hosts can
/// load them from configuration without depending on a template engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplates {
    /// System instruction prepended to every model request.
    pub system: String,
    /// Single-pass synthesis over the assembled context.
    pub synthesis: String,
    /// One passage fed into iterative refinement.
    pub passage: String,
    /// Refinement over the running draft and one passage.
    pub refine: String,
    /// Draft compression used before another refinement pass.
    pub compress: String,
    /// Decomposition into independent sub-questions.
    pub decompose: String,
    /// Hypothetical answer passage used as an extra retrieval variant.
    pub hypothetical: String,
    /// Question rewrite using first-pass excerpts.
    pub feedback: String,
    /// Standalone rewrite of a conversational follow-up.
    pub rewrite: String,
    /// Diverse search variants covering one question.
    pub variants: String,
}

impl Default for PromptTemplates {
    fn default() -> Self {
        Self {
            system: "Use only the supplied passages when answering; cite numbered passages.".to_string(),
            synthesis: "Answer the question using the numbered passages.\n\nQuestion: {query}\n\nPassages:\n{context}".to_string(),
            passage: "Answer the question using this passage numbered {index} from {source}.\n\nQuestion: {query}\n\nPassage {index}: {excerpt}".to_string(),
            refine: "Refine the draft answer using this passage numbered {index} from {source}.\n\nQuestion: {query}\n\nDraft:\n{draft}\n\nPassage {index}: {excerpt}".to_string(),
            compress: "Compress the draft answer while keeping its citations.\n\nDraft:\n{draft}".to_string(),
            decompose: "Break the question below into independent sub-questions that together cover it. List each sub-question on its own line starting with '- '. Keep them short.\n\nQuestion: {query}".to_string(),
            hypothetical: "Write a short factual passage that answers the question below. Output only the passage.\n\nQuestion: {query}".to_string(),
            feedback: "Rewrite the question below into a sharper retrieval query using the passages as context. Output only the rewritten question.\n\nQuestion: {query}\n\nPassages:\n{context}".to_string(),
            rewrite: "Rewrite the follow-up question into a standalone question using the conversation history. Output only the rewritten question.\n\nHistory:\n{history}\n\nFollow-up: {query}".to_string(),
            variants: "Rewrite the question below into {count} diverse search variants covering the same need. List each variant on its own line starting with '- '.\n\nQuestion: {query}".to_string(),
        }
    }
}

impl PromptTemplates {
    /// Renders one template by replacing every `{name}` placeholder.
    pub fn render(&self, template: &str, values: &[(&str, &str)]) -> String {
        let mut text = template.to_string();
        for (name, value) in values {
            text = text.replace(&format!("{{{name}}}"), value);
        }
        text
    }

    /// Builds a role-separated completion request from one template.
    pub fn request(&self, template: &str, values: &[(&str, &str)]) -> CompletionRequest {
        CompletionRequest::system_user(&self.system, self.render(template, values))
    }
}

/// Minimal text completion used by synthesis and query transforms.
#[async_trait]
pub trait CompletionLlm: Send + Sync {
    /// Completes one prompt into answer text. Empty answers are rejected
    /// by the callers that require model output.
    async fn complete(&self, prompt: &str) -> Result<String>;

    /// Completes a structured request. The default concatenates system and
    /// user prompts and truncates the result, so existing gateways keep
    /// working without role-aware handling.
    async fn complete_request(&self, request: &CompletionRequest) -> Result<String> {
        let text = self.complete(&request.concatenated()).await?;
        Ok(request.truncate_output(text))
    }
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
