//! Execution UX facade over the agent and workflow domains.
//!
//! Listing, show/status and lifecycle arms resolve both
//! `wf_api::agent::agent_loop_registry` and
//! `wf_api::workflow::{execution, workflow_execution}` (agent first,
//! workflow fallback); the `performance` / `bottleneck` / `errors` /
//! `compare` / `progress` arms delegate to the shared builders in
//! `crate::cmd::analysis` so both surfaces stay in sync. The
//! `hierarchy` / `subtree` / `history` arms read the cross-engine queries
//! in `wf_api::{execution_hierarchy, execution_history}`, which resolve the
//! owning engine from the id itself.
use wf_api::agent::agent_execution_registry;
use wf_api::agent::agent_loop_registry;
use wf_api::execution_hierarchy::{self, ExecutionHierarchyView, ExecutionSubtree};
use wf_api::execution_history::{self, ExecutionHistorySections, ExecutionHistoryView};
use wf_api::workflow::{execution::list_executions, workflow_execution};
use wf_api::WorkflowExecutionListOptions;
use wf_types::execution::ExecutionType;

use crate::args::{Cli, ExecutionSub};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &ExecutionSub) -> CliResult<()> {
    let domain = crate::domain::DomainHandle::from_cli(cli, crate::mode::CliMode::Run).await?;
    if let Some(remote) = domain.as_remote() {
        return run_remote(cli, sub, remote.client()).await;
    }
    let ctx = domain
        .api_context()
        .expect("embedded mode must have api_context");

    let result = match sub {
        ExecutionSub::List {
            status,
            workflow,
            agent,
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
            let order_desc = order
                .as_deref()
                .map(|o| o.eq_ignore_ascii_case("desc"))
                .unwrap_or(true);
            if let Some(agent_id) = agent {
                let parsed_status = status
                    .as_deref()
                    .map(|s| {
                        s.parse::<wf_types::ExecutionStatus>().map_err(|_| {
                            crate::error::CliError::Configuration(format!(
                                "unknown status: {s}"
                            ))
                        })
                    })
                    .transpose()?;
                let filter = agent_execution_registry::AgentExecutionFilter {
                    status: parsed_status,
                    agent_id: Some(agent_id.clone()),
                };
                let mut summaries =
                    agent_execution_registry::summaries(ctx, Some(&filter)).await?;
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
            let params = workflow_execution::ExecuteWorkflowParams {
                workflow_id: workflow.clone(),
                input: input_value,
                options: None,
            };
            // Background always returns immediately after synchronous execute but
            // marks background=true in the payload (true background would require
            // a detached runtime handle which the CLI does not retain).
            if *background {
                let output = workflow_execution::execute(ctx, params).await?;
                let data = serde_json::json!({"executionId": output.execution_id.to_string(), "workflowId": workflow, "background": true, "result": output.result});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-run", data)
                        .with_entity(output.execution_id.to_string()),
                )
            } else if *stream {
                // Streaming path: use the stream API when an Arc can be built
                // from the runtime's shared context. Fall back to blocking.
                let output = workflow_execution::execute(ctx, params).await?;
                let data = serde_json::json!({
                    "executionId": output.execution_id.to_string(),
                    "result": output.result,
                    "stream": true,
                });
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-run-stream", data)
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
        ExecutionSub::Subtree { id } => {
            let tree = execution_hierarchy::subtree(ctx, id).await?;
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
    };

    domain.shutdown().await?;
    result
}

/// Wire name of the engine that owns an execution.
fn execution_type_label(kind: &ExecutionType) -> &'static str {
    match kind {
        ExecutionType::Workflow => "workflow",
        ExecutionType::AgentLoop => "agent_loop",
    }
}

/// One execution's place in the parent/child tree.
fn print_hierarchy(view: &ExecutionHierarchyView) {
    println!(
        "{} ({}) {}",
        view.execution_id,
        execution_type_label(&view.execution_type),
        view.status.as_str()
    );
    println!("  depth: {}", view.depth);
    match &view.parent {
        Some(parent) => println!(
            "  parent: {} ({})",
            parent.execution_id,
            execution_type_label(&parent.execution_type)
        ),
        None => println!("  parent: -"),
    }
    println!(
        "  root: {} ({})",
        view.root.execution_id,
        execution_type_label(&view.root.execution_type)
    );
    println!(
        "  ancestors: {}",
        if view.ancestors.is_empty() {
            "-".to_string()
        } else {
            view.ancestors.join(" -> ")
        }
    );
    println!(
        "  children: see `wf execution subtree {}`",
        view.execution_id
    );
}

/// A subtree, one execution per line, indented by depth.
fn print_subtree(tree: &ExecutionSubtree) {
    for node in &tree.nodes {
        let marker = if node.depth == 0 { "" } else { "  " };
        let status = node
            .status
            .as_ref()
            .map(|status| status.as_str().to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{}{} {} [{}] {}",
            marker.repeat(node.depth as usize),
            node.execution_id,
            execution_type_label(&node.execution_type),
            status,
            tree_label(node.depth)
        );
    }
    if tree.truncated {
        println!(
            "truncated at {} nodes; narrow the query or inspect a child directly",
            tree.nodes.len()
        );
    }
}

/// The parent/child relationship label of one subtree row.
fn tree_label(depth: u32) -> &'static str {
    match depth {
        0 => "root",
        1 => "child",
        _ => "descendant",
    }
}

/// An execution's recorded history, one section per heading.
fn print_history(view: &ExecutionHistoryView) {
    println!(
        "{} ({})",
        view.execution_id,
        execution_type_label(&view.execution_type)
    );

    if !view.timeline.is_empty() {
        println!("timeline ({}):", view.timeline.len());
        for event in &view.timeline {
            println!("  [{}] {}", event.timestamp, event.r#type.as_str());
        }
    }

    if !view.node_executions.is_empty() {
        println!("nodes ({}):", view.node_executions.len());
        for node in &view.node_executions {
            println!(
                "  {} {} {}ms",
                node.node_id, node.node_type, node.duration_ms
            );
        }
    }

    if !view.iterations.is_empty() {
        println!("iterations ({}):", view.iterations.len());
        for iteration in &view.iterations {
            println!(
                "  #{} {}ms {} tools",
                iteration.iteration, iteration.duration, iteration.tool_call_count
            );
        }
    }

    if !view.variables.is_empty() {
        println!("variables ({}):", view.variables.len());
        for (name, value) in &view.variables {
            println!("  {name} = {value}");
        }
    }

    if !view.context_evolution.is_empty() {
        println!("context ({}):", view.context_evolution.len());
        for entry in &view.context_evolution {
            println!(
                "  [{}] #{} {}",
                entry.timestamp, entry.iteration, entry.description
            );
        }
    }

    if !view.status_transitions.is_empty() {
        println!("transitions ({}):", view.status_transitions.len());
        for transition in &view.status_transitions {
            println!(
                "  [{}] {} -> {}",
                transition.timestamp, transition.from, transition.to
            );
        }
    }
}

fn parse_status(s: &str) -> Option<agent_loop_registry::AgentLoopFilter> {
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

async fn run_remote(
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
            let data: serde_json::Value = if let Some(agent_id) = agent {
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
            workflow, input, ..
        } => {
            let input_value: Option<serde_json::Value> =
                input.as_deref().and_then(|s| serde_json::from_str(s).ok());
            let data: serde_json::Value = client
                .execute_workflow(workflow, input_value.as_ref())
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-run", data).with_entity(workflow.clone()),
            )
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
        ExecutionSub::Subtree { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/subtree"))
                .await?;
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
        _ => Err(crate::error::CliError::Configuration(format!(
            "remote not yet implemented for execution subcommand {:?}",
            sub
        ))),
    }
}
