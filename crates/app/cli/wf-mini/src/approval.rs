//! Prompt-time approval for the native session: pending server
//! interactions are answered from the shared stdin reader.
//!
//! The engine runs on the server and surfaces tool approvals as pending
//! interaction records. The session spawns one [`MiniApprover`] per turn;
//! it polls `by-status/pending`, prompts `Allow tool 'x'? [y/N]` on stderr
//! for approvals concerning this turn, and posts the answer. Anything else
//! (follow-ups, foreign turns) is left for the turn to finish: the server
//! host default decides unanswered approvals.

use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

use wf_client::WfClient;

use crossterm::style::{Attribute, Stylize};

use crate::input::LineReader;
use crate::output::diag_line;

/// Approval prompter for one mini turn. Answers come from the shared stdin
/// reader (the only stdin reader in the process), so a timed-out answer
/// leaves its late lines in the channel where the session drains them
/// instead of misreading them as the next prompt.
pub struct MiniApprover {
    client: WfClient,
    lines: Arc<tokio::sync::Mutex<LineReader>>,
    timeout_secs: u64,
    /// Turn-cancel flag shared with the session pump. The approver sets it
    /// when the user presses Ctrl-C during a prompt; the pump treats the
    /// flag as an interrupt so the whole turn stops instead of re-prompting
    /// for the next sensitive tool.
    cancel: tokio::sync::watch::Sender<bool>,
    stderr_color: bool,
}

impl MiniApprover {
    pub fn new(
        client: &WfClient,
        timeout_secs: u64,
        lines: Arc<tokio::sync::Mutex<LineReader>>,
        cancel: tokio::sync::watch::Sender<bool>,
    ) -> Self {
        Self {
            client: client.clone(),
            lines,
            timeout_secs: timeout_secs.max(1),
            cancel,
            stderr_color: false,
        }
    }

    /// Same as [`Self::new`] with explicit stderr styling for TTY prompts.
    pub fn with_stderr_color(mut self, stderr_color: bool) -> Self {
        self.stderr_color = stderr_color;
        self
    }

    /// Poll pending interactions until `stop` flips, answering tool
    /// approvals from stdin. Each record answers at most once.
    pub async fn watch(&self, stop: Arc<tokio::sync::watch::Receiver<bool>>) {
        // Owned receiver: `changed()` needs `&mut self`, which an `Arc`
        // cannot provide. Cloning keeps the same watch channel.
        let mut stop = (*stop).clone();
        let mut seen = std::collections::HashSet::new();
        loop {
            if *stop.borrow() {
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                _ = stop.changed() => break,
            }
            let pending: serde_json::Value = match self
                .client
                .get_json("/api/v1/interactions/by-status/pending")
                .await
            {
                Ok(value) => value,
                Err(_) => continue,
            };
            let items = pending
                .get("items")
                .and_then(|v| v.as_array())
                .cloned()
                .or_else(|| pending.as_array().cloned())
                .unwrap_or_default();
            for item in items {
                let id = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                if id.is_empty() || !seen.insert(id.clone()) {
                    continue;
                }
                if item.get("interaction_type").and_then(|v| v.as_str()) == Some("tool_approval") {
                    self.answer_approval(&id, &item).await;
                }
                if *stop.borrow() {
                    break;
                }
            }
        }
    }

    async fn answer_approval(&self, id: &str, item: &serde_json::Value) {
        let request = item.get("request_data").cloned().unwrap_or_default();
        let tool_name = request
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("tool");
        let risk = request
            .get("risk_level")
            .and_then(|v| v.as_str())
            .map(|r| format!(" [{r}]"))
            .unwrap_or_default();
        {
            let stderr = io::stderr();
            let mut err = stderr.lock();
            let prompt = format!(
                "Allow tool '{tool_name}'{risk}? [y/N] (default N, {}s timeout)",
                self.timeout_secs
            );
            let prompt = if self.stderr_color {
                prompt.attribute(Attribute::Bold).to_string()
            } else {
                prompt
            };
            let _ = writeln!(err, "{prompt}");
            let _ = err.flush();
        }
        let answer = tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                let _ = self.cancel.send(true);
                None
            }
            result = self.read_answer_line() => result,
        };
        let approved = answer.is_some_and(|line| is_affirmative(&line));
        if approved {
            diag_line(&format!("allowed {tool_name}"));
        } else {
            diag_line(&format!("denied {tool_name}"));
        }
        let body = if approved {
            serde_json::json!({"response_data": {"approved": true}})
        } else {
            serde_json::json!({"response_data": {"approved": false, "rejection_reason": "denied by user"}})
        };
        let _ = self
            .client
            .post_json::<_, serde_json::Value>(&format!("/api/v1/interactions/{id}/respond"), &body)
            .await;
    }

    /// Take the first channel line as the answer. The timeout wraps the
    /// channel wait (not a raw stdin read), so cancelling it leaves nothing
    /// behind in the terminal: any late line stays queued for the session
    /// drain.
    async fn read_answer_line(&self) -> Option<String> {
        tokio::time::timeout(Duration::from_secs(self.timeout_secs), async {
            self.lines.lock().await.next_line().await
        })
        .await
        .ok()
        .flatten()
    }
}

/// Accept common affirmative answers; everything else denies.
fn is_affirmative(answer: &str) -> bool {
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_yes_variants() {
        assert!(is_affirmative("y"));
        assert!(is_affirmative("Y"));
        assert!(is_affirmative("yes\n"));
        assert!(!is_affirmative(""));
        assert!(!is_affirmative("n"));
        assert!(!is_affirmative("yolo"));
    }
}
