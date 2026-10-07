//! Error analysis over live entity error chains and persisted execution
//! records. Lives on top of `wf-common::error_chain::ErrorRecord` (the shape
//! the workflow/agent engines record on failure) and the `FailurePolicyManager`
//! recovery semantics exposed through `RecoveryAction`.

mod advanced;
mod context;
mod recovery;
mod records;
mod subscription;
mod views;

pub use context::{analyze_root_cause, error_context, error_context_chain, ErrorContextView, WorkflowRootCauseAnalysis};
pub use queries::{
    get_advanced_error_analysis, get_error_chain, get_recovery_proposal,
    recovery_recommendations, similar_errors, stream_error_chain, workflow_error_stats,
};
pub use subscription::{subscribe_to_errors, ErrorSubscription};
pub use views::{
    AdvancedWorkflowErrorAnalysis, ErrorRecommendation, ProblematicNode, RecoveryProposal,
    SimilarErrorGroup, WorkflowErrorHotspot, WorkflowErrorStats, WorkflowNodeRef,
    MAX_ERROR_CONTEXT_CHAIN, MAX_RECOVERY_RECOMMENDATIONS, MAX_SIMILAR_EXECUTIONS_PER_GROUP,
    MAX_SIMILAR_GROUPS,
};

mod queries;

#[cfg(test)]
mod tests;
