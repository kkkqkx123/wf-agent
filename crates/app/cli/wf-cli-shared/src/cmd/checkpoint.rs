//! Checkpoint management across the checkpoint domain: unified records,
//! file checkpoints, and agent-loop checkpoints. Change approvals live in
//! the sibling `approval` command.

use crate::args::{CheckpointDomain, CheckpointSub, Cli};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::{OutputEnvelope, OutputFormat};

fn entity_type(domain: Option<CheckpointDomain>) -> Option<&'static str> {
    match domain {
        Some(CheckpointDomain::Workflow) => Some("workflow"),
        Some(CheckpointDomain::Agent) => Some("agent_loop"),
        None => None,
    }
}

/// Percent-encode a file path for single-segment `{id}` routes.
fn encode_segment(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub async fn run(cli: &Cli, sub: &CheckpointSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
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
        CheckpointSub::FilePartitions => {
            let data: serde_json::Value = client
                .get_json("/api/v1/file-checkpoint/partitions")
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-partitions", data),
            )
        }
        CheckpointSub::FileChanges {
            actor,
            path,
            start,
            end,
        } => {
            let mut params: Vec<String> = Vec::new();
            if let Some(actor) = actor {
                params.push(format!("actor={actor}"));
            } else if let Some(path) = path {
                params.push(format!("path={path}"));
            } else {
                return Err(crate::error::CliError::Business(
                    "one of --actor or --path is required".to_string(),
                ));
            }
            match (start, end) {
                (None, None) => {}
                (start, end) => {
                    let start_ms = start.unwrap_or(i64::MIN / 2).saturating_mul(1000);
                    let end_ms = end.unwrap_or(i64::MAX / 2).saturating_mul(1000);
                    params.push(format!("start={start_ms}&end={end_ms}"));
                }
            }
            let data: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/file-checkpoint/changes?{}",
                    params.join("&")
                ))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-changes", data),
            )
        }
        CheckpointSub::FileDiffMain { actor } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/file-checkpoint/diff/main/{actor}"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-diff", data).with_entity(actor.clone()),
            )
        }
        CheckpointSub::FileTimeline { path } => {
            let data: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/file-checkpoint/timeline/{}",
                    encode_segment(path)
                ))
                .await?;
            if cli.output == OutputFormat::Text {
                crate::cmd::checkpoint_view::print_file_timeline(&data);
                Ok(())
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-file-timeline", data)
                        .with_entity(path.clone()),
                )
            }
        }
        CheckpointSub::FileSessionList { limit, offset } => {
            let mut params: Vec<String> = Vec::new();
            if let Some(limit) = limit {
                params.push(format!("limit={limit}"));
            }
            if let Some(offset) = offset {
                params.push(format!("offset={offset}"));
            }
            let path = if params.is_empty() {
                "/api/v1/file-checkpoint/sessions".to_string()
            } else {
                format!("/api/v1/file-checkpoint/sessions?{}", params.join("&"))
            };
            let data: serde_json::Value = client.get_json(&path).await?;
            if cli.output == OutputFormat::Text {
                crate::cmd::checkpoint_view::print_edit_sessions(&data);
                Ok(())
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-file-session-list", data),
                )
            }
        }
        CheckpointSub::FileSessionBegin { label } => {
            let body = serde_json::json!({"label": label});
            let session: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/sessions", &body)
                .await?;
            let entity = session.as_str().unwrap_or_default().to_string();
            let data = serde_json::json!({"session": session});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-session-begin", data).with_entity(entity),
            )
        }
        CheckpointSub::FileSessionRollback { session, actor } => {
            let snapshot: serde_json::Value = client
                .post_json(
                    &format!(
                        "/api/v1/file-checkpoint/sessions/{}/rollback/{}",
                        encode_segment(session),
                        encode_segment(actor)
                    ),
                    &serde_json::json!({}),
                )
                .await?;
            let data =
                serde_json::json!({"session": session, "actor": actor, "snapshot": snapshot});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-session-rollback", data)
                    .with_entity(actor.clone()),
            )
        }
        CheckpointSub::FileUndo { actor } => {
            let snapshot: serde_json::Value = client
                .post_json(
                    &format!("/api/v1/file-checkpoint/undo/{}", encode_segment(actor)),
                    &serde_json::json!({}),
                )
                .await?;
            let data = serde_json::json!({"actor": actor, "snapshot": snapshot});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-undo", data).with_entity(actor.clone()),
            )
        }
        CheckpointSub::FileRedo { actor } => {
            let snapshot: serde_json::Value = client
                .post_json(
                    &format!("/api/v1/file-checkpoint/redo/{}", encode_segment(actor)),
                    &serde_json::json!({}),
                )
                .await?;
            let data = serde_json::json!({"actor": actor, "snapshot": snapshot});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-redo", data).with_entity(actor.clone()),
            )
        }
        CheckpointSub::FileWorkspace { actor } => {
            let data: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/file-checkpoint/workspace/{}",
                    encode_segment(actor)
                ))
                .await?;
            if cli.output == OutputFormat::Text {
                crate::cmd::checkpoint_view::print_workspace(&data);
                Ok(())
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-file-workspace", data)
                        .with_entity(actor.clone()),
                )
            }
        }
        CheckpointSub::FileTree { actor, prefix } => {
            let mut path = format!("/api/v1/file-checkpoint/tree/{}", encode_segment(actor));
            if let Some(prefix) = prefix {
                path.push_str(&format!("?prefix={}", encode_segment(prefix)));
            }
            let data: serde_json::Value = client.get_json(&path).await?;
            if cli.output == OutputFormat::Text {
                crate::cmd::checkpoint_view::print_file_tree(&data);
                Ok(())
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-file-tree", data)
                        .with_entity(actor.clone()),
                )
            }
        }
        CheckpointSub::FileConflicts => {
            let data: serde_json::Value =
                client.get_json("/api/v1/file-checkpoint/conflicts").await?;
            if cli.output == OutputFormat::Text {
                crate::cmd::checkpoint_view::print_conflicts(&data);
                Ok(())
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-file-conflicts", data),
                )
            }
        }
        CheckpointSub::FileConflictResolve {
            actor,
            feature,
            resolutions,
            session,
        } => {
            let mut map = serde_json::Map::new();
            for pair in resolutions {
                let Some((path, file)) = pair.split_once('=') else {
                    return Err(crate::error::CliError::Business(format!(
                        "resolution must be PATH=FILE, got {pair:?}"
                    )));
                };
                let content = std::fs::read_to_string(file).map_err(|e| {
                    crate::error::CliError::Business(format!(
                        "failed to read resolution file {file}: {e}"
                    ))
                })?;
                map.insert(path.to_string(), serde_json::Value::String(content));
            }
            if map.is_empty() {
                return Err(crate::error::CliError::Business(
                    "at least one --resolve PATH=FILE is required".to_string(),
                ));
            }
            let body = serde_json::json!({
                "actor": actor,
                "feature": feature,
                "resolutions": map,
                "session": session,
            });
            let data: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/conflicts/resolve", &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-conflict-resolve", data)
                    .with_entity(actor.clone()),
            )
        }
        CheckpointSub::FileContent {
            actor,
            path,
            commit,
        } => {
            let mut query = format!(
                "actor={}&path={}",
                encode_segment(actor),
                encode_segment(path)
            );
            if let Some(commit) = commit {
                query.push_str(&format!("&version={}", encode_segment(commit)));
            }
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/file-checkpoint/content?{query}"))
                .await?;
            if cli.output == OutputFormat::Text {
                crate::cmd::checkpoint_view::print_file_content(&data);
                Ok(())
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-file-content", data)
                        .with_entity(path.clone()),
                )
            }
        }
        CheckpointSub::FileDiffActors { actor_a, actor_b } => {
            let data: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/file-checkpoint/diff/actors/{}/{}",
                    encode_segment(actor_a),
                    encode_segment(actor_b)
                ))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-diff-actors", data)
                    .with_entity(actor_a.clone()),
            )
        }
        CheckpointSub::FileStats => {
            let data: serde_json::Value =
                client.get_json("/api/v1/file-checkpoint/gc/stats").await?;
            if cli.output == OutputFormat::Text {
                crate::cmd::checkpoint_view::print_store_stats(&data);
                Ok(())
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-file-stats", data),
                )
            }
        }
        CheckpointSub::FileRestore { entity, checkpoint } => {
            let body = serde_json::json!({"entity": entity, "checkpoint": checkpoint});
            let data: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/restore", &body)
                .await?;
            let restored = data.get("restored").cloned().unwrap_or(data.clone());
            let data = serde_json::json!({"entity": entity, "checkpoint": checkpoint, "restored": restored});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-restore", data)
                    .with_entity(entity.clone()),
            )
        }
        CheckpointSub::FileGc { keep_recent_heads } => {
            let data: serde_json::Value = client
                .post_json(
                    &format!("/api/v1/file-checkpoint/gc?keep_recent_heads={keep_recent_heads}"),
                    &serde_json::json!({}),
                )
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-gc", data),
            )
        }
        CheckpointSub::FileSubmit { actor, session } => {
            let body = serde_json::json!({"actor": actor, "session": session});
            let data: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/submit", &body)
                .await?;
            let review_commit = data.get("review_commit").cloned().unwrap_or(data.clone());
            let data = serde_json::json!({"actor": actor, "reviewCommit": review_commit});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-submit", data).with_entity(actor.clone()),
            )
        }
        CheckpointSub::FileMergeToFeature {
            actor,
            feature,
            session,
        } => {
            let body = serde_json::json!({"actor": actor, "feature": feature, "session": session});
            let data: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/merge-to-feature", &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-merge-to-feature", data)
                    .with_entity(actor.clone()),
            )
        }
        CheckpointSub::FileMergeToMain {
            actor,
            features,
            session,
        } => {
            if features.is_empty() {
                return Err(crate::error::CliError::Business(
                    "at least one FEATURE is required".to_string(),
                ));
            }
            let body =
                serde_json::json!({"actor": actor, "features": features, "session": session});
            let data: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/merge-to-main", &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-merge-to-main", data)
                    .with_entity(actor.clone()),
            )
        }
        CheckpointSub::FileRollbackFeature {
            actor,
            feature_head,
            session,
        } => {
            let body = serde_json::json!({
                "actor": actor,
                "feature_head": feature_head,
                "session": session,
            });
            let data: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/rollback-feature", &body)
                .await?;
            let revert_commit = data.get("revert_commit").cloned().unwrap_or(data.clone());
            let data = serde_json::json!({"actor": actor, "featureHead": feature_head, "revertCommit": revert_commit});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-rollback-feature", data)
                    .with_entity(actor.clone()),
            )
        }
        CheckpointSub::FileIndexRebuild => {
            let data: serde_json::Value = client
                .post_json(
                    "/api/v1/file-checkpoint/index/rebuild",
                    &serde_json::json!({}),
                )
                .await?;
            let indexed = data.get("indexed_commits").cloned().unwrap_or(data.clone());
            let data = serde_json::json!({"indexedCommits": indexed});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-index-rebuild", data),
            )
        }
        CheckpointSub::FileIndexStatus => {
            let data: serde_json::Value = client
                .get_json("/api/v1/file-checkpoint/index/status")
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-index-status", data),
            )
        }
        CheckpointSub::FileCreate { id, path: _ } => {
            // The server attributes workspace checkpoints to an actor; the
            // CLI id names that actor.
            let body = serde_json::json!({"actor": id});
            let data: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/checkpoints", &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-create", data).with_entity(id.clone()),
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
        CheckpointSub::Restore { id, resume } => {
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
                let data: serde_json::Value =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                let execution_id = data
                    .get("execution_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-restore", data).with_entity(execution_id),
                )
            }
        }
        CheckpointSub::RestoreAgent { id, checkpoint } => {
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
        CheckpointSub::Chain { id, domain: _ } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/checkpoints/chain"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-chain", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Gc { id, before, domain } => {
            let mut path = match before {
                Some(ts) => format!("/api/v1/executions/{id}/checkpoints/gc?before={ts}"),
                None => format!("/api/v1/executions/{id}/checkpoints/gc"),
            };
            if let Some(entity) = entity_type(*domain) {
                path.push_str(&format!("&domain={entity}"));
            }
            let deleted: serde_json::Value =
                client.post_json(&path, &serde_json::json!({})).await?;
            let deleted = deleted.as_u64().unwrap_or(0) as usize;
            let data = serde_json::json!({"executionId": id, "deleted": deleted});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-gc", data).with_entity(id.clone()),
            )
        }
    }
}
