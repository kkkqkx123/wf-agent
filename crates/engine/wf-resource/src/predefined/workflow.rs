pub mod child;
pub mod registration;

use wf_types::workflow::{Edge, EdgeType, WorkflowMetadata};

/// Shared edge constructor for predefined workflow templates.
pub fn edge(id: &str, source: &str, target: &str) -> Edge {
    Edge {
        id: id.into(),
        source_node_id: source.into(),
        target_node_id: target.into(),
        r#type: EdgeType::Default,
        condition: None,
        label: None,
        description: None,
        weight: None,
        metadata: None,
        error_route: None,
    }
}

/// Shared workflow metadata for predefined templates: system author and
/// category over the given tag set.
pub fn workflow_metadata(tags: &[&str]) -> WorkflowMetadata {
    WorkflowMetadata {
        author: Some("system".into()),
        tags: Some(tags.iter().map(|tag| (*tag).to_string()).collect()),
        category: Some("system".into()),
    }
}

pub use child::prefetch::{
    create_prefetch_workflow, prefetch_inline_definition, PREFETCH_AGENT_NODE_ID,
    PREFETCH_AVAILABLE_TOOLS, PREFETCH_END_NODE_ID, PREFETCH_START_NODE_ID, PREFETCH_WORKFLOW_ID,
};

pub use child::fold_summary::{
    create_fold_summary_workflow, create_fold_summary_workflow_with_policy,
    create_fold_summary_workflow_with_service, FOLD_SUMMARY_END_NODE_ID, FOLD_SUMMARY_FOLD_NODE_ID,
    FOLD_SUMMARY_LLM_NODE_ID, FOLD_SUMMARY_START_NODE_ID, FOLD_SUMMARY_WORKFLOW_ID,
};

pub use registration::register;
