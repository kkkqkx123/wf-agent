use std::io::Write;

use wf_api::agent::agent_loop_registry;

use crate::args::{Cli, ExecutionSub};
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &ExecutionSub) -> CliResult<()> {
    let adapter = crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run)
        .await?;
    let ctx = adapter.api_context();

    let result = match sub {
        ExecutionSub::List { status } => {
            let filter = status.as_deref().and_then(parse_status);
            let summaries = agent_loop_registry::summaries(ctx, filter.as_ref()).await?;
            let data = serde_json::to_value(&summaries)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("execution-list", data),
            )
        }
        ExecutionSub::Show { id } => {
            let summary = agent_loop_registry::summary(ctx, id).await?;
            match summary {
                Some(s) => {
                    let data = serde_json::to_value(&s)?;
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("execution-show", data).with_entity(id.clone()),
                    )
                }
                None => render_envelope(
                    cli.output,
                    OutputEnvelope::failure("execution-show", format!("execution not found: {id}")),
                ),
            }
        }
    };

    adapter.shutdown().await?;
    result
}

fn parse_status(s: &str) -> Option<agent_loop_registry::AgentLoopFilter> {
    let status = match s.to_ascii_lowercase().as_str() {
        "running" => wf_types::ExecutionStatus::Running,
        "paused" => wf_types::ExecutionStatus::Paused,
        "completed" => wf_types::ExecutionStatus::Completed,
        "failed" => wf_types::ExecutionStatus::Failed,
        _ => return None,
    };
    Some(agent_loop_registry::AgentLoopFilter {
        ids: None,
        status: Some(status),
        profile_id: None,
        tags: None,
        created_at_range: None,
    })
}

fn render_envelope(format: crate::output::OutputFormat, envelope: OutputEnvelope) -> CliResult<()> {
    match format {
        crate::output::OutputFormat::Text => {
            let text = envelope.render(format);
            if let Some(line) = text {
                let mut stdout = std::io::stdout();
                writeln!(stdout, "{line}")?;
            }
            Ok(())
        }
        crate::output::OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&envelope)?;
            let mut stdout = std::io::stdout();
            writeln!(stdout, "{json}")?;
            Ok(())
        }
        crate::output::OutputFormat::JsonLines => {
            let json = serde_json::to_string(&envelope)?;
            let mut stdout = std::io::stdout();
            writeln!(stdout, "{json}")?;
            Ok(())
        }
        crate::output::OutputFormat::Silent => Ok(()),
    }
}