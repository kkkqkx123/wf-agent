//! Stream drivers to the terminal event and shared terminal handling.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::StreamExt;

use serde_json::Value;

use crate::error::{CliError, CliResult};
use crate::output::OutputFormat;
use crate::run::{RunIo, RunOptions, RunOutcome};
use crate::run_diag::DiagWriter;
use crate::run_render::SessionRenderer;
use crate::run_summary::{
    had_output_for_result, render_value_text, write_failure_envelope, write_summary, SummaryParams,
};
use wf_api::infra::stream::ExecutionStreamEvent;

pub(crate) enum Terminal {
    Completed {
        iterations: u32,
        result: Option<Value>,
    },
    Failed {
        error: String,
    },
    Interrupted {
        reason: String,
    },
    Sigint,
}

/// Drive an embedded event stream to its terminal event, feeding every
/// progress event through the shared [`SessionRenderer`].
/// Returns the terminal state plus the `Completed` payload when present.
pub(crate) async fn drive_embedded_stream<S>(
    mut stream: S,
    renderer: &mut SessionRenderer<'_>,
    diag: &Arc<Mutex<DiagWriter>>,
) -> CliResult<Terminal>
where
    S: futures::Stream<Item = ExecutionStreamEvent> + Unpin,
{
    loop {
        tokio::select! {
            event = stream.next() => match event {
                Some(ExecutionStreamEvent::Completed { iterations: n, result }) => {
                    break Ok(Terminal::Completed { iterations: n, result: Some(result) });
                }
                Some(ExecutionStreamEvent::Failed { error }) => {
                    break Ok(Terminal::Failed { error });
                }
                Some(ExecutionStreamEvent::Interrupted { reason }) => {
                    break Ok(Terminal::Interrupted { reason });
                }
                Some(event) => {
                    renderer.on_event(&event, diag)?;
                }
                None => break Ok(Terminal::Failed {
                    error: "agent stream ended without a terminal event".to_string(),
                }),
            },
            _ = tokio::signal::ctrl_c() => break Ok(Terminal::Sigint),
        }
    }
}

/// Drive a remote (SSE) event stream to its terminal event. `RemoteError`
/// maps onto [`CliError`] so callers share the same terminal handling as
/// the embedded path.
pub(crate) async fn drive_remote_stream<S>(
    mut stream: S,
    renderer: &mut SessionRenderer<'_>,
    diag: &Arc<Mutex<DiagWriter>>,
) -> CliResult<Terminal>
where
    S: futures::Stream<Item = Result<ExecutionStreamEvent, crate::remote::RemoteError>> + Unpin,
{
    loop {
        tokio::select! {
            event = stream.next() => match event {
                Some(Ok(ExecutionStreamEvent::Completed { iterations: n, result })) => {
                    break Ok(Terminal::Completed { iterations: n, result: Some(result) });
                }
                Some(Ok(ExecutionStreamEvent::Failed { error })) => {
                    break Ok(Terminal::Failed { error });
                }
                Some(Ok(ExecutionStreamEvent::Interrupted { reason })) => {
                    break Ok(Terminal::Interrupted { reason });
                }
                Some(Ok(event)) => {
                    renderer.on_event(&event, diag)?;
                }
                Some(Err(err)) => {
                    break Err(CliError::from(err));
                }
                None => break Ok(Terminal::Failed {
                    error: "remote stream ended without a terminal event".to_string(),
                }),
            },
            _ = tokio::signal::ctrl_c() => break Ok(Terminal::Sigint),
        }
    }
}

/// Shared terminal handling for streamed workflow runs (embedded and
/// remote): render the `Completed` payload, write the closing summary and
/// map failures/interrupts onto [`CliError`].
pub(crate) fn finish_workflow_stream_terminal(
    terminal: Terminal,
    io: &mut RunIo,
    opts: &RunOptions,
    execution_id: &str,
    started: Instant,
    had_output: bool,
) -> CliResult<RunOutcome> {
    match terminal {
        Terminal::Completed { iterations, result } => {
            let result = result.unwrap_or(serde_json::Value::Null);
            let had_output = had_output || had_output_for_result(&result);
            if !io.format.is_silent() && io.format == OutputFormat::Text {
                render_value_text(io.sink.as_mut(), &result)?;
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
                result: Some(&result),
            })?;
            io.sink.flush()?;
            Ok(RunOutcome {
                execution_id: execution_id.to_string(),
                iterations,
                duration_ms,
                had_output,
            })
        }
        Terminal::Failed { error } => {
            write_failure_envelope(io.sink.as_mut(), io.format, execution_id, &error);
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
