use crate::args::{AuditSub, Cli};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &AuditSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        AuditSub::Summary { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/audit/summary"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-summary", data).with_entity(id.clone()),
            )
        }
        AuditSub::Report { id } => {
            let report: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/audit/report"))
                .await?;
            // Inject explicit source string alongside the report so consumers can
            // tell live/persisted/checkpoint without parsing the summary.
            let source_str = report
                .get("summary")
                .and_then(|s| s.get("source"))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown")
                .to_string();
            let data = serde_json::json!({
                "source": source_str,
                "report": report,
            });
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-report", data).with_entity(id.clone()),
            )
        }
        AuditSub::Timeline { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/audit/timeline"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-timeline", data).with_entity(id.clone()),
            )
        }
        AuditSub::Iterations { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/audit/iterations"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-iterations", data).with_entity(id.clone()),
            )
        }
        AuditSub::ToolCalls { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/audit/tool-calls"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-tool-calls", data).with_entity(id.clone()),
            )
        }
        AuditSub::LlmCalls { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/audit/llm-calls"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-llm-calls", data).with_entity(id.clone()),
            )
        }
        AuditSub::NodeExecutions { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{id}/audit/node-executions"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("audit-node-executions", data).with_entity(id.clone()),
            )
        }
    }
}
