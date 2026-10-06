//! First-class execution log query.
//!
//! Lifecycle events are the system's append-only log: every engine publishes
//! timestamped [`BaseEvent`]s while a run proceeds. Raw event endpoints
//! expose those records verbatim; this module projects them into uniform
//! log entries (oldest first) with a one-line human message so tailing,
//! filtering and paging work the same for workflow and agent runs.
//!
//! Per-execution reads merge the workflow and agent timelines by timestamp;
//! the global read pages the newest-first event history. Both slice one
//! page with one extra item kept to report a following page.

use serde::Serialize;
use wf_types::events::{BaseEvent, EventType};

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;
use crate::infra::events::{self, EventQueryOptions};

/// One log line projected from a lifecycle event.
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    pub timestamp: i64,
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
    /// One-line human summary: the event type plus node / tool / error
    /// hints when the event metadata carries them.
    pub message: String,
}

/// Filter for log queries. Every field is optional; an unset field matches
/// everything.
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    pub execution_id: Option<String>,
    pub event_types: Option<Vec<EventType>>,
    /// Only entries at or after this timestamp (ms epoch).
    pub since: Option<i64>,
    /// Case-insensitive substring match against the rendered message.
    pub message_contains: Option<String>,
}

fn metadata_text(metadata: Option<&std::collections::HashMap<String, serde_json::Value>>, key: &str) -> Option<String> {
    metadata?.get(key).and_then(|v| {
        v.as_str()
            .map(|s| s.to_string())
            .or_else(|| {
                if v.is_string() {
                    None
                } else {
                    Some(v.to_string())
                }
            })
    })
}

fn render_message(event: &BaseEvent) -> String {
    let mut parts = vec![event.r#type.as_str().to_string()];
    if let Some(name) = &event.event_name {
        parts.push(name.clone());
    }
    for key in ["node_id", "nodeId", "node"] {
        if let Some(node) = metadata_text(event.metadata.as_ref(), key) {
            parts.push(format!("node={node}"));
            break;
        }
    }
    for key in ["tool_name", "tool", "name"] {
        if let Some(tool) = metadata_text(event.metadata.as_ref(), key) {
            parts.push(format!("tool={tool}"));
            break;
        }
    }
    if let Some(error) = metadata_text(event.metadata.as_ref(), "error") {
        parts.push(format!("error={error}"));
    }
    parts.join(" ")
}

impl From<&BaseEvent> for LogEntry {
    fn from(event: &BaseEvent) -> Self {
        Self {
            execution_id: event
                .execution_id
                .as_deref()
                .or(event.agent_loop_id.as_deref())
                .map(|s| s.to_string()),
            workflow_id: event.workflow_id.as_deref().map(|s| s.to_string()),
            timestamp: event.timestamp,
            event_type: event.r#type.as_str().to_string(),
            event_name: event.event_name.clone(),
            message: render_message(event),
        }
    }
}

fn matches(entry: &LogEntry, filter: &LogFilter) -> bool {
    if let Some(since) = filter.since {
        if entry.timestamp < since {
            return false;
        }
    }
    if let Some(needle) = filter.message_contains.as_deref() {
        if !entry.message.to_lowercase().contains(&needle.to_lowercase()) {
            return false;
        }
    }
    true
}

/// Logs of one execution, oldest first, as one page plus a continuation flag.
pub async fn logs_for_execution(
    ctx: &ApiContext,
    execution_id: &str,
    filter: Option<&LogFilter>,
    limit: usize,
    offset: usize,
) -> ApiResult<(Vec<LogEntry>, bool)> {
    let mut events = events::timeline(ctx, execution_id).await?;
    events.extend(events::agent_timeline(ctx, execution_id).await?);
    events.sort_by_key(|e| e.timestamp);

    if let Some(types) = filter.and_then(|f| f.event_types.clone()) {
        events.retain(|e| types.contains(&e.r#type));
    }

    let mut entries: Vec<LogEntry> = events.iter().map(LogEntry::from).collect();
    if let Some(filter) = filter {
        entries.retain(|e| matches(e, filter));
    }

    let page: Vec<LogEntry> = entries.into_iter().skip(offset).take(limit + 1).collect();
    let has_more = page.len() > limit;
    let mut page = page;
    page.truncate(limit);
    Ok((page, has_more))
}

/// Global log read across executions, oldest first, as one page plus flag.
///
/// The underlying history is newest-first and bounded, so the page window
/// is taken from its oldest end to keep tailing order stable.
pub async fn query_logs(
    ctx: &ApiContext,
    filter: Option<&LogFilter>,
    limit: usize,
    offset: usize,
) -> ApiResult<(Vec<LogEntry>, bool)> {
    let fetch = offset.saturating_add(limit).saturating_add(1);
    let options = EventQueryOptions {
        execution_id: filter.and_then(|f| f.execution_id.clone()),
        event_types: filter.and_then(|f| f.event_types.clone()),
        limit: Some(fetch.max(1)),
        ..EventQueryOptions::default()
    };
    let mut events = events::history(ctx, &options).await?;
    events.sort_by_key(|e| e.timestamp);

    let mut entries: Vec<LogEntry> = events.iter().map(LogEntry::from).collect();
    if let Some(filter) = filter {
        entries.retain(|e| matches(e, filter));
    }

    let page: Vec<LogEntry> = entries.into_iter().skip(offset).take(limit + 1).collect();
    let has_more = page.len() > limit;
    let mut page = page;
    page.truncate(limit);
    Ok((page, has_more))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    fn event(id: &str, exec: Option<&str>, kind: EventType, ts: i64) -> BaseEvent {
        BaseEvent {
            id: id.to_string(),
            r#type: kind,
            timestamp: ts,
            event_name: None,
            workflow_id: None,
            execution_id: exec.map(|s| s.to_string()),
            agent_loop_id: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn renders_type_and_metadata_hints() {
        let mut meta = std::collections::HashMap::new();
        meta.insert(
            "node_id".to_string(),
            serde_json::Value::String("n1".to_string()),
        );
        let e = BaseEvent {
            metadata: Some(meta),
            ..event("e1", Some("exec-1"), EventType::NodeStarted, 5)
        };
        let entry = LogEntry::from(&e);
        assert_eq!(entry.execution_id.as_deref(), Some("exec-1"));
        assert!(entry.message.contains("NODE_STARTED"));
        assert!(entry.message.contains("node=n1"));
    }

    #[tokio::test]
    async fn per_execution_logs_page_oldest_first() {
        let ctx = make_ctx();
        for (i, ts) in [30, 10, 20].iter().enumerate() {
            ctx.persistence
                .save_event(&event(
                    &format!("e{i}"),
                    Some("exec-1"),
                    EventType::NodeStarted,
                    *ts,
                ))
                .await
                .unwrap();
        }
        let (page, has_more) = logs_for_execution(&ctx, "exec-1", None, 2, 0).await.unwrap();
        assert!(has_more);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].timestamp, 10);
        let (rest, has_more) = logs_for_execution(&ctx, "exec-1", None, 2, 2).await.unwrap();
        assert!(!has_more);
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].timestamp, 30);
    }

    #[tokio::test]
    async fn global_logs_filter_by_message() {
        let ctx = make_ctx();
        ctx.persistence
            .save_event(&event(
                "e1",
                Some("exec-1"),
                EventType::WorkflowExecutionFailed,
                1,
            ))
            .await
            .unwrap();
        ctx.persistence
            .save_event(&event("e2", Some("exec-2"), EventType::NodeStarted, 2))
            .await
            .unwrap();
        let filter = LogFilter {
            message_contains: Some("failed".to_string()),
            ..LogFilter::default()
        };
        let (page, _) = query_logs(&ctx, Some(&filter), 10, 0).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].execution_id.as_deref(), Some("exec-1"));
    }
}
