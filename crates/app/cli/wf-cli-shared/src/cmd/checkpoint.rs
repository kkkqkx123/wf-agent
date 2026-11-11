//! Checkpoint management across the checkpoint domain: unified records,
//! file checkpoints, and agent-loop checkpoints. Change approvals live in
//! the sibling `approval` command.

use std::io::IsTerminal;

use crate::args::{CheckpointDomain, CheckpointSub, Cli};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat};

fn entity_type(domain: Option<CheckpointDomain>) -> Option<&'static str> {
    // Workflow checkpoints are stored under the `workflow_execution`
    // entity type; only agent loops use `agent_loop`.
    match domain {
        Some(CheckpointDomain::Workflow) => Some("workflow_execution"),
        Some(CheckpointDomain::Agent) => Some("agent_loop"),
        None => None,
    }
}

/// Percent-encode a file path for single-segment `{id}` routes.
fn encode_segment(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Prompt for confirmation before a destructive checkpoint operation.
///
/// Skipped when `--yes` is set or stdin is not a TTY (scripted/CI mode).
fn confirm_destructive(cli: &Cli, action: &str, target: &str) -> CliResult<()> {
    if cli.yes || !std::io::stdin().is_terminal() {
        return Ok(());
    }
    eprint!("? CONFIRM {action} {target} (y/N) ");
    let _ = std::io::Write::flush(&mut std::io::stderr());
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(CliError::Io)?;
    if !matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        return Err(CliError::Interrupted("aborted by user".to_string()));
    }
    Ok(())
}

mod file_read;
mod file_sync;
mod file_write;
mod unified;

pub async fn run(cli: &Cli, sub: &CheckpointSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    if unified::handle_unified(cli, &client, sub).await?.is_some() {
        return Ok(());
    }
    if file_read::handle_file_read(cli, &client, sub)
        .await?
        .is_some()
    {
        return Ok(());
    }
    if file_write::handle_file_write(cli, &client, sub)
        .await?
        .is_some()
    {
        return Ok(());
    }
    if file_sync::handle_file_sync(cli, &client, sub)
        .await?
        .is_some()
    {
        return Ok(());
    }
    Err(CliError::Arguments(
        "unknown checkpoint subcommand".to_string(),
    ))
}
