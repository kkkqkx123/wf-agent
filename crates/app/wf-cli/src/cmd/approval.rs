use wf_api::workflow::file_approval;

use crate::args::{ApprovalSub, Cli};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &ApprovalSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        ApprovalSub::List => {
            let pending = file_approval::list_pending_approvals(ctx)?;
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
            let outcome = file_approval::approve_changes(ctx, instance, feature_name, path_vec)?;
            let data = serde_json::to_value(&outcome)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("approval-approve", data).with_entity(instance.clone()),
            )
        }
        ApprovalSub::Reject { instance } => {
            let snapshot = file_approval::reject_changes(ctx, instance)?;
            let data =
                serde_json::json!({"instance": instance, "rejected": true, "snapshot": snapshot});
            render_envelope(
                cli.output,
                OutputEnvelope::success("approval-reject", data).with_entity(instance.clone()),
            )
        }
    };
    adapter.shutdown().await?;
    result
}
