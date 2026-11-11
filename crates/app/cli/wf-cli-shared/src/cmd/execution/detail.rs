//! Execution detail views: show, variables, status and inspect.

use crate::args::Cli;
use crate::cmd::render::{render_envelope, render_failure};
use crate::error::CliResult;
use crate::output::OutputEnvelope;
use wf_client::WfClient;

pub(crate) async fn show_variables(client: &WfClient, cli: &Cli, id: &str) -> CliResult<()> {
    // Escape hatch: variables are an entity payload with caller-defined shape.
    if let Ok(vars) = client
        .get_json::<serde_json::Value>(&crate::paths::executions_id_variables(id))
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

pub(crate) async fn show_execution(client: &WfClient, cli: &Cli, id: &str) -> CliResult<()> {
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
        Err(_) => render_failure(
            cli.output,
            "execution-show",
            format!("execution not found: {id}"),
        ),
    }
}

pub(crate) async fn status_execution(client: &WfClient, cli: &Cli, id: &str) -> CliResult<()> {
    if let Ok(status) = client.execution_status(id).await {
        let text = status;
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
            let status = exec
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let data = serde_json::json!({"executionId": id, "status": status, "source": "persisted", "execution": exec});
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-status", data).with_entity(id.to_string()),
            )
        }
        Err(_) => render_failure(
            cli.output,
            "execution-status",
            format!("execution not found: {id}"),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn inspect_execution(
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
    memory: bool,
) -> CliResult<()> {
    let mut out = serde_json::Map::new();
    // Escape hatch: inspect merges entity payloads and dynamic observations.
    let state: serde_json::Value = client
        .get_json(&crate::paths::executions_id_state(id))
        .await
        .unwrap_or(serde_json::Value::Null);
    out.insert("state".into(), state);
    if variables {
        let vars: serde_json::Value = client
            .get_json(&crate::paths::executions_id_variables(id))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("variables".into(), vars);
    }
    if transitions {
        let trans: serde_json::Value = client
            .get_json(&crate::paths::executions_id_transitions(id))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("transitions".into(), trans);
    }
    if context {
        let evo: serde_json::Value = client
            .get_json(&crate::paths::executions_id_context_evolution(id))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("contextEvolution".into(), evo);
    }
    if call_stack {
        let stack: serde_json::Value = client
            .get_json(&crate::paths::executions_id_call_stack(id))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("callStack".into(), stack);
    }
    if variable_history {
        if let Some(name) = var_name {
            let hist: serde_json::Value = client
                .get_json(&crate::paths::executions_id_state_record_variable_history(
                    id, name,
                ))
                .await?;
            out.insert("variableHistory".into(), hist);
        } else {
            out.insert(
                "variableHistory".into(),
                serde_json::Value::String("missing --var-name for --variable-history".to_string()),
            );
        }
    }
    if context_transitions {
        let trans: serde_json::Value = client
            .get_json(&crate::paths::executions_id_context_transitions(id))
            .await
            .unwrap_or(serde_json::Value::Null);
        out.insert("contextTransitions".into(), trans);
    }
    if memory {
        let mem: serde_json::Value = client
            .get_json(&crate::paths::executions_id_memory(id))
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
