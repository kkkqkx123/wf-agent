//! Checkpoint file write operations.

use std::io::IsTerminal;

use crate::args::{CheckpointDomain, CheckpointSub, Cli};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat};
use wf_client::WfClient;

use super::{confirm_destructive, encode_segment, entity_type};

pub(crate) async fn handle_file_write(
    cli: &Cli,
    client: &WfClient,
    sub: &CheckpointSub,
) -> CliResult<Option<()>> {
    let result: CliResult<()> = match sub {
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
        CheckpointSub::FileUndoLastInputPreview {
            execution,
            checkpoint_prefixes,
        } => {
            let body = serde_json::json!({
                "execution_id": execution,
                "checkpoint_prefixes": if checkpoint_prefixes.is_empty() { None } else { Some(checkpoint_prefixes) }
            });
            let data: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/undo/preview", &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-undo-preview", data),
            )
        }
        CheckpointSub::FileUndoLastInput {
            execution,
            checkpoint_prefixes,
        } => {
            confirm_destructive(cli, "undo last input", execution)?;
            let body = serde_json::json!({
                "execution_id": execution,
                "checkpoint_prefixes": if checkpoint_prefixes.is_empty() { None } else { Some(checkpoint_prefixes) }
            });
            let data: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/undo/apply", &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-undo", data),
            )
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
                )
                .map(|_| Some(()));
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
        _ => return Ok(None),
    };
    result.map(|_| Some(()))
}
