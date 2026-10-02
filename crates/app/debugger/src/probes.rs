pub mod agent;
pub mod hook;
pub mod policy;
pub mod trigger;

pub use agent::{
    analyze_agent_trace, snapshot_meta, AgentAnalysis, AgentViolation, POLICY_SNAPSHOT_VERSION,
    VIOLATION_MISSING_APPROVAL, VIOLATION_UNEXPECTED_DENIAL_TEXT, VIOLATION_UNEXPECTED_SUCCESS,
    VIOLATION_VISIBILITY_MISMATCH,
};
pub use hook::{collect_hook_points, explain_hook_point, is_gate_point, HookPointReport, HookSkip};
pub use policy::{
    builtin_policy, BuiltinAgentPolicy, PolicySnapshotMeta, EXPLORER_AGENT_TEMPLATE_ID,
    MAIN_AGENT_TEMPLATE_ID, NOT_ACTIVATED, NOT_CALLABLE, NOT_IN_AVAILABLE_SET, VIA_GENERAL,
    WORKER_AGENT_TEMPLATE_ID,
};
pub use trigger::{
    dry_run, is_before_hook_target, is_compression_target, summarize_seen, TriggerDrop,
    TriggerDryRun, TriggerPermitView,
};
