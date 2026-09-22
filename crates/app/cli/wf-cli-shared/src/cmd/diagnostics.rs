use wf_api::infra::diagnostics;

use crate::args::Cli;
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run_health(cli: &Cli) -> CliResult<()> {
    let domain =
        crate::domain::DomainHandle::require_embedded(cli, crate::mode::CliMode::Run, "health")
            .await?;
    let ctx = domain
        .api_context()
        .expect("embedded mode must have api_context");
    let report = diagnostics::health(ctx).await?;
    let data = serde_json::to_value(&report)?;
    render_envelope(cli.output, OutputEnvelope::success("health", data))?;
    domain.shutdown().await?;
    Ok(())
}

pub async fn run_diagnostics(cli: &Cli) -> CliResult<()> {
    let domain = crate::domain::DomainHandle::require_embedded(
        cli,
        crate::mode::CliMode::Run,
        "diagnostics",
    )
    .await?;
    let ctx = domain
        .api_context()
        .expect("embedded mode must have api_context");
    let report = diagnostics::diagnose(ctx).await?;
    let data = serde_json::to_value(&report)?;
    render_envelope(cli.output, OutputEnvelope::success("diagnostics", data))?;
    domain.shutdown().await?;
    Ok(())
}
