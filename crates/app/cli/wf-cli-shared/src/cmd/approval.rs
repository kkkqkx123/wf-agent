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
            let envelope = client
                .generated()
                .handle_list_pending_approvals()
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            render_envelope(cli.output, OutputEnvelope::success("approval-list", data))
        }
        ApprovalSub::Approve {
            instance,
            feature,
            paths,
        } => {
            // Named request body; response payload stays passthrough JSON
            // because the merge outcome is rendered verbatim.
            let body = wf_client::models::ApproveRequest {
                feature: Some(feature.as_deref().unwrap_or("").to_string()),
                paths: paths.as_deref().map(|s| {
                    s.split(',')
                        .map(|v| v.trim().to_string())
                        .filter(|v| !v.is_empty())
                        .collect::<Vec<_>>()
                }),
            };
            let envelope = client
                .generated()
                .handle_approve_changes(instance, &body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("approval-approve", data).with_entity(instance.clone()),
            )
        }
        ApprovalSub::Reject { instance, reason } => {
            let body = wf_client::models::RejectRequest {
                reason: reason.clone(),
            };
            let envelope = client
                .generated()
                .handle_reject_changes(instance, &body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let view: wf_client::models::RejectResponse =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let data = serde_json::json!({"instance": instance, "rejected": true, "snapshot": view.baseline_snapshot_id, "reason": reason});
            render_envelope(
                cli.output,
                OutputEnvelope::success("approval-reject", data).with_entity(instance.clone()),
            )
        }
    }
}
