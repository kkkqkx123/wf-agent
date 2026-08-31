use std::io::Write;

use wf_api::analysis::search;

use crate::args::Cli;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, query: &str, limit: Option<usize>) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();

    let options = search::SearchOptions {
        types: None,
        limit_per_type: limit.map(|l| l / 3),
        limit_total: limit,
    };

    let result = search::search(ctx, query, &options).await?;
    let data = serde_json::to_value(&result)?;
    let envelope = OutputEnvelope::success("search", data);

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
