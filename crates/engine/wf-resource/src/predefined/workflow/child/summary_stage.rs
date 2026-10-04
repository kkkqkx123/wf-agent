use serde_json::{json, Value};

use wf_types::node::BaseStaticNode;
use wf_types::node::StaticNodeType;
use wf_types::workflow::{TriggeredSubworkflowConfig, WorkflowMetadata};

use crate::embedded_assets;

pub use crate::predefined::agent_prompts::LLM_SUMMARY_PROMPT_KEY as DEFAULT_LLM_SUMMARY_PROMPT_KEY;

pub const DEFAULT_LLM_SUMMARY_PROFILE: &str = "DEFAULT";

pub const SUMMARY_NODE_TIMEOUT_SECS: u64 = 150;
pub const COMPRESSION_CHAIN_TIMEOUT_MS: u64 = 240_000;

pub fn summary_llm_config(compression_prompt: Option<String>, profile_id: Option<String>) -> Value {
    let profile = profile_id.unwrap_or_else(|| DEFAULT_LLM_SUMMARY_PROFILE.to_string());
    json!({
        "profile_id": profile,
        "context_id": "current",
        "output_context": "compressed",
        "system_prompt": compression_prompt.unwrap_or_else(|| {
            embedded_assets::agent_prompt(DEFAULT_LLM_SUMMARY_PROMPT_KEY).to_string()
        }),
        "enable_token_tracking": false,
        "timeout_seconds": SUMMARY_NODE_TIMEOUT_SECS
    })
}

pub fn chain_start_node(node_id: &str) -> BaseStaticNode {
    BaseStaticNode {
        id: node_id.into(),
        node_type: StaticNodeType::StartFromMessage,
        name: Some("Start Fold Summary".into()),
        description: Some(
            "Receive the full conversation history from the emitting execution".into(),
        ),
        config: Some(json!({
            "message_inputs": [{
                "source_context_id": "conversationHistory",
                "internal_name": "current",
                "required": true,
                "description": "Full conversation history to be compressed"
            }]
        })),
        execution_config: None,
    }
}

pub fn chain_llm_node(
    node_id: &str,
    compression_prompt: Option<String>,
    profile_id: Option<String>,
) -> BaseStaticNode {
    BaseStaticNode {
        id: node_id.into(),
        node_type: StaticNodeType::Llm,
        name: Some("Summarize Context".into()),
        description: Some(
            "Use LLM to generate a compressed summary of the conversation history".into(),
        ),
        config: Some(summary_llm_config(compression_prompt, profile_id)),
        execution_config: None,
    }
}

pub fn chain_end_node(node_id: &str) -> BaseStaticNode {
    BaseStaticNode {
        id: node_id.into(),
        node_type: StaticNodeType::ContinueFromMessage,
        name: Some("Complete Fold Summary".into()),
        description: Some(
            "Pass the compressed conversation summary back to the emitting execution".into(),
        ),
        config: Some(json!({
            "message_outputs": [{
                "internal_name": "compressed",
                "target_context_id": "current",
                "description": "Compressed conversation summary"
            }]
        })),
        execution_config: None,
    }
}

pub fn chain_subworkflow_config() -> TriggeredSubworkflowConfig {
    TriggeredSubworkflowConfig {
        enable_checkpoints: Some(false),
        timeout: Some(COMPRESSION_CHAIN_TIMEOUT_MS),
        compression_fallback: None,
    }
}

pub fn chain_metadata(extra_tags: &[&str]) -> WorkflowMetadata {
    let mut tags = vec![
        "context",
        "compression",
        "fold",
        "summary",
        "token",
        "memory",
        "predefined",
    ];
    for tag in extra_tags {
        if !tags.contains(tag) {
            tags.push(tag);
        }
    }
    super::super::workflow_metadata(&tags)
}
