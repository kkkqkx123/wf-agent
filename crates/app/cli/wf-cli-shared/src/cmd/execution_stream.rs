//! Execution streaming and watch helpers over HTTP.

use futures::StreamExt;

use crate::args::Cli;
use crate::cmd::render::render_envelope;
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
    if let Ok(value) = client.execution_status(id).await {
        if let Some(status) = value.as_str().map(str::to_string).or_else(|| {
            value
                .get("status")
                .and_then(|s| s.as_str())
                .map(str::to_string)
        }) {
            return Ok(status);
        }
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
        ExecutionStreamEvent::ReasoningDelta { content } => format!("thinking: {content}"),
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
                format!("tool {tool_name} ok: {result}")
            } else {
                format!(
                    "tool {tool_name} failed: {}",
                    error.as_deref().unwrap_or(result.as_str())
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
        ExecutionStreamEvent::Interrupted { reason } => Some(Err(CliError::Business(format!(
            "execution interrupted: {reason}"
        )))),
        _ => None,
    }
}

/// Run a workflow through the streaming server API: `--stream` prints
/// events live, `--background` waits quietly and prints only the terminal
/// result, otherwise the blocking execute endpoint runs it synchronously.
pub(crate) async fn run_workflow_streaming(
    cli: &Cli,
    client: &WfClient,
    workflow: &str,
    input: Option<serde_json::Value>,
    background: bool,
) -> CliResult<()> {
    if background {
        let typed = wf_client::models::ExecuteBody {
            input,
            file_scope: None,
            version: None,
        };
        let view = client.execute_workflow_background(workflow, &typed).await?;
        let data = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
        return render_envelope(
            cli.output,
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
    let typed = wf_client::models::ExecuteBody {
        input,
        file_scope: None,
        version: None,
    };
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
                            println!("streaming execution {id}");
                        } else {
                            println!("streaming workflow {workflow}");
                        }
                    }
                    _ => println!("streaming workflow {workflow}"),
                }
            }
        }
        if text {
            println!("{}", stream_event_line(&event));
        } else {
            let data = serde_json::to_value(&event)?;
            render_envelope(
                OutputFormat::JsonLines,
                OutputEnvelope::success("execution-run-event", data)
                    .with_entity(workflow.to_string()),
            )?;
        }
        if let Some(outcome) = stream_terminal(&event) {
            let result = outcome?;
            let data =
                serde_json::json!({"workflowId": workflow, "stream": true, "result": result});
            render_envelope(
                cli.output,
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

/// Poll an execution until it stops changing.
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
        println!("{id} {last}");
    }
    if once || is_terminal_status(&last) {
        let data = serde_json::json!({"executionId": id, "status": last});
        return render_envelope(
            cli.output,
            OutputEnvelope::success("execution-watch", data).with_entity(id.to_string()),
        );
    }
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(interval)) => {
                let status = fetch_execution_status(client, id).await?;
                if status != last {
                    last = status;
                    if text {
                        println!("{id} {last}");
                    }
                    if is_terminal_status(&last) {
                        let data = serde_json::json!({"executionId": id, "status": last});
                        return render_envelope(
                            cli.output,
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
/// stream metadata frame.
pub(crate) fn map_frames(
    stream: impl futures::Stream<Item = Result<wf_client::SseFrame, wf_client::ClientError>>
        + Send
        + 'static,
) -> futures::stream::BoxStream<'static, Result<ExecutionStreamEvent, wf_client::ClientError>> {
    Box::pin(stream)
        .filter_map(|item| async move {
            match item {
                Err(e) => Some(Err(e)),
                Ok(frame) => {
                    if frame.event == "metadata" {
                        return None;
                    }
                    match serde_json::from_value::<ExecutionStreamEvent>(frame.data) {
                        Ok(event) => Some(Ok(event)),
                        Err(e) => Some(Err(wf_client::ClientError::Stream(format!(
                            "stream frame: {e}"
                        )))),
                    }
                }
            }
        })
        .boxed()
}
