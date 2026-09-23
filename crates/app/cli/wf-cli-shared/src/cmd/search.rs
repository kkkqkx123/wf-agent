use wf_api::analysis::search;

use crate::args::Cli;
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, query: &str, limit: Option<usize>) -> CliResult<()> {
    let domain =
        crate::domain::DomainHandle::require_embedded(cli, crate::mode::CliMode::Run, "search")
            .await?;
    let ctx = domain
        .api_context()
        .expect("embedded mode must have api_context");

    let options = search::SearchOptions {
        types: None,
        limit_per_type: limit.map(|l| l / 3),
        limit_total: limit,
        cursor: None,
    };

    let result = search::search(ctx, query, &options).await?;
    let data = serde_json::to_value(&result)?;
    let envelope = OutputEnvelope::success("search", data);

    render_envelope(cli.output, envelope)?;
    domain.shutdown().await?;
    Ok(())
}
