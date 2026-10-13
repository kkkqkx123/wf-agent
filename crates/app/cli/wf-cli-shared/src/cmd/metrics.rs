//! Retained passthrough: metrics export switches payload shape by format
//! and analysis stats are rendered verbatim.

use crate::args::{Cli, MetricsSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &MetricsSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        MetricsSub::Show { export } => {
            if let Some(fmt) = export {
                return export_metrics(cli, client, fmt, "metrics-show").await;
            }
            let data: serde_json::Value = client.get_json("/api/v1/analysis/stats").await?;
            render_envelope(cli.output, OutputEnvelope::success("metrics-show", data))
        }
        MetricsSub::Export { format } => {
            export_metrics(cli, client, format, "metrics-export").await
        }
    }
}

async fn export_metrics(
    cli: &Cli,
    client: &wf_client::WfClient,
    format: &str,
    envelope: &str,
) -> CliResult<()> {
    match format.to_ascii_lowercase().as_str() {
        "json" => {
            let value: serde_json::Value = client
                .get_json("/api/v1/metrics/export?format=json")
                .await?;
            if cli.output == crate::output::OutputFormat::Text {
                println!("{}", serde_json::to_string_pretty(&value)?);
                Ok(())
            } else {
                render_envelope(cli.output, OutputEnvelope::success(envelope, value))
            }
        }
        "prometheus" | "prom" => {
            let value: serde_json::Value = client
                .get_json("/api/v1/metrics/export?format=prometheus")
                .await?;
            let text = value.as_str().unwrap_or("").to_string();
            if cli.output == crate::output::OutputFormat::Text {
                println!("{text}");
                Ok(())
            } else {
                let data = serde_json::json!({"prometheus": text});
                render_envelope(cli.output, OutputEnvelope::success(envelope, data))
            }
        }
        _ => Err(CliError::Arguments(format!(
            "invalid export format {format}: expected json|prometheus"
        ))),
    }
}
