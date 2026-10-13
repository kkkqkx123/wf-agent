//! Server-side interaction answering for headless runs.
//!
//! The engine runs on the server and surfaces tool approvals and follow-up
//! questions as pending interaction records. This module polls
//! `GET /api/v1/interactions/by-status/pending` while a run is in flight
//! and answers from stdin (`--interactive`), from the prefix whitelist, or
//! blindly (`--assume-yes`). Without any answering mode the run proceeds
//! under the server host default and follow-ups fail the run afterwards.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use wf_client::WfClient;

use crate::error::CliResult;
use crate::run_diag::DiagWriter;
use crate::stdio_prompt::{
    extract_followup_options, extract_followup_prompt, parse_approval_answer, parse_followup_answer,
    render_approval_prompt, render_followup_prompt, StdioPromptSource,
};

/// How pending interactions are answered during a run.
#[derive(Debug, Clone)]
pub struct AnswerPolicy {
    /// Read answers from stdin.
    pub interactive: bool,
    /// Approve every tool approval without prompting.
    pub assume_yes: bool,
    /// Tool name prefixes pre-authorized without prompting.
    pub approve_prefixes: Vec<String>,
    /// Wait bound for one stdin answer line.
    pub approval_timeout: Duration,
    /// JSON prompt protocol (matches `--output json*`).
    pub json: bool,
}

impl AnswerPolicy {
    pub fn needs_poller(&self) -> bool {
        self.interactive || self.assume_yes || !self.approve_prefixes.is_empty()
    }
}

/// Polls pending interactions and answers them per policy until stopped.
pub(crate) struct InteractionPoller {
    client: WfClient,
    policy: AnswerPolicy,
    prompt: Option<Arc<StdioPromptSource>>,
    diag: Arc<Mutex<DiagWriter>>,
    followup_requested: Arc<AtomicBool>,
    seen: HashSet<String>,
}

impl InteractionPoller {
    pub(crate) fn new(
        client: &WfClient,
        policy: AnswerPolicy,
        prompt: Option<Arc<StdioPromptSource>>,
        diag: Arc<Mutex<DiagWriter>>,
        followup_requested: Arc<AtomicBool>,
    ) -> Self {
        Self {
            client: client.clone(),
            policy,
            prompt,
            diag,
            followup_requested,
            seen: HashSet::new(),
        }
    }

    /// Run until `stop` flips. Each unseen pending interaction is answered
    /// exactly once; already-answered races resolve as no-ops server-side.
    pub(crate) async fn run_until(&mut self, stop: Arc<AtomicBool>) {
        while !stop.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let pending: Value = match self
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
                if id.is_empty() || !self.seen.insert(id.clone()) {
                    continue;
                }
                self.answer_one(&id, &item).await;
            }
        }
    }

    async fn answer_one(&self, id: &str, item: &Value) {
        let kind = item
            .get("interaction_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if kind == "tool_approval" {
            self.answer_approval(id, item).await;
        } else {
            self.answer_followup(id, item).await;
        }
    }

    async fn answer_approval(&self, id: &str, item: &Value) {
        let request = item.get("request_data").cloned().unwrap_or(Value::Null);
        let tool_name = request
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("tool");
        let tool_call_id = request
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or(id);
        if self.policy.assume_yes
            || self
                .policy
                .approve_prefixes
                .iter()
                .any(|p| tool_name.starts_with(p.as_str()))
        {
            {
                let mut diag = wf_common::lock::lock_ok(self.diag.lock());
                let _ = diag.ok(&format!("▲ {tool_name} (pre-authorized)"));
            }
            let _ = self.respond(id, serde_json::json!({"approved": true})).await;
            return;
        }
        if !self.policy.interactive {
            return;
        }
        let line = render_approval_prompt(tool_call_id, tool_name, "policy ask", self.policy.json);
        {
            let mut diag = wf_common::lock::lock_ok(self.diag.lock());
            let _ = diag.line(&line);
        }
        let approved = match &self.prompt {
            Some(prompt) => prompt
                .next_answer(self.policy.approval_timeout)
                .await
                .is_some_and(|text| parse_approval_answer(&text)),
            None => false,
        };
        {
            let mut diag = wf_common::lock::lock_ok(self.diag.lock());
            if approved {
                let _ = diag.ok(&format!("▲ {tool_name} (approved by user)"));
            } else {
                let _ = diag.err(&format!("✗ {tool_name} denied"));
            }
        }
        let body = if approved {
            serde_json::json!({"approved": true})
        } else {
            serde_json::json!({"approved": false, "rejection_reason": "denied from stdin"})
        };
        let _ = self.respond(id, body).await;
    }

    async fn answer_followup(&self, id: &str, item: &Value) {
        let request = item.get("request_data").cloned().unwrap_or(Value::Null);
        if !self.policy.interactive {
            self.followup_requested.store(true, Ordering::SeqCst);
            let mut diag = wf_common::lock::lock_ok(self.diag.lock());
            let _ = diag.warn(
                "follow-up question requested; headless mode cannot answer it \
                 (re-run with --interactive to answer from stdin)",
            );
            return;
        }
        let prompt_text = extract_followup_prompt(&request);
        let options = extract_followup_options(&request);
        let line = render_followup_prompt(id, &prompt_text, &options, self.policy.json);
        {
            let mut diag = wf_common::lock::lock_ok(self.diag.lock());
            let _ = diag.line(&line);
        }
        let response = match &self.prompt {
            Some(prompt) => match prompt.next_answer(self.policy.approval_timeout).await {
                Some(text) => parse_followup_answer(&text, &request),
                None => {
                    let mut diag = wf_common::lock::lock_ok(self.diag.lock());
                    let _ = diag.warn(&format!(
                        "follow-up answer timed out after {}s; responding cancelled",
                        self.policy.approval_timeout.as_secs()
                    ));
                    Value::Null
                }
            },
            None => Value::Null,
        };
        let _ = self.respond(id, response).await;
    }

    async fn respond(&self, id: &str, response: Value) -> CliResult<()> {
        // Named request body; response stays passthrough JSON.
        let body = wf_client::models::RespondBody {
            response_data: Some(response),
            result_data: None,
        };
        self.client
            .generated()
            .post_interactions_id_respond(id, &body)
            .await
            .map_err(wf_client::client::into_typed_client_error)
            .and_then(|e| {
                wf_client::client::unwrap_generated_envelope::<_, Value>(e.into_inner())
            })
            .map(|_| ())
            .map_err(crate::error::CliError::from)
    }
}
