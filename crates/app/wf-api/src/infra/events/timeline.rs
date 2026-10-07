//! Structured execution timeline: lifecycle phase construction and the
//! timeline summary view.

use serde::Serialize;
use wf_types::events::{BaseEvent, EventType};

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::timeline_events::timeline;

/// One lifecycle phase of an execution timeline.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionTimelinePhase {
    pub name: String,
    pub start_event: EventType,
    pub end_event: EventType,
    pub start_time: i64,
    /// End timestamp; `None` while the phase is still in progress.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    /// Phase duration; `None` while the phase is still in progress.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    pub events: Vec<BaseEvent>,
}

/// Structured execution timeline grouped into lifecycle phases.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionTimeline {
    pub execution_id: String,
    pub workflow_id: Option<String>,
    pub status: String,
    pub start_time: i64,
    pub end_time: i64,
    pub total_elapsed: i64,
    pub phases: Vec<ExecutionTimelinePhase>,
    pub events: Vec<BaseEvent>,
}

/// Phase definition pairs driving timeline phase construction.
const PHASE_DEFINITIONS: &[(&str, EventType, EventType)] = &[
    (
        "Execution",
        EventType::WorkflowExecutionStarted,
        EventType::WorkflowExecutionCompleted,
    ),
    (
        "Node Execution",
        EventType::NodeStarted,
        EventType::NodeCompleted,
    ),
    (
        "Tool Call",
        EventType::ToolCallStarted,
        EventType::ToolCallCompleted,
    ),
    (
        "Agent Turn",
        EventType::AgentTurnStarted,
        EventType::AgentTurnCompleted,
    ),
    (
        "Agent Iteration",
        EventType::AgentIterationStarted,
        EventType::AgentIterationCompleted,
    ),
    (
        "Checkpoint",
        EventType::CheckpointCreated,
        EventType::CheckpointRestored,
    ),
];

/// Structured execution timeline with lifecycle phases for an execution.
pub async fn get_execution_timeline(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<ExecutionTimeline>> {
    let events = timeline(ctx, execution_id).await?;
    if events.is_empty() {
        return Ok(None);
    }

    let status = determine_status(&events);
    let start_time = events[0].timestamp;
    let end_time = events[events.len() - 1].timestamp;
    let workflow_id = events.iter().find_map(|e| e.workflow_id.clone());
    let phases = build_phases(&events);

    Ok(Some(ExecutionTimeline {
        execution_id: execution_id.to_string(),
        workflow_id,
        status: status.to_string(),
        start_time,
        end_time,
        total_elapsed: end_time - start_time,
        phases,
        events,
    }))
}

/// Compact summary of an execution timeline.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionTimelineSummary {
    pub execution_id: String,
    pub status: String,
    pub total_events: usize,
    pub start_time: i64,
    pub end_time: i64,
    pub total_elapsed: i64,
    pub phase_count: usize,
}

/// A compact digest of [`get_execution_timeline`]; `None` when the execution
/// has no events.
pub async fn execution_timeline_summary(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<ExecutionTimelineSummary>> {
    let Some(timeline) = get_execution_timeline(ctx, execution_id).await? else {
        return Ok(None);
    };
    Ok(Some(ExecutionTimelineSummary {
        execution_id: timeline.execution_id,
        status: timeline.status,
        total_events: timeline.events.len(),
        start_time: timeline.start_time,
        end_time: timeline.end_time,
        total_elapsed: timeline.total_elapsed,
        phase_count: timeline.phases.len(),
    }))
}

fn determine_status(events: &[BaseEvent]) -> &'static str {
    for event in events.iter().rev() {
        match event.r#type {
            EventType::WorkflowExecutionCompleted => return "completed",
            EventType::WorkflowExecutionFailed => return "failed",
            EventType::WorkflowExecutionPaused => return "paused",
            EventType::WorkflowExecutionCancelled => return "cancelled",
            _ => {}
        }
    }
    "running"
}

fn build_phases(events: &[BaseEvent]) -> Vec<ExecutionTimelinePhase> {
    let mut phases = Vec::new();
    for (name, start_event, end_event) in PHASE_DEFINITIONS {
        let start_event = start_event.clone();
        let end_event = end_event.clone();
        let starts: Vec<&BaseEvent> = events.iter().filter(|e| e.r#type == start_event).collect();
        if starts.is_empty() {
            continue;
        }
        let ends: Vec<&BaseEvent> = events.iter().filter(|e| e.r#type == end_event).collect();
        let mut used_starts = std::collections::HashSet::new();
        let mut used_ends = std::collections::HashSet::new();
        for (si, start) in starts.iter().enumerate() {
            if used_starts.contains(&si) {
                continue;
            }
            let mut matched = false;
            for (ei, end) in ends.iter().enumerate() {
                if used_ends.contains(&ei) || end.timestamp < start.timestamp {
                    continue;
                }
                used_starts.insert(si);
                used_ends.insert(ei);
                phases.push(ExecutionTimelinePhase {
                    name: (*name).to_string(),
                    start_event: start_event.clone(),
                    end_event: end_event.clone(),
                    start_time: start.timestamp,
                    end_time: Some(end.timestamp),
                    duration: Some(end.timestamp - start.timestamp),
                    events: events
                        .iter()
                        .filter(|e| e.timestamp >= start.timestamp && e.timestamp <= end.timestamp)
                        .cloned()
                        .collect(),
                });
                matched = true;
                break;
            }
            if !matched {
                phases.push(ExecutionTimelinePhase {
                    name: (*name).to_string(),
                    start_event: start_event.clone(),
                    end_event: end_event.clone(),
                    start_time: start.timestamp,
                    end_time: None,
                    duration: None,
                    events: events
                        .iter()
                        .filter(|e| e.timestamp >= start.timestamp)
                        .cloned()
                        .collect(),
                });
            }
        }
    }
    phases.sort_by_key(|p| p.start_time);
    phases
}
