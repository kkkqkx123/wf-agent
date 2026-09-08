use wf_api::analysis::search;

use crate::args::Cli;
use crate::cmd::render::render_envelope;
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
