use wf_api::analysis::stats;

use crate::args::{Cli, MetricsSub};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &MetricsSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        MetricsSub::Show { export } => {
            let registry = stats::registry(ctx)?;
            if let Some(fmt) = export {
                match fmt.to_ascii_lowercase().as_str() {
                    "json" => {
                        let value = stats::export_json(registry);
                        if cli.output == crate::output::OutputFormat::Text {
                            println!("{}", serde_json::to_string_pretty(&value)?);
                            Ok(())
                        } else {
                            render_envelope(
                                cli.output,
                                OutputEnvelope::success("metrics-show", value),
                            )
                        }
                    }
                    "prometheus" | "prom" => {
                        let text = stats::export_prometheus(registry);
                        if cli.output == crate::output::OutputFormat::Text {
                            println!("{text}");
                            Ok(())
                        } else {
                            let data = serde_json::json!({"prometheus": text});
                            render_envelope(
                                cli.output,
                                OutputEnvelope::success("metrics-show", data),
                            )
                        }
                    }
                    _ => Err(CliError::Arguments(format!(
                        "invalid export format {fmt}: expected json|prometheus"
                    ))),
                }
            } else {
                let workflow = stats::workflow_stats(registry);
                let node = stats::node_stats(registry);
                let agent = stats::agent_stats(registry);
                let tool = stats::tool_stats(registry);
                let error = stats::error_stats(registry);
                let event = stats::event_stats(registry);
                let data = serde_json::json!({
                    "workflow": workflow,
                    "node": node,
                    "agent": agent,
                    "tool": tool,
                    "error": error,
                    "event": event,
                });
                render_envelope(cli.output, OutputEnvelope::success("metrics-show", data))
            }
        }
        MetricsSub::Export { format } => {
            let registry = stats::registry(ctx)?;
            match format.to_ascii_lowercase().as_str() {
                "json" => {
                    let value = stats::export_json(registry);
                    if cli.output == crate::output::OutputFormat::Text {
                        println!("{}", serde_json::to_string_pretty(&value)?);
                        Ok(())
                    } else {
                        render_envelope(
                            cli.output,
                            OutputEnvelope::success("metrics-export", value),
                        )
                    }
                }
                "prometheus" | "prom" => {
                    let text = stats::export_prometheus(registry);
                    if cli.output == crate::output::OutputFormat::Text {
                        println!("{text}");
                        Ok(())
                    } else {
                        let data = serde_json::json!({"prometheus": text});
                        render_envelope(cli.output, OutputEnvelope::success("metrics-export", data))
                    }
                }
                _ => Err(CliError::Arguments(format!(
                    "invalid format {format}: expected json|prometheus"
                ))),
            }
        }
    };
    adapter.shutdown().await?;
    result
}
