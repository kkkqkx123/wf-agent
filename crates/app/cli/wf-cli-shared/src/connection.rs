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

/// Backend affinity verdict for the resolved connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Affinity {
    /// Same project by path id or shared content fingerprint.
    Confirmed,
    /// Neither side carries a comparable identity; the server may still
    /// be correct, so the caller proceeds with a warning.
    Weak,
    /// Both sides carry identity and they disagree.
    Mismatch,
}

/// Decide affinity from the local and remote project identities. Path ids
/// only match on a shared filesystem; fingerprints bridge machines. A
/// missing fingerprint on either side weakens the verdict instead of
/// failing it, since plain checkouts carry no cross-machine identity.
pub(crate) fn check_affinity(
    local_id: &str,
    local_fingerprint: &wf_common::workspace::ProjectFingerprint,
    remote_id: &str,
    remote_fingerprint: &wf_common::workspace::ProjectFingerprint,
) -> Affinity {
    if local_id == remote_id {
        return Affinity::Confirmed;
    }
    if local_fingerprint.same_project(remote_fingerprint) {
        return Affinity::Confirmed;
    }
    if local_fingerprint.is_empty() || remote_fingerprint.is_empty() {
        return Affinity::Weak;
    }
    Affinity::Mismatch
}

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
        handle.check_api_version().await?;
        handle.check_affinity(cli).await?;
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

    /// Confirm the resolved server actually serves the current project.
    /// The probe runs right after the health check so a wrong backend
    /// fails here instead of surfacing later as writes to another ledger.
    async fn check_affinity(&self, cli: &Cli) -> CliResult<()> {
        if cli.no_affinity_check {
            return Ok(());
        }
        let Some(directory) = self.directory.as_deref() else {
            return Ok(());
        };
        let identity = self.client.backend_identity().await.map_err(|e| {
            probe_error(
                &format!(
                    "backend affinity probe failed at {}; check --remote and --directory",
                    self.client.base_url()
                ),
                e,
            )
        })?;
        let workspace = std::path::PathBuf::from(directory);
        let local_project = wf_common::workspace::discover_project_root(&workspace);
        let local_id = wf_common::workspace::project_id_for(&local_project);
        let local_fingerprint = wf_common::workspace::project_fingerprint(&local_project).await;
        let remote_fingerprint = wf_common::workspace::ProjectFingerprint {
            git_origin: identity.fingerprint.git_origin,
            git_head: identity.fingerprint.git_head,
            marker_id: identity.fingerprint.marker_id,
        };
        match check_affinity(
            &local_id,
            &local_fingerprint,
            &identity.project_id,
            &remote_fingerprint,
        ) {
            Affinity::Confirmed => Ok(()),
            Affinity::Weak => {
                eprintln!(
                    "warning: cannot confirm backend affinity with {} (no shared project identity); \
                     check --remote and --directory, or pass --no-affinity-check to silence",
                    self.client.base_url()
                );
                Ok(())
            }
            Affinity::Mismatch => Err(CliError::Configuration(format!(
                "backend affinity mismatch: server at {} serves a different project; \
                 check --remote / WF_REMOTE_TUNNEL_PORT / --directory, \
                 or pass --no-affinity-check to override",
                self.client.base_url()
            ))),
        }
    }

    /// Refuse a connection whose API version differs from the one this
    /// client was built against. The server exposes the same constant on
    /// `/api/v1/info`, so a mismatch means an incompatible wire contract.
    async fn check_api_version(&self) -> CliResult<()> {
        let info = self.client.info_typed().await.map_err(|e| {
            probe_error(
                &format!(
                    "cannot read API version from {}; check the server version",
                    self.client.base_url()
                ),
                e,
            )
        })?;
        if !wf_client::api_version_compatible(&info.api_version) {
            return Err(CliError::Configuration(format!(
                "API version mismatch with {}: client expects {}, server reports {}; \
                 upgrade the client or the server",
                self.client.base_url(),
                wf_types::API_VERSION,
                info.api_version
            )));
        }
        Ok(())
    }

    async fn check_reachable(&self) -> CliResult<()> {
        self.client
            .health_typed()
            .await
            .map(|_| ())
            .map_err(|e| match e {
                wf_client::ClientError::Http(msg) => {
                    let mut hint = format!(
                        "cannot reach wf-server at {} ({msg}); start wf-server first",
                        self.client.base_url()
                    );
                    if wf_client::tunnel_base_url_from_env().is_some() {
                        hint.push_str(
                            "; the ssh tunnel bridge is set, so the tunnel itself may be down: \
                             re-establish it (`wf remote tunnel <name>`) and retry",
                        );
                    }
                    CliError::Unavailable(hint)
                }
                other => CliError::from(other),
            })
    }
}

/// Map a probe failure to an error, keeping transport outages distinct from
/// configuration problems. A local relay and a remote service are equivalent:
/// `Http`/`Stream` always mean "service unreachable", never "bad flags".
fn probe_error(context: &str, err: wf_client::ClientError) -> CliError {
    use wf_client::ClientError as C;
    match err {
        C::Http(msg) | C::Stream(msg) => {
            CliError::Unavailable(format!("{context}: {msg}"))
        }
        other => CliError::Configuration(format!("{context}: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_common::workspace::ProjectFingerprint;

    fn fingerprinted(origin: Option<&str>, marker: Option<&str>) -> ProjectFingerprint {
        ProjectFingerprint {
            git_origin: origin.map(str::to_string),
            git_head: Some("abc".to_string()),
            marker_id: marker.map(str::to_string),
        }
    }

    #[test]
    fn api_version_matches_only_the_shared_constant() {
        assert!(wf_client::api_version_compatible(wf_types::API_VERSION));
        assert!(!wf_client::api_version_compatible("v0"));
        assert!(!wf_client::api_version_compatible(""));
    }

    #[test]
    fn same_path_id_confirms_without_fingerprint() {
        assert_eq!(
            check_affinity(
                "id",
                &ProjectFingerprint::default(),
                "id",
                &ProjectFingerprint::default()
            ),
            Affinity::Confirmed
        );
    }

    #[test]
    fn shared_origin_confirms_across_machines() {
        assert_eq!(
            check_affinity(
                "local-id",
                &fingerprinted(Some("git@example:repo.git"), None),
                "remote-id",
                &fingerprinted(Some("git@example:repo.git"), None),
            ),
            Affinity::Confirmed
        );
    }

    #[test]
    fn shared_marker_confirms_without_vcs() {
        assert_eq!(
            check_affinity(
                "local-id",
                &fingerprinted(None, Some("proj-1")),
                "remote-id",
                &fingerprinted(None, Some("proj-1")),
            ),
            Affinity::Confirmed
        );
    }

    #[test]
    fn missing_identity_weakens_instead_of_failing() {
        assert_eq!(
            check_affinity(
                "local-id",
                &ProjectFingerprint::default(),
                "remote-id",
                &fingerprinted(Some("git@example:repo.git"), None),
            ),
            Affinity::Weak
        );
    }

    #[test]
    fn conflicting_identity_mismatches() {
        assert_eq!(
            check_affinity(
                "local-id",
                &fingerprinted(Some("git@example:a.git"), None),
                "remote-id",
                &fingerprinted(Some("git@example:b.git"), None),
            ),
            Affinity::Mismatch
        );
    }
}
