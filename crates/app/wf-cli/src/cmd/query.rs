use std::io::Write;

use wf_api::query::{self, FilterCriteria, PaginationOptions};

use crate::args::Cli;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(
    cli: &Cli,
    status: Option<&str>,
    workflow_id: Option<&str>,
    limit: Option<usize>,
) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();

    let filters = FilterCriteria {
        workflow_id: workflow_id.map(String::from),
        status: status.map(String::from),
        start_time_from: None,
        start_time_to: None,
        tags: None,
        custom: None,
    };

    let pagination = PaginationOptions {
        limit: limit.unwrap_or(query::DEFAULT_QUERY_LIMIT),
        offset: 0,
    };

    let records = query::query(ctx, Some(&filters), None, Some(&pagination)).await?;
    let data = serde_json::to_value(&records)?;
    let envelope = OutputEnvelope::success("query-executions", data);

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
