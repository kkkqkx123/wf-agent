use wf_client::WfClient;
use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};

use crate::error::{CliError, CliResult};
use crate::transcript::{HistoryLine, Role};

/// Rebuild scrollback lines for a session id (execution or agent loop id).
///
/// The function tries live then persisted sources: conversation messages plus
/// iteration tool details. The produced lines use the same roles as the live
/// session so replay is visually identical.
pub async fn replay_scrollack(client: &WfClient, session_id: &str) -> CliResult<Vec<HistoryLine>> {
    // Validate the session exists (live or persisted).
    let summary = loop_summary(client, session_id).await?;
    if summary.is_none() {
        return Err(CliError::Business(format!(
            "execution not found: {session_id}"
        )));
    }

    // Collect messages. Prefer agent-loop scoped messages, fall back to
    // execution scoped messages.
    let mut records = loop_messages(client, session_id).await;
    if records.is_empty() {
        records = execution_messages(client, session_id).await;
    }
    // Already sorted by timestamp server-side, but ensure order.
    records.sort_by_key(|r| r.timestamp);

    // Shared record -> row conversion (also used by the paged loader).
    let mut lines = records_to_lines(&records);

    // Enrich with iteration tool details when messages did not cover them.
    // This also covers cases where no messages were persisted (pure iteration
    // history path).
    if lines.is_empty() {
        let history = iteration_details(client, session_id).await;
        for detail in history {
            push_iteration_detail(&mut lines, &detail);
        }
    } else {
        // Even when messages exist, append any missing tool iterations that
        // were not represented as Tool messages (some executors store tool
        // calls only in iteration records).
        let history = iteration_details(client, session_id).await;
        // Check if we already have tool lines; if not, add them.
        let has_tool_line = lines.iter().any(|l| {
            let txt = message_line_text(l);
            txt.starts_with("▲ ") || txt.starts_with("✓ ") || txt.starts_with("✗ ")
        });
        if !has_tool_line && !history.is_empty() {
            for detail in &history {
                push_iteration_tool_calls(&mut lines, detail);
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
    if let Some(summary) = loop_summary(client, session_id).await? {
        lines.push(summary_history_line(
            summary
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or(session_id),
            summary
                .get("current_iteration")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            summary.get("start_time").and_then(|v| v.as_i64()),
            summary.get("end_time").and_then(|v| v.as_i64()),
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
/// loader [`replay_scrollack`] stays available for the TUI inline mode and tests.
pub async fn replay_scrollack_page(
    client: &WfClient,
    session_id: &str,
    before_timestamp: Option<i64>,
    limit: u64,
) -> CliResult<ReplayPage> {
    // Validate the session exists (live or persisted), like the full loader.
    let summary = loop_summary(client, session_id).await?;
    if summary.is_none() {
        return Err(CliError::Business(format!(
            "execution not found: {session_id}"
        )));
    }

    // The conversation endpoint pages by offset, not by timestamp: pull the
    // full ledger once and slice the requested window locally so the pager
    // keeps its timestamp-cursor contract.
    let mut records = loop_messages(client, session_id).await;
    if records.is_empty() {
        records = execution_messages(client, session_id).await;
    }
    records.sort_by_key(|r| r.timestamp);
    let older_count = match before_timestamp {
        None => records.len(),
        Some(before) => records.iter().filter(|r| r.timestamp < before).count(),
    };
    let mut window: Vec<Message> = match before_timestamp {
        None => records.into_iter().rev().take(limit as usize).collect(),
        Some(before) => records
            .into_iter()
            .filter(|r| r.timestamp < before)
            .rev()
            .take(limit as usize)
            .collect(),
    };
    // Newest-first take above; restore oldest -> newest for rendering.
    window.reverse();
    let has_more = older_count > window.len();
    let next_before = window.first().map(|r| r.timestamp).filter(|_| has_more);
    let mut lines = records_to_lines(&window);

    // The newest page is the bottom of the conversation: close it with the
    // same summary line a live session (and the full loader) appends. Older
    // pages are pure history and must not repeat it.
    if before_timestamp.is_none() && lines.is_empty() && !has_more {
        // No persisted messages at all (pure iteration-history path): fall
        // back to the full loader once so the tail page stays identical to a
        // full replay. Iteration history has no timestamps to page over.
        return Ok(ReplayPage {
            lines: replay_scrollack(client, session_id).await?,
            next_before: None,
            has_more: false,
        });
    }
    if before_timestamp.is_none() {
        if let Some(s) = summary.as_ref() {
            lines.push(summary_history_line(
                s.get("id").and_then(|v| v.as_str()).unwrap_or(session_id),
                s.get("current_iteration").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                s.get("start_time").and_then(|v| v.as_i64()),
                s.get("end_time").and_then(|v| v.as_i64()),
            ));
        }
    }

    Ok(ReplayPage {
        lines,
        next_before,
        has_more,
    })
}

/// Find the most recent session id (by start time descending). Considers
/// completed, failed, running and any status, live and persisted.
pub async fn latest_session_id(client: &WfClient) -> CliResult<Option<String>> {
    let data: serde_json::Value = client.get_json("/api/v1/agent-loops/summaries").await?;
    let mut summaries = data
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| data.as_array().cloned())
        .unwrap_or_default();
    if summaries.is_empty() {
        return Ok(None);
    }
    // Prefer Completed/Failed/Running but fallback to any if none match.
    summaries.sort_by_key(|s| {
        std::cmp::Reverse(s.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0))
    });
    // First try to find a Completed or Failed, then any.
    if let Some(found) = summaries.iter().find(|s| {
        matches!(
            s.get("status").and_then(|v| v.as_str()).unwrap_or(""),
            "completed" | "failed" | "Completed" | "Failed"
        )
    }) {
        return Ok(found
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string));
    }
    Ok(summaries
        .first()
        .and_then(|s| s.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_string))
}

async fn loop_summary(client: &WfClient, session_id: &str) -> CliResult<Option<serde_json::Value>> {
    match client
        .get_json::<serde_json::Value>(&format!("/api/v1/agent-loops/{session_id}/summary"))
        .await
    {
        Ok(summary) if !summary.is_null() => Ok(Some(summary)),
        _ => Ok(None),
    }
}

async fn loop_messages(client: &WfClient, session_id: &str) -> Vec<Message> {
    let data: Result<serde_json::Value, _> = client
        .get_json(&format!(
            "/api/v1/agent-loops/{session_id}/conversation?limit=5000"
        ))
        .await;
    conversation_items(data)
}

async fn execution_messages(client: &WfClient, session_id: &str) -> Vec<Message> {
    let data: Result<serde_json::Value, _> = client
        .get_json(&format!(
            "/api/v1/messages/by-execution/{session_id}?limit=5000&offset=0"
        ))
        .await;
    conversation_items(data)
}

fn conversation_items(data: Result<serde_json::Value, wf_client::ClientError>) -> Vec<Message> {
    let items = data
        .ok()
        .and_then(|v| {
            v.get("items")
                .and_then(|items| serde_json::from_value(items.clone()).ok())
                .or_else(|| serde_json::from_value(v).ok())
        })
        .unwrap_or_default();
    items
}

async fn iteration_details(client: &WfClient, session_id: &str) -> Vec<serde_json::Value> {
    let data: Result<serde_json::Value, _> = client
        .get_json(&format!("/api/v1/agent-loops/{session_id}/iteration-history"))
        .await;
    data.ok()
        .and_then(|v| {
            v.get("items")
                .and_then(|items| items.as_array().cloned())
                .or_else(|| v.as_array().cloned())
        })
        .unwrap_or_default()
}

fn push_iteration_detail(lines: &mut Vec<HistoryLine>, detail: &serde_json::Value) {
    if let Some(content) = detail.get("response_content").and_then(|v| v.as_str()) {
        if !content.trim().is_empty() {
            lines.push(HistoryLine::new_role(content.to_string(), Role::Default));
        }
    }
    push_iteration_tool_calls(lines, detail);
}

fn push_iteration_tool_calls(lines: &mut Vec<HistoryLine>, detail: &serde_json::Value) {
    let calls = detail
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for call in &calls {
        let name = call.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
        lines.push(HistoryLine::new_role(format!("▲ {name}"), Role::Muted));
        let success = call.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
        let result = if success {
            format!("✓ {name}")
        } else {
            format!("✗ {name}")
        };
        let role = if success { Role::Add } else { Role::Error };
        lines.push(HistoryLine::new_role(result, role));
    }
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

/// Convert messages (ascending by timestamp) into the same lines the
/// live session renders. Shared by the full loader ([`replay_scrollack`]) and
/// the paged loader ([`replay_scrollack_page`]) so a paged replay stays
/// visually identical to a full one.
fn records_to_lines(records: &[Message]) -> Vec<HistoryLine> {
    let mut lines: Vec<HistoryLine> = Vec::new();
    for message in records {
        let text = message_text(message);
        if text.trim().is_empty() {
            // Still handle tool calls carried on the message.
            if let Some(calls) = &message.tool_calls {
                for call in calls {
                    lines.push(HistoryLine::new_role(
                        format!("▲ {}", call.function.name),
                        Role::Muted,
                    ));
                }
            }
            continue;
        }
        match &message.role {
            MessageRole::User => {
                lines.push(HistoryLine::new_role(format!("> {text}"), Role::Accent));
                if let Some(calls) = &message.tool_calls {
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
                if let Some(calls) = &message.tool_calls {
                    for call in calls {
                        lines.push(HistoryLine::new_role(
                            format!("▲ {}", call.function.name),
                            Role::Muted,
                        ));
                    }
                }
            }
            MessageRole::Tool => {
                let name = message
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
    use wf_types::message::{MessageContentValue, MessageRole};

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
            thinking_signature: None,
            metadata: None,
        }
    }

    #[test]
    fn records_convert_to_lines() {
        let records = vec![
            make_message("m1", MessageRole::User, "hello", 1001),
            make_message("m2", MessageRole::Assistant, "hi there", 1002),
        ];
        let lines = records_to_lines(&records);
        let texts: Vec<String> = lines.iter().flat_map(|l| l.raw_lines(80)).collect();
        assert!(texts.iter().any(|t| t.contains("hello")));
        assert!(texts.iter().any(|t| t.contains("hi there")));
    }

    #[test]
    fn conversation_items_parse_page_shell() {
        let data = serde_json::json!({
            "items": [
                {"id": "m1", "role": "user", "content": "hi", "timestamp": 5},
            ],
            "limit": 10,
            "offset": 0,
            "total": 1,
        });
        let items = conversation_items(Ok(data));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id.as_str(), "m1");
    }
}
