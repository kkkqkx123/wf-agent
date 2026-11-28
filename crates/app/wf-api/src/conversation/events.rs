//! The events a tree session publishes.
//!
//! Subscribers (SSE endpoints, the TUI, later a web app) never read the tree to
//! find out what happened: the driver publishes a frame per fact, the
//! subscription endpoint replays the ones it missed and streams the rest. That
//! is why a turn can outlive the connection that started it.
//!
//! Every event is anchored on `execution_id = session_id`, which is the same
//! anchor the chat session already uses for its messages, so one filter finds
//! the whole conversation.

use wf_types::events::{BaseEvent, EventType};
use wf_types::llm::TokenUsageStats;
use wf_types::Metadata;

use super::query::DirtyList;
use crate::infra::context::ApiContext;

fn now() -> i64 {
    wf_common::now()
}

/// Publish one event. A tree session has no persistence bridge of its own, so
/// publishing is fire-and-forget: subscribers that are already attached see it,
/// and a later subscriber replays the tree instead of a frame log.
fn publish(ctx: &ApiContext, event_type: EventType, session_id: &str, metadata: Metadata) {
    let event = BaseEvent {
        id: wf_common::generate_id(),
        r#type: event_type,
        timestamp: now(),
        event_name: None,
        workflow_id: None,
        execution_id: Some(session_id.to_string()),
        agent_loop_id: None,
        metadata: Some(metadata),
    };
    let _ = ctx
        .event_bus
        .publish_logged(event, "conversation tree session");
}

fn metadata(pairs: Vec<(&str, serde_json::Value)>) -> Metadata {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

pub fn turn_started(
    ctx: &ApiContext,
    session_id: &str,
    turn_id: &str,
    node_id: &str,
    parent: &str,
) {
    publish(
        ctx,
        EventType::ConversationTurnStarted,
        session_id,
        metadata(vec![
            ("turn_id", serde_json::json!(turn_id)),
            ("node_id", serde_json::json!(node_id)),
            ("parent", serde_json::json!(parent)),
        ]),
    );
}

pub fn turn_delta(ctx: &ApiContext, session_id: &str, turn_id: &str, node_id: &str, content: &str) {
    publish(
        ctx,
        EventType::LlmStreamChunk,
        session_id,
        metadata(vec![
            ("turn_id", serde_json::json!(turn_id)),
            ("node_id", serde_json::json!(node_id)),
            ("kind", serde_json::json!("delta")),
            ("content", serde_json::json!(content)),
        ]),
    );
}

pub fn turn_reasoning(
    ctx: &ApiContext,
    session_id: &str,
    turn_id: &str,
    node_id: &str,
    content: &str,
) {
    publish(
        ctx,
        EventType::LlmStreamChunk,
        session_id,
        metadata(vec![
            ("turn_id", serde_json::json!(turn_id)),
            ("node_id", serde_json::json!(node_id)),
            ("kind", serde_json::json!("reasoning")),
            ("content", serde_json::json!(content)),
        ]),
    );
}

pub fn turn_usage(
    ctx: &ApiContext,
    session_id: &str,
    turn_id: &str,
    node_id: &str,
    usage: &TokenUsageStats,
) {
    publish(
        ctx,
        EventType::LlmStreamDone,
        session_id,
        metadata(vec![
            ("turn_id", serde_json::json!(turn_id)),
            ("node_id", serde_json::json!(node_id)),
            ("kind", serde_json::json!("usage")),
            ("usage", serde_json::json!(usage)),
        ]),
    );
}

/// One allow-listed tool executed during a read-only tool round.
pub fn turn_tool(
    ctx: &ApiContext,
    session_id: &str,
    turn_id: &str,
    node_id: &str,
    tool_name: &str,
    call_id: &str,
    success: bool,
) {
    publish(
        ctx,
        EventType::ConversationTurnTool,
        session_id,
        metadata(vec![
            ("turn_id", serde_json::json!(turn_id)),
            ("node_id", serde_json::json!(node_id)),
            ("tool_name", serde_json::json!(tool_name)),
            ("call_id", serde_json::json!(call_id)),
            ("success", serde_json::json!(success)),
        ]),
    );
}

pub fn turn_completed(
    ctx: &ApiContext,
    session_id: &str,
    turn_id: &str,
    node_id: &str,
    message_id: &str,
    usage: Option<&TokenUsageStats>,
) {
    let mut pairs = vec![
        ("turn_id", serde_json::json!(turn_id)),
        ("node_id", serde_json::json!(node_id)),
        ("message_id", serde_json::json!(message_id)),
    ];
    if let Some(usage) = usage {
        pairs.push(("usage", serde_json::json!(usage)));
    }
    publish(
        ctx,
        EventType::ConversationTurnCompleted,
        session_id,
        metadata(pairs),
    );
}

pub fn turn_failed(
    ctx: &ApiContext,
    session_id: &str,
    turn_id: &str,
    node_id: &str,
    error: &str,
    reason: &str,
) {
    publish(
        ctx,
        EventType::ConversationTurnFailed,
        session_id,
        metadata(vec![
            ("turn_id", serde_json::json!(turn_id)),
            ("node_id", serde_json::json!(node_id)),
            ("error", serde_json::json!(error)),
            ("reason", serde_json::json!(reason)),
        ]),
    );
}

pub fn tree_changed(
    ctx: &ApiContext,
    session_id: &str,
    version: u64,
    change: &str,
    node_id: Option<&str>,
) {
    let mut pairs = vec![
        ("version", serde_json::json!(version)),
        ("change", serde_json::json!(change)),
    ];
    if let Some(node_id) = node_id {
        pairs.push(("node_id", serde_json::json!(node_id)));
    }
    publish(
        ctx,
        EventType::ConversationTreeChanged,
        session_id,
        metadata(pairs),
    );
}

pub fn dirty_changed(ctx: &ApiContext, session_id: &str, version: u64, dirty: &DirtyList) {
    let nodes: Vec<serde_json::Value> = dirty
        .entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "node_id": entry.node_id,
                "index": entry.index,
                "stale_since_version": entry.stale_since_version,
            })
        })
        .collect();
    publish(
        ctx,
        EventType::ConversationDirtyChanged,
        session_id,
        metadata(vec![
            ("version", serde_json::json!(version)),
            ("total", serde_json::json!(dirty.entries.len())),
            ("other_branches", serde_json::json!(dirty.other_branches)),
            ("nodes", serde_json::json!(nodes)),
        ]),
    );
}

pub fn step_started(ctx: &ApiContext, session_id: &str, job_id: &str, mode: &str, plan: &[String]) {
    publish(
        ctx,
        EventType::ConversationStepStarted,
        session_id,
        metadata(vec![
            ("job_id", serde_json::json!(job_id)),
            ("mode", serde_json::json!(mode)),
            ("plan", serde_json::json!(plan)),
        ]),
    );
}

pub fn step_progress(
    ctx: &ApiContext,
    session_id: &str,
    job_id: &str,
    done: usize,
    total: usize,
    node_id: &str,
) {
    publish(
        ctx,
        EventType::ConversationStepCompleted,
        session_id,
        metadata(vec![
            ("job_id", serde_json::json!(job_id)),
            ("done", serde_json::json!(done)),
            ("total", serde_json::json!(total)),
            ("node_id", serde_json::json!(node_id)),
            ("reason", serde_json::json!("step")),
        ]),
    );
}

pub fn step_finished(
    ctx: &ApiContext,
    session_id: &str,
    job_id: &str,
    reason: &str,
    done: usize,
    node_id: Option<&str>,
) {
    let mut pairs = vec![
        ("job_id", serde_json::json!(job_id)),
        ("reason", serde_json::json!(reason)),
        ("done", serde_json::json!(done)),
    ];
    if let Some(node_id) = node_id {
        pairs.push(("node_id", serde_json::json!(node_id)));
    }
    publish(
        ctx,
        EventType::ConversationStepCompleted,
        session_id,
        metadata(pairs),
    );
}
