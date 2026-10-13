//! Retained passthrough: analysis search has no fixed record schema.

use crate::args::Cli;
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, query: &str, limit: Option<usize>) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let mut path = format!("/api/v1/search?q={query}");
    if let Some(l) = limit {
        path.push_str(&format!("&limit={l}&limit_per_type={}", l / 3));
    }
    let data: serde_json::Value = handle.client().get_json(&path).await?;
    let envelope = OutputEnvelope::success("search", data);

    render_envelope(cli.output, envelope)?;
    Ok(())
}
