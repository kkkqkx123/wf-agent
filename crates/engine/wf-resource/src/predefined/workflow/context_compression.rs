use serde_json::json;

use wf_integration::{CodeContextConfig, FoldPolicy};
use wf_types::node::BaseStaticNode;
use wf_types::node::StaticNodeType;
use wf_types::workflow::{
    Edge, EdgeType, TriggeredSubworkflowConfig, WorkflowDefinition, WorkflowMetadata,
    WorkflowTemplate,
};

use super::llm_summary::{DEFAULT_LLM_SUMMARY_PROFILE, DEFAULT_LLM_SUMMARY_PROMPT};

pub const CONTEXT_COMPRESSION_WORKFLOW_ID: &str = "@standard/context-compression";

/// Fold node id carrying the baked fold snapshot.
const FOLD_NODE_ID: &str = "compression-fold";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Compression chain template: fold oversized file contents first,
/// then summarize. The fold stage is deterministic (never fails the
/// chain); the summary stage carries the existing prompt and budgets.
/// Terminal failure handling stays undeclared, so a failed chain stops the
/// emitting execution for external handling (the `fail` default).
pub fn create_context_compression_workflow(compression_prompt: Option<String>) -> WorkflowTemplate {
    create_context_compression_workflow_with_policy(compression_prompt, FoldPolicy::default())
}

/// Compression chain template with an explicit fold policy. The policy is
/// baked into the fold node config at assembly time, so startup code
/// can inject the live policy once instead of freezing compile-time
/// defaults into every chain. No service address is baked; the fold
/// stage skips until the snapshot is applied.
pub fn create_context_compression_workflow_with_policy(
    compression_prompt: Option<String>,
    fold: FoldPolicy,
) -> WorkflowTemplate {
    build_template(compression_prompt, &fold, None, 60_000)
}

/// Compression chain template with the effective service snapshot baked
/// into the fold node config. Execution reads only the node config.
pub fn create_context_compression_workflow_with_service(
    compression_prompt: Option<String>,
    service: &CodeContextConfig,
) -> WorkflowTemplate {
    let (base_url, timeout_ms) = match service.external_base_url() {
        Some(url) if service.enabled => (Some(url), service.transport.timeout_ms),
        _ => (None, service.transport.timeout_ms),
    };
    build_template(compression_prompt, &service.fold, base_url, timeout_ms)
}

/// Re-bake the effective service snapshot into an assembled template.
/// Finds the fold node by id and rewrites its policy plus service
/// address; other nodes stay untouched.
pub fn bake_code_context_service(
    template: &mut WorkflowTemplate,
    service: &CodeContextConfig,
) {
    let (base_url, timeout_ms) = match service.external_base_url() {
        Some(url) if service.enabled => (Some(url), service.transport.timeout_ms),
        _ => (None, service.transport.timeout_ms),
    };
    for node in &mut template.definition.nodes {
        if node.id != FOLD_NODE_ID {
            continue;
        }
        let Some(config) = node.config.as_mut().and_then(|v| v.as_object_mut()) else {
            continue;
        };
        config.insert(
            "min_tokens".to_string(),
            json!(service.fold.min_tokens),
        );
        config.insert(
            "max_tokens".to_string(),
            json!(service.fold.max_tokens),
        );
        config.insert(
            "max_items".to_string(),
            json!(service.fold.max_items),
        );
        config.insert(
            "max_batches".to_string(),
            json!(service.fold.max_batches),
        );
        match base_url.clone() {
            Some(url) => {
                config.insert("service_base_url".to_string(), json!(url));
            }
            None => {
                config.remove("service_base_url");
            }
        }
        config.insert("service_timeout_ms".to_string(), json!(timeout_ms));
    }
}

fn build_template(
    compression_prompt: Option<String>,
    fold: &FoldPolicy,
    service_base_url: Option<String>,
    service_timeout_ms: u64,
) -> WorkflowTemplate {
    let t = now_ms();

    let mut fold_config = json!({
        "fold": true,
        "source_context": "current",
        "target_context": "current",
        "min_tokens": fold.min_tokens,
        "max_tokens": fold.max_tokens,
        "max_items": fold.max_items,
        "max_batches": fold.max_batches,
        "service_timeout_ms": service_timeout_ms,
    });
    if let Some(url) = service_base_url {
        fold_config["service_base_url"] = json!(url);
    }
    let nodes = vec![
        BaseStaticNode {
            id: "compression-start".into(),
            node_type: StaticNodeType::StartFromMessage,
            name: Some("Start Context Compression".into()),
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
        },
        BaseStaticNode {
            id: FOLD_NODE_ID.into(),
            node_type: StaticNodeType::ContextProcessor,
            name: Some("Fold File Contents".into()),
            description: Some(
                "Fold oversized tool-result file contents into symbol skeletons before summarization".into(),
            ),
            config: Some(fold_config),
            execution_config: None,
        },
        BaseStaticNode {
            id: "compression-llm".into(),
            node_type: StaticNodeType::Llm,
            name: Some("Summarize Context".into()),
            description: Some(
                "Use LLM to generate a compressed summary of the conversation history".into(),
            ),
            config: Some(json!({
                "profile_id": DEFAULT_LLM_SUMMARY_PROFILE,
                "context_id": "current",
                "output_context": "compressed",
                "system_prompt": compression_prompt.unwrap_or_else(|| DEFAULT_LLM_SUMMARY_PROMPT.into()),
                // The summary input is an already over-budget snapshot: this
                // node never participates in compression decisions (the
                // depth guard is the backstop; this switch is the intent).
                "enable_token_tracking": false,
                // Must exceed the LLM call budget (llm.timeout_ms) and stay
                // below the whole-chain run timeout, so a slow summary fails
                // inside the node and the chain can still retry.
                "timeout_seconds": 150
            })),
            execution_config: None,
        },
        BaseStaticNode {
            id: "compression-end".into(),
            node_type: StaticNodeType::ContinueFromMessage,
            name: Some("Complete Context Compression".into()),
            description: Some(
                "Pass the compressed conversation summary back to the emitting execution"
                    .into(),
            ),
            config: Some(json!({
                "message_outputs": [{
                    "internal_name": "compressed",
                    "target_context_id": "current",
                    "description": "Compressed conversation summary"
                }]
            })),
            execution_config: None,
        },
    ];

    let edges = vec![
        Edge {
            id: "e-compression-start-to-fold".into(),
            source_node_id: "compression-start".into(),
            target_node_id: "compression-fold".into(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
            error_route: None,
        },
        Edge {
            id: "e-compression-fold-to-llm".into(),
            source_node_id: "compression-fold".into(),
            target_node_id: "compression-llm".into(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
            error_route: None,
        },
        Edge {
            id: "e-compression-llm-to-end".into(),
            source_node_id: "compression-llm".into(),
            target_node_id: "compression-end".into(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
            error_route: None,
        },
    ];

    WorkflowTemplate {
        id: CONTEXT_COMPRESSION_WORKFLOW_ID.into(),
        name: "Context Compression Workflow".into(),
        description: "Builtin context compression chain: fold file contents -> summarize -> replace original context with summary".into(),
        definition: WorkflowDefinition {
            id: CONTEXT_COMPRESSION_WORKFLOW_ID.into(),
            name: "Context Compression Workflow".into(),
            description: Some("Fold-then-summarize compression chain".into()),
            r#type: None,
            version: Some("1.0.0".into()),
            nodes,
            edges,
            config: None,
            variables: None,
            triggered_subworkflow_config: Some(TriggeredSubworkflowConfig {
                enable_checkpoints: Some(false),
                // Chain budget: fold transport plus the summary node timeout
                // (150s) plus write-back must fit inside this, and the
                // compression service wraps each attempt with the same
                // budget before retrying.
                timeout: Some(240_000),
                // No declared fallback: a terminal failure stops the emitting
                // execution for external handling (the `fail` default).
                compression_fallback: None,
            }),
            metadata: Some(WorkflowMetadata {
                author: Some("system".into()),
                tags: Some(vec![
                    "context".into(),
                    "compression".into(),
                    "fold".into(),
                    "summary".into(),
                    "token".into(),
                    "memory".into(),
                    "predefined".into(),
                ]),
                category: Some("system".into()),
            }),
            available_tools: None,
            created_at: t,
            updated_at: t,
            hooks: None,
        },
        template_category: Some("system".into()),
        template_tags: Some(vec!["context".into(), "compression".into()]),
        is_public: Some(true),
        enabled: Some(true),
    }
}
