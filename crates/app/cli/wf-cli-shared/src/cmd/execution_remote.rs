//! Remote execution management over HTTP.

use super::execution_logs::show_execution_logs_remote;
use super::execution_stream::{run_remote_stream, watch_execution_remote};
use crate::args::{Cli, ExecutionSub};
use crate::error::CliResult;

pub(crate) async fn run_remote(
    cli: &Cli,
    sub: &ExecutionSub,
    client: &crate::remote::RemoteClient,
) -> CliResult<()> {
    use crate::cmd::render::render_envelope;
    use crate::output::OutputEnvelope;
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
                let mut params: Vec<String> = Vec::new();
                if let Some(l) = limit {
                    params.push(format!("limit={l}"));
                }
                if let Some(o) = offset {
                    params.push(format!("cursor={o}"));
                }
                if let Some(s) = status {
                    params.push(format!("status={s}"));
                }
                let path = if params.is_empty() {
                    "/api/v1/unified-executions".to_string()
                } else {
                    format!("/api/v1/unified-executions?{}", params.join("&"))
                };
                let mut page: serde_json::Value = client.get_json(&path).await?;
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
                client
                    .list_agent_executions(
                        *limit,
                        *offset,
                        status.as_deref(),
                        Some(agent_id.as_str()),
                        order.as_deref(),
                    )
                    .await?
            } else {
                client
                    .list_executions(
                        *limit,
                        *offset,
                        status.as_deref(),
                        workflow.as_deref(),
                        order.as_deref(),
                    )
                    .await?
            };
            render_envelope(cli.output, OutputEnvelope::success("execution-list", data))
        }
        ExecutionSub::Show { id, .. } => {
            let data: serde_json::Value = client.get_execution(id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-show", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Run {
            workflow,
            input,
            background,
            stream,
        } => {
            let input_value: Option<serde_json::Value> =
                input.as_deref().and_then(|s| serde_json::from_str(s).ok());
            if *background {
                let body = serde_json::json!({ "input": input_value });
                let data: serde_json::Value = client
                    .post_json(
                        &format!("/api/v1/workflows/{workflow}/execute/background"),
                        &body,
                    )
                    .await?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-run", data).with_entity(workflow.clone()),
                )
            } else if *stream {
                run_remote_stream(cli, client, workflow, input_value).await
            } else {
                let data: serde_json::Value = client
                    .execute_workflow(workflow, input_value.as_ref())
                    .await?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-run", data).with_entity(workflow.clone()),
                )
            }
        }
        ExecutionSub::Watch { id, interval, once } => {
            watch_execution_remote(cli, client, id, *interval, *once).await
        }
        ExecutionSub::Logs {
            id,
            follow,
            limit,
            types,
            interval,
        } => {
            show_execution_logs_remote(
                cli,
                client,
                id,
                *follow,
                *limit,
                types.as_deref(),
                *interval,
            )
            .await
        }
        ExecutionSub::Cancel { id, .. } => {
            let data = client.cancel_execution(id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-cancel", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Pause { id, .. } => {
            let data = client.pause_execution(id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-pause", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Resume { id, .. } => {
            let data = client.resume_execution(id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-resume", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Status { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{}/status", id))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-status", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Hierarchy { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/hierarchy"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-hierarchy", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Subtree { id, limit, cursor } => {
            let mut params: Vec<String> = Vec::new();
            if let Some(l) = limit {
                params.push(format!("limit={l}"));
            }
            if let Some(c) = cursor {
                params.push(format!("cursor={c}"));
            }
            let path = if params.is_empty() {
                format!("/api/v1/executions/{id}/subtree")
            } else {
                format!("/api/v1/executions/{id}/subtree?{}", params.join("&"))
            };
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-subtree", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::History { id, include } => {
            let path = match include {
                Some(sections) => {
                    format!("/api/v1/executions/{id}/history?include={sections}")
                }
                None => format!("/api/v1/executions/{id}/history"),
            };
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-history", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Delete { id, .. } => {
            let data = client.delete_execution(id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-delete", data).with_entity(id.clone()),
            )
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
            if *node_transitions {
                return Err(crate::error::CliError::Configuration(
                    "remote --node-transitions needs a node id; use the node transitions endpoint directly".to_string(),
                ));
            }
            let mut out = serde_json::Map::new();
            let state: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/state"))
                .await?;
            out.insert("state".into(), state);
            if *variables {
                let vars: serde_json::Value = client
                    .get_json(&format!("/api/v1/executions/{id}/variables"))
                    .await?;
                out.insert("variables".into(), vars);
            }
            if *transitions {
                let trans: serde_json::Value = client
                    .get_json(&format!("/api/v1/executions/{id}/transitions"))
                    .await?;
                out.insert("transitions".into(), trans);
            }
            if *context {
                let evo: serde_json::Value = client
                    .get_json(&format!("/api/v1/executions/{id}/context-evolution"))
                    .await?;
                out.insert("contextEvolution".into(), evo);
            }
            if *call_stack {
                let stack: serde_json::Value = client
                    .get_json(&format!("/api/v1/executions/{id}/call-stack"))
                    .await?;
                out.insert("callStack".into(), stack);
            }
            if *variable_history {
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
            if *context_transitions {
                let trans: serde_json::Value = client
                    .get_json(&format!("/api/v1/executions/{id}/context-transitions"))
                    .await?;
                out.insert("contextTransitions".into(), trans);
            }
            if *memory {
                let mem: serde_json::Value = client
                    .get_json(&format!("/api/v1/executions/{id}/memory"))
                    .await?;
                out.insert("memory".into(), mem);
            }
            let data = serde_json::Value::Object(out);
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-inspect", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Performance { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/performance"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-performance", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Bottleneck { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/performance/bottlenecks"))
                .await?;
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
            let mut out = serde_json::Map::new();
            let stats: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/error-analysis"))
                .await?;
            out.insert("stats".into(), stats);
            if *chain {
                let chain_data: serde_json::Value = client
                    .get_json(&format!("/api/v1/executions/{id}/error-analysis/context"))
                    .await?;
                out.insert("chain".into(), chain_data);
            }
            if *root_cause {
                let rc: serde_json::Value = client
                    .get_json(&format!(
                        "/api/v1/executions/{id}/error-analysis/root-cause"
                    ))
                    .await?;
                out.insert("rootCause".into(), rc);
            }
            if *recovery {
                let recs: serde_json::Value = client
                    .get_json(&format!(
                        "/api/v1/executions/{id}/error-analysis/recovery-recommendations"
                    ))
                    .await?;
                out.insert("recovery".into(), recs);
            }
            if !chain && !root_cause && !recovery {
                let advanced: serde_json::Value = client
                    .get_json(&format!("/api/v1/executions/{id}/error-analysis/advanced"))
                    .await?;
                out.insert("advanced".into(), advanced);
            }
            let data = serde_json::Value::Object(out);
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-errors", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Compare { baseline, compared } => {
            let data: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/analysis/performance/compare?baseline={baseline}&compared={compared}"
                ))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-compare", data)
                    .with_entity(format!("{baseline}:{compared}")),
            )
        }
        ExecutionSub::Progress { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/progress"))
                .await?;
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
        _ => Err(crate::error::CliError::Configuration(format!(
            "remote not yet implemented for execution subcommand {:?}",
            sub
        ))),
    }
}
