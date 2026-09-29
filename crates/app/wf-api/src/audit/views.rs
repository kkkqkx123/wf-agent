//! Audit view types shared by the audit query and timeline modules.

use serde::Serialize;

/// Where the audit data was resolved from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSource {
    /// From a live entity in the runtime registry.
    Live,
    /// From the persisted execution record.
    Persisted,
    /// From the most recent checkpoint blob (degraded fallback).
    CheckpointSnapshot,
    /// No data source resolved (unknown execution).
    Unknown,
}

/// Summary of one execution's audit trail.
#[derive(Debug, Clone, Serialize)]
pub struct AuditSummary {
    pub execution_id: String,
    /// `agent_loop` or `workflow`.
    pub entity_kind: String,
    pub source: AuditSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    pub iteration_count: usize,
    pub tool_call_count: usize,
    pub llm_call_count: usize,
    pub node_execution_count: usize,
    pub checkpoint_count: usize,
}

/// One tool call execution in audit form.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallAuditView {
    /// Owning iteration (`None` for pre-recorded legacy entries).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u32>,
    /// Owning loop's definition id. Loops started by a workflow node carry
    /// the launching node id here, so consumers can join tool calls to
    /// graph nodes by exact id equality. Standalone loops carry the agent
    /// definition id, which never matches a graph node. `None` when the
    /// owner is unknown (checkpoint snapshots).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub success: bool,
}

/// One LLM call issued by an agent iteration, in audit form.
#[derive(Debug, Clone, Serialize)]
pub struct LlmCallAuditView {
    /// Owning iteration.
    pub iteration: u32,
    pub seq: u32,
    pub profile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_summary: Option<wf_types::agent_execution::LlmRequestSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_summary: Option<wf_types::agent_execution::LlmResponseSummary>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    pub duration_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One agent iteration in audit form.
#[derive(Debug, Clone, Serialize)]
pub struct IterationAuditView {
    pub iteration: u32,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    pub duration_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub tool_call_count: usize,
    pub llm_call_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallAuditView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub llm_calls: Vec<LlmCallAuditView>,
}

/// One workflow node execution attempt, in audit form.
#[derive(Debug, Clone, Serialize)]
pub struct NodeExecutionAuditView {
    pub node_id: String,
    pub node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    pub duration_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
}

/// Combined audit report of an execution.
#[derive(Debug, Clone, Serialize)]
pub struct AuditReport {
    pub summary: AuditSummary,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub iterations: Vec<IterationAuditView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub node_executions: Vec<NodeExecutionAuditView>,
    /// True when iterations or node executions were capped for size.
    #[serde(default)]
    pub truncated: bool,
    /// Item counts before capping, for magnitude estimates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_estimate: Option<AuditTotalEstimate>,
}

/// Pre-truncation magnitudes of a capped audit report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AuditTotalEstimate {
    pub iterations: usize,
    pub node_executions: usize,
}

/// Hard caps keeping large dumps bounded for the frontend.
pub const MAX_AUDIT_ITERATIONS: usize = 500;
pub const MAX_AUDIT_NODE_EXECUTIONS: usize = 2000;
pub const MAX_AUDIT_TIMELINE_ENTRIES: usize = 5000;
