use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LimitsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentLimits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<WorkflowLimits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_defaults: Option<ExecutionDefaults>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<CompressionLimits>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations_cap: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_max_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_sub_agent_depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_pause_duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WorkflowLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_max_iterations_cap: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_default_max_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_navigation_multiplier: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ExecutionDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_execution_time_ms: Option<u64>,
}

/// Compression service policy: how many recent pre-existing messages stay
/// visible beside the summary. Compression runs without timeout and without
/// retries; a failed run stops the emitting execution for manual handling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CompressionLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail_keep: Option<usize>,
}
