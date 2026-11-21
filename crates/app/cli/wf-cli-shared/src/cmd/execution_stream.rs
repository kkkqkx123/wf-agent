//! Execution streaming and watch helpers over HTTP.

use futures::StreamExt;

use crate::args::Cli;
use crate::cmd::render::{print_chunk_to_cli, print_line_to_cli, render_envelope_to_cli};
use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat};
use wf_client::WfClient;
use wf_types::events::ExecutionStreamEvent;

/// Statuses after which an execution no longer changes without intervention.
pub(crate) fn is_terminal_status(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "completed" | "failed" | "cancelled" | "timeout" | "stopped"
    )
}

/// Current status of one execution: live status first, then the stored
/// detail record. Mirrors the `Status` arm so watch polling and status
/// reporting never disagree.
pub(crate) async fn fetch_execution_status(client: &WfClient, id: &str) -> CliResult<String> {
    if let Ok(status) = client.execution_status(id).await {
        return Ok(status);
    }
    let detail: serde_json::Value = client
        .execution_detail(id)
        .await
        .map_err(|e| CliError::Configuration(format!("status query failed: {e}")))?;
    detail
        .get("status")
        .and_then(|s| s.as_str())
        .map(str::to_string)
        .ok_or_else(|| CliError::Configuration(format!("execution has no status: {id}")))
}

/// Terminal preview budget for one `--stream` row. Tool results and
/// reasoning deltas arrive unbounded; rows beyond this stay readable and
/// never flood the terminal. Failures stay visible because the head of the
/// message is kept and only the tail is marked as omitted.
pub(crate) const STREAM_TOOL_PREVIEW_CHARS: usize = 2000;
pub(crate) const STREAM_REASONING_PREVIEW_CHARS: usize = 1000;

/// Bounded head preview with an omission marker. Short inputs pass
/// through unchanged so the common case keeps exact bytes.
pub(crate) fn truncate_stream_text(text: &str, max_chars: usize) -> String {
    let total: usize = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    format!("{}...[truncated {} chars]", head, total - max_chars)
}

/// One-line text rendering of a stream event for `--stream` TTY output.
pub(crate) fn stream_event_line(event: &ExecutionStreamEvent) -> String {
    match event {
        ExecutionStreamEvent::Engine(base) => format!(
            "event {} {}",
            base.r#type.as_str(),
            base.execution_id.as_deref().unwrap_or("-")
        ),
        ExecutionStreamEvent::IterationStart { iteration, .. } => {
            format!("iteration #{iteration} started")
        }
        ExecutionStreamEvent::LlmDelta { content } => content.clone(),
        ExecutionStreamEvent::ReasoningDelta { content } => format!(
            "thinking: {}",
            truncate_stream_text(content, STREAM_REASONING_PREVIEW_CHARS)
        ),
        ExecutionStreamEvent::ToolStart {
            tool_name,
            tool_call_id,
        } => format!("tool {tool_name} started ({tool_call_id})"),
        ExecutionStreamEvent::ToolEnd {
            tool_name,
            success,
            result,
            error,
            ..
        } => {
            if *success {
                format!(
                    "tool {tool_name} ok: {}",
                    truncate_stream_text(result, STREAM_TOOL_PREVIEW_CHARS)
                )
            } else {
                let detail = error.as_deref().unwrap_or(result.as_str());
                format!(
                    "tool {tool_name} failed: {}",
                    truncate_stream_text(detail, STREAM_TOOL_PREVIEW_CHARS)
                )
            }
        }
        ExecutionStreamEvent::IterationEnd { iteration, .. } => {
            format!("iteration #{iteration} done")
        }
        ExecutionStreamEvent::Usage {
            prompt_tokens,
            completion_tokens,
            cost,
            cache_read_tokens,
            ..
        } => {
            let mut line = format!("usage {prompt_tokens}+{completion_tokens}");
            if let Some(cached) = cache_read_tokens {
                line.push_str(&format!(" cached={cached}"));
            }
            match cost {
                Some(cost) => format!("{line} cost={cost:.4}"),
                None => line,
            }
        }
        ExecutionStreamEvent::SubAgentStarted {
            id,
            name,
            tool_call_id,
            ..
        } => {
            let link = tool_call_id
                .as_deref()
                .map(|call| format!(" [{call}]"))
                .unwrap_or_default();
            format!("sub-agent {name} started ({id}){link}")
        }
        ExecutionStreamEvent::SubAgentEnded {
            id, name, success, ..
        } => {
            format!("sub-agent {name} ended ({id}) success={success}")
        }
        ExecutionStreamEvent::SubAgentProgress { id, activity, .. } => {
            let what = activity.as_deref().unwrap_or("working");
            format!("sub-agent {id} progress: {what}")
        }
        ExecutionStreamEvent::Interrupted { reason } => format!("interrupted: {reason}"),
        ExecutionStreamEvent::Completed { result, iterations } => {
            format!("completed iterations={iterations} result={result}")
        }
        ExecutionStreamEvent::Failed { error } => format!("failed: {error}"),
    }
}

/// Terminal outcome of a stream: the success payload, or an execution error
/// naming the failure.
pub(crate) fn stream_terminal(
    event: &ExecutionStreamEvent,
) -> Option<CliResult<serde_json::Value>> {
    match event {
        ExecutionStreamEvent::Completed { result, iterations } => Some(Ok(serde_json::json!({
            "result": result,
            "iterations": iterations,
        }))),
        ExecutionStreamEvent::Failed { error } => Some(Err(CliError::Business(error.clone()))),
        ExecutionStreamEvent::Interrupted { reason } => Some(Err(CliError::Interrupted(format!(
            "execution interrupted: {reason}"
        )))),
        _ => None,
    }
}

/// Run a workflow through the streaming server API: `--stream` prints
/// events live, `--background` returns the execution id immediately
/// through the server background endpoint and exits, otherwise the
/// blocking execute endpoint runs it synchronously.
pub(crate) async fn run_workflow_streaming(
    cli: &Cli,
    client: &WfClient,
    workflow: &str,
    input: Option<serde_json::Value>,
    background: bool,
) -> CliResult<()> {
    crate::turn::validate_workflow_input_value(input.as_ref()).map_err(CliError::Arguments)?;
    if background {
        let typed = crate::turn::TurnParams::workflow_typed_body(input.as_ref());
        let view = client.execute_workflow_background(workflow, &typed).await?;
        let data = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
        return render_envelope_to_cli(
            cli,
            OutputEnvelope::success("execution-run", data).with_entity(workflow.to_string()),
        );
    }
    run_stream(cli, client, workflow, input).await
}

/// Stream a workflow run event by event.
pub(crate) async fn run_stream(
    cli: &Cli,
    client: &WfClient,
    workflow: &str,
    input: Option<serde_json::Value>,
) -> CliResult<()> {
    let text = cli.output == OutputFormat::Text;
    crate::turn::validate_workflow_input_value(input.as_ref()).map_err(CliError::Arguments)?;
    let typed = crate::turn::TurnParams::workflow_typed_body(input.as_ref());
    let body = serde_json::to_value(&typed).unwrap_or(serde_json::Value::Null);
    let stream = client
        .post_stream(&wf_client::endpoints::workflow_execute(workflow), &body)
        .await
        .map_err(|e| CliError::Configuration(format!("stream failed: {e}")))?;
    let mut stream = map_frames(stream);
    // The stream metadata frame is skipped, so the execution id only
    // arrives inside engine events; announce the workflow being streamed
    // and the execution id once an engine event names it.
    let mut announced = false;
    while let Some(item) = stream.next().await {
        let event = item.map_err(|e| CliError::Configuration(format!("stream failed: {e}")))?;
        if !announced {
            announced = true;
            if text {
                match &event {
                    ExecutionStreamEvent::Engine(base) => {
                        if let Some(id) = base.execution_id.as_deref() {
                            print_line_to_cli(cli, &format!("streaming execution {id}"))?;
                        } else {
                            print_line_to_cli(cli, &format!("streaming workflow {workflow}"))?;
                        }
                    }
                    _ => print_line_to_cli(cli, &format!("streaming workflow {workflow}"))?,
                }
            }
        }
        if text {
            // Answer deltas stream without an extra newline per frame so
            // the terminal shows continuous text; every other event keeps
            // one row per event.
            if let ExecutionStreamEvent::LlmDelta { content } = &event {
                print_chunk_to_cli(cli, content)?;
            } else {
                print_line_to_cli(cli, &stream_event_line(&event))?;
            }
        } else {
            let data = serde_json::to_value(&event)?;
            render_envelope_to_cli(
                cli,
                OutputEnvelope::success("execution-run-event", data)
                    .with_entity(workflow.to_string()),
            )?;
        }
        if let Some(outcome) = stream_terminal(&event) {
            let result = outcome?;
            let data =
                serde_json::json!({"workflowId": workflow, "stream": true, "result": result});
            render_envelope_to_cli(
                cli,
                OutputEnvelope::success("execution-run-stream", data)
                    .with_entity(workflow.to_string()),
            )?;
            return Ok(());
        }
    }
    Err(CliError::Business(
        "stream ended without a terminal event".to_string(),
    ))
}

/// Bus event types after which an execution no longer changes. Mirrors
/// `EventSubscriptionOptions::is_terminal` so stream-driven followers and
/// the server subscription agree on when a run ends.
pub(crate) fn is_terminal_event(event_type: &wf_types::events::EventType) -> bool {
    use wf_types::events::EventType as T;
    matches!(
        event_type,
        T::WorkflowExecutionCompleted
            | T::WorkflowExecutionFailed
            | T::WorkflowExecutionCancelled
            | T::AgentCompleted
            | T::AgentFailed
            | T::AgentCancelled
    )
}

/// Watch an execution until it stops changing. The event stream is the
/// primary channel: every bus event re-reads the status and terminal events
/// end the watch. The fixed interval survives only as the degraded path
/// when the stream cannot be opened or drops mid-watch.
pub(crate) async fn watch_execution(
    cli: &Cli,
    client: &WfClient,
    id: &str,
    interval: u64,
    once: bool,
) -> CliResult<()> {
    let text = cli.output == OutputFormat::Text;
    let mut last = fetch_execution_status(client, id).await?;
    if text {
        print_line_to_cli(cli, &format!("{id} {last}"))?;
    }
    if once || is_terminal_status(&last) {
        let data = serde_json::json!({"executionId": id, "status": last});
        return render_envelope_to_cli(
            cli,
            OutputEnvelope::success("execution-watch", data).with_entity(id.to_string()),
        );
    }
    match client
        .get_stream(&wf_client::endpoints::events_stream_for_execution(id, None))
        .await
    {
        Ok(stream) => watch_via_stream(cli, client, id, text, &mut last, Box::pin(stream)).await,
        Err(_) => watch_via_poll(cli, client, id, text, &mut last, interval).await,
    }
}

/// Stream-driven watch: status is re-read on bus events (bounded by actual
/// activity, silent when idle) and terminal events end the watch after one
/// confirming read. A dropped stream degrades to interval polling.
async fn watch_via_stream(
    cli: &Cli,
    client: &WfClient,
    id: &str,
    text: bool,
    last: &mut String,
    mut stream: std::pin::Pin<
        Box<dyn futures::Stream<Item = Result<wf_client::SseFrame, wf_client::ClientError>> + Send>,
    >,
) -> CliResult<()> {
    loop {
        tokio::select! {
            frame = stream.next() => {
                match frame {
                    Some(Ok(frame)) => {
                        let terminal = wf_client::sse::decode_system_frame(&frame)
                            .into_base_event()
                            .is_some_and(|base| is_terminal_event(&base.r#type));
                        let status = fetch_execution_status(client, id).await?;
                        if status != *last {
                            *last = status;
                            if text {
                                print_line_to_cli(cli, &format!("{id} {last}"))?;
                            }
                        }
                        if terminal || is_terminal_status(last) {
                            let data = serde_json::json!({"executionId": id, "status": last});
                            return render_envelope_to_cli(
                                cli,
                                OutputEnvelope::success("execution-watch", data).with_entity(id.to_string()),
                            );
                        }
                    }
                    Some(Err(_)) | None => {
                        return watch_via_poll(cli, client, id, text, last, 1_000).await;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                return Err(CliError::Interrupted("watch cancelled".to_string()));
            }
        }
    }
}

/// Interval watch: the degraded path when the event stream is unavailable.
async fn watch_via_poll(
    cli: &Cli,
    client: &WfClient,
    id: &str,
    text: bool,
    last: &mut String,
    interval: u64,
) -> CliResult<()> {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(interval)) => {
                let status = fetch_execution_status(client, id).await?;
                if status != *last {
                    *last = status.clone();
                    if text {
                        print_line_to_cli(cli, &format!("{id} {last}"))?;
                    }
                    if is_terminal_status(last) {
                        let data = serde_json::json!({"executionId": id, "status": last});
                        return render_envelope_to_cli(
                            cli,
                            OutputEnvelope::success("execution-watch", data).with_entity(id.to_string()),
                        );
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                return Err(CliError::Interrupted("watch cancelled".to_string()));
            }
        }
    }
}

/// Adapt server SSE frames into execution stream events, skipping the
/// stream metadata frame. Engine lines decode as bus events and flow into
/// the existing ignore path; unknown payloads are skipped instead of
/// failing the stream, keeping old clients forward compatible.
pub(crate) fn map_frames(
    stream: impl futures::Stream<Item = Result<wf_client::SseFrame, wf_client::ClientError>>
        + Send
        + 'static,
) -> futures::stream::BoxStream<'static, Result<ExecutionStreamEvent, wf_client::ClientError>> {
    Box::pin(stream)
        .filter_map(|item| async move {
            match item {
                Err(e) => Some(Err(e)),
                Ok(frame) => match wf_client::sse::decode_execution_frame(&frame) {
                    wf_client::sse::ExecutionFrame::Metadata(_) => None,
                    wf_client::sse::ExecutionFrame::Engine(base) => {
                        Some(Ok(ExecutionStreamEvent::Engine(base)))
                    }
                    wf_client::sse::ExecutionFrame::Event(event) => Some(Ok(event)),
                    wf_client::sse::ExecutionFrame::Raw(_) => None,
                },
            }
        })
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_text_passes_short_inputs_through() {
        assert_eq!(truncate_stream_text("ok", 2000), "ok");
        assert_eq!(truncate_stream_text("", 2000), "");
    }

    #[test]
    fn stream_tool_result_truncates_with_marker() {
        let big = "x".repeat(STREAM_TOOL_PREVIEW_CHARS + 10);
        let line = stream_event_line(&ExecutionStreamEvent::ToolEnd {
            tool_call_id: "t1".into(),
            tool_name: "read_file".into(),
            success: true,
            result: big,
            error: None,
            error_code: None,
            display: None,
            background: None,
        });
        assert!(line.starts_with("tool read_file ok: "), "{line}");
        assert!(line.contains("truncated 10 chars"), "{line}");
    }

    #[test]
    fn stream_reasoning_truncates_with_marker() {
        let big = "y".repeat(STREAM_REASONING_PREVIEW_CHARS + 5);
        let line = stream_event_line(&ExecutionStreamEvent::ReasoningDelta { content: big });
        assert!(line.starts_with("thinking: "), "{line}");
        assert!(line.contains("truncated 5 chars"), "{line}");
    }

    #[test]
    fn interrupted_event_maps_to_interrupted_exit_code() {
        let outcome = stream_terminal(&ExecutionStreamEvent::Interrupted {
            reason: "user cancelled".into(),
        });
        let err = match outcome {
            Some(Err(err)) => err,
            other => panic!("expected an error outcome, got {other:?}"),
        };
        assert_eq!(err.exit_code(), wf_common::exit_code::INTERRUPTED);
        assert!(matches!(err, CliError::Interrupted(_)));
    }
}
