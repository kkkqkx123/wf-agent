//! Change-approval management over the file-checkpoint approval API.
//!
//! This is the checkpoint domain's approval surface (pending file-change
//! approvals). Runtime tool-call approvals during a session are answered by
//! the run interaction poller instead.

use crate::args::{ApprovalSub, Cli};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &ApprovalSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        ApprovalSub::List => {
            let data: serde_json::Value = client
                .get_json("/api/v1/file-checkpoint/approvals/pending")
                .await?;
            render_envelope(cli.output, OutputEnvelope::success("approval-list", data))
        }
        ApprovalSub::Approve {
            instance,
            feature,
            paths,
        } => {
            let feature_name = feature.as_deref().unwrap_or("");
            let path_vec = paths.as_deref().map(|s| {
                s.split(',')
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .collect::<Vec<_>>()
            });
            let body = serde_json::json!({"feature": feature_name, "paths": path_vec});
            let data: serde_json::Value = client
                .post_json(
                    &format!("/api/v1/file-checkpoint/approvals/{instance}/approve"),
                    &body,
                )
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("approval-approve", data).with_entity(instance.clone()),
            )
        }
        ApprovalSub::Reject { instance, reason } => {
            let body = serde_json::json!({"reason": reason});
            let data: serde_json::Value = client
                .post_json(
                    &format!("/api/v1/file-checkpoint/approvals/{instance}/reject"),
                    &body,
                )
                .await?;
            let snapshot = data.get("baseline_snapshot_id").cloned().unwrap_or(data.clone());
            let data = serde_json::json!({"instance": instance, "rejected": true, "snapshot": snapshot, "reason": reason});
            render_envelope(
                cli.output,
                OutputEnvelope::success("approval-reject", data).with_entity(instance.clone()),
            )
        }
    }
}
