//! Checkpoint file read queries.

use std::io::IsTerminal;

use crate::args::{CheckpointDomain, CheckpointSub, Cli};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat};
use wf_client::WfClient;

use super::{confirm_destructive, encode_segment, entity_type};

pub(crate) async fn handle_file_read(
    cli: &Cli,
    client: &WfClient,
    sub: &CheckpointSub,
) -> CliResult<Option<()>> {
    let result: CliResult<()> = match sub {
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
        _ => return Ok(None),
    };
    result.map(|_| Some(()))
}
