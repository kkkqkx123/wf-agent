use wf_api::agent::agent_loop_registry;
use wf_api::workflow::{execution::list_executions, workflow_execution};
use wf_api::WorkflowExecutionListOptions;

use crate::args::{Cli, ExecutionSub};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &ExecutionSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();

    let result = match sub {
        ExecutionSub::List { status, workflow } => {
            if let Some(wf) = workflow {
                let executions = list_executions(
                    ctx,
                    Some(WorkflowExecutionListOptions {
                        workflow_id_filter: Some(wf.clone()),
                        ..Default::default()
                    }),
                )
                .await?;
                let filtered = if let Some(s) = status {
                    executions
                        .into_iter()
                        .filter(|e| e.status.as_str().eq_ignore_ascii_case(s))
                        .collect::<Vec<_>>()
                } else {
                    executions
                };
                let data = serde_json::to_value(&filtered)?;
                render_envelope(cli.output, OutputEnvelope::success("execution-list", data))
            } else {
                let filter = status.as_deref().and_then(parse_status);
                let summaries = agent_loop_registry::summaries(ctx, filter.as_ref()).await?;
                let data = serde_json::to_value(&summaries)?;
                render_envelope(cli.output, OutputEnvelope::success("execution-list", data))
            }
        }
        ExecutionSub::Show {
            id,
            timeline,
            iterations,
            variables: _,
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
        } => {
            let input_value = if let Some(json) = input {
                Some(serde_json::from_str::<serde_json::Value>(json)?)
            } else {
                None
            };
            let params = workflow_execution::ExecuteWorkflowParams {
                workflow_id: workflow.clone(),
                input: input_value,
                options: None,
            };
            if *background {
                let output = workflow_execution::execute(ctx, params).await?;
                let data = serde_json::json!({"executionId": output.execution_id.to_string(), "workflowId": workflow, "background": true, "result": output.result});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-run", data)
                        .with_entity(output.execution_id.to_string()),
                )
            } else {
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
        ExecutionSub::Status { id } => {
            let status = workflow_execution::status(ctx, id).await;
            match status {
                Ok(s) => {
                    let data = serde_json::json!({"executionId": id, "status": format!("{s:?}")});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("execution-status", data).with_entity(id.clone()),
                    )
                }
                Err(_) => {
                    let s = agent_loop_registry::summary(ctx, id).await?;
                    if let Some(sum) = s {
                        let data = serde_json::to_value(&sum)?;
                        render_envelope(
                            cli.output,
                            OutputEnvelope::success("execution-status", data)
                                .with_entity(id.clone()),
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
            }
        }
        ExecutionSub::Cancel { id } => {
            let _ = workflow_execution::cancel(ctx, id).await;
            let _ = agent_loop_registry::update_status(ctx, id, wf_types::ExecutionStatus::Failed)
                .await;
            let data = serde_json::json!({"executionId": id, "cancelled": true});
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-cancel", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Pause { id } => {
            workflow_execution::pause(ctx, id).await?;
            let data = serde_json::json!({"executionId": id, "paused": true});
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-pause", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Resume { id } => {
            let output = workflow_execution::resume(ctx, id).await?;
            let data = serde_json::json!({"executionId": id, "result": output.result});
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-resume", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Inspect {
            id,
            variables,
            transitions,
            context,
            call_stack,
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
            let data = serde_json::Value::Object(out);
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-inspect", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Performance { id } => {
            let profile = wf_api::analysis::performance::analyze_performance(ctx, id).await?;
            let data = serde_json::to_value(&profile)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-performance", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Bottleneck { id } => {
            let bottlenecks = wf_api::analysis::performance::identify_bottlenecks(ctx, id).await?;
            let data = serde_json::to_value(&bottlenecks)?;
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
            let stats = wf_api::analysis::error_analysis::workflow_error_stats(ctx, id).await?;
            out.insert(
                "stats".into(),
                serde_json::to_value(&stats).unwrap_or(serde_json::Value::Null),
            );
            if *chain {
                let c = wf_api::analysis::error_analysis::get_error_chain(ctx, id, None).await?;
                out.insert(
                    "chain".into(),
                    serde_json::to_value(&c).unwrap_or(serde_json::Value::Null),
                );
            }
            if *root_cause {
                let rc = wf_api::analysis::error_analysis::analyze_root_cause(ctx, id).await?;
                out.insert(
                    "rootCause".into(),
                    serde_json::to_value(&rc).unwrap_or(serde_json::Value::Null),
                );
            }
            if *recovery {
                let recs =
                    wf_api::analysis::error_analysis::recovery_recommendations(ctx, id).await?;
                out.insert(
                    "recovery".into(),
                    serde_json::to_value(&recs).unwrap_or(serde_json::Value::Null),
                );
            }
            if !*chain && !*root_cause && !*recovery {
                let adv =
                    wf_api::analysis::error_analysis::get_advanced_error_analysis(ctx, id).await?;
                out.insert(
                    "advanced".into(),
                    serde_json::to_value(&adv).unwrap_or(serde_json::Value::Null),
                );
            }
            let data = serde_json::Value::Object(out);
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-errors", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::Compare { baseline, compared } => {
            let cmp = wf_api::analysis::performance::compare(ctx, baseline, compared).await?;
            let data = serde_json::to_value(&cmp)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-compare", data)
                    .with_entity(format!("{baseline}:{compared}")),
            )
        }
        ExecutionSub::Progress { id } => {
            let metrics = wf_api::analysis::progress::get_progress(ctx, id).await?;
            let data = serde_json::to_value(&metrics)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-progress", data).with_entity(id.clone()),
            )
        }
        ExecutionSub::State { id, at_iteration } => {
            if let Some(n) = at_iteration {
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
    };

    adapter.shutdown().await?;
    result
}

fn parse_status(s: &str) -> Option<agent_loop_registry::AgentLoopFilter> {
    let status = match s.to_ascii_lowercase().as_str() {
        "running" => wf_types::ExecutionStatus::Running,
        "paused" => wf_types::ExecutionStatus::Paused,
        "completed" => wf_types::ExecutionStatus::Completed,
        "failed" => wf_types::ExecutionStatus::Failed,
        _ => return None,
    };
    Some(agent_loop_registry::AgentLoopFilter {
        ids: None,
        status: Some(status),
        profile_id: None,
        tags: None,
        created_at_range: None,
    })
}
