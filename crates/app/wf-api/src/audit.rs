//! Execution audit queries.
//!
//! One entry point per audit facet, resolving the best available data
//! source for an execution id in priority order:
//!
//! 1. **live** — the entity still registered in the runtime registry
//!    (`agent_loop` / `workflow_execution`),
//! 2. **persisted** — the execution record in the storage adapters,
//! 3. **checkpoint snapshot** — the most recent checkpoint blob, kept as
//!    the audit fallback when the execution record was cleaned up.
//!
//! The facet names mirror the audit vocabulary: iterations, tool calls,
//! LLM calls and node executions. Every view records its `source` so a
//! consumer can tell whether the data is live, persisted or degraded to the
//! checkpoint fallback.

use serde::Serialize;

use wf_checkpoint::serializer::CheckpointSerializer;
use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::checkpoint::CheckpointStorageAdapter;
use wf_storage::domain::store::Store;
use wf_types::agent_execution::LlmCallRecord;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// Entity type tag of a checkpoint row (agent checkpoints are stored under
/// `"checkpoint"`, workflow checkpoints under `"execution"`).
const AGENT_CHECKPOINT_ENTITY_TYPE: &str = "checkpoint";
const WORKFLOW_CHECKPOINT_ENTITY_TYPE: &str = "execution";

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
    /// Set when the only source was a checkpoint snapshot truncated by the
    /// size budget (footprint philosophy).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation_stats: Option<wf_types::checkpoint::workflow::SnapshotTruncationStats>,
}

/// One tool call execution in audit form.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallAuditView {
    /// Owning iteration (`None` for pre-recorded legacy entries).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u32>,
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
}

// ─── resolution helpers ────────────────────────────────────────────────────

struct AgentAuditData {
    source: AuditSource,
    status: Option<String>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    iterations: Vec<IterationAuditView>,
    truncation_stats: Option<wf_types::checkpoint::workflow::SnapshotTruncationStats>,
}

struct WorkflowAuditData {
    source: AuditSource,
    status: Option<String>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    node_executions: Vec<NodeExecutionAuditView>,
    truncation_stats: Option<wf_types::checkpoint::workflow::SnapshotTruncationStats>,
}

/// Resolve the audit data of an agent loop execution.
async fn resolve_agent(ctx: &ApiContext, execution_id: &str) -> ApiResult<Option<AgentAuditData>> {
    if let Some(entity) = ctx.agent_loop(execution_id) {
        let snapshot = entity
            .state
            .read()
            .await
            .create_snapshot()
            .await
            .map_err(|e| crate::ApiError::execution(format!("state snapshot failed: {e}")))?;
        let status: wf_types::ExecutionStatus = snapshot.status.clone().into();
        let iterations = snapshot
            .iteration_history
            .iter()
            .map(live_iteration_view)
            .collect();
        return Ok(Some(AgentAuditData {
            source: AuditSource::Live,
            status: Some(status.as_str().to_string()),
            started_at: Some(snapshot.start_time),
            ended_at: snapshot.end_time,
            iterations,
            truncation_stats: None,
        }));
    }

    if let Some(record) = ctx.storage.agent_execution.load(execution_id).await? {
        let iterations = record
            .iteration_history
            .unwrap_or_default()
            .iter()
            .map(persisted_iteration_view)
            .collect();
        return Ok(Some(AgentAuditData {
            source: AuditSource::Persisted,
            status: Some(record.status.as_str().to_string()),
            started_at: Some(record.started_at),
            ended_at: record.completed_at,
            iterations,
            truncation_stats: None,
        }));
    }

    if let Some(snapshot) = agent_checkpoint_snapshot(ctx, execution_id).await? {
        let iterations = snapshot
            .iteration_history
            .unwrap_or_default()
            .iter()
            .filter_map(|value| {
                serde_json::from_value::<wf_agent::state::IterationRecord>(value.clone()).ok()
            })
            .map(|record| live_iteration_view(&record))
            .collect();
        return Ok(Some(AgentAuditData {
            source: AuditSource::CheckpointSnapshot,
            status: Some(snapshot.status.clone()),
            started_at: snapshot.started_at,
            ended_at: snapshot.completed_at,
            iterations,
            truncation_stats: None,
        }));
    }

    Ok(None)
}

/// Resolve the audit data of a workflow execution.
async fn resolve_workflow(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<WorkflowAuditData>> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let snapshot = entity
            .state
            .read()
            .await
            .create_snapshot()
            .await
            .map_err(|e| crate::ApiError::execution(format!("state snapshot failed: {e}")))?;
        let node_executions = snapshot
            .node_execution_history
            .iter()
            .map(live_node_view)
            .collect();
        return Ok(Some(WorkflowAuditData {
            source: AuditSource::Live,
            status: Some(snapshot.status.as_str().to_string()),
            started_at: Some(snapshot.start_time),
            ended_at: snapshot.end_time,
            node_executions,
            truncation_stats: None,
        }));
    }

    if let Some(record) = ctx.storage.workflow_execution.load(execution_id).await? {
        let node_executions = record
            .node_results
            .unwrap_or_default()
            .iter()
            .map(persisted_node_view)
            .collect();
        return Ok(Some(WorkflowAuditData {
            source: AuditSource::Persisted,
            status: Some(record.status.as_str().to_string()),
            started_at: Some(record.started_at),
            ended_at: record.completed_at,
            node_executions,
            truncation_stats: None,
        }));
    }

    if let Some(snapshot) = workflow_checkpoint_snapshot(ctx, execution_id).await? {
        let node_executions = snapshot
            .node_execution_records
            .unwrap_or_default()
            .iter()
            .map(checkpoint_node_view)
            .collect();
        return Ok(Some(WorkflowAuditData {
            source: AuditSource::CheckpointSnapshot,
            status: Some(snapshot.status.clone()),
            started_at: None,
            ended_at: None,
            node_executions,
            truncation_stats: snapshot.truncation_stats.clone(),
        }));
    }

    Ok(None)
}

/// Latest checkpoint blob of an agent loop, or `None` when none exists.
async fn agent_checkpoint_snapshot(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<wf_types::checkpoint::agent::AgentStateSnapshot>> {
    let checkpoint = ctx
        .storage
        .checkpoint
        .get_latest_by_entity(execution_id, AGENT_CHECKPOINT_ENTITY_TYPE)
        .await?;
    checkpoint_blob(ctx, checkpoint).await.map(|bytes| {
        bytes.and_then(|bytes| {
            CheckpointSerializer::auto_deserialize::<
                    wf_types::checkpoint::agent::AgentStateSnapshot,
                >(&bytes)
                .ok()
        })
    })
}

/// Latest checkpoint blob of a workflow execution, or `None`.
async fn workflow_checkpoint_snapshot(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot>> {
    let checkpoint = ctx
        .storage
        .checkpoint
        .get_latest_by_entity(execution_id, WORKFLOW_CHECKPOINT_ENTITY_TYPE)
        .await?;
    checkpoint_blob(ctx, checkpoint).await.map(|bytes| {
        bytes.and_then(|bytes| {
            CheckpointSerializer::auto_deserialize::<
                wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot,
            >(&bytes)
            .ok()
        })
    })
}

async fn checkpoint_blob(
    ctx: &ApiContext,
    checkpoint: Option<wf_types::Checkpoint>,
) -> ApiResult<Option<Vec<u8>>> {
    match checkpoint {
        Some(checkpoint) => {
            let Some((bytes, _)) = ctx.checkpoint_store.load(&checkpoint.id).await? else {
                return Ok(None);
            };
            Ok(Some(bytes))
        }
        None => Ok(None),
    }
}

// ─── view builders ─────────────────────────────────────────────────────────

fn live_iteration_view(record: &wf_agent::state::IterationRecord) -> IterationAuditView {
    IterationAuditView {
        iteration: record.iteration,
        started_at: record.start_time,
        completed_at: record.end_time,
        duration_ms: record
            .end_time
            .map(|end| end - record.start_time)
            .unwrap_or(0),
        response_content: record.response_content.clone(),
        error: None,
        tool_call_count: record.tool_calls.len(),
        llm_call_count: record.llm_calls.len(),
        tool_calls: record
            .tool_calls
            .iter()
            .map(|call| ToolCallAuditView {
                iteration: Some(record.iteration),
                name: call.name.clone(),
                arguments: if call.arguments.is_null() {
                    None
                } else {
                    Some(call.arguments.clone())
                },
                result: call.result.clone(),
                error: call.error.clone(),
                started_at: None,
                completed_at: None,
                duration_ms: Some(call.duration_ms),
                success: call.success,
            })
            .collect(),
        llm_calls: record
            .llm_calls
            .iter()
            .map(|call| llm_view(record.iteration, call))
            .collect(),
    }
}

fn persisted_iteration_view(
    record: &wf_types::agent_execution::IterationRecord,
) -> IterationAuditView {
    IterationAuditView {
        iteration: record.iteration,
        started_at: record.started_at,
        completed_at: record.completed_at,
        duration_ms: record
            .completed_at
            .map(|end| end - record.started_at)
            .unwrap_or(0),
        response_content: record.response_content.clone(),
        error: record.error.clone(),
        tool_call_count: record
            .tool_calls
            .as_ref()
            .map(|calls| calls.len())
            .unwrap_or(0),
        llm_call_count: record
            .llm_calls
            .as_ref()
            .map(|calls| calls.len())
            .unwrap_or(0),
        tool_calls: record
            .tool_calls
            .iter()
            .flatten()
            .map(|call| ToolCallAuditView {
                iteration: Some(record.iteration),
                name: call.name.clone(),
                arguments: if call.arguments.is_null() {
                    None
                } else {
                    Some(call.arguments.clone())
                },
                result: call.result.clone(),
                error: call.error.clone(),
                started_at: Some(call.started_at),
                completed_at: call.completed_at,
                duration_ms: call.completed_at.map(|end| end - call.started_at),
                success: call.error.is_none(),
            })
            .collect(),
        llm_calls: record
            .llm_calls
            .iter()
            .flatten()
            .map(|call| llm_view(record.iteration, call))
            .collect(),
    }
}

fn llm_view(iteration: u32, call: &LlmCallRecord) -> LlmCallAuditView {
    LlmCallAuditView {
        iteration,
        seq: call.seq,
        profile_id: call.profile_id.clone(),
        model: call.model.clone(),
        request_summary: call.request_summary.clone(),
        response_summary: call.response_summary.clone(),
        prompt_tokens: call.prompt_tokens,
        completion_tokens: call.completion_tokens,
        started_at: call.started_at,
        completed_at: call.completed_at,
        duration_ms: call.duration_ms,
        error: call.error.clone(),
    }
}

fn live_node_view(record: &wf_workflow::state::NodeExecutionRecord) -> NodeExecutionAuditView {
    NodeExecutionAuditView {
        node_id: record.node_id.clone(),
        node_type: record.node_type.clone(),
        input: record.input.clone(),
        result: record.result.clone(),
        error: record.error.clone(),
        started_at: record.start_time,
        completed_at: record.end_time,
        duration_ms: record
            .end_time
            .map(|end| end - record.start_time)
            .unwrap_or(0),
        branch_id: record.branch_id.clone(),
    }
}

fn persisted_node_view(
    record: &wf_types::workflow_execution::NodeExecutionResult,
) -> NodeExecutionAuditView {
    NodeExecutionAuditView {
        node_id: record.node_id.clone(),
        node_type: String::new(),
        input: record.input.clone(),
        result: record.output.clone(),
        error: record.error.clone(),
        started_at: record.started_at.unwrap_or(0),
        completed_at: record.completed_at,
        duration_ms: record
            .completed_at
            .zip(record.started_at)
            .map(|(end, start)| end - start)
            .unwrap_or(0),
        branch_id: None,
    }
}

fn checkpoint_node_view(
    record: &wf_types::checkpoint::workflow::NodeExecutionRecord,
) -> NodeExecutionAuditView {
    NodeExecutionAuditView {
        node_id: record.node_id.clone(),
        node_type: record.node_type.clone(),
        input: record.input.clone(),
        result: record.result.clone(),
        error: record.error.clone(),
        started_at: record.started_at,
        completed_at: record.completed_at,
        duration_ms: record.duration_ms,
        branch_id: record.branch_id.clone(),
    }
}

/// Number of checkpoints persisted for the execution across both entity
/// type tags (agent `"checkpoint"` and workflow `"execution"`).
async fn checkpoint_count(ctx: &ApiContext, execution_id: &str) -> ApiResult<usize> {
    let agent = ctx
        .storage
        .checkpoint
        .list_by_entity(execution_id, AGENT_CHECKPOINT_ENTITY_TYPE)
        .await?;
    let workflow = ctx
        .storage
        .checkpoint
        .list_by_entity(execution_id, WORKFLOW_CHECKPOINT_ENTITY_TYPE)
        .await?;
    let mut ids = std::collections::HashSet::new();
    for checkpoint in agent.into_iter().chain(workflow) {
        ids.insert(checkpoint.id);
    }
    Ok(ids.len())
}

// ─── public API ────────────────────────────────────────────────────────────

/// Audit summary of an execution (agent loop or workflow).
pub async fn audit_summary(ctx: &ApiContext, execution_id: &str) -> ApiResult<AuditSummary> {
    let checkpoints = checkpoint_count(ctx, execution_id).await?;
    if let Some(data) = resolve_agent(ctx, execution_id).await? {
        return Ok(AuditSummary {
            execution_id: execution_id.to_string(),
            entity_kind: "agent_loop".to_string(),
            source: data.source,
            status: data.status,
            started_at: data.started_at,
            ended_at: data.ended_at,
            iteration_count: data.iterations.len(),
            tool_call_count: data
                .iterations
                .iter()
                .map(|iteration| iteration.tool_calls.len())
                .sum(),
            llm_call_count: data
                .iterations
                .iter()
                .map(|iteration| iteration.llm_calls.len())
                .sum(),
            node_execution_count: 0,
            checkpoint_count: checkpoints,
            truncation_stats: data.truncation_stats,
        });
    }
    if let Some(data) = resolve_workflow(ctx, execution_id).await? {
        return Ok(AuditSummary {
            execution_id: execution_id.to_string(),
            entity_kind: "workflow".to_string(),
            source: data.source,
            status: data.status,
            started_at: data.started_at,
            ended_at: data.ended_at,
            iteration_count: 0,
            tool_call_count: 0,
            llm_call_count: 0,
            node_execution_count: data.node_executions.len(),
            checkpoint_count: checkpoints,
            truncation_stats: data.truncation_stats,
        });
    }
    Ok(AuditSummary {
        execution_id: execution_id.to_string(),
        entity_kind: "unknown".to_string(),
        source: AuditSource::Unknown,
        status: None,
        started_at: None,
        ended_at: None,
        iteration_count: 0,
        tool_call_count: 0,
        llm_call_count: 0,
        node_execution_count: 0,
        checkpoint_count: 0,
        truncation_stats: None,
    })
}

/// Iterations of an agent loop execution with their tool/LLM audit trails.
pub async fn list_iterations(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<IterationAuditView>> {
    match resolve_agent(ctx, execution_id).await? {
        Some(data) => Ok(data.iterations),
        None => Ok(Vec::new()),
    }
}

/// Flattened tool call audit trail of an agent loop execution.
pub async fn list_tool_calls(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ToolCallAuditView>> {
    Ok(list_iterations(ctx, execution_id)
        .await?
        .into_iter()
        .flat_map(|iteration| iteration.tool_calls)
        .collect())
}

/// Flattened LLM call audit trail of an agent loop execution.
pub async fn list_llm_calls(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<LlmCallAuditView>> {
    Ok(list_iterations(ctx, execution_id)
        .await?
        .into_iter()
        .flat_map(|iteration| iteration.llm_calls)
        .collect())
}

// ─── offline timeline reconstruction ─────────────────────────────────────

/// Kind of a reconstructed timeline entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditTimelineEntryType {
    ExecutionStart,
    ExecutionEnd,
    IterationStart,
    IterationEnd,
    ToolCallStart,
    ToolCallEnd,
    ToolCall,
    LlmCallStart,
    LlmCallEnd,
    NodeExecutionStart,
    NodeExecutionEnd,
    NodeExecution,
    Error,
}

/// One entry of the reconstructed execution timeline.
///
/// Entries are merged from the iteration / tool / LLM / node records of the
/// resolved data source, sorted by timestamp, and carry the owning record
/// (iteration, `seq` for LLM calls, node id) as the provenance marker.
#[derive(Debug, Clone, Serialize)]
pub struct AuditTimelineEntry {
    pub timestamp: i64,
    pub r#type: AuditTimelineEntryType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u32>,
    /// Per-iteration `seq` of the owning LLM call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Owning record kind (`iteration` / `tool_call` / `llm_call` /
    /// `node_execution`).
    pub source: String,
}

/// Reconstruct the execution timeline from the resolved audit data:
/// a chronologically ordered, time-annotated stream of iteration / tool /
/// LLM / node events. Pure audit view — it never participates in restore.
/// Live executions should use the online `timeline`/`agent_timeline` APIs
/// (event stream) instead; this builds the offline view from records and
/// checkpoint snapshots.
pub async fn audit_timeline(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<AuditTimelineEntry>> {
    let mut entries = Vec::new();
    if let Some(data) = resolve_agent(ctx, execution_id).await? {
        entries.extend(agent_timeline_events(&data.iterations));
    } else if let Some(data) = resolve_workflow(ctx, execution_id).await? {
        entries.extend(workflow_timeline_events(&data.node_executions));
    }
    // Phase sorting: end-of-phase entries trail same-timestamp starts so a
    // zero-duration call still renders as start → end.
    entries.sort_by_key(|entry| (entry.timestamp, timeline_phase(&entry.r#type)));
    Ok(entries)
}

fn timeline_phase(t: &AuditTimelineEntryType) -> u8 {
    match t {
        AuditTimelineEntryType::ExecutionStart => 0,
        AuditTimelineEntryType::IterationStart => 1,
        AuditTimelineEntryType::ToolCallStart
        | AuditTimelineEntryType::LlmCallStart
        | AuditTimelineEntryType::NodeExecutionStart
        | AuditTimelineEntryType::ToolCall
        | AuditTimelineEntryType::NodeExecution => 2,
        AuditTimelineEntryType::IterationEnd
        | AuditTimelineEntryType::ToolCallEnd
        | AuditTimelineEntryType::LlmCallEnd
        | AuditTimelineEntryType::NodeExecutionEnd => 3,
        AuditTimelineEntryType::Error => 4,
        AuditTimelineEntryType::ExecutionEnd => 5,
    }
}

fn agent_timeline_events(iterations: &[IterationAuditView]) -> Vec<AuditTimelineEntry> {
    let mut entries = Vec::new();
    for iteration in iterations {
        entries.push(AuditTimelineEntry {
            timestamp: iteration.started_at,
            r#type: AuditTimelineEntryType::IterationStart,
            iteration: Some(iteration.iteration),
            seq: None,
            node_id: None,
            tool_name: None,
            profile_id: None,
            model: None,
            duration_ms: None,
            error: None,
            source: "iteration".to_string(),
        });
        for tool_call in &iteration.tool_calls {
            match (tool_call.started_at, tool_call.completed_at) {
                (Some(start), Some(end)) => {
                    entries.push(AuditTimelineEntry {
                        timestamp: start,
                        r#type: AuditTimelineEntryType::ToolCallStart,
                        iteration: tool_call.iteration,
                        seq: None,
                        node_id: None,
                        tool_name: Some(tool_call.name.clone()),
                        profile_id: None,
                        model: None,
                        duration_ms: Some(tool_call.duration_ms.unwrap_or(end - start)),
                        error: None,
                        source: "tool_call".to_string(),
                    });
                    entries.push(AuditTimelineEntry {
                        timestamp: end,
                        r#type: AuditTimelineEntryType::ToolCallEnd,
                        iteration: tool_call.iteration,
                        seq: None,
                        node_id: None,
                        tool_name: Some(tool_call.name.clone()),
                        profile_id: None,
                        model: None,
                        duration_ms: Some(tool_call.duration_ms.unwrap_or(end - start)),
                        error: tool_call.error.clone(),
                        source: "tool_call".to_string(),
                    });
                }
                // Live-shaped records carry no timestamps; emit a single
                // midpoint-annotated entry anchored to the iteration start.
                _ => entries.push(AuditTimelineEntry {
                    timestamp: iteration.started_at,
                    r#type: AuditTimelineEntryType::ToolCall,
                    iteration: tool_call.iteration,
                    seq: None,
                    node_id: None,
                    tool_name: Some(tool_call.name.clone()),
                    profile_id: None,
                    model: None,
                    duration_ms: tool_call.duration_ms,
                    error: tool_call.error.clone(),
                    source: "tool_call".to_string(),
                }),
            }
        }
        for llm_call in &iteration.llm_calls {
            entries.push(AuditTimelineEntry {
                timestamp: llm_call.started_at,
                r#type: AuditTimelineEntryType::LlmCallStart,
                iteration: Some(llm_call.iteration),
                seq: Some(llm_call.seq),
                node_id: None,
                tool_name: None,
                profile_id: Some(llm_call.profile_id.clone()),
                model: llm_call.model.clone(),
                duration_ms: Some(llm_call.duration_ms),
                error: None,
                source: "llm_call".to_string(),
            });
            if let Some(completed_at) = llm_call.completed_at {
                entries.push(AuditTimelineEntry {
                    timestamp: completed_at,
                    r#type: AuditTimelineEntryType::LlmCallEnd,
                    iteration: Some(llm_call.iteration),
                    seq: Some(llm_call.seq),
                    node_id: None,
                    tool_name: None,
                    profile_id: Some(llm_call.profile_id.clone()),
                    model: llm_call.model.clone(),
                    duration_ms: Some(llm_call.duration_ms),
                    error: llm_call.error.clone(),
                    source: "llm_call".to_string(),
                });
            }
        }
        if let Some(completed_at) = iteration.completed_at {
            entries.push(AuditTimelineEntry {
                timestamp: completed_at,
                r#type: AuditTimelineEntryType::IterationEnd,
                iteration: Some(iteration.iteration),
                seq: None,
                node_id: None,
                tool_name: None,
                profile_id: None,
                model: None,
                duration_ms: Some(iteration.duration_ms),
                error: iteration.error.clone(),
                source: "iteration".to_string(),
            });
        }
        if let Some(error) = &iteration.error {
            entries.push(AuditTimelineEntry {
                timestamp: iteration.completed_at.unwrap_or(iteration.started_at),
                r#type: AuditTimelineEntryType::Error,
                iteration: Some(iteration.iteration),
                seq: None,
                node_id: None,
                tool_name: None,
                profile_id: None,
                model: None,
                duration_ms: None,
                error: Some(error.clone()),
                source: "iteration".to_string(),
            });
        }
    }
    entries
}

fn workflow_timeline_events(node_executions: &[NodeExecutionAuditView]) -> Vec<AuditTimelineEntry> {
    let mut entries = Vec::new();
    for node in node_executions {
        match node.completed_at {
            Some(completed_at) => {
                entries.push(AuditTimelineEntry {
                    timestamp: node.started_at,
                    r#type: AuditTimelineEntryType::NodeExecutionStart,
                    iteration: None,
                    seq: None,
                    node_id: Some(node.node_id.clone()),
                    tool_name: None,
                    profile_id: None,
                    model: None,
                    duration_ms: Some(node.duration_ms),
                    error: None,
                    source: "node_execution".to_string(),
                });
                entries.push(AuditTimelineEntry {
                    timestamp: completed_at,
                    r#type: AuditTimelineEntryType::NodeExecutionEnd,
                    iteration: None,
                    seq: None,
                    node_id: Some(node.node_id.clone()),
                    tool_name: None,
                    profile_id: None,
                    model: None,
                    duration_ms: Some(node.duration_ms),
                    error: node.error.clone(),
                    source: "node_execution".to_string(),
                });
            }
            None => entries.push(AuditTimelineEntry {
                timestamp: node.started_at,
                r#type: AuditTimelineEntryType::NodeExecution,
                iteration: None,
                seq: None,
                node_id: Some(node.node_id.clone()),
                tool_name: None,
                profile_id: None,
                model: None,
                duration_ms: Some(node.duration_ms),
                error: node.error.clone(),
                source: "node_execution".to_string(),
            }),
        }
        if let Some(error) = &node.error {
            entries.push(AuditTimelineEntry {
                timestamp: node.completed_at.unwrap_or(node.started_at),
                r#type: AuditTimelineEntryType::Error,
                iteration: None,
                seq: None,
                node_id: Some(node.node_id.clone()),
                tool_name: None,
                profile_id: None,
                model: None,
                duration_ms: None,
                error: Some(error.clone()),
                source: "node_execution".to_string(),
            });
        }
    }
    entries
}

/// Per-node execution audit trail of a workflow execution.
pub async fn list_node_executions(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<NodeExecutionAuditView>> {
    match resolve_workflow(ctx, execution_id).await? {
        Some(data) => Ok(data.node_executions),
        None => Ok(Vec::new()),
    }
}

/// Full audit report of an execution.
pub async fn audit_report(ctx: &ApiContext, execution_id: &str) -> ApiResult<AuditReport> {
    let summary = audit_summary(ctx, execution_id).await?;
    let iterations = match summary.entity_kind.as_str() {
        "agent_loop" => Some(list_iterations(ctx, execution_id).await?),
        _ => None,
    };
    let node_executions = match summary.entity_kind.as_str() {
        "workflow" => Some(list_node_executions(ctx, execution_id).await?),
        _ => None,
    };
    Ok(AuditReport {
        summary,
        iterations: iterations.unwrap_or_default(),
        node_executions: node_executions.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wf_core::registry::MutableRegistry;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;
    use wf_workflow::entity::WorkflowExecutionEntity;
    use wf_workflow::state::NodeExecutionRecord;

    use super::*;

    fn make_ctx() -> ApiContext {
        ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        )
    }

    async fn seed_agent_loop(ctx: &ApiContext, id: &str) {
        use wf_agent::entity::AgentLoopEntity;
        let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(id.to_string())));
        {
            let mut state = entity.state.write().await;
            state.start().unwrap();
            state.start_iteration();
            state.record_tool_call("search", 100, true);
            state.record_llm_call(wf_types::agent_execution::LlmCallRecord {
                seq: 0,
                profile_id: "p1".into(),
                model: Some("mock".into()),
                request_summary: None,
                response_summary: None,
                prompt_tokens: 10,
                completion_tokens: 5,
                started_at: wf_common::now(),
                completed_at: Some(wf_common::now() + 50),
                duration_ms: 50,
                error: None,
            });
            state.end_iteration_with_content(Some("hi".into()));
            state.complete().unwrap();
        }
        let _ = ctx.agent_loops.register(entity);
    }

    async fn seed_workflow_entity(ctx: &ApiContext, id: &str) {
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from(id.to_string()),
            wf_types::Id::from(format!("wf-{id}")),
        ));
        let now = wf_common::now();
        {
            let mut state = entity.state.write().await;
            let _ = state.start();
            state.record_node_execution(NodeExecutionRecord {
                node_id: "n1".into(),
                node_name: "n1".into(),
                node_type: "LLM".into(),
                start_time: now,
                end_time: Some(now + 100),
                success: true,
                error: None,
                input: Some(serde_json::json!({"q": 1})),
                result: Some(serde_json::json!({"ok": true})),
                branch_id: None,
            });
            state.record_node_execution(NodeExecutionRecord {
                node_id: "n2".into(),
                node_name: "n2".into(),
                node_type: "HTTP".into(),
                start_time: now + 200,
                end_time: Some(now + 300),
                success: false,
                error: Some("boom".into()),
                input: None,
                result: None,
                branch_id: None,
            });
            let _ = state.complete();
        }
        ctx.workflow_executions
            .register(id.to_string(), entity.clone())
            .expect("register");
    }

    #[tokio::test]
    async fn agent_live_source_iterations_tool_and_llm_calls() {
        let ctx = make_ctx();
        seed_agent_loop(&ctx, "loop-audit-1").await;

        let summary = audit_summary(&ctx, "loop-audit-1").await.unwrap();
        assert_eq!(summary.entity_kind, "agent_loop");
        assert!(matches!(summary.source, AuditSource::Live));
        assert_eq!(summary.iteration_count, 1);
        assert_eq!(summary.tool_call_count, 1);
        assert_eq!(summary.llm_call_count, 1);
        assert_eq!(summary.node_execution_count, 0);
        assert_eq!(summary.status.as_deref(), Some("completed"));

        let iterations = list_iterations(&ctx, "loop-audit-1").await.unwrap();
        assert_eq!(iterations.len(), 1);
        assert_eq!(iterations[0].tool_calls[0].name, "search");
        assert!(iterations[0].tool_calls[0].success);
        assert_eq!(iterations[0].llm_calls[0].model.as_deref(), Some("mock"));
        assert_eq!(iterations[0].llm_calls[0].prompt_tokens, 10);

        let tool_calls = list_tool_calls(&ctx, "loop-audit-1").await.unwrap();
        assert_eq!(tool_calls.len(), 1);
        let llm_calls = list_llm_calls(&ctx, "loop-audit-1").await.unwrap();
        assert_eq!(llm_calls.len(), 1);
        assert_eq!(llm_calls[0].iteration, 1);

        let report = audit_report(&ctx, "loop-audit-1").await.unwrap();
        assert_eq!(report.iterations.len(), 1);
        assert!(report.node_executions.is_empty());
    }

    #[tokio::test]
    async fn workflow_live_source_node_executions() {
        let ctx = make_ctx();
        seed_workflow_entity(&ctx, "wf-audit-1").await;

        let summary = audit_summary(&ctx, "wf-audit-1").await.unwrap();
        assert_eq!(summary.entity_kind, "workflow");
        assert!(matches!(summary.source, AuditSource::Live));
        assert_eq!(summary.node_execution_count, 2);

        let nodes = list_node_executions(&ctx, "wf-audit-1").await.unwrap();
        assert_eq!(nodes.len(), 2);
        let n1 = nodes.iter().find(|n| n.node_id == "n1").unwrap();
        assert_eq!(n1.node_type, "LLM");
        assert_eq!(n1.input, Some(serde_json::json!({"q": 1})));
        assert_eq!(n1.result, Some(serde_json::json!({"ok": true})));
        assert!(n1.error.is_none());
        assert_eq!(n1.duration_ms, 100);
        let n2 = nodes.iter().find(|n| n.node_id == "n2").unwrap();
        assert_eq!(n2.error.as_deref(), Some("boom"));

        let report = audit_report(&ctx, "wf-audit-1").await.unwrap();
        assert_eq!(report.node_executions.len(), 2);
        assert!(report.iterations.is_empty());
    }

    #[tokio::test]
    async fn agent_persisted_source_fallback() {
        let ctx = make_ctx();
        let record = wf_types::AgentExecution {
            id: wf_types::Id::from("loop-persisted".to_string()),
            definition_id: wf_types::Id::from("agent-x".to_string()),
            status: wf_types::ExecutionStatus::Completed,
            current_iteration: 1,
            tool_call_count: 2,
            iteration_history: Some(vec![wf_types::agent_execution::IterationRecord {
                iteration: 1,
                started_at: 1000,
                completed_at: Some(2000),
                tool_calls: Some(vec![wf_types::agent_execution::ToolCallRecord {
                    id: "t1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "/tmp/x"}),
                    result: Some(serde_json::json!({"lines": 3})),
                    error: None,
                    started_at: 1100,
                    completed_at: Some(1200),
                }]),
                llm_calls: Some(vec![wf_types::agent_execution::LlmCallRecord {
                    seq: 0,
                    profile_id: "p1".into(),
                    model: None,
                    request_summary: None,
                    response_summary: None,
                    prompt_tokens: 4,
                    completion_tokens: 4,
                    started_at: 1000,
                    completed_at: Some(1500),
                    duration_ms: 500,
                    error: None,
                }]),
                response_content: Some("persisted".into()),
                error: None,
            }]),
            started_at: 1000,
            completed_at: Some(5000),
            error: None,
            context: None,
        };
        ctx.storage.agent_execution.save(&record).await.unwrap();

        let summary = audit_summary(&ctx, "loop-persisted").await.unwrap();
        assert!(matches!(summary.source, AuditSource::Persisted));
        assert_eq!(summary.iteration_count, 1);
        assert_eq!(summary.tool_call_count, 1);
        assert_eq!(summary.llm_call_count, 1);

        let iterations = list_iterations(&ctx, "loop-persisted").await.unwrap();
        assert_eq!(iterations[0].response_content.as_deref(), Some("persisted"));
        assert_eq!(iterations[0].tool_calls[0].started_at, Some(1100));
    }

    #[tokio::test]
    async fn unknown_execution_degrades_to_unknown_summary() {
        let ctx = make_ctx();
        let summary = audit_summary(&ctx, "missing").await.unwrap();
        assert_eq!(summary.entity_kind, "unknown");
        assert_eq!(summary.source, AuditSource::Unknown);
        assert_eq!(summary.tool_call_count, 0);
    }

    #[tokio::test]
    async fn timeline_reconstructs_chronological_agent_stream() {
        let ctx = make_ctx();
        seed_agent_loop(&ctx, "loop-timeline").await;

        let entries = audit_timeline(&ctx, "loop-timeline").await.unwrap();
        assert!(!entries.is_empty());
        // Chronological: starts precede same-timestamp ends.
        for pair in entries.windows(2) {
            assert!(
                pair[0].timestamp <= pair[1].timestamp,
                "timeline must be time-ordered"
            );
        }
        let kinds: Vec<AuditTimelineEntryType> = entries.iter().map(|e| e.r#type).collect();
        assert!(kinds.contains(&AuditTimelineEntryType::IterationStart));
        assert!(kinds.contains(&AuditTimelineEntryType::IterationEnd));
        // LLM call entry carries provenance (iteration + seq + model).
        let llm_start = entries
            .iter()
            .find(|e| e.r#type == AuditTimelineEntryType::LlmCallStart)
            .unwrap();
        assert_eq!(llm_start.iteration, Some(1));
        assert_eq!(llm_start.seq, Some(0));
        assert_eq!(llm_start.model.as_deref(), Some("mock"));
        // Live tool calls (no timestamps) render as a single ToolCall entry.
        let tool = entries
            .iter()
            .find(|e| e.r#type == AuditTimelineEntryType::ToolCall)
            .unwrap();
        assert_eq!(tool.tool_name.as_deref(), Some("search"));
    }

    #[tokio::test]
    async fn timeline_reconstructs_workflow_node_stream() {
        let ctx = make_ctx();
        seed_workflow_entity(&ctx, "wf-timeline").await;

        let entries = audit_timeline(&ctx, "wf-timeline").await.unwrap();
        assert_eq!(
            entries
                .iter()
                .filter(|e| matches!(
                    e.r#type,
                    AuditTimelineEntryType::NodeExecutionStart
                        | AuditTimelineEntryType::NodeExecutionEnd
                ))
                .count(),
            4
        );
        for pair in entries.windows(2) {
            assert!(pair[0].timestamp <= pair[1].timestamp);
        }
        // Failed node n2 produces an Error entry with provenance.
        let err_entry = entries
            .iter()
            .find(|e| e.r#type == AuditTimelineEntryType::Error)
            .unwrap();
        assert_eq!(err_entry.node_id.as_deref(), Some("n2"));
        assert_eq!(err_entry.error.as_deref(), Some("boom"));
    }
}
