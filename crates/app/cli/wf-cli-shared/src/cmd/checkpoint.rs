//! Checkpoint management across the checkpoint domain: unified records,
//! file checkpoints, and agent-loop checkpoints. Change approvals live in
//! the sibling `approval` command.

use std::io::IsTerminal;

use crate::args::{CheckpointDomain, CheckpointSub, Cli};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat};

fn entity_type(domain: Option<CheckpointDomain>) -> Option<&'static str> {
    // Workflow checkpoints are stored under the `checkpoint` entity type;
    // only agent loops use `agent_loop`.
    match domain {
        Some(CheckpointDomain::Workflow) => Some("checkpoint"),
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

/// Prompt for confirmation before a destructive checkpoint operation.
///
/// Skipped when `--yes` is set or stdin is not a TTY (scripted/CI mode).
fn confirm_destructive(cli: &Cli, action: &str, target: &str) -> CliResult<()> {
    if cli.yes || !std::io::stdin().is_terminal() {
        return Ok(());
    }
    eprint!("? CONFIRM {action} {target} (y/N) ");
    let _ = std::io::Write::flush(&mut std::io::stderr());
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(CliError::Io)?;
    if !matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        return Err(CliError::Interrupted("aborted by user".to_string()));
    }
    Ok(())
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
            confirm_destructive(cli, "rollback edit session", session)?;
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
            confirm_destructive(cli, "resolve conflicts", "all")?;
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
        CheckpointSub::FileRestore {
            entity,
            checkpoint,
            dry_run,
        } => {
            if *dry_run {
                let data = serde_json::json!({"dry_run": true, "entity": entity, "checkpoint": checkpoint});
                return render_envelope(
                    cli.output,
                    OutputEnvelope::success("file-checkpoint-restore-dry-run", data),
                );
            }
            confirm_destructive(cli, "restore file checkpoint", checkpoint)?;
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
            confirm_destructive(cli, "garbage collect file store", "all")?;
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
            confirm_destructive(cli, "merge features to main", &features.join(","))?;
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
            confirm_destructive(cli, "rollback feature from main", feature_head)?;
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
        CheckpointSub::FileCreate { id } => {
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
                );
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
                );
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
            let path = match domain {
                Some(CheckpointDomain::Agent) => {
                    format!("/api/v1/agent-loops/{id}/checkpoints/chain")
                }
                _ => format!("/api/v1/executions/{id}/checkpoints/chain"),
            };
            let data: serde_json::Value = client.get_json(&path).await?;
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
                Some(CheckpointDomain::Agent) => format!("{path}&domain=agent"),
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
        CheckpointSub::FilePartitionHistory {
            partition,
            limit,
            offset,
        } => {
            let mut params: Vec<String> = Vec::new();
            if let Some(limit) = limit {
                params.push(format!("limit={limit}"));
            }
            if let Some(offset) = offset {
                params.push(format!("offset={offset}"));
            }
            let mut path = format!(
                "/api/v1/file-checkpoint/partitions/{}/history",
                encode_segment(partition)
            );
            if !params.is_empty() {
                path.push_str(&format!("?{}", params.join("&")));
            }
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-partition-history", data)
                    .with_entity(partition.clone()),
            )
        }
        CheckpointSub::FileRename {
            actor,
            from,
            to,
            content_file,
        } => {
            let content = match content_file {
                Some(file) => std::fs::read_to_string(file).map_err(|e| {
                    crate::error::CliError::Business(format!(
                        "failed to read content file {file}: {e}"
                    ))
                })?,
                None => String::new(),
            };
            let body = serde_json::json!({
                "actor": actor,
                "from_path": from,
                "to_path": to,
                "content": content,
            });
            let snapshot: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/rename", &body)
                .await?;
            let data = serde_json::json!({
                "actor": actor,
                "from": from,
                "to": to,
                "snapshot": snapshot,
            });
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-rename", data).with_entity(actor.clone()),
            )
        }
        CheckpointSub::FileStateLinks { ids } => {
            if ids.is_empty() {
                return Err(crate::error::CliError::Business(
                    "at least one checkpoint id is required".to_string(),
                ));
            }
            let data: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/file-checkpoint/state-links?ids={}",
                    ids.join(",")
                ))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-state-links", data),
            )
        }
    }
}
