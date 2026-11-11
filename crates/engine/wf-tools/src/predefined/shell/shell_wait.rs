//! Definition and stateful instance of the shell_wait tool.

use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use wf_shell::engine::BackgroundShellStore;
use wf_shell::terminal_session::TerminalSession;

pub static SHELL_WAIT: ToolDefinition = ToolDefinition {
    id: "shell_wait",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "shell",
    tags: &["wait"],
    description: "Wait for a background shell session command to finish or for its output to match a pattern. Returns the current status, exit code and full output; 'timed_out' is true when the deadline elapsed first. When the session is moved to the background while waiting, the wait returns immediately with 'backgrounded' true instead of waiting out the deadline. Provide 'pattern' (a regex) to stop early once matching output appears. Absolute byte cursors from shell_output window responses stay valid across truncation; incremental reads are a convenience over the same cursor.",
    parameters: &[
        ToolParameter { name: "session_id", r#type: "string", required: true, description: "The session ID returned by backend_shell", default_json: None, constraints: None },
        ToolParameter { name: "timeout", r#type: "number", required: false, description: "Timeout in seconds (default 300)", default_json: Some("300"), constraints: None },
        ToolParameter { name: "pattern", r#type: "string", required: false, description: "Regex; stop waiting once output contains a match", default_json: None, constraints: None },
        ToolParameter { name: "settle_ms", r#type: "number", required: false, description: "After a pattern match, keep polling until output is quiet for this long in milliseconds before returning (debounces output jitter; default 0)", default_json: Some("0"), constraints: None },
        ToolParameter { name: "max_output_bytes", r#type: "number", required: false, description: "Cap the returned output at this many bytes (tail-kept); sets 'output_truncated' with session window fields for paging the earlier output via shell_output", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["shell_wait(\"abc123\")", "shell_wait(\"abc123\", pattern=\"BUILD SUCCESS\")"]),
};

struct ShellWaitInstance {
    store: Arc<BackgroundShellStore>,
}

impl StatefulInstance for ShellWaitInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'session_id' parameter".into())
            })?;
        let timeout_ms = super::timeout_secs_to_ms(params).unwrap_or(300_000);
        let settle_ms = params
            .get("settle_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let max_output_bytes = params
            .get("max_output_bytes")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let pattern = params.get("pattern").and_then(|v| v.as_str());
        let matcher = match pattern {
            Some(p) => Some(regex::Regex::new(p).map_err(|e| {
                ToolError::ValidationFailed(format!("Invalid 'pattern' regex: {e}"))
            })?),
            None => None,
        };

        let session = self.store.get(session_id).ok_or_else(|| {
            ToolError::NotFound(format!("No background shell session '{session_id}'"))
        })?;

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut matched_at: Option<Instant> = None;
        let mut last_growth = Instant::now();
        let mut last_len = 0usize;
        loop {
            // Background handoff races the blocking wait: once another call
            // moves the session to the background, return the background
            // branch immediately instead of waiting out the deadline.
            if wf_shell::engine::background_task_registry().is_backgrounded(session_id) {
                let snapshot = session.snapshot();
                let output = snapshot
                    .get("output")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let status = snapshot
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let exit_code = snapshot.get("exit_code").cloned().unwrap_or(Value::Null);
                let capped = capped_output(&session, &output, max_output_bytes);
                let mut response = serde_json::json!({
                    "session_id": session_id,
                    "status": status,
                    "exit_code": exit_code,
                    "output": capped.text.clone(),
                    "output_truncated": capped.truncated,
                    "timed_out": false,
                    "matched": false,
                    "backgrounded": true,
                    "guidance": "Command moved to the background. Do not wait, sleep or poll until its completion event arrives; then read the result with shell_output or shell_tasks.",
                });
                attach_session_pointer(&mut response, session_id, &capped);
                return Ok(response);
            }
            let snapshot = session.snapshot();
            let output = snapshot
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let status = snapshot
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let exit_code = snapshot.get("exit_code").cloned().unwrap_or(Value::Null);

            if output.len() != last_len {
                last_len = output.len();
                last_growth = Instant::now();
            }
            let capped = capped_output(&session, &output, max_output_bytes);

            if let Some(ref re) = matcher {
                if let Some(hit) = output.lines().find(|line| re.is_match(line)) {
                    let matched_line = hit.to_string();
                    if settle_ms == 0 {
                        let mut response = serde_json::json!({
                            "session_id": session_id,
                            "status": status,
                            "exit_code": exit_code,
                            "output": capped.text.clone(),
                            "output_truncated": capped.truncated,
                            "timed_out": false,
                            "matched": true,
                            "matched_line": matched_line,
                            "backgrounded": false,
                        });
                        attach_session_pointer(&mut response, session_id, &capped);
                        return Ok(response);
                    }
                    let first = *matched_at.get_or_insert_with(Instant::now);
                    let quiet = last_growth.elapsed() >= Duration::from_millis(settle_ms);
                    let starved = first.elapsed()
                        >= Duration::from_millis(settle_ms.saturating_mul(5).max(settle_ms + 5000));
                    if quiet || starved {
                        let mut response = serde_json::json!({
                            "session_id": session_id,
                            "status": status,
                            "exit_code": exit_code,
                            "output": capped.text.clone(),
                            "output_truncated": capped.truncated,
                            "timed_out": false,
                            "matched": true,
                            "matched_line": matched_line,
                            "backgrounded": false,
                        });
                        attach_session_pointer(&mut response, session_id, &capped);
                        return Ok(response);
                    }
                } else {
                    matched_at = None;
                }
            } else if status != "busy" {
                let mut response = serde_json::json!({
                    "session_id": session_id,
                    "status": status,
                    "exit_code": exit_code,
                    "output": capped.text.clone(),
                    "output_truncated": capped.truncated,
                    "timed_out": false,
                    "matched": false,
                    "backgrounded": false,
                });
                attach_session_pointer(&mut response, session_id, &capped);
                return Ok(response);
            }

            if Instant::now() >= deadline {
                let mut response = serde_json::json!({
                    "session_id": session_id,
                    "status": status,
                    "exit_code": exit_code,
                    "output": capped.text.clone(),
                    "output_truncated": capped.truncated,
                    "timed_out": true,
                    "matched": false,
                    "backgrounded": false,
                });
                attach_session_pointer(&mut response, session_id, &capped);
                return Ok(response);
            }
            if status != "busy" {
                let mut response = serde_json::json!({
                    "session_id": session_id,
                    "status": status,
                    "exit_code": exit_code,
                    "output": capped.text.clone(),
                    "output_truncated": capped.truncated,
                    "timed_out": false,
                    "matched": false,
                    "backgrounded": false,
                });
                attach_session_pointer(&mut response, session_id, &capped);
                return Ok(response);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Capped view of the session output. The full stream stays in the session;
/// truncation only narrows the returned window.
struct CappedOutput {
    text: String,
    truncated: bool,
    window_start: Option<usize>,
    window_end: Option<usize>,
    total_written: Option<usize>,
    prefix_dropped: bool,
}

/// Cap the session output at `max_bytes` tail bytes through the session
/// window protocol, so the returned cursor stays absolute and recoverable
/// via shell_output. `None` keeps the historical full-output behavior.
fn capped_output(
    session: &TerminalSession,
    output: &str,
    max_bytes: Option<usize>,
) -> CappedOutput {
    let empty = CappedOutput {
        text: output.to_string(),
        truncated: false,
        window_start: None,
        window_end: None,
        total_written: None,
        prefix_dropped: false,
    };
    let Some(cap) = max_bytes else {
        return empty;
    };
    let total = session
        .output_window(0, 0)
        .get("total_written")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(output.len());
    let window = session.output_window(total.saturating_sub(cap), cap);
    let text = window
        .get("output")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let start = window
        .get("window_start")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(0);
    let end = window
        .get("window_end")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(total);
    CappedOutput {
        text,
        truncated: start > 0,
        window_start: Some(start),
        window_end: Some(end),
        total_written: Some(total),
        prefix_dropped: window
            .get("prefix_dropped")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

/// Attach the session recovery cursor to a truncated wait response: the
/// earlier output is paged through shell_output, not lost.
fn attach_session_pointer(response: &mut Value, session_id: &str, capped: &CappedOutput) {
    if !capped.truncated {
        return;
    }
    let (Some(start), Some(end), Some(total)) =
        (capped.window_start, capped.window_end, capped.total_written)
    else {
        return;
    };
    response["output_window_start"] = start.into();
    response["output_window_end"] = end.into();
    response["output_total_written"] = total.into();
    response["output_prefix_dropped"] = capped.prefix_dropped.into();
    response["output_pointer"] = serde_json::json!({
        "tool": "shell_output",
        "session_id": session_id,
        "start": 0,
        "window_end": end,
    });
}

/// Register the shell_wait stateful factory into the registry.
pub fn register(registry: &ToolRegistry, store: &Arc<BackgroundShellStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "shell_wait",
        Arc::new(move |_execution_id| {
            Box::new(ShellWaitInstance {
                store: store.clone(),
            })
        }),
    );
    Ok(())
}
