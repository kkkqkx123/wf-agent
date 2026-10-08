use serde_json::json;

use wf_integration::{CodeContextConfig, FoldPolicy};
use wf_types::node::BaseStaticNode;
use wf_types::node::StaticNodeType;
use wf_types::workflow::{WorkflowDefinition, WorkflowDefinitionType, WorkflowTemplate};

use super::super::edge;
use super::summary_stage::{
    chain_end_node, chain_llm_node, chain_metadata, chain_start_node, chain_subworkflow_config,
};

pub const FOLD_SUMMARY_WORKFLOW_ID: &str = "@standard/fold-summary";

pub const FOLD_SUMMARY_START_NODE_ID: &str = "fold-summary-start";
pub const FOLD_SUMMARY_FOLD_NODE_ID: &str = "fold-summary-fold";
pub const FOLD_SUMMARY_LLM_NODE_ID: &str = "fold-summary-llm";
pub const FOLD_SUMMARY_END_NODE_ID: &str = "fold-summary-end";

pub fn create_fold_summary_workflow(compression_prompt: Option<String>) -> WorkflowTemplate {
    create_fold_summary_workflow_with_policy(compression_prompt, FoldPolicy::default())
}

pub fn create_fold_summary_workflow_with_policy(
    compression_prompt: Option<String>,
    fold: FoldPolicy,
) -> WorkflowTemplate {
    build_template(compression_prompt, &fold, None, 60_000)
}

pub fn create_fold_summary_workflow_with_service(
    compression_prompt: Option<String>,
    service: &CodeContextConfig,
) -> WorkflowTemplate {
    let (base_url, timeout_ms) = match service.external_base_url() {
        Some(url) if service.enabled => (Some(url), service.transport.timeout_ms),
        _ => (None, service.transport.timeout_ms),
    };
    build_template(compression_prompt, &service.fold, base_url, timeout_ms)
}

fn fold_node(
    fold: &FoldPolicy,
    service_base_url: Option<String>,
    service_timeout_ms: u64,
) -> BaseStaticNode {
    let mut fold_config = json!({
        "fold": true,
        "source_context": "current",
        "target_context": "current",
        "min_tokens": fold.min_tokens,
        "max_tokens": fold.max_tokens,
        "max_items": fold.max_items,
        "max_retries": fold.max_retries,
        "service_timeout_ms": service_timeout_ms,
    });
    if let Some(url) = service_base_url {
        fold_config["service_base_url"] = json!(url);
    }
    BaseStaticNode {
        id: FOLD_SUMMARY_FOLD_NODE_ID.into(),
        node_type: StaticNodeType::ContextProcessor,
        name: Some("Fold Tool Results".into()),
        description: Some(
            "Deterministically fold oversized tool results before summarization".into(),
        ),
        config: Some(fold_config),
        execution_config: None,
    }
}

fn build_template(
    compression_prompt: Option<String>,
    fold: &FoldPolicy,
    service_base_url: Option<String>,
    service_timeout_ms: u64,
) -> WorkflowTemplate {
    let t = wf_common::now();

    let nodes = vec![
        chain_start_node(FOLD_SUMMARY_START_NODE_ID),
        fold_node(fold, service_base_url, service_timeout_ms),
        chain_llm_node(FOLD_SUMMARY_LLM_NODE_ID, compression_prompt, None),
        chain_end_node(FOLD_SUMMARY_END_NODE_ID),
    ];

    let edges = vec![
        edge(
            "e-fold-summary-start-to-fold",
            FOLD_SUMMARY_START_NODE_ID,
            FOLD_SUMMARY_FOLD_NODE_ID,
        ),
        edge(
            "e-fold-summary-fold-to-llm",
            FOLD_SUMMARY_FOLD_NODE_ID,
            FOLD_SUMMARY_LLM_NODE_ID,
        ),
        edge(
            "e-fold-summary-llm-to-end",
            FOLD_SUMMARY_LLM_NODE_ID,
            FOLD_SUMMARY_END_NODE_ID,
        ),
    ];

    WorkflowTemplate {
        id: FOLD_SUMMARY_WORKFLOW_ID.into(),
        name: "Fold Summary Workflow".into(),
        description: "Builtin compression chain: fold tool results -> summarize -> replace original context with summary".into(),
        definition: WorkflowDefinition {
            id: FOLD_SUMMARY_WORKFLOW_ID.into(),
            name: "Fold Summary Workflow".into(),
            description: Some("Fold-then-summarize compression chain".into()),
            r#type: Some(WorkflowDefinitionType::TriggeredSubworkflow),
            version: Some("1.0.0".into()),
            nodes,
            edges,
            config: None,
            variables: None,
            triggered_subworkflow_config: Some(chain_subworkflow_config()),
            metadata: Some(chain_metadata(&[])),
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
