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

/// Compression service policy for the context-compression chain.
///
/// Timeout nesting (outer must cover the inner worst case):
/// `settle_timeout_ms` (emitter wait) >= `(1 + max_retries) * timeout_ms`
/// plus the backoffs, and `timeout_ms` bounds one summary run attempt
/// (node wrap of the summary LLM call included). Failed runs are retried
/// within the chain layer (`max_retries` additional attempts, exponential
/// backoff, reusing the same array-version anchor). What happens at the
/// terminal state is not decided here: it is declared by the summary
/// workflow resource (`compression_fallback` on the triggered-subworkflow
/// config), which chooses between failing the emitter onto the pause path
/// and landing a visible degraded window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CompressionLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail_keep: Option<usize>,
    /// Additional summary runs after the first attempt. 0 disables chain
    /// retries (transport-level profile retries still apply).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    /// Outer wall-clock budget for one summary run attempt (covers the
    /// sub-workflow and its write-back).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Budget for the emitting execution's wait (settle) for an in-flight
    /// compression to land before it pauses for external handling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settle_timeout_ms: Option<u64>,
}
