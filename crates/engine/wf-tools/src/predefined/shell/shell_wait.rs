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

pub static SHELL_WAIT: ToolDefinition = ToolDefinition {
    id: "shell_wait",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "shell",
    tags: &["wait"],
    description: "Wait for a background shell session command to finish or for its output to match a pattern. Returns the current status, exit code and full output; 'timed_out' is true when the deadline elapsed first. Provide 'pattern' (a regex) to stop early once matching output appears.",
    parameters: &[
        ToolParameter { name: "session_id", r#type: "string", required: true, description: "The session ID returned by backend_shell", default_json: None, constraints: None },
        ToolParameter { name: "timeout", r#type: "number", required: false, description: "Timeout in milliseconds (default 30000)", default_json: Some("30000"), constraints: None },
        ToolParameter { name: "pattern", r#type: "string", required: false, description: "Regex; stop waiting once output contains a match", default_json: None, constraints: None },
        ToolParameter { name: "settle_ms", r#type: "number", required: false, description: "After a pattern match, keep polling until output is quiet for this long before returning (debounces output jitter; default 0)", default_json: Some("0"), constraints: None },
        ToolParameter { name: "max_output_bytes", r#type: "number", required: false, description: "Cap the returned output at this many bytes (tail-kept); sets 'output_truncated' when cut", default_json: None, constraints: None },
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
        let timeout_ms = params
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30_000);
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
            let (capped_output, output_truncated) = cap_response_output(&output, max_output_bytes);

            if let Some(ref re) = matcher {
                if let Some(hit) = output.lines().find(|line| re.is_match(line)) {
                    let matched_line = hit.to_string();
                    if settle_ms == 0 {
                        return Ok(serde_json::json!({
                            "session_id": session_id,
                            "status": status,
                            "exit_code": exit_code,
                            "output": capped_output,
                            "output_truncated": output_truncated,
                            "timed_out": false,
                            "matched": true,
                            "matched_line": matched_line,
                        }));
                    }
                    let first = *matched_at.get_or_insert_with(Instant::now);
                    let quiet = last_growth.elapsed() >= Duration::from_millis(settle_ms);
                    let starved = first.elapsed()
                        >= Duration::from_millis(settle_ms.saturating_mul(5).max(settle_ms + 5000));
                    if quiet || starved {
                        return Ok(serde_json::json!({
                            "session_id": session_id,
                            "status": status,
                            "exit_code": exit_code,
                            "output": capped_output,
                            "output_truncated": output_truncated,
                            "timed_out": false,
                            "matched": true,
                            "matched_line": matched_line,
                        }));
                    }
                } else {
                    matched_at = None;
                }
            } else if status != "busy" {
                return Ok(serde_json::json!({
                    "session_id": session_id,
                    "status": status,
                    "exit_code": exit_code,
                    "output": capped_output,
                    "output_truncated": output_truncated,
                    "timed_out": false,
                    "matched": false,
                }));
            }

            if Instant::now() >= deadline {
                return Ok(serde_json::json!({
                    "session_id": session_id,
                    "status": status,
                    "exit_code": exit_code,
                    "output": capped_output,
                    "output_truncated": output_truncated,
                    "timed_out": true,
                    "matched": false,
                }));
            }
            if status != "busy" {
                return Ok(serde_json::json!({
                    "session_id": session_id,
                    "status": status,
                    "exit_code": exit_code,
                    "output": capped_output,
                    "output_truncated": output_truncated,
                    "timed_out": false,
                    "matched": false,
                }));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Cap a tool response payload at `max_bytes` tail bytes. `None` keeps the
/// historical full-output behavior.
fn cap_response_output(output: &str, max_bytes: Option<usize>) -> (String, bool) {
    match max_bytes {
        Some(cap) => {
            let (kept, truncated) = wf_script::truncate_tail(output, cap);
            (kept, truncated)
        }
        None => (output.to_string(), false),
    }
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
