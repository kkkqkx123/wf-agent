//! Checkpoint unified records.

use std::io::IsTerminal;

use crate::args::{CheckpointDomain, CheckpointSub, Cli};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat};
use wf_client::WfClient;

use super::{confirm_destructive, encode_segment, entity_type};

pub(crate) async fn handle_unified(
    cli: &Cli,
    client: &WfClient,
    sub: &CheckpointSub,
) -> CliResult<Option<()>> {
    let result: CliResult<()> = match sub {
        CheckpointSub::Create { id } => {
            let envelope = client
                .generated()
                .post_executions_id_checkpoints(id)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let checkpoint_id: String =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let data = serde_json::json!({"executionId": id, "checkpointId": checkpoint_id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-create", data)
                    .with_entity(checkpoint_id.clone()),
            )
        }
        CheckpointSub::CreateAgent { id, name } => {
            // Named request body; response stays passthrough JSON.
            let body = wf_client::models::CreateCheckpointBody {
                description: name.clone(),
            };
            let envelope = client
                .generated()
                .post_agent_loops_id_checkpoints(id, &body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let created: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let checkpoint_id = created
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let data = serde_json::json!({"agentLoopId": id, "checkpointId": checkpoint_id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-create", data).with_entity(checkpoint_id),
            )
        }
        CheckpointSub::List {
            id,
            limit,
            offset,
            domain,
        } => {
            let entity = entity_type(*domain).map(|s| s.to_string());
            let envelope = client
                .generated()
                .handle_list_checkpoints_by_entity(
                    id,
                    entity.as_deref(),
                    limit.map(|v| v as i64),
                    offset.map(|v| v as i64),
                )
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let mut list = data
                .get("items")
                .and_then(|v| v.as_array())
                .cloned()
                .or_else(|| data.as_array().cloned())
                .unwrap_or_default();
            list.sort_by_key(|c| c.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0));
            let off = offset.unwrap_or(0);
            let lim = limit.unwrap_or(usize::MAX);
            let paged: Vec<_> = list.into_iter().skip(off).take(lim).collect();
            let data = serde_json::Value::Array(paged);
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-list", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Show { id } => {
            let envelope = client
                .generated()
                .handle_get_checkpoint(id)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let view: wf_client::models::CheckpointDoc =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let data = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-show", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Tag { id, tags } => {
            let data: serde_json::Value = client
                .put_json(
                    &format!("/api/v1/checkpoints/{}/tags", encode_segment(id)),
                    tags,
                )
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-tag", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Untag { id } => {
            let data: serde_json::Value = client
                .put_json(
                    &format!("/api/v1/checkpoints/{}/tags", encode_segment(id)),
                    &Vec::<String>::new(),
                )
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-untag", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Diff { a, b } => {
            let env_a = client
                .generated()
                .handle_get_checkpoint(a)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let cp_a: wf_client::models::CheckpointDoc =
                wf_client::client::unwrap_generated_envelope(env_a.into_inner())?;
            let env_b = client
                .generated()
                .handle_get_checkpoint(b)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let cp_b: wf_client::models::CheckpointDoc =
                wf_client::client::unwrap_generated_envelope(env_b.into_inner())?;
            let val_a = serde_json::to_value(&cp_a).unwrap_or(serde_json::Value::Null);
            let val_b = serde_json::to_value(&cp_b).unwrap_or(serde_json::Value::Null);
            let data = serde_json::json!({
                "a": val_a,
                "b": val_b,
                "summary": {
                    "same_entity": cp_a.entity_id == cp_b.entity_id,
                    "same_type": cp_a.checkpoint_type == cp_b.checkpoint_type,
                    "same_status": cp_a.status == cp_b.status,
                }
            });
            render_envelope(cli.output, OutputEnvelope::success("checkpoint-diff", data))
        }
        CheckpointSub::Restore {
            id,
            resume,
            dry_run,
        } => {
            if *dry_run {
                // Preview: fetch and display checkpoint details without restoring.
                let envelope = client
                    .generated()
                    .handle_get_checkpoint(id)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let checkpoint: wf_client::models::CheckpointDoc =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                let data = serde_json::json!({"dry_run": true, "checkpoint": serde_json::to_value(&checkpoint).unwrap_or(serde_json::Value::Null)});
                return render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-restore-dry-run", data)
                        .with_entity(id.clone()),
                )
                .map(|_| Some(()));
            }
            if !resume {
                confirm_destructive(cli, "restore checkpoint", id)?;
            }
            if *resume {
                let envelope = client
                    .generated()
                    .handle_restore_and_resume(id)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let view: wf_client::models::ExecuteView =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                let data = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
                let execution_id = view.execution_id.clone();
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-restore-resume", data)
                        .with_entity(execution_id),
                )
            } else {
                let envelope = client
                    .generated()
                    .post_executions_checkpoints_cid_restore(id)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let view: wf_client::models::RestoredCheckpointDoc =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                let data = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
                let execution_id = view.execution_id.clone();
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-restore", data).with_entity(execution_id),
                )
            }
        }
        CheckpointSub::RestoreAgent {
            id,
            checkpoint,
            dry_run,
        } => {
            if *dry_run {
                let envelope = client
                    .generated()
                    .handle_get_checkpoint(checkpoint)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let cp: wf_client::models::CheckpointDoc =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                let data = serde_json::json!({"dry_run": true, "checkpoint": serde_json::to_value(&cp).unwrap_or(serde_json::Value::Null)});
                return render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-restore-agent-dry-run", data)
                        .with_entity(checkpoint.clone()),
                )
                .map(|_| Some(()));
            }
            confirm_destructive(cli, "restore agent checkpoint", checkpoint)?;
            let envelope = client
                .generated()
                .post_agent_loops_id_checkpoints_cid_restore(id, checkpoint)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let entity = data
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or(id.as_str())
                .to_string();
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-restore", data).with_entity(entity),
            )
        }
        CheckpointSub::Delete { id } => {
            confirm_destructive(cli, "delete checkpoint", id)?;
            let envelope = client
                .generated()
                .handle_delete_checkpoint(id)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let deleted: bool =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let data = serde_json::json!({"deleted": id, "ok": deleted});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-delete", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Chain { id, domain } => {
            // Typed chain read; the loop route returns the capped item view
            // while the execution route returns the chain analysis shape.
            let data = match domain {
                Some(CheckpointDomain::Agent) => {
                    let view = client.agent_loop_checkpoints_chain(id).await?;
                    serde_json::to_value(&view).unwrap_or(serde_json::Value::Null)
                }
                _ => {
                    let view = client.execution_checkpoints_chain(id).await?;
                    serde_json::to_value(&view).unwrap_or(serde_json::Value::Null)
                }
            };
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-chain", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Gc { id, before, domain } => {
            let path = match before {
                Some(ts) => format!("/api/v1/executions/{id}/checkpoints/gc?before={ts}"),
                None => format!("/api/v1/executions/{id}/checkpoints/gc"),
            };
            // The GC endpoint takes an execution-domain token, not the
            // checkpoint entity type.
            let path = match domain {
                Some(CheckpointDomain::Workflow) => format!("{path}&domain=workflow"),
                Some(CheckpointDomain::Agent) => format!("{path}&domain=agent_loop"),
                None => path,
            };
            let deleted: serde_json::Value =
                client.post_json(&path, &serde_json::json!({})).await?;
            let deleted = deleted.as_u64().unwrap_or(0) as usize;
            let data = serde_json::json!({"executionId": id, "deleted": deleted});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-gc", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Latest { entity, domain } => {
            let mut path = format!(
                "/api/v1/checkpoints/entity/{}/latest",
                encode_segment(entity)
            );
            if let Some(entity_type) = entity_type(*domain) {
                path.push_str(&format!("?entity_type={entity_type}"));
            }
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-latest", data).with_entity(entity.clone()),
            )
        }
        CheckpointSub::Entities {
            ids,
            domain,
            limit,
            offset,
        } => {
            if ids.is_empty() {
                return Err(crate::error::CliError::Business(
                    "at least one entity id is required".to_string(),
                ));
            }
            let mut params = vec![format!("entity_ids={}", ids.join(","))];
            if let Some(entity_type) = entity_type(*domain) {
                params.push(format!("entity_type={entity_type}"));
            }
            if let Some(limit) = limit {
                params.push(format!("limit={limit}"));
            }
            if let Some(offset) = offset {
                params.push(format!("offset={offset}"));
            }
            let data: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/checkpoints/entities?{}",
                    params.join("&")
                ))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-entities", data),
            )
        }
        CheckpointSub::TimeRange {
            workflow,
            start,
            end,
            limit,
            offset,
        } => {
            let mut params = vec![format!(
                "workflowId={}&start={start}&end={end}",
                encode_segment(workflow)
            )];
            if let Some(limit) = limit {
                params.push(format!("limit={limit}"));
            }
            if let Some(offset) = offset {
                params.push(format!("offset={offset}"));
            }
            let data: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/checkpoints/time-range?{}",
                    params.join("&")
                ))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-time-range", data)
                    .with_entity(workflow.clone()),
            )
        }
        CheckpointSub::MetadataGet { entity } => {
            let data: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/checkpoints/entity/{}/metadata",
                    encode_segment(entity)
                ))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-metadata", data).with_entity(entity.clone()),
            )
        }
        CheckpointSub::MetadataSet { entity, json } => {
            let parsed: serde_json::Value = serde_json::from_str(json).map_err(|e| {
                crate::error::CliError::Business(format!("--json must be a JSON object: {e}"))
            })?;
            let object = match parsed {
                serde_json::Value::Object(map) => map,
                _ => {
                    return Err(crate::error::CliError::Business(
                        "--json must be a JSON object".to_string(),
                    ));
                }
            };
            let path = format!(
                "/api/v1/checkpoints/entity/{}/metadata",
                encode_segment(entity)
            );
            let data: serde_json::Value = client.put_json(&path, &object).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-metadata-set", data)
                    .with_entity(entity.clone()),
            )
        }
        CheckpointSub::DeleteByEntity { entity, domain } => {
            confirm_destructive(cli, "delete all checkpoints for entity", entity)?;
            let mut path = format!("/api/v1/checkpoints/entity/{}", encode_segment(entity));
            if let Some(entity_type) = entity_type(*domain) {
                path.push_str(&format!("?entity_type={entity_type}"));
            }
            let removed: serde_json::Value = client.delete_json(&path).await?;
            let removed = removed.as_u64().unwrap_or(0);
            let data = serde_json::json!({"entity": entity, "deleted": removed});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-delete-by-entity", data)
                    .with_entity(entity.clone()),
            )
        }
        _ => return Ok(None),
    };
    result.map(|_| Some(()))
}
