use wf_api::infra::context::ApiContext;
use wf_api::infra::error::ApiError;
use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};

use crate::transcript::{HistoryLine, Role};

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

    // Shared record -> row conversion (also used by the paged loader).
    let mut lines = records_to_lines(&records);

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
        lines.push(summary_history_line(
            &summary.id,
            summary.current_iteration,
            summary.start_time,
            summary.end_time,
        ));
    }

    Ok(lines)
}

/// Default number of records fetched per replay page when a session is too
/// long to materialize in one pass.
pub const REPLAY_PAGE_LIMIT: u64 = 200;

/// One page of replay history: the converted lines plus the cursor needed to
/// fetch the page that precedes it (older messages).
pub struct ReplayPage {
    pub lines: Vec<HistoryLine>,
    /// Timestamp cursor of the earliest record on this page; pass it as
    /// `before_timestamp` to load the older page. `None` means this page
    /// already reached the beginning of the session.
    pub next_before: Option<i64>,
    /// Whether older records still exist beyond this page.
    pub has_more: bool,
}

/// Load one page of persisted scrollback for a session id, newest first.
///
/// `before_timestamp = None` returns the most recent `limit` records (the
/// tail the user sees first). Each older page is fetched by passing the
/// previous page's [`ReplayPage::next_before`] as the cursor. The full
/// loader [`replay_scrollack`] stays available for mini mode and tests.
pub async fn replay_scrollack_page(
    ctx: &ApiContext,
    session_id: &str,
    before_timestamp: Option<i64>,
    limit: u64,
) -> Result<ReplayPage, ApiError> {
    // Validate the session exists (live or persisted), like the full loader.
    let summary = wf_api::agent::agent_loop_registry::summary(ctx, session_id).await?;
    if summary.is_none() {
        return Err(ApiError::execution_not_found(session_id));
    }

    // Prefer agent-loop scoped messages, fall back to execution scoped.
    let mut page =
        wf_api::entity::message::page_by_agent_loop(ctx, session_id, before_timestamp, limit)
            .await
            .unwrap_or_default();
    if page.records.is_empty() && !page.has_more {
        page = wf_api::entity::message::page_by_execution(ctx, session_id, before_timestamp, limit)
            .await
            .unwrap_or_default();
    }

    // Records arrive newest first; convert oldest -> newest so the page reads
    // like the corresponding slice of a full replay.
    page.records.sort_by_key(|r| r.message.timestamp);
    let next_before = page
        .records
        .first()
        .map(|r| r.message.timestamp)
        .filter(|_| page.has_more);
    let mut lines = records_to_lines(&page.records);

    // The newest page is the bottom of the conversation: close it with the
    // same summary line a live session (and the full loader) appends. Older
    // pages are pure history and must not repeat it.
    if before_timestamp.is_none() && lines.is_empty() && !page.has_more {
        // No persisted messages at all (pure iteration-history path): fall
        // back to the full loader once so the tail page stays identical to a
        // full replay. Iteration history has no timestamps to page over.
        return Ok(ReplayPage {
            lines: replay_scrollack(ctx, session_id).await?,
            next_before: None,
            has_more: false,
        });
    }
    if before_timestamp.is_none() {
        if let Some(s) = summary.as_ref() {
            lines.push(summary_history_line(
                &s.id,
                s.current_iteration,
                s.start_time,
                s.end_time,
            ));
        }
    }

    Ok(ReplayPage {
        lines,
        next_before,
        has_more: page.has_more,
    })
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

/// Convert message records (ascending by timestamp) into the same lines the
/// live session renders. Shared by the full loader ([`replay_scrollack`]) and
/// the paged loader ([`replay_scrollack_page`]) so a paged replay stays
/// visually identical to a full one.
fn records_to_lines(records: &[wf_types::MessageStorageMetadata]) -> Vec<HistoryLine> {
    let mut lines: Vec<HistoryLine> = Vec::new();
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
        match &rec.message.role {
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
                let is_error =
                    text.to_lowercase().contains("error") || text.to_lowercase().contains("failed");
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
    lines
}

/// Closing summary line (▣ id · iterations · duration), shared by the full
/// and paged loaders so the newest page ends exactly like a live session.
fn summary_history_line(
    id: &str,
    iterations: u32,
    start: Option<i64>,
    end: Option<i64>,
) -> HistoryLine {
    let duration_ms = match (start, end) {
        (Some(start), Some(end)) => (end - start).max(0) as u64,
        _ => 0,
    };
    let summary_line = if duration_ms > 0 {
        format!(
            "▣ {id} · {iterations} iterations · {}",
            format_duration_short(duration_ms)
        )
    } else {
        format!("▣ {id} · {iterations} iterations")
    };
    HistoryLine::new_role(summary_line, Role::Muted)
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

    /// Seed an agent execution plus `count` single-line messages.
    async fn seed_session(ctx: &Arc<ApiContext>, id: &str, count: usize) {
        let storage = ctx.storage.clone();
        let record = wf_types::AgentExecution {
            id: wf_types::Id::from(id.to_string()),
            definition_id: wf_types::Id::from("agent-x".to_string()),
            status: wf_types::ExecutionStatus::Completed,
            current_iteration: (count as u32).max(1),
            tool_call_count: 0,
            iteration_history: None,
            started_at: 1000,
            completed_at: Some(5000),
            error: None,
            context: None,
        };
        storage.agent_execution.save(&record).await.unwrap();
        for i in 0..count {
            wf_api::entity::message::add_message(
                ctx,
                id,
                Some(id),
                make_message(
                    &format!("{id}-m{i}"),
                    MessageRole::User,
                    &format!("message {i}"),
                    2000 + i as i64,
                ),
            )
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn replay_page_starts_at_tail_and_pages_older() {
        let ctx = make_ctx();
        seed_session(&ctx, "sess-page", 5).await;

        // First page returns the newest records (tail), newest within the page
        // last, plus a summary line exactly like the full loader.
        let first = replay_scrollack_page(&ctx, "sess-page", None, 2)
            .await
            .unwrap();
        assert!(first.has_more);
        let first_texts: Vec<String> = first.lines.iter().flat_map(|l| l.raw_lines(80)).collect();
        assert!(first_texts.iter().any(|t| t.contains("message 4")));
        assert!(first_texts.iter().any(|t| t.contains("message 3")));
        assert!(first_texts.iter().any(|t| t.contains("▣ sess-page")));

        // The cursor continues strictly before the earliest page record.
        let older = replay_scrollack_page(&ctx, "sess-page", first.next_before, 2)
            .await
            .unwrap();
        assert!(older.has_more);
        let older_texts: Vec<String> = older.lines.iter().flat_map(|l| l.raw_lines(80)).collect();
        assert!(older_texts.iter().any(|t| t.contains("message 2")));
        assert!(older_texts.iter().any(|t| t.contains("message 1")));
        // Older pages do not repeat the tail summary.
        assert!(!older_texts.iter().any(|t| t.contains("▣ sess-page")));

        // Final page reaches the beginning and reports no more records.
        let last = replay_scrollack_page(&ctx, "sess-page", older.next_before, 2)
            .await
            .unwrap();
        assert!(!last.has_more);
        assert_eq!(last.next_before, None);
        let last_texts: Vec<String> = last.lines.iter().flat_map(|l| l.raw_lines(80)).collect();
        assert!(last_texts.iter().any(|t| t.contains("message 0")));

        // Paged assembly (older pages prepended) covers the same rows as the
        // full loader, in the same order and without the summary line.
        let full = replay_scrollack(&ctx, "sess-page").await.unwrap();
        let full_texts: Vec<String> = full.iter().flat_map(|l| l.raw_lines(80)).collect();
        let mut paged: Vec<String> = last
            .lines
            .iter()
            .chain(older.lines.iter())
            .chain(first.lines.iter())
            .flat_map(|l| l.raw_lines(80))
            .collect();
        paged.retain(|t| !t.contains("▣"));
        let full_wo_summary: Vec<String> = full_texts
            .iter()
            .filter(|t| !t.contains("▣"))
            .cloned()
            .collect();
        assert_eq!(paged, full_wo_summary);
    }

    #[tokio::test]
    async fn replay_page_falls_back_for_iteration_only_sessions() {
        let ctx = make_ctx();
        // Persisted execution with zero messages: paging cannot work without
        // timestamps, so the first page degrades to the full loader output.
        let storage = ctx.storage.clone();
        let record = wf_types::AgentExecution {
            id: wf_types::Id::from("sess-no-msg".to_string()),
            definition_id: wf_types::Id::from("agent-x".to_string()),
            status: wf_types::ExecutionStatus::Completed,
            current_iteration: 1,
            tool_call_count: 0,
            iteration_history: None,
            started_at: 1000,
            completed_at: Some(2000),
            error: None,
            context: None,
        };
        storage.agent_execution.save(&record).await.unwrap();

        let page = replay_scrollack_page(&ctx, "sess-no-msg", None, 2)
            .await
            .unwrap();
        assert!(!page.has_more);
        assert_eq!(page.next_before, None);
        assert!(!page.lines.is_empty());
    }
}
