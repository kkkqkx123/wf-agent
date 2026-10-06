//! Headless follow-up interaction guard (stdin answers).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use crate::run_diag::DiagWriter;
use crate::stdio_prompt::{
    extract_followup_options, extract_followup_prompt, extract_interaction_id,
    parse_followup_answer, render_followup_prompt, StdioPromptSource,
};
use wf_api::entity::user_interaction::{AgentUserInteractionEventRecord, UserInteractionHandler};
use wf_api::infra::context::ApiContext;

// ── interaction guard (follow-up questions) ──────────────────────────

/// Headless follow-up handling. Without `--interactive` a follow-up request
/// cannot be answered, so the driver records it and fails the run with exit
/// code 1 afterwards. With `--interactive` the request renders a `? ANSWER`
/// prompt to the diagnostics channel and a spawned task delivers one stdin
/// line through the persisted interaction API.
pub(crate) struct HeadlessInteractionGuard {
    followup_requested: Arc<AtomicBool>,
    diag: Arc<Mutex<DiagWriter>>,
    followup_answer: Option<FollowupAnswer>,
}

/// Interactive follow-up delivery: prompt source, owning API context for the
/// persisted respond path, answer timeout and prompt protocol.
#[derive(Clone)]
pub(crate) struct FollowupAnswer {
    prompt: Arc<StdioPromptSource>,
    ctx: Arc<ApiContext>,
    followup_requested: Arc<AtomicBool>,
    timeout: Duration,
    json: bool,
}

impl FollowupAnswer {
    async fn answer(&self, request: &Value, diag: &Arc<Mutex<DiagWriter>>) {
        let interaction_id = extract_interaction_id(request);
        if interaction_id.is_empty() {
            self.followup_requested.store(true, Ordering::SeqCst);
            let mut diag = wf_common::lock::lock_ok(diag.lock());
            let _ = diag
                .warn("follow-up question without an interaction id cannot be answered from stdin");
            return;
        }
        let prompt_text = extract_followup_prompt(request);
        let options = extract_followup_options(request);
        let line = render_followup_prompt(&interaction_id, &prompt_text, &options, self.json);
        {
            let mut diag = wf_common::lock::lock_ok(diag.lock());
            let _ = diag.line(&line);
        }
        let response = match self.prompt.next_answer(self.timeout).await {
            Some(text) => parse_followup_answer(&text, request),
            None => {
                let mut diag = wf_common::lock::lock_ok(diag.lock());
                let _ = diag.warn(&format!(
                    "follow-up answer timed out after {}s; responding cancelled",
                    self.timeout.as_secs()
                ));
                Value::Null
            }
        };
        // Storage-backed interactions resolve through the persisted respond
        // path, which also completes the live registry wait. Ephemeral
        // interactions (workflow nodes without a storage record) resolve the
        // live registry wait directly.
        let stored = wf_api::entity::user_interaction::respond_interaction(
            &self.ctx.storage,
            &interaction_id,
            Some(response.clone()),
            None,
        )
        .await;
        if stored.is_err() {
            let _ = wf_api::complete_interaction(&interaction_id, response);
        }
    }
}

impl HeadlessInteractionGuard {
    pub(crate) fn flag_only(
        followup_requested: Arc<AtomicBool>,
        diag: Arc<Mutex<DiagWriter>>,
    ) -> Self {
        Self {
            followup_requested,
            diag,
            followup_answer: None,
        }
    }

    pub(crate) fn interactive(
        followup_requested: Arc<AtomicBool>,
        diag: Arc<Mutex<DiagWriter>>,
        prompt: Arc<StdioPromptSource>,
        ctx: Arc<ApiContext>,
        timeout: Duration,
        json: bool,
    ) -> Self {
        Self {
            followup_requested: followup_requested.clone(),
            diag,
            followup_answer: Some(FollowupAnswer {
                prompt,
                ctx,
                followup_requested,
                timeout,
                json,
            }),
        }
    }
}

impl UserInteractionHandler for HeadlessInteractionGuard {
    fn on_interaction(&self, _record: &AgentUserInteractionEventRecord) {}

    fn on_tool_approval_requested(&self, _execution_id: &str, _request: &Value) {
        // Tool approvals are decided synchronously by the approval handler;
        // nothing to ask the user here.
    }

    fn on_followup_question_requested(&self, _execution_id: &str, request: &Value) {
        match &self.followup_answer {
            Some(answer) => {
                let answer = answer.clone();
                let request = request.clone();
                let diag = self.diag.clone();
                tokio::spawn(async move {
                    answer.answer(&request, &diag).await;
                });
            }
            None => {
                self.followup_requested.store(true, Ordering::SeqCst);
                let mut diag = wf_common::lock::lock_ok(self.diag.lock());
                let _ = diag.warn(
                    "follow-up question requested; headless mode cannot answer it \
                     (re-run with --interactive to answer from stdin)",
                );
            }
        }
    }
}
