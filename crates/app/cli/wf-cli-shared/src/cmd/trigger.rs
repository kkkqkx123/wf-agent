//! Trigger domain commands: template registry plus the firing ledger.

use crate::args::{Cli, TriggerSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &TriggerSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        TriggerSub::List {
            trigger_type,
            category,
            tags,
            enabled,
            name,
        } => {
            let mut params: Vec<String> = Vec::new();
            if let Some(t) = trigger_type {
                params.push(format!("trigger_type={t}"));
            }
            let path = if params.is_empty() {
                "/api/v1/templates/trigger".to_string()
            } else {
                format!("/api/v1/templates/trigger?{}", params.join("&"))
            };
            let data: serde_json::Value = client.get_json(&path).await?;
            let items = data
                .get("items")
                .and_then(|v| v.as_array())
                .cloned()
                .or_else(|| data.as_array().cloned())
                .unwrap_or_default();
            let wanted_tags: Vec<String> = tags
                .as_deref()
                .map(|s| {
                    s.split(',')
                        .map(|v| v.trim().to_string())
                        .filter(|v| !v.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let filtered: Vec<_> = items
                .into_iter()
                .filter(|item| {
                    if let Some(cat) = category {
                        if item.get("category").and_then(|v| v.as_str()) != Some(cat.as_str()) {
                            return false;
                        }
                    }
                    if !wanted_tags.is_empty() {
                        let item_tags: Vec<String> = item
                            .get("tags")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
                            })
                            .unwrap_or_default();
                        if !wanted_tags.iter().any(|w| item_tags.contains(w)) {
                            return false;
                        }
                    }
                    if let Some(en) = enabled {
                        if item.get("enabled").and_then(|v| v.as_bool()) != Some(*en) {
                            return false;
                        }
                    }
                    if let Some(n) = name {
                        let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let nm = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        if id != n && nm != n {
                            return false;
                        }
                    }
                    true
                })
                .collect();
            let data = serde_json::Value::Array(filtered);
            render_envelope(cli.output, OutputEnvelope::success("trigger-list", data))
        }
        TriggerSub::Show { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/templates/trigger/{id}"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-show", data).with_entity(id.clone()),
            )
        }
        TriggerSub::Save { file } => {
            let meta = load_json(std::path::Path::new(file))?;
            let id = meta.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            client.post_json::<_, serde_json::Value>("/api/v1/templates/trigger", &meta).await?;
            let data = serde_json::json!({"saved": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-save", data).with_entity(id),
            )
        }
        TriggerSub::Delete { id } => {
            let deleted: bool = client
                .delete_json(&format!("/api/v1/templates/trigger/{id}"))
                .await?;
            if deleted {
                let data = serde_json::json!({"deleted": id});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("trigger-delete", data).with_entity(id.clone()),
                )
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::failure("trigger-delete", format!("trigger not found: {id}")),
                )
            }
        }
        TriggerSub::History { execution, trigger } => {
            let mut path = format!("/api/v1/triggers/history?execution_id={execution}");
            if let Some(t) = trigger {
                path.push_str(&format!("&trigger_name={t}"));
            }
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-history", data).with_entity(execution.clone()),
            )
        }
        TriggerSub::Executions {
            trigger,
            execution,
            workflow,
            outcome,
            limit,
            offset,
        } => {
            let mut params: Vec<String> = Vec::new();
            if let Some(t) = trigger {
                params.push(format!("trigger_name={t}"));
            }
            if let Some(e) = execution {
                params.push(format!("execution_id={e}"));
            }
            if let Some(w) = workflow {
                params.push(format!("workflow_id={w}"));
            }
            if let Some(o) = outcome {
                params.push(format!("outcome={o}"));
            }
            if let Some(l) = limit {
                params.push(format!("limit={l}"));
            }
            if let Some(o) = offset {
                params.push(format!("offset={o}"));
            }
            let path = if params.is_empty() {
                "/api/v1/trigger-executions".to_string()
            } else {
                format!("/api/v1/trigger-executions?{}", params.join("&"))
            };
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-executions", data),
            )
        }
        TriggerSub::ExecutionShow { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/trigger-executions/{id}"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-execution-show", data).with_entity(id.clone()),
            )
        }
        TriggerSub::Stats => {
            let data: serde_json::Value = client
                .get_json("/api/v1/trigger-executions/stats")
                .await?;
            render_envelope(cli.output, OutputEnvelope::success("trigger-stats", data))
        }
        TriggerSub::Cleanup { older_than } => {
            let cutoff = older_than.unwrap_or_else(wf_common::now);
            let body = serde_json::json!({"older_than": cutoff});
            let removed: usize = client
                .post_json("/api/v1/trigger-executions/cleanup", &body)
                .await?;
            let data = serde_json::json!({"removed": removed, "olderThan": cutoff});
            render_envelope(cli.output, OutputEnvelope::success("trigger-cleanup", data))
        }
    }
}

fn load_json(path: &std::path::Path) -> CliResult<serde_json::Value> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    serde_json::from_str(&content).map_err(|e| {
        CliError::Arguments(format!("invalid trigger JSON in {}: {e}", path.display()))
    })
}
