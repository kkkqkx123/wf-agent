//! Checkpoint file sync operations.

use std::io::IsTerminal;

use crate::args::{CheckpointDomain, CheckpointSub, Cli};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat};
use wf_client::WfClient;

use super::{confirm_destructive, encode_segment, entity_type};

pub(crate) async fn handle_file_sync(
    cli: &Cli,
    client: &WfClient,
    sub: &CheckpointSub,
) -> CliResult<Option<()>> {
    let result: CliResult<()> = match sub {
        CheckpointSub::FileSyncExport {
            confirm_overwrite,
            auto_commit,
            message,
        } => {
            if !confirm_overwrite {
                return Err(crate::error::CliError::Business(
                    "export overwrites the worktree; pass --confirm-overwrite".to_string(),
                ));
            }
            let body = serde_json::json!({
                "confirm_overwrite": confirm_overwrite,
                "auto_commit": auto_commit,
                "message": message,
            });
            let data: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/sync/export", &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-sync-export", data),
            )
        }
        CheckpointSub::FileSyncImport => {
            let data: serde_json::Value = client
                .post_json(
                    "/api/v1/file-checkpoint/sync/import",
                    &serde_json::json!({}),
                )
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-sync-import", data),
            )
        }
        CheckpointSub::FileSyncRebase {
            branch,
            commit_prefix,
            behind,
            mode,
        } => {
            confirm_destructive(cli, "rebase baseline", "all")?;
            let body = serde_json::json!({
                "branch": branch,
                "commit_prefix": commit_prefix,
                "behind": behind,
                "mode": mode,
                "confirm": true,
            });
            let data: serde_json::Value = client
                .post_json("/api/v1/file-checkpoint/sync/rebase", &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-sync-rebase", data),
            )
        }
        CheckpointSub::FileSyncHint => {
            let data: serde_json::Value =
                client.get_json("/api/v1/file-checkpoint/sync/hint").await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-sync-hint", data),
            )
        }
        CheckpointSub::FileSyncReconcile => {
            let data: serde_json::Value = client
                .get_json("/api/v1/file-checkpoint/sync/reconcile")
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-sync-reconcile", data),
            )
        }
        CheckpointSub::FileSyncConfig => {
            let data: serde_json::Value = client
                .get_json("/api/v1/file-checkpoint/sync/config")
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-sync-config", data),
            )
        }
        _ => return Ok(None),
    };
    result.map(|_| Some(()))
}
