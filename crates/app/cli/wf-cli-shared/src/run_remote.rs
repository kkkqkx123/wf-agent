//! Remote headless session driver (HTTP server transport).

use crate::error::{CliError, CliResult};
use crate::output::{OutputFormat, OutputMessage};
use crate::run::{RunIo, RunOptions, RunOutcome};
use crate::run_render::SessionRenderer;
use crate::run_stream::{drive_remote_stream, finish_workflow_stream_terminal, Terminal};
use crate::run_summary::{
    had_output_for_result, render_value_text, write_failure_envelope, write_summary, SummaryParams,
};
use wf_api::{DEFAULT_AGENT, DEFAULT_MODEL};

/// Remote execution path: drive the session through the HTTP server instead of
/// an embedded runtime. Prefers SSE streaming (same renderer/summary
/// contract as embedded) and falls back to the blocking POST endpoints when
/// the server does not support streaming.
pub async fn run_session_remote(
    client: &crate::remote::RemoteClient,
    opts: RunOptions,
    mut io: RunIo,
) -> CliResult<RunOutcome> {
    use std::time::Instant;
    if opts.interactive || opts.assume_yes {
        return Err(CliError::Arguments(
            "--interactive/--assume-yes need an embedded runtime; \
             remote runs cannot read local stdin"
                .into(),
        ));
    }
    let started = Instant::now();
    if let Some(workflow_id) = opts.workflow.clone() {
        let input = crate::turn::parse_workflow_input(opts.workflow_input.as_deref())
            .map_err(CliError::Arguments)?;
        if !io.format.is_silent() {
            io.sink.write_message(&OutputMessage::new(
                "user",
                format!("workflow:{workflow_id}"),
            ))?;
        }
        // Prefer SSE streaming for live progress.
        if let Ok(stream) = client
            .stream_workflow_execution(&workflow_id, input.clone())
            .await
        {
            let execution_id = wf_common::generate_id();
            let mut renderer = SessionRenderer::new(io.sink.as_mut(), io.format);
            let terminal = drive_remote_stream(stream, &mut renderer, &io.diag).await?;
            renderer.finish()?;
            let had_output = renderer.had_output;
            drop(renderer);
            io.sink.flush()?;
            return finish_workflow_stream_terminal(
                terminal,
                &mut io,
                &opts,
                &execution_id,
                started,
                had_output,
            );
        }
        let body = serde_json::json!({ "input": input });
        let resp: serde_json::Value = client
            .post_json(&format!("/api/v1/workflows/{}/execute", workflow_id), &body)
            .await?;
        let execution_id = resp
            .get("execution_id")
            .or_else(|| resp.get("executionId"))
            .and_then(|v| v.as_str())
            .unwrap_or("remote-exec")
            .to_string();
        let result = resp.get("result").cloned().unwrap_or(resp.clone());
        let had_output = had_output_for_result(&result);
        if !io.format.is_silent() && io.format == OutputFormat::Text {
            render_value_text(io.sink.as_mut(), &result)?;
        }
        io.sink.flush()?;
        let duration_ms = started.elapsed().as_millis() as u64;
        write_summary(SummaryParams {
            sink: io.sink.as_mut(),
            format: io.format,
            execution_id: &execution_id,
            iterations: 1,
            duration_ms,
            had_output,
            opts: &opts,
            result: Some(&result),
        })?;
        io.sink.flush()?;
        return Ok(RunOutcome {
            execution_id,
            iterations: 1,
            duration_ms,
            had_output,
        });
    }

    if opts.prompt.trim().is_empty() {
        return Err(CliError::Arguments(
            "no prompt given: pass a positional argument or pipe stdin".into(),
        ));
    }
    if !io.format.is_silent() {
        io.sink
            .write_message(&OutputMessage::new("user", &opts.prompt))?;
    }
    // Prefer SSE streaming so remote runs render deltas live like embedded.
    let turn_params = opts.as_turn_params();
    if let Ok(stream) = client.stream_agent_execution(turn_params).await {
        let execution_id = wf_common::generate_id();
        let mut renderer = SessionRenderer::new(io.sink.as_mut(), io.format);
        let terminal = drive_remote_stream(stream, &mut renderer, &io.diag).await?;
        renderer.finish()?;
        let had_output = renderer.had_output;
        drop(renderer);
        io.sink.flush()?;
        return finish_remote_agent_terminal(
            terminal,
            &mut io,
            &opts,
            &execution_id,
            started,
            had_output,
        );
    }
    let sanitized = crate::sanitize::sanitize_user_text(&opts.prompt);
    let body = serde_json::json!({
        "agent_id": opts.agent_id.clone().unwrap_or_else(|| DEFAULT_AGENT.to_string()),
        "model": opts.model.clone().unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        "message": sanitized,
        "max_iterations": 50,
        "context": {},
    });
    let resp: serde_json::Value = client
        .post_json("/api/v1/agent-loops/cli/run", &body)
        .await?;
    let execution_id = resp
        .get("agent_loop_id")
        .or_else(|| resp.get("agentLoopId"))
        .or_else(|| resp.get("execution_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("remote-agent")
        .to_string();
    let result = resp
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let iterations = resp.get("iterations").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
    let had_output = had_output_for_result(&result);
    if !io.format.is_silent() && io.format == OutputFormat::Text {
        render_value_text(io.sink.as_mut(), &result)?;
    }
    io.sink.flush()?;
    let duration_ms = started.elapsed().as_millis() as u64;
    write_summary(SummaryParams {
        sink: io.sink.as_mut(),
        format: io.format,
        execution_id: &execution_id,
        iterations,
        duration_ms,
        had_output,
        opts: &opts,
        result: Some(&result),
    })?;
    io.sink.flush()?;
    Ok(RunOutcome {
        execution_id,
        iterations,
        duration_ms,
        had_output,
    })
}

/// Shared terminal handling for streamed remote agent runs: the `Completed`
/// payload is rendered as text (structured formats keep it in the summary),
/// then the standard summary/failure mapping applies.
pub(crate) fn finish_remote_agent_terminal(
    terminal: Terminal,
    io: &mut RunIo,
    opts: &RunOptions,
    execution_id: &str,
    started: std::time::Instant,
    had_output: bool,
) -> CliResult<RunOutcome> {
    match terminal {
        Terminal::Completed { iterations, result } => {
            let result = result.unwrap_or(serde_json::Value::Null);
            // SSE `Completed` carries the final value; mirror the blocking
            // path by rendering it in text mode before the summary line.
            // `LlmDelta` events already streamed string answers, so skip
            // re-rendering plain strings when deltas produced output.
            let streamed = had_output;
            let text_had_output = had_output_for_result(&result);
            let had_output = streamed || text_had_output;
            if !io.format.is_silent()
                && io.format == OutputFormat::Text
                && text_had_output
                && (!streamed || !result.is_string())
            {
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
