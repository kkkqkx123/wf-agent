//! Server connection: the single place a CLI frontend resolves its client.
//!
//! Every CLI form talks to a running `wf-server` over HTTP. The address
//! resolves through the shared `wf-client` resolution entry (`--remote` or
//! the per-command `--remote`, then an active SSH tunnel entry
//! (`WF_REMOTE_TUNNEL_PORT`), then `WF_REMOTE`, then the local default); the
//! credential comes from `--api-key` or `WF_API_KEY`. The business directory
//! resolves through [`crate::workspace`] (`--directory`, positional
//! `DIRECTORY`, `WF_DIRECTORY`, then the process working directory) and
//! travels as the `x-wf-directory` default header, so workspace-bound
//! handlers (agent runs, workflows, PTY, fs, git) anchor on the directory
//! the user named. Construction probes `GET /health` so a stopped server
//! fails fast with an actionable message instead of surfacing as a later 404.

use wf_client::WfClient;

use crate::args::Cli;
use crate::error::{CliError, CliResult};

/// Connected client handle shared by all commands and session drivers.
pub struct ClientHandle {
    client: WfClient,
    directory: Option<String>,
}

impl ClientHandle {
    /// Resolve connection settings from CLI flags / environment and verify
    /// the server is reachable. The workspace is adopted as the process
    /// working directory first, so local file access and the transmitted
    /// header observe the same root.
    pub async fn from_cli(cli: &Cli) -> CliResult<Self> {
        let workspace =
            crate::workspace::resolve_workspace_dir(cli).map_err(CliError::Arguments)?;
        crate::workspace::adopt_workspace_dir(&workspace).map_err(CliError::Arguments)?;
        let remote = cli.remote.clone().or_else(|| {
            if let Some(crate::args::Command::Run { remote, .. }) = &cli.command {
                remote.clone()
            } else {
                None
            }
        });
        let directory = workspace.to_string_lossy().into_owned();
        let handle = Self {
            client: WfClient::from_remote_with_directory(
                remote,
                cli.api_key.clone(),
                Some(directory.clone()),
            ),
            directory: Some(directory),
        };
        handle.check_reachable().await?;
        Ok(handle)
    }

    pub fn client(&self) -> &WfClient {
        &self.client
    }

    /// Canonical business directory this handle runs in, when resolved.
    /// Callers building explicit `?directory=` filters (session listings,
    /// resume anchors) use this instead of re-resolving.
    pub fn directory(&self) -> Option<&str> {
        self.directory.as_deref()
    }

    pub fn into_client(self) -> WfClient {
        self.client
    }

    async fn check_reachable(&self) -> CliResult<()> {
        self.client
            .health_typed()
            .await
            .map(|_| ())
            .map_err(|e| match e {
                wf_client::ClientError::Http(msg) => CliError::Configuration(format!(
                    "cannot reach wf-server at {} ({msg}); start wf-server first",
                    self.client.base_url()
                )),
                other => CliError::from(other),
            })
    }
}
