use wf_api::infra::context::ApiContext;
use wf_api::infra::error::ApiError;
use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};

use crate::scrollback::{HistoryLine, Role};

/// Rebuild scrollback lines for a session id (execution or agent loop id).
///
/// The function tries live then persisted sources: conversation messages plus
/// iteration tool details. The produced lines use the same roles as the live
/// session so replay is visually identical.
pub async fn replay_scrollack(
    ctx: &ApiContext,
    session_id: &str,
) -> Result<Vec<HistoryLine>, ApiError> {
    // Validate the session exists (live or persisted).
    let summary = wf_api::agent::agent_loop_registry::summary(ctx, session_id).await?;
    if summary.is_none() {
        return Err(ApiError::execution_not_found(session_id));
    }

    // Collect messages. Prefer agent-loop scoped messages, fall back to
    // execution scoped messages.
    let mut records = wf_api::entity::message::by_agent_loop(ctx, session_id)
        .await
        .unwrap_or_default();
    if records.is_empty() {
        records = wf_api::entity::message::by_execution(ctx, session_id)
            .await
            .unwrap_or_default();
    }
    // Already sorted by timestamp inside storage adapters, but ensure order.
    records.sort_by_key(|r| r.message.timestamp);

    let mut lines: Vec<HistoryLine> = Vec::new();

    if !records.is_empty() {
        for rec in records {
            let text = message_text(&rec.message);
            if text.trim().is_empty() {
                // Still handle tool calls carried on the message.
                if let Some(calls) = &rec.message.tool_calls {
                    for call in calls {
                        lines.push(HistoryLine::new_role(
                            format!("▲ {}", call.function.name),
                            Role::Muted,
                        ));
                    }
                }
                continue;
            }
            match rec.message.role {
                MessageRole::User => {
                    lines.push(HistoryLine::new_role(format!("> {text}"), Role::Accent));
                    if let Some(calls) = &rec.message.tool_calls {
                        for call in calls {
                            lines.push(HistoryLine::new_role(
                                format!("▲ {}", call.function.name),
                                Role::Muted,
                            ));
                        }
                    }
                }
                MessageRole::Assistant => {
                    lines.push(HistoryLine::new_role(text.clone(), Role::Default));
                    if let Some(calls) = &rec.message.tool_calls {
                        for call in calls {
                            lines.push(HistoryLine::new_role(
                                format!("▲ {}", call.function.name),
                                Role::Muted,
                            ));
                        }
                    }
                }
                MessageRole::Tool => {
                    let name = rec
                        .message
                        .tool_name
                        .clone()
                        .unwrap_or_else(|| "tool".to_string());
                    let is_error = text.to_lowercase().contains("error")
                        || text.to_lowercase().contains("failed");
                    let prefix = if is_error { "✗" } else { "✓" };
                    let role = if is_error { Role::Error } else { Role::Add };
                    if text.is_empty() {
                        lines.push(HistoryLine::new_role(format!("{prefix} {name}"), role));
                    } else {
                        lines.push(HistoryLine::new_role(
                            format!("{prefix} {name}: {text}"),
                            role,
                        ));
                    }
                }
                MessageRole::System => {
                    lines.push(HistoryLine::new_role(text, Role::Muted));
                }
            }
        }
    }

    // Enrich with iteration tool details when messages did not cover them.
    // This also covers cases where no messages were persisted (pure iteration
    // history path).
    if lines.is_empty() {
        let history =
            wf_api::agent::agent_loop_registry::iteration_history(ctx, session_id).await?;
        for detail in history {
            if let Some(content) = detail.response_content {
                if !content.trim().is_empty() {
                    lines.push(HistoryLine::new_role(content, Role::Default));
                }
            }
            for call in detail.tool_calls {
                lines.push(HistoryLine::new_role(
                    format!("▲ {}", call.name),
                    Role::Muted,
                ));
                let result = if call.success {
                    format!("✓ {}", call.name)
                } else {
                    format!("✗ {}", call.name)
                };
                let role = if call.success { Role::Add } else { Role::Error };
                lines.push(HistoryLine::new_role(result, role));
            }
        }
    } else {
        // Even when messages exist, append any missing tool iterations that
        // were not represented as Tool messages (some executors store tool
        // calls only in iteration records).
        let history = wf_api::agent::agent_loop_registry::iteration_history(ctx, session_id)
            .await
            .unwrap_or_default();
        // Check if we already have tool lines; if not, add them.
        let has_tool_line = lines.iter().any(|l| {
            let txt = message_line_text(l);
            txt.starts_with("▲ ") || txt.starts_with("✓ ") || txt.starts_with("✗ ")
        });
        if !has_tool_line && !history.is_empty() {
            for detail in history {
                for call in detail.tool_calls {
                    lines.push(HistoryLine::new_role(
                        format!("▲ {}", call.name),
                        Role::Muted,
                    ));
                    let result = if call.success {
                        format!("✓ {}", call.name)
                    } else {
                        format!("✗ {}", call.name)
                    };
                    let role = if call.success { Role::Add } else { Role::Error };
                    lines.push(HistoryLine::new_role(result, role));
                }
            }
        }
    }

    if lines.is_empty() {
        // Fallback: at least show the session header so replay is not empty.
        lines.push(HistoryLine::new_role(
            format!("session {session_id}"),
            Role::Muted,
        ));
    }

    // Append a closing summary line similar to live sessions when we have
    // a summary with timing.
    if let Some(summary) = wf_api::agent::agent_loop_registry::summary(ctx, session_id).await? {
        let duration_ms = match (summary.start_time, summary.end_time) {
            (Some(start), Some(end)) => (end - start).max(0) as u64,
            _ => 0,
        };
        let summary_line = if duration_ms > 0 {
            format!(
                "▣ {} · {} iterations · {}",
                summary.id,
                summary.current_iteration,
                format_duration_short(duration_ms)
            )
        } else {
            format!(
                "▣ {} · {} iterations",
                summary.id, summary.current_iteration
            )
        };
        lines.push(HistoryLine::new_role(summary_line, Role::Muted));
    }

    Ok(lines)
}

/// Find the most recent session id (by start time descending). Considers
/// completed, failed, running and any status, live and persisted.
pub async fn latest_session_id(ctx: &ApiContext) -> Result<Option<String>, ApiError> {
    let mut summaries = wf_api::agent::agent_loop_registry::summaries(ctx, None).await?;
    if summaries.is_empty() {
        return Ok(None);
    }
    // Prefer Completed/Failed/Running but fallback to any if none match.
    summaries.sort_by_key(|s| std::cmp::Reverse(s.start_time.unwrap_or(0)));
    // First try to find a Completed or Failed, then any.
    if let Some(found) = summaries.iter().find(|s| {
        matches!(
            s.status,
            wf_types::ExecutionStatus::Completed | wf_types::ExecutionStatus::Failed
        )
    }) {
        return Ok(Some(found.id.clone()));
    }
    Ok(summaries.first().map(|s| s.id.clone()))
}

fn message_text(message: &Message) -> String {
    match &message.content {
        MessageContentValue::Text(text) => text.clone(),
        MessageContentValue::Rich(parts) => parts
            .iter()
            .filter_map(|part| match part {
                MessageContent::Text { text } => Some(text.clone()),
                MessageContent::Thinking { thinking, .. } => Some(thinking.clone()),
                MessageContent::ToolResult { tool_result } => Some(tool_result.content.clone()),
                MessageContent::ToolUse { .. } | MessageContent::ImageUrl { .. } => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn message_line_text(line: &HistoryLine) -> String {
    // Extract plain text from HistoryLine's display at large width (single
    // line). We approximate by using raw_lines with large width.
    line.raw_lines(1000).join(" ")
}

fn format_duration_short(ms: u64) -> String {
    if ms >= 60_000 {
        format!("{}m{:02}s", ms / 60_000, (ms % 60_000) / 1_000)
    } else if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{ms}ms")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_api::infra::context::ApiContext;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::message::{Message, MessageContentValue, MessageRole};

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    fn make_message(id: &str, role: MessageRole, text: &str, ts: i64) -> Message {
        Message {
            id: id.into(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: ts,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn replay_requires_existing_session() {
        let ctx = make_ctx();
        let err = replay_scrollack(&ctx, "missing").await.unwrap_err();
        assert!(matches!(
            err,
            wf_api::infra::error::ApiError::ExecutionNotFound { .. }
        ));
    }

    #[tokio::test]
    async fn replay_builds_lines_from_messages() {
        let ctx = make_ctx();
        // Seed a persisted agent execution so summary exists.
        let storage = ctx.storage.clone();
        let record = wf_types::AgentExecution {
            id: wf_types::Id::from("sess-1".to_string()),
            definition_id: wf_types::Id::from("agent-x".to_string()),
            status: wf_types::ExecutionStatus::Completed,
            current_iteration: 2,
            tool_call_count: 0,
            iteration_history: None,
            started_at: 1000,
            completed_at: Some(5000),
            error: None,
            context: None,
        };
        storage.agent_execution.save(&record).await.unwrap();
        wf_api::entity::message::add_message(
            &ctx,
            "sess-1",
            Some("sess-1"),
            make_message("m1", MessageRole::User, "hello", 1001),
        )
        .await
        .unwrap();
        wf_api::entity::message::add_message(
            &ctx,
            "sess-1",
            Some("sess-1"),
            make_message("m2", MessageRole::Assistant, "hi there", 1002),
        )
        .await
        .unwrap();

        let lines = replay_scrollack(&ctx, "sess-1").await.unwrap();
        assert!(lines.len() >= 2);
        let texts: Vec<String> = lines.iter().flat_map(|l| l.raw_lines(80)).collect();
        assert!(texts.iter().any(|t| t.contains("hello")));
        assert!(texts.iter().any(|t| t.contains("hi there")));
    }

    #[tokio::test]
    async fn latest_session_picks_most_recent() {
        let ctx = make_ctx();
        let storage = ctx.storage.clone();
        for (id, start) in [("a", 1000), ("b", 2000), ("c", 1500)] {
            let rec = wf_types::AgentExecution {
                id: wf_types::Id::from(id.to_string()),
                definition_id: wf_types::Id::from("ag".to_string()),
                status: wf_types::ExecutionStatus::Completed,
                current_iteration: 1,
                tool_call_count: 0,
                iteration_history: None,
                started_at: start,
                completed_at: Some(start + 500),
                error: None,
                context: None,
            };
            storage.agent_execution.save(&rec).await.unwrap();
        }
        let latest = latest_session_id(&ctx).await.unwrap().unwrap();
        assert_eq!(latest, "b");
    }
}
