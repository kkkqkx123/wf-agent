//! Retained passthrough: storage health and diagnose follow backend
//! internals and are rendered verbatim.

use crate::args::Cli;
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run_health(cli: &Cli) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let data: serde_json::Value = handle
        .client()
        .get_json("/api/v1/storage/health")
        .await?;
    render_envelope(cli.output, OutputEnvelope::success("health", data))?;
    Ok(())
}

pub async fn run_diagnostics(cli: &Cli) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let data: serde_json::Value = handle
        .client()
        .get_json("/api/v1/storage/diagnose")
        .await?;
    render_envelope(cli.output, OutputEnvelope::success("diagnostics", data))?;
    Ok(())
}
