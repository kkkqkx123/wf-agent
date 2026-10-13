//! Execution UX facade over the agent and workflow domains.
//!
//! Listing, show/status and lifecycle arms resolve through `wf-server`,
//! agent first then workflow fallback where the id itself does not name an
//! engine. The `performance` / `bottleneck` / `errors` / `compare` /
//! `progress` arms delegate to the shared builders in
//! `crate::cmd::analysis` so both surfaces stay in sync.

use super::execution_logs::show_execution_logs;
use super::execution_stream::{run_workflow_streaming, watch_execution};
use super::execution_view::{print_hierarchy, print_history, print_subtree};

use crate::args::{Cli, ExecutionSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;
use wf_client::WfClient;

pub async fn run(cli: &Cli, sub: &ExecutionSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    run_with_client(cli, sub, handle.client()).await
}

pub(crate) async fn run_with_client(
    cli: &Cli,
    sub: &ExecutionSub,
    client: &WfClient,
) -> CliResult<()> {
    match sub {
        ExecutionSub::List {
            status,
            workflow,
            agent,
            unified,
            limit,
            offset,
            order,
        } => {
            if workflow.is_some() && agent.is_some() {
                return Err(crate::error::CliError::Configuration(
                    "--workflow cannot be combined with --agent; query one engine at a time"
                        .to_string(),
                ));
            }
            let data: serde_json::Value = if *unified {
                if workflow.is_some() || agent.is_some() {
                    return Err(crate::error::CliError::Configuration(
                        "--unified cannot be combined with --workflow or --agent".to_string(),
                    ));
                }
                let cursor = offset.map(|v| v.to_string());
                let envelope = client
                    .generated()
                    .handle_unified_executions(
                        cursor.as_deref(),
                        None,
                        limit.map(|v| v as i64),
                        None,
                        None,
                        status.as_deref(),
                    )
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let mut page: serde_json::Value =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                if order
                    .as_deref()
                    .is_some_and(|o| o.eq_ignore_ascii_case("asc"))
                {
                    if let Some(items) = page.get_mut("items").and_then(|v| v.as_array_mut()) {
                        items.reverse();
                    }
                }
                page
            } else if let Some(agent_id) = agent {
                list_agent_executions(client, *limit, *offset, status.as_deref(), agent_id, order.as_deref()).await?
            } else if let Some(wf) = workflow {
                list_workflow_executions(client, *limit, *offset, status.as_deref(), wf, order.as_deref()).await?
            } else {
                list_loop_summaries(client, *limit, *offset, status.as_deref(), order.as_deref()).await?
            };
            render_envelope(cli.output, OutputEnvelope::success("execution-list", data))
        }
        ExecutionSub::Show {
            id,
            timeline,
            iterations,
            variables,
            context_evolution,
        } => {
            if *timeline {
                let data: serde_json::Value = client
                    .get_json(&format!("/api/v1/agent-loops/{id}/timeline"))
                    .await?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-timeline", data).with_entity(id.clone()),
                )
            } else if *iterations {
                let data: serde_json::Value = client
                    .get_json(&format!("/api/v1/agent-loops/{id}/iteration-history"))
                    .await?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-iterations", data).with_entity(id.clone()),
                )
            } else if *context_evolution {
                let data: serde_json::Value = client
                    .get_json(&format!("/api/v1/agent-loops/{id}/context-evolution"))
                    .await?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-context-evolution", data)
                        .with_entity(id.clone()),
                )
            } else if *variables {
                show_variables(client, cli, id).await
            } else {
                show_execution(client, cli, id).await
            }
        }
        ExecutionSub::Run {
            workflow,
            input,
            background,
            stream,
        } => {
            let input_value: Option<serde_json::Value> =
                input.as_deref().and_then(|s| serde_json::from_str(s).ok());
            if *background || *stream {
                run_workflow_streaming(cli, client, workflow, input_value, *background).await
            } else {
                let typed = wf_client::models::ExecuteBody {
                    input: input_value,
                    file_scope: None,
                };
                let view = client.execute_workflow(workflow, &typed).await?;
                let data =
                    serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-run", data).with_entity(workflow.clone()),
                )
            }
        }
        ExecutionSub::Watch { id, interval, once } => {
            watch_execution(cli, client, id, *interval, *once).await
        }
        ExecutionSub::Logs {
            id,
            follow,
            limit,
            types,
            interval,
        } => {
            let validated = super::execution_logs::parse_log_types(types.as_deref())?;
            let _ = validated;
            show_execution_logs(cli, client, id, *follow, *limit, types.as_deref(), *interval).await
        }
        ExecutionSub::Cancel { id, .. } => {
            let data = cancel_execution(client, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-cancel", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Pause { id, .. } => {
            let data = pause_execution(client, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-pause", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Resume { id, .. } => {
            let data = resume_execution(client, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-resume", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Status { id } => {
            status_execution(client, cli, id).await
        }
        ExecutionSub::Hierarchy { id } => {
            let envelope = client
                .generated()
                .get_executions_id_hierarchy(id)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let view: wf_client::models::HierarchyDoc =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let data = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
            if cli.output == crate::output::OutputFormat::Text {
                print_hierarchy(&data);
                Ok(())
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-hierarchy", data).with_entity(id.clone()),
                )
            }
        }
        ExecutionSub::Subtree { id, limit, cursor } => {
            let envelope = client
                .generated()
                .get_executions_id_subtree(
                    id,
                    cursor.as_deref(),
                    limit.map(|v| v as i64),
                )
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let view: wf_client::models::SubtreeDoc =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let data = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
            if cli.output == crate::output::OutputFormat::Text {
                print_subtree(&data);
                Ok(())
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-subtree", data).with_entity(id.clone()),
                )
            }
        }
        ExecutionSub::History { id, include } => {
            let envelope = client
                .generated()
                .get_executions_id_history(id, include.as_deref())
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let view: wf_client::models::HistoryDoc =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let data = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
            if cli.output == crate::output::OutputFormat::Text {
                print_history(&data);
                Ok(())
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-history", data).with_entity(id.clone()),
                )
            }
        }
        ExecutionSub::Delete { id, force: _ } => {
            let data: serde_json::Value = client
                .delete_json(&format!("/api/v1/executions/{id}"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-delete", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Cleanup { before: _ } => {
            let removed: serde_json::Value = client
                .post_json("/api/v1/agent-loops/cleanup-completed", &serde_json::json!({}))
                .await?;
            let removed = removed.as_u64().unwrap_or(0);
            let data = serde_json::json!({"removed": removed});
            render_envelope(cli.output, OutputEnvelope::success("execution-cleanup", data))
        }
        ExecutionSub::Inspect {
            id,
            variables,
            transitions,
            context,
            call_stack,
            variable_history,
            var_name,
            context_transitions,
            node_transitions,
            memory,
        } => {
            inspect_execution(
                client,
                cli,
                id,
                *variables,
                *transitions,
                *context,
                *call_stack,
                *variable_history,
                var_name.as_deref(),
                *context_transitions,
                *node_transitions,
                *memory,
            )
            .await
        }
        ExecutionSub::Performance { id } => {
            let data = crate::cmd::analysis::performance_data(client, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-performance", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Bottleneck { id } => {
            let data = crate::cmd::analysis::bottleneck_data(client, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-bottleneck", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Errors {
            id,
            chain,
            root_cause,
            recovery,
        } => {
            let data =
                crate::cmd::analysis::errors_data(client, id, *chain, *root_cause, *recovery).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-errors", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Compare { baseline, compared } => {
            let data = crate::cmd::analysis::compare_data(client, baseline, compared).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-compare", data)
                    .with_entity(format!("{baseline}:{compared}")),
            )
        }
        ExecutionSub::Progress { id } => {
            let data = crate::cmd::analysis::progress_data(client, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-progress", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::State {
            id,
            at_iteration,
            variable,
            most_changed,
            memory,
            limit,
        } => {
            let data: serde_json::Value = if let Some(name) = variable {
                client
                    .get_json(&format!(
                        "/api/v1/executions/{id}/state-records/variables/{name}/history"
                    ))
                    .await?
            } else if *most_changed {
                client
                    .get_json(&format!(
                        "/api/v1/executions/{id}/state-records/most-changed?limit={limit}"
                    ))
                    .await?
            } else if *memory {
                client
                    .get_json(&format!("/api/v1/executions/{id}/memory"))
                    .await?
            } else if let Some(n) = at_iteration {
                client
                    .get_json(&format!(
                        "/api/v1/executions/{id}/state-records/iterations/{n}"
                    ))
                    .await?
            } else {
                client
                    .get_json(&format!("/api/v1/executions/{id}/state-records"))
                    .await?
            };
            let kind = if variable.is_some() {
                "execution-state-variable-history"
            } else if *most_changed {
                "execution-state-most-changed"
            } else if *memory {
                "execution-state-memory"
            } else if at_iteration.is_some() {
                "execution-state"
            } else {
                "execution-state-list"
            };
            render_envelope(
                cli.output,
                OutputEnvelope::success(kind, data).with_entity(id.clone()),
            )
        }
    }
}

async fn list_agent_executions(
    client: &WfClient,
    limit: Option<usize>,
    offset: Option<usize>,
    status: Option<&str>,
    agent_id: &str,
    order: Option<&str>,
) -> CliResult<serde_json::Value> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit {
        params.push(format!("limit={l}"));
    }
    if let Some(o) = offset {
        params.push(format!("offset={o}"));
    }
    if let Some(s) = status {
        params.push(format!("status={s}"));
    }
    params.push(format!("agent_id={agent_id}"));
    if let Some(o) = order {
        params.push(format!("order={o}"));
    }
    let data: serde_json::Value = client
        .get_json(&format!("/api/v1/agent-executions?{}", params.join("&")))
        .await?;
    Ok(paged_or_array(data, limit, offset, order))
}

async fn list_workflow_executions(
    client: &WfClient,
    limit: Option<usize>,
    offset: Option<usize>,
    status: Option<&str>,
    workflow: &str,
    order: Option<&str>,
) -> CliResult<serde_json::Value> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit {
        params.push(format!("limit={l}"));
    }
    if let Some(o) = offset {
        params.push(format!("offset={o}"));
    }
    if let Some(s) = status {
        params.push(format!("status={s}"));
    }
    params.push(format!("workflow_id={workflow}"));
    if let Some(o) = order {
        params.push(format!("order={o}"));
    }
    let data: serde_json::Value = client
        .get_json(&format!("/api/v1/executions?{}", params.join("&")))
        .await?;
    Ok(paged_or_array(data, limit, offset, order))
}

async fn list_loop_summaries(
    client: &WfClient,
    limit: Option<usize>,
    offset: Option<usize>,
    status: Option<&str>,
    order: Option<&str>,
) -> CliResult<serde_json::Value> {
    let mut params: Vec<String> = Vec::new();
    if let Some(s) = status {
        params.push(format!("status={s}"));
    }
    if let Some(l) = limit {
        params.push(format!("limit={l}"));
    }
    if let Some(o) = offset {
        params.push(format!("offset={o}"));
    }
    let path = if params.is_empty() {
        "/api/v1/agent-loops/summaries".to_string()
    } else {
        format!("/api/v1/agent-loops/summaries?{}", params.join("&"))
    };
    let data: serde_json::Value = client.get_json(&path).await?;
    Ok(paged_or_array(data, limit, offset, order))
}

/// Normalize list responses for display: page shells keep their shape while
/// bare arrays take client-side paging and order.
fn paged_or_array(
    data: serde_json::Value,
    limit: Option<usize>,
    offset: Option<usize>,
    order: Option<&str>,
) -> serde_json::Value {
    let items = data.get("items").and_then(|v| v.as_array()).cloned();
    match items {
        None => data,
        Some(mut entries) => {
            if order.is_some_and(|o| o.eq_ignore_ascii_case("asc")) {
                entries.reverse();
            }
            let off = offset.unwrap_or(0);
            let lim = limit.unwrap_or(usize::MAX);
            serde_json::Value::Array(entries.into_iter().skip(off).take(lim).collect())
        }
    }
}

async fn show_variables(client: &WfClient, cli: &Cli, id: &str) -> CliResult<()> {
    if let Ok(vars) = client
        .get_json::<serde_json::Value>(&format!("/api/v1/executions/{id}/variables"))
        .await
    {
        if vars.as_array().is_some_and(|v| !v.is_empty()) {
            return render_envelope(
                cli.output,
                OutputEnvelope::success("execution-variables", vars).with_entity(id.to_string()),
            );
        }
    }
    show_execution(client, cli, id).await
}

async fn show_execution(client: &WfClient, cli: &Cli, id: &str) -> CliResult<()> {
    if let Ok(summary) = client.agent_loop_summary(id).await {
        return render_envelope(
            cli.output,
            OutputEnvelope::success("execution-show", summary).with_entity(id.to_string()),
        );
    }
    match client.execution_detail(id).await {
        Ok(exec) => render_envelope(
            cli.output,
            OutputEnvelope::success("execution-show", exec).with_entity(id.to_string()),
        ),
        Err(_) => render_envelope(
            cli.output,
            OutputEnvelope::failure("execution-show", format!("execution not found: {id}")),
        ),
    }
}

async fn status_execution(client: &WfClient, cli: &Cli, id: &str) -> CliResult<()> {
    if let Ok(status) = client.execution_status(id).await {
        let text = status.as_str().unwrap_or(
            status.get("status").and_then(|s| s.as_str()).unwrap_or("unknown"),
        );
        let data = serde_json::json!({"executionId": id, "status": text, "source": "live"});
        return render_envelope(
            cli.output,
            OutputEnvelope::success("execution-status", data).with_entity(id.to_string()),
        );
    }
    if let Ok(summary) = client.agent_loop_summary(id).await {
        let status = summary
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        let data = serde_json::json!({"executionId": id, "status": status, "source": "persisted", "summary": summary});
        return render_envelope(
            cli.output,
            OutputEnvelope::success("execution-status", data).with_entity(id.to_string()),
        );
    }
    match client.execution_detail(id).await {
        Ok(exec) => {
            let status = exec.get("status").and_then(|s| s.as_str()).unwrap_or("unknown");
            let data = serde_json::json!({"executionId": id, "status": status, "source": "persisted", "execution": exec});
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-status", data).with_entity(id.to_string()),
            )
        }
        Err(_) => render_envelope(
            cli.output,
            OutputEnvelope::failure("execution-status", format!("execution not found: {id}")),
        ),
    }
}

async fn cancel_execution(client: &WfClient, id: &str) -> CliResult<serde_json::Value> {
    if let Ok(data) = client.cancel_execution(id).await {
        return Ok(data);
    }
    client.cancel_agent_loop(id).await.map_err(CliError::from)
}

async fn pause_execution(client: &WfClient, id: &str) -> CliResult<serde_json::Value> {
    if let Ok(data) = client.pause_execution(id).await {
        return Ok(data);
    }
    client.pause_agent_loop(id).await.map_err(CliError::from)
}

async fn resume_execution(client: &WfClient, id: &str) -> CliResult<serde_json::Value> {
    if let Ok(data) = client.resume_execution(id).await {
        return Ok(data);
    }
    client.resume_agent_loop(id).await.map_err(CliError::from)
}

#[allow(clippy::too_many_arguments)]
async fn inspect_execution(
    client: &WfClient,
    cli: &Cli,
    id: &str,
    variables: bool,
    transitions: bool,
    context: bool,
    call_stack: bool,
    variable_history: bool,
    var_name: Option<&str>,
    context_transitions: bool,
    node_transitions: bool,
    memory: bool,
) -> CliResult<()> {
    let mut out = serde_json::Map::new();
    let state: serde_json::Value = client
        .get_json(&format!("/api/v1/executions/{id}/state"))
        .await
        .unwrap_or(serde_json::Value::Null);
    out.insert("state".into(), state);
    if variables {
        let vars: serde_json::Value = client
            .get_json(&format!("/api/v1/executions/{id}/variables"))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("variables".into(), vars);
    }
    if transitions {
        let trans: serde_json::Value = client
            .get_json(&format!("/api/v1/executions/{id}/transitions"))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("transitions".into(), trans);
    }
    if context {
        let evo: serde_json::Value = client
            .get_json(&format!("/api/v1/executions/{id}/context-evolution"))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("contextEvolution".into(), evo);
    }
    if call_stack {
        let stack: serde_json::Value = client
            .get_json(&format!("/api/v1/executions/{id}/call-stack"))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("callStack".into(), stack);
    }
    if variable_history {
        if let Some(name) = var_name {
            let hist: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/executions/{id}/state-records/variables/{name}/history"
                ))
                .await?;
            out.insert("variableHistory".into(), hist);
        } else {
            out.insert(
                "variableHistory".into(),
                serde_json::Value::String(
                    "missing --var-name for --variable-history".to_string(),
                ),
            );
        }
    }
    if context_transitions {
        let trans: serde_json::Value = client
            .get_json(&format!("/api/v1/executions/{id}/context-transitions"))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("contextTransitions".into(), trans);
    }
    if node_transitions {
        let trans: serde_json::Value = client
            .get_json(&format!("/api/v1/executions/{id}/node-transitions"))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("nodeTransitions".into(), trans);
    }
    if memory {
        let mem: serde_json::Value = client
            .get_json(&format!("/api/v1/executions/{id}/memory"))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("memory".into(), mem);
    }
    let data = serde_json::Value::Object(out);
    render_envelope(
        cli.output,
        OutputEnvelope::success("execution-inspect", data).with_entity(id.to_string()),
    )
}
