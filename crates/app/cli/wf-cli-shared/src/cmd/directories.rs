//! Workspace self-check: which directory this invocation runs in, where
//! the choice came from, and what the server allows and remembers.
//!
//! Read-only by nature: resolution never creates directories, and the
//! server calls only read the allowlist and the validated-directory cache.

use crate::args::Cli;
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;
use crate::workspace::{self, DIRECTORY_ENV_VAR};

/// Show the resolved workspace, its source, and the server directory state.
pub async fn run(cli: &Cli) -> CliResult<()> {
    let source = if cli.directory.is_some() {
        "flag"
    } else if cli.directory_arg.is_some() {
        "positional"
    } else if std::env::var(DIRECTORY_ENV_VAR)
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
    {
        "env"
    } else {
        "cwd"
    };
    let workspace = workspace::resolve_workspace_dir(cli).map_err(CliError::Arguments)?;
    let in_workspace = Cli {
        directory: Some(workspace.clone()),
        directory_arg: None,
        ..cli.clone()
    };
    let server = match ClientHandle::from_cli(&in_workspace).await {
        Ok(handle) => handle
            .client()
            .get_json::<serde_json::Value>("/api/v1/system/directories")
            .await
            .map(|v| v.get("data").cloned().unwrap_or(serde_json::Value::Null))
            .map_err(|e| format!("cannot read server directories: {e}")),
        Err(e) => Err(e.to_string()),
    };
    let mut data = serde_json::json!({
        "directory": workspace.to_string_lossy(),
        "source": source,
        "project": workspace::discover_project_root(&workspace).to_string_lossy(),
    });
    match server {
        Ok(state) => {
            data["server"] = state;
        }
        Err(detail) => {
            data["server_error"] = serde_json::Value::String(detail);
        }
    }
    render_envelope(cli.output, OutputEnvelope::success("directories", data))?;
    Ok(())
}
