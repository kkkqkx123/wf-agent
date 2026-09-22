//! Change-approval management over `wf_api::checkpoint::approval`.
//!
//! This is the checkpoint domain's approval surface (pending file-change
//! approvals). Runtime tool-call approvals during a session are a separate
//! concern: decided by the runtime policy in `run` / `turn`, optionally
//! advised by `crate::approval::LlmApprovalHandler`.
use wf_api::checkpoint::approval;

use crate::args::{ApprovalSub, Cli};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &ApprovalSub) -> CliResult<()> {
    let domain =
        crate::domain::DomainHandle::require_embedded(cli, crate::mode::CliMode::Run, "approval")
            .await?;
    let ctx = domain
        .api_context()
        .expect("embedded mode must have api_context");
    let result = match sub {
        ApprovalSub::List => {
            let pending = approval::list_pending_approvals(ctx)?;
            let data = serde_json::to_value(&pending)?;
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
            let outcome = approval::approve_changes(ctx, instance, feature_name, path_vec)?;
            let data = serde_json::to_value(&outcome)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("approval-approve", data).with_entity(instance.clone()),
            )
        }
        ApprovalSub::Reject { instance, reason } => {
            let snapshot = approval::reject_changes(ctx, instance, reason.as_deref())?;
            let data = serde_json::json!({"instance": instance, "rejected": true, "snapshot": snapshot, "reason": reason});
            render_envelope(
                cli.output,
                OutputEnvelope::success("approval-reject", data).with_entity(instance.clone()),
            )
        }
    };
    domain.shutdown().await?;
    result
}
