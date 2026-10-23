//! Server connection: the single place a CLI frontend resolves its client.
//!
//! Every CLI form talks to a running `wf-server` over HTTP. The address
//! resolves from `--remote` (or the per-command `--remote`), then an active
//! SSH tunnel entry (`WF_REMOTE_TUNNEL_PORT`), then `WF_REMOTE`, then the
//! local default; the credential comes from `--api-key` or `WF_API_KEY`.
//! Construction probes `GET /health` so a stopped server fails fast with an
//! actionable message instead of surfacing as a later 404.

use wf_client::WfClient;

use crate::args::Cli;
use crate::error::{CliError, CliResult};

/// Connected client handle shared by all commands and session drivers.
pub struct ClientHandle {
    client: WfClient,
}

impl ClientHandle {
    /// Resolve connection settings from CLI flags / environment and verify
    /// the server is reachable.
    pub async fn from_cli(cli: &Cli) -> CliResult<Self> {
        let mut remote = cli.remote.clone().or_else(|| {
            if let Some(crate::args::Command::Run { remote, .. }) = &cli.command {
                remote.clone()
            } else {
                None
            }
        });
        if remote.as_deref().is_none_or(|s| s.trim().is_empty()) {
            remote = wf_client::tunnel_base_url_from_env().or(remote);
        }
        let handle = Self {
            client: WfClient::from_remote(remote, cli.api_key.clone()),
        };
        handle.check_reachable().await?;
        Ok(handle)
    }

    pub fn client(&self) -> &WfClient {
        &self.client
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
