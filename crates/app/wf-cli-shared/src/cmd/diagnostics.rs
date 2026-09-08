use wf_api::infra::diagnostics;

use crate::args::Cli;
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run_health(cli: &Cli) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let report = diagnostics::health(ctx).await?;
    let data = serde_json::to_value(&report)?;
    render_envelope(cli.output, OutputEnvelope::success("health", data))?;
    adapter.shutdown().await?;
    Ok(())
}

pub async fn run_diagnostics(cli: &Cli) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let report = diagnostics::diagnose(ctx).await?;
    let data = serde_json::to_value(&report)?;
    render_envelope(cli.output, OutputEnvelope::success("diagnostics", data))?;
    adapter.shutdown().await?;
    Ok(())
}
