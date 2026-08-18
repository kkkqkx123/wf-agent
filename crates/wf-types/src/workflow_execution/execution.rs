use serde::{Deserialize, Serialize};

use super::NodeExecutionResult;
use super::WorkflowExecutionStatus;
use crate::Id;
use crate::Timestamp;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetryBudgetOption {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_budget_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_execution_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_checkpoints: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_pause_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_budget: Option<RetryBudgetOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_delay_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exponential_backoff: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_output: Option<serde_json::Value>,
    /// Navigation-budget multiplier for the infinite-loop backstop: the
    /// runtime aborts when the navigation count exceeds
    /// `node_count * multiplier`. Legitimate loops re-arm the counter at
    /// LOOP_START, so this only bounds accidental cycles. `None` = default 5.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_navigation_multiplier: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionResultMetadata {
    pub status: WorkflowExecutionStatus,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub execution_time: i64,
    pub node_count: u32,
    pub error_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionResult {
    pub execution_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    pub execution_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_results: Option<Vec<NodeExecutionResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<WorkflowExecutionResultMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
    pub workflow_retry_count: u32,
    pub total_retry_count: u32,
}
