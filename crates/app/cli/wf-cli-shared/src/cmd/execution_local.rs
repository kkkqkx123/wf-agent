//! Embedded execution management (local runtime).

use wf_api::agent::agent_execution_registry;
use wf_api::agent::agent_loop_registry;
use wf_api::execution_hierarchy;
use wf_api::execution_history::{self, ExecutionHistorySections};
use wf_api::workflow::{execution::list_executions, workflow_execution};
use wf_api::WorkflowExecutionListOptions;

use super::execution_logs::{parse_log_types, show_execution_logs};
use super::execution_stream::{parse_status, run_workflow_streaming, watch_execution};
use super::execution_view::{print_hierarchy, print_history, print_subtree};
use crate::args::{Cli, ExecutionSub};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub(crate) async fn run_embedded(
    cli: &Cli,
    ctx: &wf_api::ApiContext,
    domain: &crate::domain::DomainHandle,
    sub: &ExecutionSub,
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
            if *unified {
                if workflow.is_some() || agent.is_some() {
                    return Err(crate::error::CliError::Configuration(
                        "--unified cannot be combined with --workflow or --agent".to_string(),
                    ));
                }
                let parsed_status = status
                    .as_deref()
                    .map(|s| {
                        s.parse::<wf_types::ExecutionStatus>().map_err(|_| {
                            crate::error::CliError::Configuration(format!("unknown status: {s}"))
                        })
                    })
                    .transpose()?;
                let order_desc = order
                    .as_deref()
                    .map(|o| o.eq_ignore_ascii_case("desc"))
                    .unwrap_or(true);
                let filter = wf_api::execution_listing::UnifiedExecutionFilter {
                    status: parsed_status,
                    execution_type: None,
                    started_from: None,
                    started_to: None,
                };
                let (mut entries, _) = wf_api::execution_listing::list_unified(
                    ctx,
                    Some(&filter),
                    limit.unwrap_or(usize::MAX),
                    offset.unwrap_or(0),
                )
                .await?;
                if !order_desc {
                    entries.reverse();
                }
                let data = serde_json::to_value(&entries)?;
                return render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-list", data),
                );
            }
            let order_desc = order
                .as_deref()
                .map(|o| o.eq_ignore_ascii_case("desc"))
                .unwrap_or(true);
            if let Some(agent_id) = agent {
                let parsed_status = status
                    .as_deref()
                    .map(|s| {
                        s.parse::<wf_types::ExecutionStatus>().map_err(|_| {
                            crate::error::CliError::Configuration(format!("unknown status: {s}"))
                        })
                    })
                    .transpose()?;
                let filter = agent_execution_registry::AgentExecutionFilter {
                    status: parsed_status,
                    agent_id: Some(agent_id.clone()),
                    ..agent_execution_registry::AgentExecutionFilter::default()
                };
                let mut summaries = agent_execution_registry::summaries(ctx, Some(&filter)).await?;
                if !order_desc {
                    summaries.reverse();
                }
                let off = offset.unwrap_or(0);
                let lim = limit.unwrap_or(usize::MAX);
                let paged: Vec<_> = summaries.into_iter().skip(off).take(lim).collect();
                let data = serde_json::to_value(&paged)?;
                render_envelope(cli.output, OutputEnvelope::success("execution-list", data))
            } else if let Some(wf) = workflow {
                let executions = list_executions(
                    ctx,
                    Some(WorkflowExecutionListOptions {
                        workflow_id_filter: Some(wf.clone()),
                        status_filter: status.clone(),
                        order_desc: Some(order_desc),
                        ..Default::default()
                    }),
                )
                .await?;
                let mut filtered: Vec<wf_types::WorkflowExecution> = if let Some(s) = status {
                    executions
                        .into_iter()
                        .filter(|e| e.status.as_str().eq_ignore_ascii_case(s))
                        .collect()
                } else {
                    executions
                };
                // Final in-memory sort stabilizes the storage order for display.
                filtered.sort_by_key(|e| e.started_at);
                if order_desc {
                    filtered.reverse();
                }
                let off = offset.unwrap_or(0);
                let lim = limit.unwrap_or(usize::MAX);
                let paged: Vec<_> = filtered.into_iter().skip(off).take(lim).collect();
                let data = serde_json::to_value(&paged)?;
                render_envelope(cli.output, OutputEnvelope::success("execution-list", data))
            } else {
                let filter = status.as_deref().and_then(parse_status);
                let mut summaries = agent_loop_registry::summaries(ctx, filter.as_ref()).await?;
                summaries.sort_by_key(|s| s.start_time.unwrap_or(0));
                if order_desc {
                    summaries.reverse();
                }
                let off = offset.unwrap_or(0);
                let lim = limit.unwrap_or(usize::MAX);
                let paged: Vec<_> = summaries.into_iter().skip(off).take(lim).collect();
                let data = serde_json::to_value(&paged)?;
                render_envelope(cli.output, OutputEnvelope::success("execution-list", data))
            }
        }
        ExecutionSub::Show {
            id,
            timeline,
            iterations,
            variables,
            context_evolution,
        } => {
            if *timeline {
                let history = agent_loop_registry::execution_timeline(ctx, id)
                    .await
                    .unwrap_or_default();
                let data = serde_json::to_value(&history)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-timeline", data).with_entity(id.clone()),
                )
            } else if *iterations {
                let history = agent_loop_registry::iteration_history(ctx, id)
                    .await
                    .unwrap_or_default();
                let data = serde_json::to_value(&history)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-iterations", data).with_entity(id.clone()),
                )
            } else if *context_evolution {
                let evo = agent_loop_registry::context_evolution(ctx, id).await?;
                let data = serde_json::to_value(&evo)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-context-evolution", data)
                        .with_entity(id.clone()),
                )
            } else if *variables {
                // Show variables: prefer workflow variables, fallback to agent.
                let vars = wf_api::workflow::execution_state::workflow_execution_variables(ctx, id)
                    .await
                    .unwrap_or_default();
                if !vars.is_empty() {
                    let data = serde_json::to_value(&vars)?;
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("execution-variables", data)
                            .with_entity(id.clone()),
                    )
                } else {
                    let state =
                        wf_api::workflow::execution_state::workflow_execution_get_state(ctx, id)
                            .await?;
                    let data = serde_json::to_value(&state)?;
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("execution-show", data).with_entity(id.clone()),
                    )
                }
            } else {
                let summary = agent_loop_registry::summary(ctx, id).await?;
                match summary {
                    Some(s) => {
                        let data = serde_json::to_value(&s)?;
                        render_envelope(
                            cli.output,
                            OutputEnvelope::success("execution-show", data).with_entity(id.clone()),
                        )
                    }
                    None => {
                        if let Ok(exec) = wf_api::workflow::get_execution(ctx, id).await {
                            let data = serde_json::to_value(&exec)?;
                            render_envelope(
                                cli.output,
                                OutputEnvelope::success("execution-show", data)
                                    .with_entity(id.clone()),
                            )
                        } else {
                            render_envelope(
                                cli.output,
                                OutputEnvelope::failure(
                                    "execution-show",
                                    format!("execution not found: {id}"),
                                ),
                            )
                        }
                    }
                }
            }
        }
        ExecutionSub::Run {
            workflow,
            input,
            background,
            stream,
        } => {
            let input_value = if let Some(json) = input {
                Some(serde_json::from_str::<serde_json::Value>(json)?)
            } else {
                None
            };
            if *background || *stream {
                run_workflow_streaming(cli, domain, workflow, input_value, *background).await
            } else {
                let params = workflow_execution::ExecuteWorkflowParams {
                    workflow_id: workflow.clone(),
                    input: input_value,
                    options: None,
                };
                let output = workflow_execution::execute(ctx, params).await?;
                let data = serde_json::json!({
                    "executionId": output.execution_id.to_string(),
                    "result": output.result,
                });
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-run", data)
                        .with_entity(output.execution_id.to_string()),
                )
            }
        }
        ExecutionSub::Watch { id, interval, once } => {
            watch_execution(cli, ctx, id, *interval, *once).await
        }
        ExecutionSub::Logs {
            id,
            follow,
            limit,
            types,
            interval,
        } => {
            let event_types = parse_log_types(types.as_deref())?;
            show_execution_logs(cli, ctx, id, *follow, *limit, event_types, *interval).await
        }
        ExecutionSub::Status { id } => match workflow_execution::status(ctx, id).await {
            Ok(s) => {
                let data = serde_json::json!({"executionId": id, "status": format!("{s:?}"), "source": "live"});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-status", data).with_entity(id.clone()),
                )
            }
            Err(_) => {
                let s = agent_loop_registry::summary(ctx, id).await?;
                if let Some(sum) = s {
                    let data = serde_json::json!({"executionId": id, "status": format!("{:?}", sum.status), "source": "persisted", "summary": sum});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("execution-status", data).with_entity(id.clone()),
                    )
                } else if let Ok(exec) = wf_api::workflow::get_execution(ctx, id).await {
                    let data = serde_json::json!({"executionId": id, "status": exec.status.as_str(), "source": "persisted", "execution": exec});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("execution-status", data).with_entity(id.clone()),
                    )
                } else {
                    render_envelope(
                        cli.output,
                        OutputEnvelope::failure(
                            "execution-status",
                            format!("execution not found: {id}"),
                        ),
                    )
                }
            }
        },
        ExecutionSub::Cancel { id, reason: _ } => {
            let _ = workflow_execution::cancel(ctx, id).await;
            let _ = agent_loop_registry::update_status(ctx, id, wf_types::ExecutionStatus::Failed)
                .await;
            let data = serde_json::json!({"executionId": id, "cancelled": true});
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-cancel", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Pause { id, reason: _ } => {
            match workflow_execution::pause(ctx, id).await {
                Ok(()) => {
                    let data = serde_json::json!({"executionId": id, "paused": true});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("execution-pause", data).with_entity(id.clone()),
                    )
                }
                Err(_) => {
                    // Fall back to agent pause.
                    agent_loop_registry::update_status(ctx, id, wf_types::ExecutionStatus::Paused)
                        .await?;
                    let data = serde_json::json!({"executionId": id, "paused": true});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("execution-pause", data).with_entity(id.clone()),
                    )
                }
            }
        }
        ExecutionSub::Resume { id, reason: _ } => {
            if let Ok(output) = workflow_execution::resume(ctx, id).await {
                let data = serde_json::json!({"executionId": id, "result": output.result});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-resume", data).with_entity(id.clone()),
                )
            } else {
                agent_loop_registry::update_status(ctx, id, wf_types::ExecutionStatus::Running)
                    .await?;
                let data = serde_json::json!({"executionId": id, "resumed": true});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-resume", data).with_entity(id.clone()),
                )
            }
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
            let mut out = serde_json::Map::new();
            let state =
                wf_api::workflow::execution_state::workflow_execution_get_state(ctx, id).await;
            out.insert(
                "state".into(),
                serde_json::to_value(state.ok()).unwrap_or(serde_json::Value::Null),
            );
            if *variables {
                let vars = wf_api::workflow::execution_state::workflow_execution_variables(ctx, id)
                    .await
                    .ok();
                out.insert(
                    "variables".into(),
                    serde_json::to_value(&vars).unwrap_or(serde_json::Value::Null),
                );
            }
            if *transitions {
                let trans =
                    wf_api::workflow::execution_state::workflow_execution_status_transitions(
                        ctx, id,
                    )
                    .await
                    .ok();
                out.insert(
                    "transitions".into(),
                    serde_json::to_value(&trans).unwrap_or(serde_json::Value::Null),
                );
            }
            if *context {
                let evo =
                    wf_api::workflow::execution_state::workflow_execution_get_context_evolution(
                        ctx, id,
                    )
                    .await
                    .ok();
                out.insert(
                    "contextEvolution".into(),
                    serde_json::to_value(&evo).unwrap_or(serde_json::Value::Null),
                );
            }
            if *call_stack {
                let stack = wf_api::infra::state_tracker::get_call_stack(ctx, id)
                    .await
                    .ok();
                out.insert(
                    "callStack".into(),
                    serde_json::to_value(&stack).unwrap_or(serde_json::Value::Null),
                );
            }
            if *variable_history {
                if let Some(name) = var_name {
                    let hist =
                        wf_api::infra::state_tracker::get_variable_history(ctx, id, name).await?;
                    out.insert(
                        "variableHistory".into(),
                        serde_json::to_value(&hist).unwrap_or(serde_json::Value::Null),
                    );
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
                let t =
                    wf_api::workflow::execution_state::workflow_execution_get_context_transitions(
                        ctx, id,
                    )
                    .await
                    .ok();
                out.insert(
                    "contextTransitions".into(),
                    serde_json::to_value(&t).unwrap_or(serde_json::Value::Null),
                );
            }
            if *node_transitions {
                let t = wf_api::workflow::execution_state::workflow_execution_get_node_transitions(
                    ctx, id, None, None,
                )
                .await
                .ok();
                out.insert(
                    "nodeTransitions".into(),
                    serde_json::to_value(&t).unwrap_or(serde_json::Value::Null),
                );
            }
            if *memory {
                let cur = wf_api::infra::state_tracker::get_memory_usage(ctx, id)
                    .await
                    .ok()
                    .flatten();
                let peak = wf_api::infra::state_tracker::get_peak_memory_usage(ctx, id)
                    .await
                    .ok()
                    .flatten();
                out.insert(
                    "memory".into(),
                    serde_json::json!({"current": cur, "peak": peak}),
                );
            }
            let data = serde_json::Value::Object(out);
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-inspect", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Hierarchy { id } => {
            let view = execution_hierarchy::hierarchy(ctx, id).await?;
            if cli.output == crate::output::OutputFormat::Text {
                print_hierarchy(&view);
                Ok(())
            } else {
                let data = serde_json::to_value(&view)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-hierarchy", data).with_entity(id.clone()),
                )
            }
        }
        ExecutionSub::Subtree { id, limit, cursor } => {
            let offset = match cursor {
                None => 0,
                Some(raw) if raw.is_empty() => 0,
                Some(raw) => raw.parse::<usize>().map_err(|_| {
                    crate::error::CliError::Configuration(format!("unknown cursor: {raw}"))
                })?,
            };
            let tree =
                execution_hierarchy::subtree(ctx, id, limit.unwrap_or(usize::MAX), offset).await?;
            if cli.output == crate::output::OutputFormat::Text {
                print_subtree(&tree);
                Ok(())
            } else {
                let data = serde_json::to_value(&tree)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-subtree", data).with_entity(id.clone()),
                )
            }
        }
        ExecutionSub::History { id, include } => {
            let sections = ExecutionHistorySections::parse(include.as_deref())?;
            let view = execution_history::history(ctx, id, &sections).await?;
            if cli.output == crate::output::OutputFormat::Text {
                print_history(&view);
                Ok(())
            } else {
                let data = serde_json::to_value(&view)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-history", data).with_entity(id.clone()),
                )
            }
        }
        ExecutionSub::Performance { id } => {
            let data = crate::cmd::analysis::performance_data(ctx, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-performance", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Bottleneck { id } => {
            let data = crate::cmd::analysis::bottleneck_data(ctx, id).await?;
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
                crate::cmd::analysis::errors_data(ctx, id, *chain, *root_cause, *recovery).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-errors", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Compare { baseline, compared } => {
            let data = crate::cmd::analysis::compare_data(ctx, baseline, compared).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-compare", data)
                    .with_entity(format!("{baseline}:{compared}")),
            )
        }
        ExecutionSub::Progress { id } => {
            let data = crate::cmd::analysis::progress_data(ctx, id).await?;
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
            if let Some(name) = variable {
                let hist =
                    wf_api::infra::state_tracker::get_variable_history(ctx, id, name).await?;
                let data = serde_json::to_value(&hist)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-state-variable-history", data)
                        .with_entity(id.clone()),
                )
            } else if *most_changed {
                let list =
                    wf_api::infra::state_tracker::get_most_changed_variables(ctx, id, *limit)
                        .await?;
                let data = serde_json::to_value(&list)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-state-most-changed", data)
                        .with_entity(id.clone()),
                )
            } else if *memory {
                let cur = wf_api::infra::state_tracker::get_memory_usage(ctx, id)
                    .await
                    .ok()
                    .flatten();
                let peak = wf_api::infra::state_tracker::get_peak_memory_usage(ctx, id)
                    .await
                    .ok()
                    .flatten();
                let data = serde_json::json!({"current": cur, "peak": peak});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-state-memory", data).with_entity(id.clone()),
                )
            } else if let Some(n) = at_iteration {
                let record =
                    wf_api::infra::state_tracker::get_state_at_iteration(ctx, id, *n as u32)
                        .await?;
                let data = serde_json::to_value(&record)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-state", data).with_entity(id.clone()),
                )
            } else {
                let records = wf_api::infra::state_tracker::list_state_records(ctx, id).await?;
                let data = serde_json::to_value(&records)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-state-list", data).with_entity(id.clone()),
                )
            }
        }
        ExecutionSub::Delete { id, force: _ } => {
            let deleted = wf_api::workflow::execution::delete_execution_full(ctx, id).await?;
            let data = serde_json::json!({"deleted": id, "ok": deleted});
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-delete", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Cleanup { before: _ } => {
            let removed = agent_loop_registry::cleanup_completed(ctx).await?;
            let data = serde_json::json!({"removed": removed});
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-cleanup", data),
            )
        }
    }
}
