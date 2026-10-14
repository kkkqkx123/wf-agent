//! Display summaries for tool results: the frontend-facing channel that is
//! independent of model context.
//!
//! Folding (see `super::fold`) rewrites model-visible history to save
//! tokens; display produces a small `DisplayPayload` alongside the full
//! result for the stream event. Frontends dispatch on the payload kind and
//! apply only generic truncation, so per-tool presentation knowledge lives
//! here next to the tools instead of spreading across frontends.

use wf_types::tool::{BackgroundMarker, DisplayKind, DisplayPayload};

use super::fold::{is_file_edit_tool, is_shell_tool};

/// Shell tail excerpt budget in lines. Matches the web verbose preset so the
/// server payload is the ceiling every frontend preset cuts below.
pub const DISPLAY_SHELL_TAIL_LINES: usize = 25;
/// Generic head excerpt budget in lines. Matches the web verbose preset so
/// the server payload is the ceiling every frontend preset cuts below.
pub const DISPLAY_TEXT_HEAD_LINES: usize = 25;
/// Single-line preview width in graphemes.
pub const DISPLAY_PREVIEW_CHARS: usize = 160;

fn is_exploration_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file"
            | "read_file_folded"
            | "list_files"
            | "glob_search"
            | "grep_search"
            | "code_search"
            | "code_keyword_search"
            | "retrieve_code"
            | "web_fetch"
            | "web_search"
    )
}

fn truncate_graphemes(text: &str, max: usize) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    text.graphemes(true).take(max).collect()
}

/// Build the display payload for one tool result. The full text stays in
/// the sibling result field; the preview is a small excerpt shaped by the
/// tool kind: command tails, diff markers, count summaries, or head lines.
pub fn display_for(tool_name: Option<&str>, result: &str) -> DisplayPayload {
    let name = tool_name.unwrap_or_default();
    if is_shell_tool(name) {
        return shell_display(result);
    }
    if is_file_edit_tool(name) {
        return diff_display(result);
    }
    if is_exploration_tool(name) {
        return count_display(result);
    }
    text_display(result)
}

fn shell_display(result: &str) -> DisplayPayload {
    let tail: Vec<&str> = result
        .lines()
        .rev()
        .take(DISPLAY_SHELL_TAIL_LINES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let total = result.lines().count();
    let hidden_lines = total.saturating_sub(tail.len());
    DisplayPayload {
        kind: DisplayKind::ShellTail,
        preview: tail.join("\n"),
        truncated: hidden_lines > 0,
        hidden_lines,
    }
}

fn diff_display(result: &str) -> DisplayPayload {
    match super::fold::fold_merged_diff(result) {
        Some(folded) => {
            let total = result.lines().count();
            DisplayPayload {
                kind: DisplayKind::Diff,
                hidden_lines: total.saturating_sub(folded.lines().count()),
                preview: folded,
                truncated: true,
            }
        }
        None => DisplayPayload {
            kind: DisplayKind::Diff,
            preview: result.to_string(),
            truncated: false,
            hidden_lines: 0,
        },
    }
}

fn count_display(result: &str) -> DisplayPayload {
    let total = result.lines().count();
    if total <= 1 {
        let line = result.lines().next().unwrap_or_default();
        return DisplayPayload {
            kind: DisplayKind::Count,
            preview: truncate_graphemes(line.trim(), DISPLAY_PREVIEW_CHARS),
            truncated: false,
            hidden_lines: 0,
        };
    }
    DisplayPayload {
        kind: DisplayKind::Count,
        preview: format!("{total} lines"),
        truncated: true,
        hidden_lines: total,
    }
}

fn text_display(result: &str) -> DisplayPayload {
    let head: Vec<&str> = result.lines().take(DISPLAY_TEXT_HEAD_LINES).collect();
    let total = result.lines().count();
    let hidden_lines = total.saturating_sub(head.len());
    DisplayPayload {
        kind: DisplayKind::Text,
        preview: head.join("\n"),
        truncated: hidden_lines > 0,
        hidden_lines,
    }
}

/// Structured background linkage for one tool result. Only the tools that
/// hand a running session back instead of blocking (`backend_shell` on
/// spawn, `shell_background` on manual handoff, `shell_background_all` on
/// whole-execution handoff, `shell_wait` on background preemption) produce
/// a marker, and only when the result carries both a session id and a
/// background flag. Everything else returns `None` so frontends can match
/// on presence.
pub fn background_marker_for(tool_name: Option<&str>, result: &str) -> Option<BackgroundMarker> {
    let name = tool_name.unwrap_or_default();
    if !matches!(
        name,
        "backend_shell" | "shell_background" | "shell_background_all" | "shell_wait"
    ) {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(result).ok()?;
    if name == "shell_background_all" {
        let tasks = value.get("tasks")?.as_array()?;
        let first = tasks.first()?;
        let session_id = first.get("session_id")?.as_str()?;
        if session_id.is_empty() {
            return None;
        }
        let backgrounded = value
            .get("backgrounded")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !backgrounded {
            return None;
        }
        return Some(BackgroundMarker {
            session_id: session_id.to_string(),
            backgrounded: true,
        });
    }
    let session_id = value.get("session_id")?.as_str()?;
    if session_id.is_empty() {
        return None;
    }
    let backgrounded = value
        .get("backgrounded")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || value
            .get("background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
    if !backgrounded {
        return None;
    }
    Some(BackgroundMarker {
        session_id: session_id.to_string(),
        backgrounded: true,
    })
}

/// Compact model-visible result for background handoffs. The full result
/// stays in the stream event for UI rendering; the message appended to LLM
/// history keeps only the session linkage and follow-up guidance so large
/// background outputs never pollute the model context. Returns `None` when
/// the result carries no background marker.
pub fn model_result_for(tool_name: Option<&str>, result: &str) -> Option<String> {
    let marker = background_marker_for(tool_name, result)?;
    let value: serde_json::Value = serde_json::from_str(result).ok()?;
    let guidance = value
        .get("guidance")
        .and_then(|v| v.as_str())
        .unwrap_or("Command moved to the background. Wait for its completion event; then read the result with shell_output or shell_tasks.");
    if tool_name.unwrap_or_default() == "shell_background_all" {
        let count = value.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        return Some(
            serde_json::json!({
                "backgrounded": true,
                "count": count,
                "first_session_id": marker.session_id,
                "guidance": guidance,
            })
            .to_string(),
        );
    }
    Some(
        serde_json::json!({
            "session_id": marker.session_id,
            "backgrounded": true,
            "guidance": guidance,
        })
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_tools_keep_the_tail() {
        let result = (0..50)
            .map(|i| format!("out {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let payload = display_for(Some("execute_command"), &result);
        assert_eq!(payload.kind, DisplayKind::ShellTail);
        assert!(payload.truncated);
        assert_eq!(payload.hidden_lines, 25);
        assert!(payload.preview.contains("out 49"));
        assert!(!payload.preview.contains("out 0\n"));
    }

    #[test]
    fn short_shell_output_is_verbatim() {
        let payload = display_for(Some("shell_output"), "ok");
        assert_eq!(payload.kind, DisplayKind::ShellTail);
        assert!(!payload.truncated);
        assert_eq!(payload.preview, "ok");
    }

    #[test]
    fn edit_tools_keep_diff_markers() {
        let mut result = String::new();
        for i in 0..200 {
            result.push_str(&format!(
                "context line {i} with filler padding 0123456789\n"
            ));
            if i % 10 == 0 {
                result.push_str(&format!("@@ -{i},5 +{i},5 @@ hunk header\n"));
            }
        }
        let payload = display_for(Some("edit_file"), &result);
        assert_eq!(payload.kind, DisplayKind::Diff);
        assert!(payload.truncated);
        assert!(payload.preview.contains("@@"));
    }

    #[test]
    fn read_tools_collapse_to_a_count_summary() {
        let result = (0..100)
            .map(|i| format!("file line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let payload = display_for(Some("read_file"), &result);
        assert_eq!(payload.kind, DisplayKind::Count);
        assert!(payload.truncated);
        assert_eq!(payload.preview, "100 lines");
    }

    #[test]
    fn single_line_read_results_keep_the_line() {
        let payload = display_for(Some("glob_search"), "src/main.rs");
        assert_eq!(payload.kind, DisplayKind::Count);
        assert!(!payload.truncated);
        assert_eq!(payload.preview, "src/main.rs");
    }

    #[test]
    fn unknown_tools_fall_back_to_head_text() {
        let result = (0..50)
            .map(|i| format!("row {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let payload = display_for(Some("mystery_tool"), &result);
        assert_eq!(payload.kind, DisplayKind::Text);
        assert!(payload.truncated);
        assert_eq!(payload.hidden_lines, 25);
        assert!(payload.preview.contains("row 0"));
    }

    #[test]
    fn missing_tool_name_uses_the_generic_path() {
        let payload = display_for(None, "hello");
        assert_eq!(payload.kind, DisplayKind::Text);
        assert_eq!(payload.preview, "hello");
    }

    #[test]
    fn background_shell_spawn_carries_a_marker() {
        let result = serde_json::json!({
            "session_id": "sess-1",
            "status": "started",
            "background": true,
        })
        .to_string();
        let marker = background_marker_for(Some("backend_shell"), &result);
        assert_eq!(
            marker,
            Some(BackgroundMarker {
                session_id: "sess-1".to_string(),
                backgrounded: true,
            })
        );
    }

    #[test]
    fn manual_background_handoff_carries_a_marker() {
        let result = serde_json::json!({
            "session_id": "sess-2",
            "backgrounded": true,
        })
        .to_string();
        let marker = background_marker_for(Some("shell_background"), &result);
        assert_eq!(marker.map(|m| m.session_id), Some("sess-2".to_string()));
    }

    #[test]
    fn background_all_and_wait_preemption_carry_markers() {
        let all = serde_json::json!({
            "backgrounded": true,
            "count": 2,
            "tasks": [{"session_id": "sess-a"}, {"session_id": "sess-b"}],
        })
        .to_string();
        assert_eq!(
            background_marker_for(Some("shell_background_all"), &all).map(|m| m.session_id),
            Some("sess-a".to_string())
        );
        let waited = serde_json::json!({
            "session_id": "sess-w",
            "backgrounded": true,
        })
        .to_string();
        assert_eq!(
            background_marker_for(Some("shell_wait"), &waited).map(|m| m.session_id),
            Some("sess-w".to_string())
        );
    }

    #[test]
    fn background_model_result_compacts_to_linkage() {
        let result = serde_json::json!({
            "session_id": "sess-1",
            "backgrounded": true,
            "output": "huge output that must not reach the model",
            "guidance": "Do not poll.",
        })
        .to_string();
        let compact = model_result_for(Some("shell_wait"), &result).expect("compact");
        assert!(compact.contains("sess-1"));
        assert!(compact.contains("Do not poll."));
        assert!(!compact.contains("huge output"));
        assert_eq!(model_result_for(Some("shell_output"), &result), None);
    }

    #[test]
    fn foreground_and_foreign_results_carry_no_marker() {
        assert_eq!(
            background_marker_for(Some("shell_output"), "{\"session_id\":\"s\"}"),
            None
        );
        assert_eq!(
            background_marker_for(
                Some("backend_shell"),
                "{\"session_id\":\"s\",\"background\":false}"
            ),
            None
        );
        assert_eq!(
            background_marker_for(Some("backend_shell"), "not json"),
            None
        );
        assert_eq!(
            background_marker_for(Some("backend_shell"), "{\"background\":true}"),
            None
        );
    }
}
