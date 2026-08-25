//! Headless session driver: `wf run "<prompt>"` end to end.
//!
//! Pipeline: preset the execution id → register the headless interaction
//! guard → stream the agent loop through [`wf_api`] → render the agent
//! events (LLM text to the main sink, tool lifecycle to the diagnostics
//! channel) → terminate with a summary line / envelope and an exit code.
//!
//! Output discipline: business output (LLM text,
//! message records, summary envelope) goes through the [`OutputSink`]
//! (stdout); diagnostics (tool lines `▲/✓/✗`, approval rejections,
//! interrupt notices) go to stderr through [`DiagWriter`], so
//! `wf run | jq` keeps working.

use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::StreamExt;
use serde_json::Value;

use wf_agent::approval::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};
use wf_api::agent::agent_execution::{self, RunAgentLoopParams};
use wf_api::entity::user_interaction::{
    register_handler, AgentUserInteractionEventRecord, UserInteractionHandler,
};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_types::Id;

use crate::domain::DomainAdapter;
use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat, OutputMessage, OutputSink};
use wf_api::infra::stream::ExecutionStreamEvent;

/// Tools that mutate state or execute commands: always denied in headless
/// runs unless covered by an explicit `--approve-prefix`.
const SENSITIVE_TOOLS: &[&str] = &[
    "approve_changes",
    "write_file",
    "edit_file",
    "apply_patch",
    "apply_diff",
    "execute_command",
];

/// Read-only / side-effect-free tools auto-approved in headless runs.
const LOW_RISK_TOOLS: &[&str] = &[
    "read_file",
    "list_files",
    "grep_search",
    "glob_search",
    "update_todo_list",
    "skill",
];

/// Argument keys inspected for command pre-authorization prefixes.
const COMMAND_ARGUMENT_KEYS: &[&str] = &["command", "cmd"];

/// Default LLM profile id when `--model` is absent.
const DEFAULT_MODEL: &str = "default";

/// Default iteration budget for a headless session.
const DEFAULT_MAX_ITERATIONS: u32 = 50;

// ── diagnostics channel (stderr) ─────────────────────────────────────

const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";

/// Diagnostics writer for stderr-bound output (tool lines, rejections,
/// interrupt notices). Every line is captured in an in-memory buffer
/// (snapshot for tests / summaries) and optionally mirrored to stderr for
/// real runs; shared with the approval/interaction callbacks via
/// `Arc<Mutex<...>>`.
pub struct DiagWriter {
    captured: Vec<u8>,
    mirror: Option<Box<dyn Write + Send>>,
    color: bool,
}

impl DiagWriter {
    /// Bound to process stderr; `color` follows the TTY/no-color answer.
    pub fn stderr(color: bool) -> Self {
        Self {
            captured: Vec::new(),
            mirror: Some(Box::new(io::stderr())),
            color,
        }
    }

    /// Bound to an in-memory buffer only (tests).
    pub fn buffer() -> Self {
        Self {
            captured: Vec::new(),
            mirror: None,
            color: false,
        }
    }

    /// Append one diagnostic line (newline terminated, flushed on the
    /// mirror when present).
    pub fn line(&mut self, text: &str) -> io::Result<()> {
        self.captured.extend_from_slice(text.as_bytes());
        self.captured.push(b'\n');
        if let Some(mirror) = self.mirror.as_mut() {
            mirror.write_all(text.as_bytes())?;
            mirror.write_all(b"\n")?;
            mirror.flush()?;
        }
        Ok(())
    }

    /// Accumulated diagnostics so far (lossy UTF-8).
    pub fn snapshot(&self) -> String {
        String::from_utf8_lossy(&self.captured).into_owned()
    }

    fn ok(&mut self, text: &str) -> io::Result<()> {
        if self.color {
            self.line(&format!("{GREEN}{text}{RESET}"))
        } else {
            self.line(text)
        }
    }

    fn err(&mut self, text: &str) -> io::Result<()> {
        if self.color {
            self.line(&format!("{RED}{text}{RESET}"))
        } else {
            self.line(text)
        }
    }

    fn warn(&mut self, text: &str) -> io::Result<()> {
        if self.color {
            self.line(&format!("{YELLOW}{text}{RESET}"))
        } else {
            self.line(text)
        }
    }
}

// ── approval policy (headless degradation) ─────────────────────────

/// Outcome of the headless approval decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    Allow { reason: String },
    Deny { reason: String },
}

/// Pure headless approval policy: sensitive → deny; `--approve-prefix`
/// pre-authorization → allow; low-risk allow-list → allow; else deny.
#[derive(Debug, Clone, Default)]
pub struct ApprovalPolicy {
    approve_prefixes: Vec<String>,
}

impl ApprovalPolicy {
    pub fn new(approve_prefixes: Vec<String>) -> Self {
        Self { approve_prefixes }
    }

    /// Decide whether a tool call may execute in a headless session.
    ///
    /// Precedence: an explicit `--approve-prefix` pre-authorization wins
    /// over everything (it is the user's explicit consent, including for
    /// sensitive tools); then sensitive tools are denied; then the low-risk
    /// allow-list; anything else is denied with a hint.
    pub fn decide(&self, tool_name: &str, arguments: &Value) -> ApprovalDecision {
        if self.prefix_matches(tool_name, arguments) {
            return ApprovalDecision::Allow {
                reason: "pre-authorized by --approve-prefix".to_string(),
            };
        }
        if SENSITIVE_TOOLS.contains(&tool_name) {
            return ApprovalDecision::Deny {
                reason: format!(
                    "sensitive tool '{tool_name}' requires interactive approval; \
                     denied in headless mode"
                ),
            };
        }
        if LOW_RISK_TOOLS.contains(&tool_name) {
            return ApprovalDecision::Allow {
                reason: "low-risk tool allow-listed for headless runs".to_string(),
            };
        }
        ApprovalDecision::Deny {
            reason: format!(
                "tool '{tool_name}' is not on the headless allow-list; \
                 pass --approve-prefix '{tool_name}' to pre-authorize it"
            ),
        }
    }

    /// A prefix pre-authorizes the tool name itself or the command it runs
    /// (arguments under `command` / `cmd`).
    fn prefix_matches(&self, tool_name: &str, arguments: &Value) -> bool {
        let mut candidates: Vec<&str> = vec![tool_name];
        for key in COMMAND_ARGUMENT_KEYS {
            if let Some(command) = arguments.get(*key).and_then(Value::as_str) {
                candidates.push(command);
            }
        }
        self.approve_prefixes
            .iter()
            .any(|prefix| candidates.iter().any(|c| c.starts_with(prefix.as_str())))
    }
}

/// Approval handler carrying the headless policy into the agent loop.
struct HeadlessApprovalHandler {
    policy: ApprovalPolicy,
    diag: Arc<Mutex<DiagWriter>>,
}

#[async_trait::async_trait]
impl ToolApprovalHandler for HeadlessApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        let decision = self.policy.decide(&request.tool_name, &request.arguments);
        let mut diag = wf_common::lock::lock_ok(self.diag.lock());
        match decision {
            ApprovalDecision::Allow { reason } => {
                let _ = diag.ok(&format!("▲ {} ({reason})", request.tool_name));
                ToolApprovalResult::approved(request.tool_call_id.clone())
            }
            ApprovalDecision::Deny { reason } => {
                let _ = diag.err(&format!("✗ {}: {reason}", request.tool_name));
                ToolApprovalResult::rejected(request.tool_call_id.clone(), reason)
            }
        }
    }
}

// ── interaction guard (follow-up questions cannot be answered) ───────

/// Records follow-up question requests; a headless session cannot answer
/// them, so the driver fails the run with exit code 1 afterwards.
struct HeadlessInteractionGuard {
    followup_requested: Arc<AtomicBool>,
    diag: Arc<Mutex<DiagWriter>>,
}

impl UserInteractionHandler for HeadlessInteractionGuard {
    fn on_interaction(&self, _record: &AgentUserInteractionEventRecord) {}

    fn on_tool_approval_requested(&self, _execution_id: &str, _request: &Value) {
        // Tool approvals are decided synchronously by
        // `HeadlessApprovalHandler`; nothing to ask the user here.
    }

    fn on_followup_question_requested(&self, execution_id: &str, _request: &Value) {
        self.followup_requested.store(true, Ordering::SeqCst);
        let mut diag = wf_common::lock::lock_ok(self.diag.lock());
        let _ = diag.warn(&format!(
            "follow-up question requested by execution {execution_id}; \
             headless mode cannot answer it"
        ));
    }
}

// ── delta line buffering ──────────────────────────────────────────

/// Merges LLM deltas before they hit stdout: complete lines flush
/// immediately, partial tails wait for more input or a threshold.
#[derive(Debug)]
pub struct DeltaBuffer {
    buf: String,
    max_bytes: usize,
}

impl Default for DeltaBuffer {
    fn default() -> Self {
        Self::new(8 * 1024)
    }
}

impl DeltaBuffer {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            buf: String::new(),
            max_bytes,
        }
    }

    /// Append a delta and return the ready-to-write segment (text up to the
    /// last newline, or the whole buffer once it exceeds the threshold).
    pub fn push(&mut self, delta: &str) -> String {
        self.buf.push_str(delta);
        if self.buf.contains('\n') {
            let split = self.buf.rfind('\n').expect("checked above") + 1;
            let ready = self.buf[..split].to_string();
            self.buf.replace_range(..split, "");
            ready
        } else if self.buf.len() >= self.max_bytes {
            std::mem::take(&mut self.buf)
        } else {
            String::new()
        }
    }

    /// Flush whatever remains (iteration/terminal boundary).
    pub fn take_remaining(&mut self) -> String {
        std::mem::take(&mut self.buf)
    }
}

// ── session rendering ────────────────────────────────────────────────

/// Renders agent events into the main sink (business output) and the
/// diagnostics channel (tool lifecycle).
struct SessionRenderer<'a> {
    sink: &'a mut dyn OutputSink,
    format: OutputFormat,
    delta_buf: DeltaBuffer,
    /// Full assistant text of the current iteration (structured formats
    /// emit one message record per iteration).
    iteration_text: String,
    /// True when an emitted chunk did not end with a newline (the next
    /// diagnostic or summary must terminate the line first).
    line_pending: bool,
    saw_text: bool,
    tool_started_at: HashMap<String, Instant>,
    had_output: bool,
}

impl<'a> SessionRenderer<'a> {
    fn new(sink: &'a mut dyn OutputSink, format: OutputFormat) -> Self {
        Self {
            sink,
            format,
            delta_buf: DeltaBuffer::default(),
            iteration_text: String::new(),
            line_pending: false,
            saw_text: false,
            tool_started_at: HashMap::new(),
            had_output: false,
        }
    }

    fn on_event(
        &mut self,
        event: &ExecutionStreamEvent,
        diag: &Arc<Mutex<DiagWriter>>,
    ) -> CliResult<()> {
        match event {
            // Engine lifecycle events carry no execution progress payload
            // for a headless run; skip them.
            ExecutionStreamEvent::Engine(_) => return Ok(()),
            // Terminal and interruption events are handled by the run
            // loop, not the renderer.
            ExecutionStreamEvent::Completed { .. }
            | ExecutionStreamEvent::Failed { .. }
            | ExecutionStreamEvent::Interrupted { .. } => return Ok(()),
            ExecutionStreamEvent::LlmDelta { content } => {
                self.had_output = true;
                self.saw_text = true;
                self.iteration_text.push_str(content);
                if self.format == OutputFormat::Text {
                    let ready = self.delta_buf.push(content);
                    if !ready.is_empty() {
                        self.sink.write_chunk(&ready)?;
                        self.line_pending = !ready.ends_with('\n');
                    }
                }
            }
            ExecutionStreamEvent::ToolStart {
                tool_call_id,
                tool_name,
            } => {
                self.had_output = true;
                self.tool_started_at
                    .insert(tool_call_id.clone(), Instant::now());
                let mut diag = wf_common::lock::lock_ok(diag.lock());
                let _ = diag.line(&format!("▲ {tool_name}"));
            }
            ExecutionStreamEvent::ToolEnd {
                tool_call_id,
                tool_name,
                success,
                ..
            } => {
                let elapsed = self
                    .tool_started_at
                    .remove(tool_call_id)
                    .map(|started| started.elapsed());
                let line = match (success, elapsed) {
                    (true, Some(d)) => format!("✓ {tool_name} ({}ms)", d.as_millis()),
                    (true, None) => format!("✓ {tool_name}"),
                    (false, _) => format!("✗ {tool_name}"),
                };
                let mut diag = wf_common::lock::lock_ok(diag.lock());
                if *success {
                    let _ = diag.ok(&line);
                } else {
                    let _ = diag.err(&line);
                }
            }
            ExecutionStreamEvent::IterationEnd { .. } => self.flush_iteration()?,
            ExecutionStreamEvent::IterationStart { .. } => {}
        }
        Ok(())
    }

    /// Commit the current iteration: text mode flushes the delta tail and
    /// terminates the line; structured formats emit one assistant record.
    fn flush_iteration(&mut self) -> CliResult<()> {
        if self.format == OutputFormat::Text {
            let rest = self.delta_buf.take_remaining();
            if !rest.is_empty() {
                self.sink.write_chunk(&rest)?;
                self.line_pending = !rest.ends_with('\n');
            }
            if self.line_pending {
                self.sink.write_chunk("\n")?;
                self.line_pending = false;
            }
        } else if !self.iteration_text.is_empty() && !self.format.is_silent() {
            let text = std::mem::take(&mut self.iteration_text);
            self.sink
                .write_message(&OutputMessage::new("assistant", text))?;
        } else {
            self.iteration_text.clear();
        }
        self.saw_text = false;
        Ok(())
    }

    /// Ensure the stream ends on a fresh line (text mode).
    fn finish(&mut self) -> CliResult<()> {
        if self.line_pending {
            self.sink.write_chunk("\n")?;
            self.line_pending = false;
        }
        Ok(())
    }
}

// ── session driver ───────────────────────────────────────────────────

/// Options for one headless session.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Initial user message (required, non-empty).
    pub prompt: String,
    /// Agent definition id (defaults to `cli`).
    pub agent_id: Option<String>,
    /// LLM profile id (defaults to `default`).
    pub model: Option<String>,
    /// Pre-authorization prefixes from `--approve-prefix`.
    pub approve_prefixes: Vec<String>,
}

/// Outcome of a completed headless session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    pub execution_id: String,
    pub iterations: u32,
    pub duration_ms: u64,
    pub had_output: bool,
}

/// IO bundle for a headless session: main sink, diagnostics channel and
/// the format layer answer.
pub struct RunIo {
    pub sink: Box<dyn OutputSink + Send>,
    pub diag: Arc<Mutex<DiagWriter>>,
    pub format: OutputFormat,
}

enum Terminal {
    Completed { iterations: u32 },
    Failed { error: String },
    Interrupted { reason: String },
    Sigint,
}

/// Drive one headless agent session to its terminal event.
///
/// Returns the outcome on success; business failures map to
/// [`CliError::Business`] (exit 1) and interruptions (SIGINT or an engine
/// `Interrupted` event) to [`CliError::Interrupted`] (exit 4).
pub async fn run_session(
    adapter: &DomainAdapter,
    opts: RunOptions,
    mut io: RunIo,
) -> CliResult<RunOutcome> {
    if opts.prompt.trim().is_empty() {
        return Err(CliError::Arguments(
            "no prompt given: pass a positional argument or pipe stdin".into(),
        ));
    }

    let started = Instant::now();
    let execution_id = wf_common::generate_id();
    let ctx = adapter.api_context();

    // Follow-up questions cannot be answered without a TTY.
    let followup_requested = Arc::new(AtomicBool::new(false));
    register_handler(
        ctx,
        Arc::new(HeadlessInteractionGuard {
            followup_requested: followup_requested.clone(),
            diag: io.diag.clone(),
        }),
    )
    .await;

    // Headless approval degradation rides on the handler; the engine
    // routes every tool call through it (ask-everything fallback).
    let sanitized_prompt = crate::sanitize::sanitize_user_text(&opts.prompt);
    let params = RunAgentLoopParams {
        agent_loop_id: Some(Id::from(execution_id.clone())),
        approval_handler: Some(Arc::new(HeadlessApprovalHandler {
            policy: ApprovalPolicy::new(opts.approve_prefixes.clone()),
            diag: io.diag.clone(),
        })),
        config: AgentLoopConfig {
            agent_id: Id::from(opts.agent_id.clone().unwrap_or_else(|| "cli".to_string())),
            model: opts
                .model
                .clone()
                .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            max_iterations: Some(DEFAULT_MAX_ITERATIONS),
            max_execution_time: None,
            hooks: Vec::new(),
            available_tool_names: Vec::new(),
            initial_tool_names: Vec::new(),
            discoverable_tool_names: Vec::new(),
            enable_general_tool: None,
            activated_tool_names: Vec::new(),
            hidden_tool_names: Vec::new(),
            tool_call_format: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            general_description: None,
            discoverable_metadata_block: None,
        },
        input: AgentLoopInput {
            message: sanitized_prompt,
            context: HashMap::new(),
            conversation: Vec::new(),
        },
    };

    // Echo the user message through the sink (text line / JSON record).
    if !io.format.is_silent() {
        io.sink
            .write_message(&OutputMessage::new("user", &opts.prompt))?;
    }

    let mut stream = agent_execution::stream(ctx, params)
        .await
        .map_err(CliError::from)?;

    let mut renderer = SessionRenderer::new(io.sink.as_mut(), io.format);
    let terminal = loop {
        tokio::select! {
            event = stream.next() => match event {
                Some(ExecutionStreamEvent::Completed { iterations: n, .. }) => {
                    break Terminal::Completed { iterations: n };
                }
                Some(ExecutionStreamEvent::Failed { error }) => break Terminal::Failed { error },
                Some(ExecutionStreamEvent::Interrupted { reason }) => {
                    break Terminal::Interrupted { reason }
                }
                Some(event) => {
                    renderer.on_event(&event, &io.diag)?;
                }
                None => break Terminal::Failed {
                    error: "agent stream ended without a terminal event".to_string(),
                },
            },
            _ = tokio::signal::ctrl_c() => break Terminal::Sigint,
        }
    };
    // Dropping the stream aborts the agent driver task chain.
    drop(stream);

    renderer.finish()?;
    // End the renderer's sink borrow before the terminal arms reuse it.
    let had_output = renderer.had_output;
    drop(renderer);
    io.sink.flush()?;

    match terminal {
        Terminal::Completed { iterations } => {
            if followup_requested.load(Ordering::SeqCst) {
                return Err(CliError::Business(
                    "follow-up question requested in headless mode; \
                     re-run interactively (wf --mini) to answer it"
                        .into(),
                ));
            }
            let duration_ms = started.elapsed().as_millis() as u64;
            write_summary(
                io.sink.as_mut(),
                io.format,
                &execution_id,
                iterations,
                duration_ms,
                had_output,
                &opts,
            )?;
            io.sink.flush()?;
            Ok(RunOutcome {
                execution_id,
                iterations,
                duration_ms,
                had_output,
            })
        }
        Terminal::Failed { error } => {
            write_failure_envelope(io.sink.as_mut(), io.format, &execution_id, &error);
            let _ = io.sink.flush();
            Err(CliError::Business(error))
        }
        Terminal::Interrupted { reason } => Err(CliError::Interrupted(reason)),
        Terminal::Sigint => {
            let mut diag = wf_common::lock::lock_ok(io.diag.lock());
            let _ = diag.warn("^C interrupted");
            Err(CliError::Interrupted(
                "SIGINT during headless session".into(),
            ))
        }
    }
}

/// End-of-session summary (text) or terminal envelope (json/jsonl).
fn write_summary(
    sink: &mut dyn OutputSink,
    format: OutputFormat,
    execution_id: &str,
    iterations: u32,
    duration_ms: u64,
    had_output: bool,
    opts: &RunOptions,
) -> CliResult<()> {
    let duration = format_duration(duration_ms);
    match format {
        OutputFormat::Text => {
            let line = if had_output {
                format!("▣ {execution_id} · {iterations} iterations · {duration}")
            } else {
                format!("▣ {execution_id} · no output · {duration}")
            };
            sink.write_raw(&line)
        }
        OutputFormat::Json => {
            let envelope = OutputEnvelope::success(
                "execution",
                serde_json::json!({
                    "executionId": execution_id,
                    "iterations": iterations,
                    "durationMs": duration_ms,
                    "hadOutput": had_output,
                    "model": opts.model.clone().unwrap_or_else(|| DEFAULT_MODEL.to_string()),
                    "agentId": opts.agent_id.clone().unwrap_or_else(|| "cli".to_string()),
                }),
            )
            .with_entity("agent-loop");
            if let Some(line) = envelope.render(format) {
                sink.write_raw(&line)
            } else {
                Ok(())
            }
        }
        OutputFormat::JsonLines => {
            let record = serde_json::json!({
                "type": "execution_summary",
                "executionId": execution_id,
                "iterations": iterations,
                "durationMs": duration_ms,
                "hadOutput": had_output,
                "success": true,
            });
            let line = serde_json::to_string(&record)?;
            sink.write_raw(&line)
        }
        OutputFormat::Silent => Ok(()),
    }
    .map_err(CliError::from)
}

/// Failure envelope for structured formats (text diagnostics go through
/// stderr in `main`).
fn write_failure_envelope(
    sink: &mut dyn OutputSink,
    format: OutputFormat,
    execution_id: &str,
    error: &str,
) {
    let write = |record: String| sink.write_raw(&record).map_err(CliError::from);
    let result = match format {
        OutputFormat::Json => OutputEnvelope::failure("execution", error)
            .with_entity("agent-loop")
            .render(format)
            .map(write),
        OutputFormat::JsonLines => Some(
            serde_json::to_string(&serde_json::json!({
                "type": "execution_summary",
                "executionId": execution_id,
                "success": false,
                "error": error,
            }))
            .map_err(CliError::from)
            .and_then(write),
        ),
        _ => None,
    };
    if let Some(Err(err)) = result {
        tracing::warn!(target: "wf_cli", error = %err, "failed to write failure envelope");
    }
}

/// Human duration: `123ms` under a second, `1.3s` above.
fn format_duration(duration_ms: u64) -> String {
    if duration_ms < 1_000 {
        format!("{duration_ms}ms")
    } else {
        format!("{:.1}s", duration_ms as f64 / 1_000.0)
    }
}

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::MemorySink;

    fn shared_diag() -> Arc<Mutex<DiagWriter>> {
        Arc::new(Mutex::new(DiagWriter::buffer()))
    }

    fn diag_text(diag: &Arc<Mutex<DiagWriter>>) -> String {
        wf_common::lock::lock_ok(diag.lock()).snapshot()
    }

    #[test]
    fn sensitive_tools_are_denied_with_reason() {
        let policy = ApprovalPolicy::new(vec![]);
        for tool in SENSITIVE_TOOLS {
            let decision = policy.decide(tool, &serde_json::json!({}));
            match decision {
                ApprovalDecision::Deny { reason } => {
                    assert!(reason.contains("sensitive"), "{tool}: {reason}");
                    assert!(reason.contains(tool), "{tool}: {reason}");
                }
                other => panic!("{tool} should be denied, got {other:?}"),
            }
        }
    }

    #[test]
    fn low_risk_tools_are_allowed() {
        let policy = ApprovalPolicy::new(vec![]);
        for tool in LOW_RISK_TOOLS {
            assert!(
                matches!(
                    policy.decide(tool, &serde_json::json!({})),
                    ApprovalDecision::Allow { .. }
                ),
                "{tool} should be allowed"
            );
        }
    }

    #[test]
    fn unknown_tools_are_denied_with_hint() {
        let policy = ApprovalPolicy::new(vec![]);
        match policy.decide("rm_rf_everything", &serde_json::json!({})) {
            ApprovalDecision::Deny { reason } => {
                assert!(reason.contains("--approve-prefix"), "{reason}")
            }
            other => panic!("unknown tool should be denied, got {other:?}"),
        }
    }

    #[test]
    fn approve_prefix_preauthorizes_tool_names_and_commands() {
        let policy = ApprovalPolicy::new(vec!["git".to_string()]);
        assert!(matches!(
            policy.decide("git_status_custom", &serde_json::json!({})),
            ApprovalDecision::Allow { .. }
        ));
        // The prefix explicitly consents to sensitive tools too: a `git`
        // prefix authorizes `execute_command` running `git status`.
        assert!(matches!(
            policy.decide(
                "execute_command",
                &serde_json::json!({ "command": "git status" })
            ),
            ApprovalDecision::Allow { .. }
        ));
        // Prefixes are literal: "git" does not authorize unrelated commands.
        assert!(matches!(
            policy.decide(
                "execute_command",
                &serde_json::json!({ "command": "rm -rf /" })
            ),
            ApprovalDecision::Deny { .. }
        ));
        // Without any prefix the sensitive tool stays denied.
        let strict = ApprovalPolicy::new(vec![]);
        assert!(matches!(
            policy.decide(
                "execute_command",
                &serde_json::json!({ "command": "git status" })
            ),
            ApprovalDecision::Allow { .. }
        ));
        assert!(matches!(
            strict.decide(
                "execute_command",
                &serde_json::json!({ "command": "git status" })
            ),
            ApprovalDecision::Deny { .. }
        ));
    }

    #[test]
    fn delta_buffer_flushes_on_newline_and_threshold() {
        let mut buf = DeltaBuffer::new(8);

        // No newline yet: buffered.
        assert_eq!(buf.push("hel"), "");
        assert_eq!(buf.push("lo"), "");

        // Newline flushes the complete line, keeps the tail.
        assert_eq!(buf.push(" wo\nrld"), "hello wo\n");
        assert_eq!(buf.take_remaining(), "rld");

        // Threshold flushes without a newline.
        let mut buf = DeltaBuffer::new(4);
        assert_eq!(buf.push("abcdef"), "abcdef");
        assert_eq!(buf.take_remaining(), "");
    }

    #[test]
    fn text_renderer_streams_deltas_and_lines() {
        let mut sink = MemorySink::new();
        let format = OutputFormat::Text;
        let diag = shared_diag();
        {
            let mut renderer = SessionRenderer::new(&mut sink, format);
            renderer
                .on_event(
                    &ExecutionStreamEvent::LlmDelta {
                        content: "hello\nbig ".into(),
                    },
                    &diag,
                )
                .unwrap();
            renderer
                .on_event(
                    &ExecutionStreamEvent::LlmDelta {
                        content: "world".into(),
                    },
                    &diag,
                )
                .unwrap();
            // Iteration boundary flushes the pending tail + newline.
            renderer
                .on_event(
                    &ExecutionStreamEvent::IterationEnd {
                        iteration: 1,
                        message_count: 0,
                        array_version: 0,
                    },
                    &diag,
                )
                .unwrap();
        }
        assert_eq!(sink.text(), "hello\nbig world\n");
    }

    #[test]
    fn text_renderer_marks_tool_lifecycle_on_diag() {
        let mut sink = MemorySink::new();
        let diag = shared_diag();
        {
            let mut renderer = SessionRenderer::new(&mut sink, OutputFormat::Text);
            renderer
                .on_event(
                    &ExecutionStreamEvent::ToolStart {
                        tool_call_id: "t1".into(),
                        tool_name: "read_file".into(),
                    },
                    &diag,
                )
                .unwrap();
            renderer
                .on_event(
                    &ExecutionStreamEvent::ToolEnd {
                        tool_call_id: "t1".into(),
                        tool_name: "read_file".into(),
                        success: true,
                        result: String::new(),
                    },
                    &diag,
                )
                .unwrap();
            renderer
                .on_event(
                    &ExecutionStreamEvent::ToolEnd {
                        tool_call_id: "t2".into(),
                        tool_name: "write_file".into(),
                        success: false,
                        result: String::new(),
                    },
                    &diag,
                )
                .unwrap();
        }
        assert_eq!(sink.text(), "");
        let text = diag_text(&diag);
        assert!(text.contains("▲ read_file"), "{text}");
        assert!(text.contains("✓ read_file"), "{text}");
        assert!(text.contains("✗ write_file"), "{text}");
    }

    #[test]
    fn structured_renderer_emits_one_record_per_iteration() {
        let mut sink = MemorySink::new();
        let diag = shared_diag();
        {
            let mut renderer = SessionRenderer::new(&mut sink, OutputFormat::Json);
            for (chunk, iteration) in [
                ("part ", 1u32),
                ("one", 1),
                // Iteration boundary flushes the pending tail.
                ("two", 2),
            ] {
                renderer
                    .on_event(
                        &ExecutionStreamEvent::LlmDelta {
                            content: chunk.into(),
                        },
                        &diag,
                    )
                    .unwrap();
                if chunk == "one" || chunk == "two" {
                    renderer
                        .on_event(
                            &ExecutionStreamEvent::IterationEnd {
                                iteration,
                                message_count: 0,
                                array_version: 0,
                            },
                            &diag,
                        )
                        .unwrap();
                }
            }
        }
        let messages = sink.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "part one");
        assert_eq!(messages[1].content, "two");
    }

    #[test]
    fn summary_line_reflects_output_presence_and_duration_format() {
        assert_eq!(format_duration(950), "950ms");
        assert_eq!(format_duration(1_234), "1.2s");

        let mut sink = MemorySink::new();
        write_summary(
            &mut sink,
            OutputFormat::Text,
            "exec-1",
            3,
            1_234,
            true,
            &RunOptions::default(),
        )
        .unwrap();
        assert_eq!(sink.raw(), vec!["▣ exec-1 · 3 iterations · 1.2s"]);

        let mut sink = MemorySink::new();
        write_summary(
            &mut sink,
            OutputFormat::Text,
            "exec-2",
            0,
            5,
            false,
            &RunOptions::default(),
        )
        .unwrap();
        assert_eq!(sink.raw(), vec!["▣ exec-2 · no output · 5ms"]);
    }

    #[test]
    fn json_summary_envelope_carries_execution_fields() {
        let mut sink = MemorySink::new();
        write_summary(
            &mut sink,
            OutputFormat::Json,
            "exec-3",
            2,
            42,
            true,
            &RunOptions {
                model: Some("mock".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let raw = sink.raw()[0];
        let parsed: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["type"], "execution");
        assert_eq!(parsed["entity"], "agent-loop");
        assert_eq!(parsed["data"]["executionId"], "exec-3");
        assert_eq!(parsed["data"]["iterations"], 2);
        assert_eq!(parsed["data"]["durationMs"], 42);
        assert_eq!(parsed["data"]["model"], "mock");
        assert!(parsed["timestamp"].as_i64().unwrap() > 0);
    }

    // ── end-to-end (mock LLM full chain) ─────────────────────────────

    mod e2e {
        use super::*;
        use crate::domain::DomainAdapter;
        use std::sync::Arc;
        use wf_llm::{LlmResponseSpec, MockLlmClient};

        fn run_io(format: OutputFormat) -> (RunIo, Arc<Mutex<MemorySink>>) {
            let sink = Arc::new(Mutex::new(MemorySink::new()));
            // `run_session` needs an owned sink; the Arc handle keeps read
            // access for assertions while the session owns the writer half
            // through a shared-memory forwarder.
            let forwarding = SinkForwarder::new(sink.clone());
            let io = RunIo {
                sink: Box::new(forwarding),
                diag: Arc::new(Mutex::new(DiagWriter::buffer())),
                format,
            };
            (io, sink)
        }

        /// Forwards writes into a shared `MemorySink` so tests can read the
        /// accumulated output while the session driver owns the sink.
        struct SinkForwarder {
            shared: Arc<Mutex<MemorySink>>,
        }

        impl SinkForwarder {
            fn new(shared: Arc<Mutex<MemorySink>>) -> Self {
                Self { shared }
            }

            fn with_sink<R>(&self, f: impl FnOnce(&mut MemorySink) -> R) -> R {
                f(&mut wf_common::lock::lock_ok(self.shared.lock()))
            }
        }

        impl OutputSink for SinkForwarder {
            fn write_message(&mut self, message: &OutputMessage) -> std::io::Result<()> {
                self.with_sink(|sink| sink.write_message(message))
            }
            fn write_chunk(&mut self, chunk: &str) -> std::io::Result<()> {
                self.with_sink(|sink| sink.write_chunk(chunk))
            }
            fn write_raw(&mut self, line: &str) -> std::io::Result<()> {
                self.with_sink(|sink| sink.write_raw(line))
            }
            fn write_text(&mut self, text: &str) -> std::io::Result<()> {
                self.with_sink(|sink| sink.write_text(text))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.with_sink(|sink| sink.flush())
            }
        }

        async fn adapter_with_mock(
            script: Vec<LlmResponseSpec>,
            default: LlmResponseSpec,
        ) -> DomainAdapter {
            let adapter = DomainAdapter::bootstrap(crate::default_runtime_config())
                .await
                .unwrap();
            let mock = Arc::new(MockLlmClient::new());
            for spec in script {
                mock.script(spec);
            }
            mock.default(default);
            adapter.llm_gateway().register_mock("mock", mock.clone());
            adapter
        }

        #[tokio::test]
        async fn headless_session_streams_mock_llm_answer_end_to_end() {
            let adapter = adapter_with_mock(
                vec![LlmResponseSpec::text("hello from cli e2e")],
                LlmResponseSpec::text("fallback"),
            )
            .await;
            let (io, sink) = run_io(OutputFormat::Text);

            let outcome = run_session(
                &adapter,
                RunOptions {
                    prompt: "hi".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap();

            assert_eq!(outcome.iterations, 1);
            assert!(outcome.had_output);
            {
                let sink_guard = wf_common::lock::lock_ok(sink.lock());
                let text = sink_guard.text();
                assert!(text.contains("hello from cli e2e"), "{text}");
                // The summary line is a raw record (bypasses the format filter).
                let summary = sink_guard
                    .raw()
                    .into_iter()
                    .find(|line| line.contains('▣'))
                    .unwrap_or_else(|| panic!("summary line missing from {text:?}"));
                assert!(
                    summary.contains(&outcome.execution_id),
                    "summary {summary} lacks execution id"
                );
            }

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn headless_session_jsonl_emits_summary_record() {
            let adapter = adapter_with_mock(
                vec![LlmResponseSpec::text("jsonl answer")],
                LlmResponseSpec::text("fallback"),
            )
            .await;
            let (io, sink) = run_io(OutputFormat::JsonLines);

            let outcome = run_session(
                &adapter,
                RunOptions {
                    prompt: "hi".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap();

            let raw: Vec<String> = wf_common::lock::lock_ok(sink.lock())
                .raw()
                .into_iter()
                .map(str::to_string)
                .collect();
            let summary = raw
                .iter()
                .find(|line| line.contains("execution_summary"))
                .unwrap_or_else(|| panic!("no summary record in {raw:?}"));
            let parsed: serde_json::Value = serde_json::from_str(summary).unwrap();
            assert_eq!(parsed["success"], true);
            assert_eq!(parsed["executionId"], outcome.execution_id);
            assert_eq!(parsed["iterations"], 1);

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn sensitive_tool_call_is_denied_and_session_recovers() {
            let tool_call = wf_types::message::LlmToolCall {
                id: "call-1".into(),
                r#type: "function".into(),
                function: wf_types::message::LlmFunctionCall {
                    name: "execute_command".into(),
                    arguments: serde_json::json!({ "command": "rm -rf /" }).to_string(),
                },
            };
            let adapter = adapter_with_mock(
                vec![LlmResponseSpec::tool_calls(vec![tool_call])],
                LlmResponseSpec::text("gave up on the tool"),
            )
            .await;
            let (io, sink) = run_io(OutputFormat::Text);
            let diag = io.diag.clone();

            let outcome = run_session(
                &adapter,
                RunOptions {
                    prompt: "clean my disk".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap();

            // The denial is visible on the diagnostics channel, the run
            // itself completes with the follow-up text answer.
            let diag_text = wf_common::lock::lock_ok(diag.lock()).snapshot();
            assert!(diag_text.contains("✗"), "{diag_text}");
            assert!(diag_text.contains("execute_command"), "{diag_text}");
            assert!(diag_text.contains("sensitive"), "{diag_text}");
            assert!(wf_common::lock::lock_ok(sink.lock())
                .text()
                .contains("gave up on the tool"));

            // The rejected call is fed back: the LLM saw a second request.
            assert!(outcome.iterations >= 1);

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn empty_prompt_fails_fast_with_arguments_error() {
            let adapter = DomainAdapter::bootstrap(crate::default_runtime_config())
                .await
                .unwrap();
            let (io, _sink) = run_io(OutputFormat::Text);

            let err = run_session(&adapter, RunOptions::default(), io)
                .await
                .unwrap_err();
            assert!(matches!(err, CliError::Arguments(_)), "{err:?}");

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn low_risk_tool_is_allowed_and_executes() {
            let file = tempfile::NamedTempFile::new().unwrap();
            std::fs::write(file.path(), "line1\nline2\n").unwrap();
            let tool_call = wf_types::message::LlmToolCall {
                id: "call-read".into(),
                r#type: "function".into(),
                function: wf_types::message::LlmFunctionCall {
                    name: "read_file".into(),
                    arguments: serde_json::json!({ "path": file.path().to_string_lossy() })
                        .to_string(),
                },
            };
            let adapter = adapter_with_mock(
                vec![LlmResponseSpec::tool_calls(vec![tool_call])],
                LlmResponseSpec::text("read it"),
            )
            .await;
            let (io, _sink) = run_io(OutputFormat::Text);
            let diag = io.diag.clone();

            let outcome = run_session(
                &adapter,
                RunOptions {
                    prompt: "read the file".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap();

            // read_file sits on the low-risk allow-list: the approval
            // decision is printed (allow reason) and the tool actually
            // executes (✓ line), the session completes normally.
            let diag_text = wf_common::lock::lock_ok(diag.lock()).snapshot();
            assert!(diag_text.contains("▲ read_file"), "{diag_text}");
            assert!(diag_text.contains("✓ read_file"), "{diag_text}");
            assert!(!diag_text.contains("✗ read_file"), "{diag_text}");
            assert_eq!(outcome.iterations, 2, "tool turn + final answer turn");

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn llm_error_maps_to_business_failure() {
            let adapter = DomainAdapter::bootstrap(crate::default_runtime_config())
                .await
                .unwrap();
            let mock = Arc::new(MockLlmClient::new());
            // Every attempt (including gateway retries) errors: the script
            // queue is consumed per request, so saturate it.
            for _ in 0..16 {
                mock.script_error(wf_llm::LlmError::ProviderError("provider exploded".into()));
            }
            adapter.llm_gateway().register_mock("mock", mock);
            let (io, _sink) = run_io(OutputFormat::Text);

            let err = run_session(
                &adapter,
                RunOptions {
                    prompt: "hi".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap_err();

            match err {
                CliError::Business(ref msg) => assert!(msg.contains("provider exploded"), "{msg}"),
                other => panic!("expected business failure, got {other:?}"),
            }
            assert_eq!(err.exit_code(), 1);

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn silent_session_reports_no_output_in_summary() {
            let adapter =
                adapter_with_mock(vec![LlmResponseSpec::text("")], LlmResponseSpec::text("")).await;
            let (io, sink) = run_io(OutputFormat::Text);

            let outcome = run_session(
                &adapter,
                RunOptions {
                    prompt: "hi".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap();

            assert!(!outcome.had_output);
            let raw_lines: Vec<String> = wf_common::lock::lock_ok(sink.lock())
                .raw()
                .into_iter()
                .map(str::to_string)
                .collect();
            let summary = raw_lines
                .iter()
                .find(|line| line.contains('▣'))
                .unwrap_or_else(|| panic!("summary line missing"));
            assert!(
                summary.contains("no output"),
                "summary should report no output: {summary}"
            );

            adapter.shutdown().await.unwrap();
        }
    }
}
