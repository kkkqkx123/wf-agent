//! Execution streaming and watch helpers (embedded and remote).

use crate::args::Cli;
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat};
use wf_api::agent::agent_loop_registry;
use wf_api::infra::stream::ExecutionStreamEvent;
use wf_api::workflow::workflow_execution;

/// Owned API context for the streaming run paths (`stream` takes `Arc` to
/// spawn its detached driver). Unavailable in remote-only builds, where the
/// run arms serve through HTTP instead.
#[cfg(feature = "embedded")]
pub(crate) fn embedded_arc(
    domain: &crate::domain::DomainHandle,
) -> CliResult<std::sync::Arc<wf_api::ApiContext>> {
    domain
        .as_embedded()
        .map(|a| a.api_context_arc())
        .ok_or_else(|| {
            CliError::Configuration("embedded runtime not available in this build".to_string())
        })
}
/// Statuses after which an execution no longer changes without intervention.
pub(crate) fn is_terminal_status(status: &wf_types::ExecutionStatus) -> bool {
    use wf_types::ExecutionStatus::*;
    matches!(status, Completed | Failed | Cancelled | Timeout | Stopped)
}
/// Current status of one execution, live first then persisted, workflow
/// first then agent. Mirrors the `Status` arm so watch polling and status
/// reporting never disagree.
pub(crate) async fn fetch_execution_status(
    ctx: &wf_api::ApiContext,
    id: &str,
) -> CliResult<wf_types::ExecutionStatus> {
    if let Ok(status) = workflow_execution::status(ctx, id).await {
        return Ok(status);
    }
    if let Some(summary) = agent_loop_registry::summary(ctx, id).await? {
        return Ok(summary.status);
    }
    if let Ok(exec) = wf_api::workflow::get_execution(ctx, id).await {
        return Ok(exec.status);
    }
    Err(CliError::Configuration(format!(
        "execution not found: {id}"
    )))
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
        } => match cost {
            Some(cost) => format!("usage {prompt_tokens}+{completion_tokens} cost={cost:.4}"),
            None => format!("usage {prompt_tokens}+{completion_tokens}"),
        },
        ExecutionStreamEvent::SubAgentStarted { id, name } => {
            format!("sub-agent {name} started ({id})")
        }
        ExecutionStreamEvent::SubAgentEnded { id, name, success } => {
            format!("sub-agent {name} ended ({id}) success={success}")
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
/// Run a workflow through the streaming engine API so flags have real
/// meaning: the execution id is known before the first event, `--stream`
/// prints events live, and `--background` waits quietly and prints only
/// the terminal result. The CLI process still owns the runtime; detaching
/// past process exit needs the server background endpoint.
#[cfg(feature = "embedded")]
pub(crate) async fn run_workflow_streaming(
    cli: &Cli,
    domain: &crate::domain::DomainHandle,
    workflow: &str,
    input: Option<serde_json::Value>,
    background: bool,
) -> CliResult<()> {
    use futures::StreamExt;

    let ctx = embedded_arc(domain)?;
    let params = workflow_execution::ExecuteWorkflowParams {
        workflow_id: workflow.to_string(),
        input,
        options: None,
    };
    let (execution_id, mut stream) = workflow_execution::stream(ctx, params).await?;
    let id = execution_id.to_string();
    let text = cli.output == OutputFormat::Text;

    if background {
        if text {
            println!("started {id}");
        } else {
            let data = serde_json::json!({"executionId": id, "background": true, "started": true});
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-run", data).with_entity(id.clone()),
            )?;
        }
        while let Some(event) = stream.next().await {
            if let Some(outcome) = stream_terminal(&event) {
                let result = outcome?;
                if text {
                    println!("finished {id} {result}");
                } else {
                    let data = serde_json::json!({"executionId": id, "background": true, "result": result});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("execution-run", data).with_entity(id.clone()),
                    )?;
                }
                return Ok(());
            }
        }
        return Err(CliError::Business(
            "stream ended without a terminal event".to_string(),
        ));
    }

    if text {
        println!("started {id}");
    }
    while let Some(event) = stream.next().await {
        if text {
            println!("{}", stream_event_line(&event));
        } else {
            let data = serde_json::to_value(&event)?;
            render_envelope(
                OutputFormat::JsonLines,
                OutputEnvelope::success("execution-run-event", data).with_entity(id.clone()),
            )?;
        }
        if let Some(outcome) = stream_terminal(&event) {
            let result = outcome?;
            let data = serde_json::json!({"executionId": id, "stream": true, "result": result});
            if text {
                println!("finished {id}");
            }
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-run-stream", data).with_entity(id.clone()),
            )?;
            return Ok(());
        }
    }
    Err(CliError::Business(
        "stream ended without a terminal event".to_string(),
    ))
}

#[cfg(not(feature = "embedded"))]
pub(crate) async fn run_workflow_streaming(
    _cli: &Cli,
    _domain: &crate::domain::DomainHandle,
    _workflow: &str,
    _input: Option<serde_json::Value>,
    _background: bool,
) -> CliResult<()> {
    Err(CliError::Configuration(
        "streaming run needs the embedded runtime; use --remote against a running server"
            .to_string(),
    ))
}
/// Watch one execution until it stops changing.
pub(crate) async fn watch_execution(
    cli: &Cli,
    ctx: &wf_api::ApiContext,
    id: &str,
    interval: u64,
    once: bool,
) -> CliResult<()> {
    let text = cli.output == OutputFormat::Text;
    let mut last = fetch_execution_status(ctx, id).await?;
    if text {
        println!("{id} {}", last.as_str());
    }
    if once || is_terminal_status(&last) {
        let data = serde_json::json!({"executionId": id, "status": last.as_str()});
        return render_envelope(
            cli.output,
            OutputEnvelope::success("execution-watch", data).with_entity(id.to_string()),
        );
    }
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(interval)) => {
                let status = fetch_execution_status(ctx, id).await?;
                if status != last {
                    last = status;
                    if text {
                        println!("{id} {}", last.as_str());
                    }
                    if is_terminal_status(&last) {
                        let data = serde_json::json!({"executionId": id, "status": last.as_str()});
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
pub(crate) fn parse_status(s: &str) -> Option<agent_loop_registry::AgentLoopFilter> {
    // Strict parse: every known status is accepted, and an unrecognized one
    // is rejected instead of being coerced, so a typo never silently filters
    // on an unrelated status.
    let status = s.parse::<wf_types::ExecutionStatus>().ok()?;
    Some(agent_loop_registry::AgentLoopFilter {
        ids: None,
        status: Some(status),
        profile_id: None,
        tags: None,
        created_at_range: None,
    })
}
/// Stream a remote workflow run event by event.
pub(crate) async fn run_remote_stream(
    cli: &Cli,
    client: &crate::remote::RemoteClient,
    workflow: &str,
    input: Option<serde_json::Value>,
) -> CliResult<()> {
    use futures::StreamExt;

    let text = cli.output == OutputFormat::Text;
    let mut stream = client
        .stream_workflow_execution(workflow, input)
        .await
        .map_err(|e| CliError::Configuration(format!("remote stream failed: {e}")))?;
    // The client skips the stream metadata frame, so the execution id
    // only arrives inside engine events; announce the workflow being
    // streamed and the execution id once an engine event names it.
    let mut announced = false;
    while let Some(item) = stream.next().await {
        let event =
            item.map_err(|e| CliError::Configuration(format!("remote stream failed: {e}")))?;
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
/// Poll a remote execution until it stops changing.
pub(crate) async fn watch_execution_remote(
    cli: &Cli,
    client: &crate::remote::RemoteClient,
    id: &str,
    interval: u64,
    once: bool,
) -> CliResult<()> {
    fn status_of(value: &serde_json::Value) -> Option<String> {
        value.as_str().map(|s| s.to_string()).or_else(|| {
            value
                .get("status")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string())
        })
    }

    async fn fetch_status(client: &crate::remote::RemoteClient, id: &str) -> CliResult<String> {
        if let Ok(value) = client
            .get_json::<serde_json::Value>(&format!("/api/v1/executions/{id}/status"))
            .await
        {
            if let Some(status) = status_of(&value) {
                return Ok(status);
            }
        }
        let detail: serde_json::Value = client
            .get_execution(id)
            .await
            .map_err(|e| CliError::Configuration(format!("remote status query failed: {e}")))?;
        status_of(&detail)
            .ok_or_else(|| CliError::Configuration(format!("remote execution has no status: {id}")))
    }

    fn terminal(status: &str) -> bool {
        matches!(
            status,
            "completed" | "failed" | "cancelled" | "timeout" | "stopped"
        )
    }

    let text = cli.output == OutputFormat::Text;
    let mut last = fetch_status(client, id).await?;
    if text {
        println!("{id} {last}");
    }
    if once || terminal(&last) {
        let data = serde_json::json!({"executionId": id, "status": last});
        return render_envelope(
            cli.output,
            OutputEnvelope::success("execution-watch", data).with_entity(id.to_string()),
        );
    }
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(interval)) => {
                let status = fetch_status(client, id).await?;
                if status != last {
                    last = status;
                    if text {
                        println!("{id} {last}");
                    }
                    if terminal(&last) {
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
