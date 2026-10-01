//! Line-based stdin answer pump and stderr prompt rendering for headless runs.
//!
//! Headless sessions keep stdout as pure business output, so every question
//! is rendered to the diagnostics channel (stderr) and answered with one
//! stdin line. [`StdioPromptSource`] owns the stdin reader task: it pumps
//! lines into a buffered channel regardless of engine state, so a cached
//! answer line never blocks the execution event stream. Each prompt consumes
//! exactly one line; concurrent prompts share the line queue in order.
//!
//! Protocol (stderr, human readable by default):
//! `? APPROVE tool_call_id=<id> tool=<name> reason="<why>" (y/n)` and
//! `? ANSWER interaction_id=<id> prompt="<text>" [1] <opt> ...`.
//! With a structured output format the same prompts render as NDJSON
//! (`{"type":"approve",...}` / `{"type":"answer",...}`) for scripted peers.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

/// Default wait for one stdin answer line before failing closed.
pub const DEFAULT_APPROVAL_TIMEOUT_SECS: u64 = 120;

/// Shared stdin line source. Cheap to clone through `Arc`; every prompt
/// holder keeps the same instance so lines stay in arrival order.
pub struct StdioPromptSource {
    rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<String>>,
}

impl StdioPromptSource {
    /// Spawn the blocking stdin line reader; each line lands on the channel.
    /// Dropping the returned handle never cancels the reader task.
    pub fn spawn() -> (Arc<Self>, tokio::task::JoinHandle<()>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let handle = tokio::task::spawn_blocking(move || {
            let stdin = std::io::stdin();
            for line in std::io::BufRead::lines(stdin.lock()) {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        (
            Arc::new(Self {
                rx: tokio::sync::Mutex::new(rx),
            }),
            handle,
        )
    }

    /// Preloaded source for tests: lines drain in order, then EOF.
    pub fn new_for_test(lines: Vec<String>) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        for line in lines {
            let _ = tx.send(line);
        }
        Self {
            rx: tokio::sync::Mutex::new(rx),
        }
    }

    /// Await the next user answer line. `None` means timeout, EOF or a
    /// closed reader; callers fail closed on `None`.
    pub async fn next_answer(&self, timeout: Duration) -> Option<String> {
        let mut rx = self.rx.lock().await;
        tokio::time::timeout(timeout, rx.recv())
            .await
            .unwrap_or_default()
    }
}

/// Whether a stdin answer approves a tool call. Only an explicit `y`/`yes`
/// (any case, surrounding whitespace allowed) approves; everything else,
/// including an empty line, denies.
pub fn parse_approval_answer(text: &str) -> bool {
    matches!(text.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Render an approval prompt. Text follows the `? APPROVE ... (y/n)` line
/// protocol; structured mode emits one NDJSON object.
pub fn render_approval_prompt(
    tool_call_id: &str,
    tool_name: &str,
    reason: &str,
    json: bool,
) -> String {
    if json {
        serde_json::json!({
            "type": "approve",
            "tool_call_id": tool_call_id,
            "tool": tool_name,
            "reason": reason,
        })
        .to_string()
    } else {
        format!("? APPROVE tool_call_id={tool_call_id} tool={tool_name} reason=\"{reason}\" (y/n)")
    }
}

/// Interaction id carried by a follow-up request payload. Accepts the
/// camelCase, snake_case and bare shapes; empty when the producer sent none.
pub fn extract_interaction_id(request: &Value) -> String {
    request
        .get("interactionId")
        .or_else(|| request.get("interaction_id"))
        .or_else(|| request.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Question text of a follow-up request; degrades to a generic prompt.
pub fn extract_followup_prompt(request: &Value) -> String {
    request
        .get("prompt")
        .or_else(|| request.get("question"))
        .or_else(|| request.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("follow-up question")
        .to_string()
}

/// Offered options of a follow-up request. Accepts plain strings and
/// label/text/value objects from both the `options` and `choices` shapes.
pub fn extract_followup_options(request: &Value) -> Vec<String> {
    let raw = request
        .get("options")
        .or_else(|| request.get("choices"))
        .cloned()
        .unwrap_or(Value::Array(Vec::new()));
    match raw {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| match item {
                Value::String(label) => Some(label),
                Value::Object(map) => map
                    .get("label")
                    .or_else(|| map.get("text"))
                    .or_else(|| map.get("value"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Map one stdin answer line onto the follow-up response payload: a 1-based
/// option number or a case-insensitive option label resolves to that option,
/// any other text is a free-form answer, and an empty line cancels (`Null`).
pub fn parse_followup_answer(text: &str, request: &Value) -> Value {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Value::Null;
    }
    let options = extract_followup_options(request);
    if !options.is_empty() {
        if let Ok(n) = trimmed.parse::<usize>() {
            if n >= 1 && n <= options.len() {
                return Value::String(options[n - 1].clone());
            }
        }
        if let Some(label) = options
            .iter()
            .find(|label| label.eq_ignore_ascii_case(trimmed))
        {
            return Value::String(label.clone());
        }
    }
    Value::String(trimmed.to_string())
}

/// Render a follow-up prompt. Text follows the `? ANSWER ...` line protocol
/// with numbered options; structured mode emits one NDJSON object.
pub fn render_followup_prompt(
    interaction_id: &str,
    prompt: &str,
    options: &[String],
    json: bool,
) -> String {
    if json {
        return serde_json::json!({
            "type": "answer",
            "interaction_id": interaction_id,
            "prompt": prompt,
            "options": options,
        })
        .to_string();
    }
    let mut line = format!("? ANSWER interaction_id={interaction_id} prompt=\"{prompt}\"");
    for (index, option) in options.iter().enumerate() {
        line.push_str(&format!(" [{}] {option}", index + 1));
    }
    line.push_str(" (reply text or option number)");
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn approval_answer_needs_explicit_yes() {
        assert!(parse_approval_answer("y"));
        assert!(parse_approval_answer("Y"));
        assert!(parse_approval_answer("  yes  "));
        assert!(!parse_approval_answer("n"));
        assert!(!parse_approval_answer("no"));
        assert!(!parse_approval_answer(""));
        assert!(!parse_approval_answer("yeah"));
    }

    #[test]
    fn approval_prompt_renders_both_protocols() {
        let text = render_approval_prompt("b3f2", "bash", "policy ask", false);
        assert_eq!(
            text,
            "? APPROVE tool_call_id=b3f2 tool=bash reason=\"policy ask\" (y/n)"
        );
        let parsed: Value =
            serde_json::from_str(&render_approval_prompt("b3f2", "bash", "policy ask", true))
                .unwrap();
        assert_eq!(parsed["type"], "approve");
        assert_eq!(parsed["tool_call_id"], "b3f2");
        assert_eq!(parsed["tool"], "bash");
    }

    #[test]
    fn followup_request_shapes_parse() {
        let request = json!({
            "interactionId": "ui-2",
            "prompt": "pick one",
            "options": ["red", {"label": "green"}, {"value": "blue"}, 42],
        });
        assert_eq!(extract_interaction_id(&request), "ui-2");
        assert_eq!(extract_followup_prompt(&request), "pick one");
        assert_eq!(
            extract_followup_options(&request),
            vec!["red", "green", "blue"]
        );

        let workflow = json!({"question": "continue?", "choices": ["yes", "no"]});
        assert_eq!(extract_followup_prompt(&workflow), "continue?");
        assert_eq!(extract_followup_options(&workflow), vec!["yes", "no"]);

        let bare = json!({});
        assert_eq!(extract_interaction_id(&bare), "");
        assert_eq!(extract_followup_prompt(&bare), "follow-up question");
        assert!(extract_followup_options(&bare).is_empty());
    }

    #[test]
    fn followup_answer_maps_numbers_labels_and_text() {
        let request = json!({"options": ["red", "green"]});
        assert_eq!(
            parse_followup_answer("2", &request),
            Value::String("green".into())
        );
        assert_eq!(
            parse_followup_answer("RED", &request),
            Value::String("red".into())
        );
        assert_eq!(
            parse_followup_answer("custom path", &request),
            Value::String("custom path".into())
        );
        assert_eq!(parse_followup_answer("", &request), Value::Null);
        assert_eq!(
            parse_followup_answer("9", &request),
            Value::String("9".into())
        );
    }

    #[test]
    fn followup_prompt_renders_both_protocols() {
        let text = render_followup_prompt("ui-2", "pick one", &["red".into()], false);
        assert!(text.starts_with("? ANSWER interaction_id=ui-2"), "{text}");
        assert!(text.contains("[1] red"), "{text}");
        let parsed: Value =
            serde_json::from_str(&render_followup_prompt("ui-2", "pick one", &[], true)).unwrap();
        assert_eq!(parsed["type"], "answer");
        assert_eq!(parsed["interaction_id"], "ui-2");
    }

    #[tokio::test]
    async fn prompt_source_drains_lines_then_eof() {
        let source = StdioPromptSource::new_for_test(vec!["y".into(), "hello".into()]);
        assert_eq!(
            source.next_answer(Duration::from_secs(1)).await.as_deref(),
            Some("y")
        );
        assert_eq!(
            source.next_answer(Duration::from_secs(1)).await.as_deref(),
            Some("hello")
        );
        assert_eq!(source.next_answer(Duration::from_millis(20)).await, None);
    }
}
