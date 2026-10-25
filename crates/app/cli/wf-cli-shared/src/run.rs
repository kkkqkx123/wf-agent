//! Headless session driver: `wf run "<prompt>"` end to end.
//!
//! Pipeline: resolve the request body → stream the run through `wf-server`
//! (SSE first, blocking POST fallback) → render events (LLM text to the main
//! sink, tool lifecycle to the diagnostics channel) → terminate with a
//! summary line / envelope and an exit code.
//!
//! Output discipline: business output (LLM text, message records, summary
//! envelope) goes through the [`OutputSink`] (stdout); diagnostics (tool
//! lines `▲/✓/✗`, approval rejections, interrupt notices) go to stderr
//! through [`DiagWriter`], so `wf run | jq` keeps working.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use wf_client::WfClient;

use crate::cmd::execution_stream::map_frames;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::{OutputFormat, OutputSink};
pub use crate::run_diag::DiagWriter;
use crate::run_interaction::{AnswerPolicy, InteractionPoller};
pub use crate::run_render::DeltaBuffer;
use crate::run_stream::{
    drive_stream, finish_agent_stream_terminal, finish_workflow_stream_terminal,
};
use crate::run_summary::{had_output_for_result, render_value_text, write_summary, SummaryParams};
use crate::stdio_prompt::DEFAULT_APPROVAL_TIMEOUT_SECS;
use crate::turn::{TurnKind, TurnParams};

/// Options for one headless session.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Initial user message (required, non-empty for agent turns).
    pub prompt: String,
    /// Agent definition id (defaults to the built-in agent, server-side).
    pub agent_id: Option<String>,
    /// LLM profile id (defaults to `default`).
    pub model: Option<String>,
    /// Pre-authorization prefixes from `--approve-prefix`.
    pub approve_prefixes: Vec<String>,
    /// Workflow id to execute instead of an agent turn.
    pub workflow: Option<String>,
    /// Workflow input JSON string (requires workflow).
    pub workflow_input: Option<String>,
    /// Read approval/follow-up answers from stdin (`--interactive`).
    pub interactive: bool,
    /// Wait bound for one stdin answer line (`--approval-timeout`).
    pub approval_timeout_secs: u64,
    /// Approve every routed tool call without prompting (`--assume-yes`).
    pub assume_yes: bool,
    /// Server-side run budget in milliseconds (`--timeout`).
    pub timeout_ms: Option<u64>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            agent_id: None,
            model: None,
            approve_prefixes: Vec::new(),
            workflow: None,
            workflow_input: None,
            interactive: false,
            approval_timeout_secs: DEFAULT_APPROVAL_TIMEOUT_SECS,
            assume_yes: false,
            timeout_ms: None,
        }
    }
}

impl RunOptions {
    /// Single conversion into the shared [`TurnParams`] abstraction so the
    /// headless driver and the interactive forms assemble request bodies in
    /// exactly one place. Invalid workflow input is a hard [`CliError`]:
    /// it never degrades to `None`.
    pub fn try_as_turn_params(&self) -> CliResult<TurnParams> {
        let kind = match self.workflow.clone() {
            Some(workflow_id) => {
                let input = crate::turn::parse_workflow_input(self.workflow_input.as_deref())
                    .map_err(CliError::Arguments)?;
                TurnKind::Workflow { workflow_id, input }
            }
            None => TurnKind::Agent {
                prompt: self.prompt.clone(),
            },
        };
        Ok(TurnParams {
            agent: self.agent_id.clone(),
            model: self.model.clone(),
            approve_prefixes: self.approve_prefixes.clone(),
            timeout_ms: self.timeout_ms,
            conversation: Vec::new(),
            kind,
        })
    }

    fn answer_policy(&self, json: bool) -> AnswerPolicy {
        AnswerPolicy {
            interactive: self.interactive,
            assume_yes: self.assume_yes,
            approve_prefixes: self.approve_prefixes.clone(),
            approval_timeout: Duration::from_secs(self.approval_timeout_secs),
            json,
        }
    }
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

/// Drive one headless session against `wf-server` to its terminal event.
///
/// Returns the outcome on success; business failures map to
/// [`CliError::Business`] (exit 1) and interruptions (SIGINT or a server
/// `Interrupted` event) to [`CliError::Interrupted`] (exit 4).
pub async fn run_session(
    handle: &ClientHandle,
    opts: RunOptions,
    mut io: RunIo,
) -> CliResult<RunOutcome> {
    if let Some(workflow_id) = opts.workflow.clone() {
        return run_workflow(handle, opts, &mut io, &workflow_id).await;
    }
    run_agent(handle, opts, &mut io).await
}

async fn run_workflow(
    handle: &ClientHandle,
    opts: RunOptions,
    io: &mut RunIo,
    workflow_id: &str,
) -> CliResult<RunOutcome> {
    use std::time::Instant;
    let started = Instant::now();
    let input = crate::turn::parse_workflow_input(opts.workflow_input.as_deref())
        .map_err(CliError::Arguments)?;
    if !io.format.is_silent() {
        io.sink.write_message(&crate::output::OutputMessage::new(
            "user",
            format!("workflow:{workflow_id}"),
        ))?;
    }
    let client = handle.client().clone();
    let stop = Arc::new(AtomicBool::new(false));
    let poller = spawn_poller(
        &client,
        &opts,
        io,
        stop.clone(),
        Arc::new(AtomicBool::new(false)),
    );
    let body = TurnParams::workflow_request_body(input.as_ref());
    let outcome = if let Ok(stream) = client
        .post_stream(&wf_client::endpoints::workflow_execute(workflow_id), &body)
        .await
    {
        // The stream protocol carries no execution id upfront, so the
        // summary uses a stable placeholder instead of a minted id that
        // could be mistaken for a server-side execution.
        let execution_id = "remote-stream".to_string();
        let mut renderer = crate::run_render::SessionRenderer::new(io.sink.as_mut(), io.format);
        let terminal = drive_stream(map_frames(stream), &mut renderer, &io.diag).await?;
        renderer.finish()?;
        let had_output = renderer.had_output;
        let background_summary = renderer.background_summary();
        drop(renderer);
        if let Some(summary) = background_summary {
            let mut diag = wf_common::lock::lock_ok(io.diag.lock());
            let _ = diag.ok(&summary);
        }
        io.sink.flush()?;
        finish_workflow_stream_terminal(terminal, io, &opts, &execution_id, started, had_output)
    } else {
        let typed = TurnParams::workflow_typed_body(input.as_ref());
        let view = client.execute_workflow(workflow_id, &typed).await?;
        blocking_outcome(io, &opts, &view.execution_id, &view.result, 1, started).await
    };
    stop.store(true, Ordering::SeqCst);
    if let Some(task) = poller {
        let _ = task.await;
    }
    outcome
}

async fn run_agent(
    handle: &ClientHandle,
    opts: RunOptions,
    io: &mut RunIo,
) -> CliResult<RunOutcome> {
    use std::time::Instant;
    if opts.prompt.trim().is_empty() {
        return Err(CliError::Arguments(
            "no prompt given: pass a positional argument or pipe stdin".into(),
        ));
    }
    let started = Instant::now();
    if !io.format.is_silent() {
        io.sink
            .write_message(&crate::output::OutputMessage::new("user", &opts.prompt))?;
    }
    let client = handle.client().clone();
    let stop = Arc::new(AtomicBool::new(false));
    let followup_requested = Arc::new(AtomicBool::new(false));
    let poller = spawn_poller(&client, &opts, io, stop.clone(), followup_requested.clone());
    let body = opts.try_as_turn_params()?.agent_request_body();
    let outcome = if let Ok(stream) = client
        .post_stream(&wf_client::endpoints::agent_cli_stream(), &body)
        .await
    {
        // The stream protocol carries no execution id upfront, so the
        // summary uses a stable placeholder instead of a minted id that
        // could be mistaken for a server-side execution.
        let execution_id = "remote-stream".to_string();
        let mut renderer = crate::run_render::SessionRenderer::new(io.sink.as_mut(), io.format);
        let terminal = drive_stream(map_frames(stream), &mut renderer, &io.diag).await?;
        renderer.finish()?;
        let had_output = renderer.had_output;
        let background_summary = renderer.background_summary();
        drop(renderer);
        if let Some(summary) = background_summary {
            let mut diag = wf_common::lock::lock_ok(io.diag.lock());
            let _ = diag.ok(&summary);
        }
        io.sink.flush()?;
        finish_agent_stream_terminal(terminal, io, &opts, &execution_id, started, had_output)
    } else {
        // Blocking fallback uses the named session shortcut through the
        // generated client: the `cli` id reuses the `{id}` slot (see
        // `wf_client::endpoints::agent_cli_run`), so the path template,
        // request body and response view are all type-checked.
        let typed = opts.try_as_turn_params()?.agent_typed_body();
        let view = client.run_agent_loop("cli", &typed).await?;
        let iterations = view.iterations.max(0) as u32;
        blocking_outcome(
            io,
            &opts,
            &view.agent_loop_id,
            &view.result,
            iterations,
            started,
        )
        .await
    };
    stop.store(true, Ordering::SeqCst);
    if let Some(task) = poller {
        let _ = task.await;
    }
    let outcome = outcome?;
    if followup_requested.load(Ordering::SeqCst) {
        return Err(CliError::Business(
            "follow-up question requested in headless mode; \
             re-run interactively (wf --tui) to answer it"
                .into(),
        ));
    }
    Ok(outcome)
}

async fn blocking_outcome(
    io: &mut RunIo,
    opts: &RunOptions,
    execution_id: &str,
    result: &serde_json::Value,
    iterations: u32,
    started: std::time::Instant,
) -> CliResult<RunOutcome> {
    let had_output = had_output_for_result(result);
    if !io.format.is_silent() && io.format == OutputFormat::Text {
        render_value_text(io.sink.as_mut(), result)?;
    }
    io.sink.flush()?;
    let duration_ms = started.elapsed().as_millis() as u64;
    write_summary(SummaryParams {
        sink: io.sink.as_mut(),
        format: io.format,
        execution_id,
        iterations,
        duration_ms,
        had_output,
        opts,
        result: Some(result),
    })?;
    io.sink.flush()?;
    Ok(RunOutcome {
        execution_id: execution_id.to_string(),
        iterations,
        duration_ms,
        had_output,
    })
}

fn spawn_poller(
    client: &WfClient,
    opts: &RunOptions,
    io: &RunIo,
    stop: Arc<AtomicBool>,
    followup_requested: Arc<AtomicBool>,
) -> Option<tokio::task::JoinHandle<()>> {
    let json = matches!(io.format, OutputFormat::Json | OutputFormat::JsonLines);
    let policy = opts.answer_policy(json);
    if !policy.needs_poller() {
        return None;
    }
    let prompt = if opts.interactive {
        Some(crate::stdio_prompt::StdioPromptSource::spawn().0)
    } else {
        None
    };
    let mut poller =
        InteractionPoller::new(client, policy, prompt, io.diag.clone(), followup_requested);
    Some(tokio::spawn(async move {
        poller.run_until(stop).await;
    }))
}
