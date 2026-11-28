//! Host-side completion port for RAG synthesis and query transforms.
//!
//! The RAG crate owns no model call path: it defines the completion port and
//! assembles prompts. This adapter binds that port to the shared LLM gateway
//! of the host, so synthesis and answer flows become reachable in production
//! without introducing an infra-layer dependency back onto the API layer.

use std::sync::Arc;

use async_trait::async_trait;
use wf_llm::LlmGateway;

use crate::rag::RagHandles;
use wf_rag::{CompletionLlm, CompletionRequest, Result};

/// Completion port backed by one named gateway profile.
///
/// Each instance carries its own profile so hosts can route synthesis calls
/// to a cheaper or faster model than agent turns. System and user roles are
/// forwarded as separate messages; the output cap is applied to the text.
pub struct GatewayCompletionLlm {
    gateway: Arc<LlmGateway>,
    profile_id: String,
}

impl GatewayCompletionLlm {
    /// Builds the adapter over a shared gateway and a resolved profile id.
    pub fn new(gateway: Arc<LlmGateway>, profile_id: impl Into<String>) -> Self {
        Self {
            gateway,
            profile_id: profile_id.into(),
        }
    }
}

#[async_trait]
impl CompletionLlm for GatewayCompletionLlm {
    async fn complete_request(&self, request: &CompletionRequest) -> Result<String> {
        let mut messages = Vec::with_capacity(2);
        if let Some(system) = request
            .system
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            messages.push(wf_llm::message_builder::system_text(system.to_string()));
        }
        messages.push(wf_llm::message_builder::user_text(request.user.clone()));
        let llm_request = wf_types::llm::new_llm_request(&self.profile_id, messages);
        let result = self
            .gateway
            .generate(&llm_request, None)
            .await
            .map_err(|error| wf_rag::RagError::Llm(error.to_string()))?;
        let text = result.content.unwrap_or_default();
        Ok(request.truncate_output(text))
    }
}

/// Runs one question-answer flow over the shared retrieval handles.
///
/// The host must have attached a completion port; retrieval-only handles
/// return an input error so a missing gateway cannot silently degrade into
/// an empty answer. Rewrite, retrieval, optional rerank and synthesis all
/// run through the facade handles, keeping the free functions as defaults.
pub async fn answer_question(
    handles: &RagHandles,
    query: &str,
    collection: &str,
    namespace: &str,
    query_config: &wf_rag::RagQueryConfig,
    synthesis_config: &wf_rag::SynthesisConfig,
    rewriter: &dyn wf_rag::QueryRewriter,
) -> Result<wf_rag::SynthesizedAnswer> {
    let completion = handles.completion.as_ref().ok_or_else(|| {
        wf_rag::RagError::InvalidInput(
            "RAG answer flow requires a host completion port".to_string(),
        )
    })?;
    let flow = wf_rag::Flow {
        extensions: &handles.extensions,
        embedder: handles.embedder.as_ref(),
        llm: completion.as_ref(),
        collection,
        namespace,
        query_config,
        synth_config: synthesis_config,
    };
    let qa = wf_rag::StandardQa {
        store: handles.store.as_ref(),
        flow,
    };
    qa.answer_plain(rewriter, query).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_rag::CompletionRequest;

    struct EchoLlm;

    #[async_trait]
    impl CompletionLlm for EchoLlm {
        async fn complete_request(&self, request: &CompletionRequest) -> Result<String> {
            Ok(request.concatenated())
        }
    }

    #[tokio::test]
    async fn adapter_truncates_output() {
        let request = CompletionRequest::user("hello").with_max_output_chars(3);
        assert_eq!(request.truncate_output("abcdef".to_string()), "abc");
    }

    #[tokio::test]
    async fn boundary_completion_port_is_reachable() {
        let llm: Arc<dyn CompletionLlm> = Arc::new(EchoLlm);
        let answer = llm.complete("ping").await.expect("complete");
        assert!(answer.contains("ping"));
    }
}
