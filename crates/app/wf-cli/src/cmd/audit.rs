use wf_api::audit;

use crate::args::{AuditSub, Cli};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &AuditSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        AuditSub::Summary { id } => {
            let s = audit::audit_summary(ctx, id).await?;
            let data = serde_json::to_value(&s)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-summary", data).with_entity(id.clone()),
            )
        }
        AuditSub::Report { id } => {
            let r = audit::audit_report(ctx, id).await?;
            let data = serde_json::to_value(&r)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-report", data).with_entity(id.clone()),
            )
        }
        AuditSub::Timeline { id } => {
            let t = audit::audit_timeline(ctx, id).await?;
            let data = serde_json::to_value(&t)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-timeline", data).with_entity(id.clone()),
            )
        }
        AuditSub::Iterations { id } => {
            let list = audit::list_iterations(ctx, id).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-iterations", data).with_entity(id.clone()),
            )
        }
        AuditSub::ToolCalls { id } => {
            let list = audit::list_tool_calls(ctx, id).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-tool-calls", data).with_entity(id.clone()),
            )
        }
        AuditSub::LlmCalls { id } => {
            let list = audit::list_llm_calls(ctx, id).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-llm-calls", data).with_entity(id.clone()),
            )
        }
        AuditSub::NodeExecutions { id } => {
            let list = audit::list_node_executions(ctx, id).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-node-executions", data).with_entity(id.clone()),
            )
        }
    };
    adapter.shutdown().await?;
    result
}
