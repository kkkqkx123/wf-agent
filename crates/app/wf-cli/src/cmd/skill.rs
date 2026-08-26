use std::io::Write;

use wf_api::entity::skill;

use crate::args::Cli;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli) -> CliResult<()> {
    let adapter = crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run)
        .await?;
    let ctx = adapter.api_context();

    let skills = skill::list_skills(ctx)?;
    let data = serde_json::to_value(&skills)?;
    let envelope = OutputEnvelope::success("skill-list", data);

    render_envelope(cli.output, envelope)?;
    adapter.shutdown().await?;
    Ok(())
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