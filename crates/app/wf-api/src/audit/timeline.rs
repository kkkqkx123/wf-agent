//! Offline audit timeline reconstruction.
//!
//! Rebuilds a chronologically ordered, time-annotated event stream from the
//! resolved audit records. Pure audit view — it never participates in
//! restore. Live executions should use the online `timeline` /
//! `agent_timeline` APIs (event stream) instead.

use serde::Serialize;

use crate::audit::resolver::{resolve_agent, resolve_workflow};
use crate::audit::views::{IterationAuditView, NodeExecutionAuditView, MAX_AUDIT_TIMELINE_ENTRIES};
use crate::entity::execution::{resolve_execution, ExecutionDomain};
use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

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
    match resolve_execution(ctx, execution_id).await {
        Ok(ExecutionDomain::AgentLoop) | Err(crate::ApiError::Conflict(_)) => {
            if let Some(data) = resolve_agent(ctx, execution_id).await? {
                entries.extend(agent_timeline_events(&data.iterations));
            } else if let Some(data) = resolve_workflow(ctx, execution_id).await? {
                entries.extend(workflow_timeline_events(&data.node_executions));
            }
        }
        Ok(ExecutionDomain::Workflow) => {
            if let Some(data) = resolve_workflow(ctx, execution_id).await? {
                entries.extend(workflow_timeline_events(&data.node_executions));
            } else if let Some(data) = resolve_agent(ctx, execution_id).await? {
                entries.extend(agent_timeline_events(&data.iterations));
            }
        }
        Err(crate::ApiError::ExecutionNotFound { .. }) => {}
        Err(e) => return Err(e),
    }
    // Phase sorting: end-of-phase entries trail same-timestamp starts so a
    // zero-duration call still renders as start → end.
    entries.sort_by_key(|entry| (entry.timestamp, timeline_phase(&entry.r#type)));
    // Length cap keeps chain and timeline views bounded for large dumps.
    entries.truncate(MAX_AUDIT_TIMELINE_ENTRIES);
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
