//! Server-side interaction answering for headless runs.
//!
//! The engine runs on the server and surfaces tool approvals and follow-up
//! questions as pending interaction records. This module watches the event
//! stream while a run is in flight and reads the pending list only after a
//! relevant bus event or a reconnect, answering from stdin
//! (`--interactive`), from the prefix whitelist, or blindly
//! (`--assume-yes`). Without any answering mode the run proceeds under the
//! server host default and follow-ups fail the run afterwards.

use std::collections::HashSet;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::Stream;
use serde_json::Value;
use wf_client::WfClient;

use crate::error::CliResult;
use crate::run_diag::DiagWriter;
use crate::stdio_prompt::{
    extract_followup_options, extract_followup_prompt, parse_approval_answer,
    parse_followup_answer, render_approval_prompt, render_followup_prompt, StdioPromptSource,
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

/// Boxed SSE frame stream held by [`PendingWatcher`].
type SseFrameStream =
    Pin<Box<dyn Stream<Item = Result<wf_client::SseFrame, wf_client::ClientError>> + Send>>;

/// Reconnect backoff for event-stream consumers: 1s doubling to a 30s
/// cap, plus sub-second jitter from the wall clock so restarted processes
/// do not reconnect in lockstep. No extra dependencies for one timer.
pub fn stream_reconnect_delay(attempt: u32) -> Duration {
    const BASE: Duration = Duration::from_secs(1);
    const MAX: Duration = Duration::from_secs(30);
    let backoff = BASE
        .checked_mul(1 << attempt.min(5))
        .unwrap_or(MAX)
        .min(MAX);
    let jitter = Duration::from_millis(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| u64::from(elapsed.subsec_nanos() % 500))
            .unwrap_or(0),
    );
    backoff.saturating_add(jitter)
}

/// Outcome of one watcher step.
pub enum PendingWatchStep {
    /// The stop predicate held; the caller exits its loop.
    Stopped,
    /// Unseen pending records fetched after a wake trigger.
    Ready(Vec<Value>),
}

/// Event-driven pending-interaction watcher shared by headless runs, the
/// mini approver and the TUI turn task.
///
/// The watcher holds one `GET /api/v1/events/stream` subscription and only
/// reads the pending list after a relevant bus event or a (re)connect.
/// There is no fixed polling tick: without a stream the loop backs off
/// between reconnect attempts, and the stop predicate runs on every wakeup
/// so shutdown never waits on the network. Each record surfaces at most
/// once per watcher lifetime; already-answered races resolve as no-ops
/// server-side.
pub struct PendingWatcher {
    client: WfClient,
    stream: Option<SseFrameStream>,
    reconnect_attempt: u32,
    fetch_due: bool,
    seen: HashSet<String>,
}

impl PendingWatcher {
    /// Open the event subscription; a failed open leaves the watcher
    /// disconnected and the first step reconnects with backoff.
    pub async fn new(client: &WfClient) -> Self {
        let stream = client
            .get_stream(wf_client::endpoints::EVENTS_STREAM)
            .await
            .ok()
            .map(|stream| Box::pin(stream) as _);
        Self {
            client: client.clone(),
            stream,
            reconnect_attempt: 0,
            fetch_due: true,
            seen: HashSet::new(),
        }
    }

    /// Wait for the next trigger (relevant bus event or reconnect), then
    /// read the pending list once and return records not seen before. The
    /// stop predicate is checked on every wakeup and between reconnect
    /// backoff slices.
    pub async fn next_batch(&mut self, is_stopped: impl Fn() -> bool) -> PendingWatchStep {
        use futures::StreamExt;
        loop {
            if is_stopped() {
                return PendingWatchStep::Stopped;
            }
            if self.stream.is_none() {
                if sleep_backoff(stream_reconnect_delay(self.reconnect_attempt), &is_stopped)
                    .await
                {
                    return PendingWatchStep::Stopped;
                }
                match self
                    .client
                    .get_stream(wf_client::endpoints::EVENTS_STREAM)
                    .await
                {
                    Ok(stream) => {
                        self.stream = Some(Box::pin(stream));
                        self.reconnect_attempt = 0;
                        self.fetch_due = true;
                    }
                    Err(_) => {
                        self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
                        continue;
                    }
                }
            }
            if !self.fetch_due {
                let frame = match self.stream.as_mut() {
                    Some(stream) => stream.next().await,
                    None => None,
                };
                match frame {
                    Some(Ok(frame)) => {
                        if !is_interaction_request_event(&frame) {
                            continue;
                        }
                    }
                    Some(Err(_)) | None => {
                        self.stream = None;
                        self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
                        continue;
                    }
                }
            }
            self.fetch_due = false;
            match self
                .client
                .get_json("/api/v1/interactions/by-status/pending")
                .await
            {
                Ok(pending) => return PendingWatchStep::Ready(unseen_items(&pending, &mut self.seen)),
                Err(_) => continue,
            }
        }
    }
}

/// Sleep in one-second slices so a long backoff still notices shutdown
/// promptly. Returns true when the stop predicate held mid-sleep.
async fn sleep_backoff(total: Duration, is_stopped: &impl Fn() -> bool) -> bool {
    let mut remaining: u64 = total.as_millis().try_into().unwrap_or(u64::MAX).max(1);
    while remaining > 0 {
        let slice = remaining.min(1_000);
        tokio::time::sleep(Duration::from_millis(slice)).await;
        if is_stopped() {
            return true;
        }
        remaining -= slice;
    }
    false
}

/// Extract pending records the watcher has not surfaced yet.
fn unseen_items(pending: &Value, seen: &mut HashSet<String>) -> Vec<Value> {
    pending
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| pending.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            !id.is_empty() && seen.insert(id.to_string())
        })
        .collect()
}

/// True for bus events that announce a new approval or follow-up.
/// Pollers use this to wake early on the event stream; other frames stay on
/// the periodic fallback so noisy streams cannot cause poll storms.
pub fn is_interaction_request_event(frame: &wf_client::SseFrame) -> bool {
    let decoded = wf_client::sse::decode_system_frame(frame);
    match decoded.into_base_event() {
        Some(base) => matches!(
            base.r#type,
            wf_types::events::EventType::ToolApprovalRequested
                | wf_types::events::EventType::FollowupQuestionRequested
        ),
        None => false,
    }
}

/// Polls pending interactions and answers them per policy until stopped.
pub(crate) struct InteractionPoller {
    client: WfClient,
    policy: AnswerPolicy,
    prompt: Option<Arc<StdioPromptSource>>,
    diag: Arc<Mutex<DiagWriter>>,
    followup_requested: Arc<AtomicBool>,
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
        }
    }

    /// Run until `stop` flips. Each unseen pending interaction is answered
    /// exactly once; already-answered races resolve as no-ops server-side.
    /// The shared event-driven watcher only reads the pending list after a
    /// relevant bus event or a reconnect, with backoff while the stream is
    /// down.
    pub(crate) async fn run_until(&mut self, stop: Arc<AtomicBool>) {
        let mut watcher = PendingWatcher::new(&self.client).await;
        loop {
            match watcher.next_batch(|| stop.load(Ordering::SeqCst)).await {
                PendingWatchStep::Stopped => break,
                PendingWatchStep::Ready(items) => {
                    for item in items {
                        let id = item
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        self.answer_one(&id, &item).await;
                    }
                }
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
            let _ = self
                .respond(id, serde_json::json!({"approved": true}))
                .await;
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
            .and_then(|e| wf_client::client::unwrap_generated_envelope::<_, Value>(e.into_inner()))
            .map(|_| ())
            .map_err(crate::error::CliError::from)
    }
}
