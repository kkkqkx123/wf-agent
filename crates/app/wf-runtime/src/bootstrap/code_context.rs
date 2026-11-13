use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{info, warn};

use wf_integration::{CodeContextConfig, CodeContextService, RunningSidecar, TransportMode};

/// Resolve the configured code-context transport into the live service state
/// plus the supervised sidecar (managed mode only).
///
/// External transport passes through after an advisory readiness probe (a
/// missed probe only warns; per-call degradation still applies). Managed
/// transport decides its loopback address up front, pins the workspace data
/// layout and dispatches the supervisor; the address stays valid across
/// restarts, so the fold chain baked from it never goes stale.
///
/// Bootstrapping never blocks on the workspace index: it waits only for the
/// bounded startup budget, then decides whether the project needs a first
/// index and queues it on a background task.
pub async fn resolve_code_context_service(
    config: Option<CodeContextConfig>,
    project_root: Option<&Path>,
) -> (Arc<CodeContextService>, Option<RunningSidecar>) {
    let Some(config) = config else {
        return (Arc::new(CodeContextService::disabled()), None);
    };
    if !config.enabled {
        return (Arc::new(CodeContextService::disabled()), None);
    }
    let workspace_root = match project_root {
        Some(root) => root.to_path_buf(),
        None => match std::env::current_dir() {
            Ok(cwd) => cwd,
            Err(e) => {
                warn!("Code-context workspace root is unknown (cwd unavailable: {e})");
                PathBuf::new()
            }
        },
    };
    match config.transport.transport_mode {
        TransportMode::External => {
            let service = Arc::new(
                CodeContextService::external(config).with_workspace_root(Some(workspace_root)),
            );
            probe_advisory(&service).await;
            info!("Code-context service status: {:?}", service.status());
            queue_project_supply(&service);
            (service, None)
        }
        TransportMode::Managed => {
            let endpoint = match managed_endpoint(&config) {
                Ok(endpoint) => endpoint,
                Err(e) => {
                    warn!("Managed code-context service unavailable, degrading to skips: {e}");
                    let service = Arc::new(CodeContextService::unresolved(config));
                    service.record_startup_error(e);
                    return (service, None);
                }
            };
            let service = Arc::new(
                CodeContextService::managed(config, endpoint).with_workspace_root(Some(
                    workspace_root.clone(),
                )),
            );
            match start_managed_service(&service, &workspace_root).await {
                Ok(sidecar) => {
                    // Bounded wait keeps the first calls from idling through
                    // their whole timeout. A timeout is not terminal: the
                    // supervisor keeps retrying and the phase is observable.
                    let budget = service.config().transport.managed_startup_timeout_ms;
                    match sidecar.wait_ready(budget).await {
                        Ok(()) => {
                            probe_advisory(&service).await;
                            info!(
                                "Managed code-context service ready at {} ({:?})",
                                service.endpoint().unwrap_or_else(|| "unknown".into()),
                                service.status().sidecar
                            );
                        }
                        Err(e) => {
                            // Not terminal: the supervisor keeps retrying and
                            // the phase stays observable; calls fail fast
                            // until it reports ready.
                            warn!(
                                "Managed code-context service not ready within {budget} ms, supervision continues: {e}"
                            );
                        }
                    }
                    queue_project_supply(&service);
                    (service, Some(sidecar))
                }
                Err(e) => {
                    warn!("Managed code-context service unavailable, degrading to skips: {e}");
                    service.record_startup_error(e);
                    (service, None)
                }
            }
        }
    }
}

/// Start (or restart) the supervised local process for a managed service and
/// attach its status handle. The supervisor owns the data-directory lock and
/// every retry, so this only fails when the launch spec cannot be prepared.
pub async fn start_managed_service(
    service: &CodeContextService,
    workspace_root: &Path,
) -> Result<RunningSidecar, String> {
    let config = service.config();
    let endpoint = service
        .endpoint()
        .ok_or_else(|| "managed service address was not resolved".to_string())?;
    let port = endpoint
        .rsplit(':')
        .next()
        .and_then(|port| port.parse::<u16>().ok())
        .ok_or_else(|| format!("managed service address '{endpoint}' carries no port"))?;
    let data_dir = wf_config::layout::ensure_project_code_context_dir(workspace_root)
        .map_err(|e| format!("code-context data directory not usable: {e}"))?;
    let spec = wf_integration::prepare_managed_service(&config, port, &data_dir)?;
    let sidecar = wf_integration::start_sidecar(spec).await?;
    service.attach_sidecar(sidecar.status_handle());
    Ok(sidecar)
}

/// Loopback address of the managed process: the configured port, or an
/// ephemeral one. Decided before the process starts so every consumer sees
/// the address the process will actually bind.
pub fn managed_endpoint(config: &CodeContextConfig) -> Result<String, String> {
    let port = if config.transport.managed_port == 0 {
        wf_integration::pick_loopback_port()
            .map_err(|e| format!("no free loopback port for managed service: {e}"))?
    } else {
        config.transport.managed_port
    };
    Ok(format!("http://127.0.0.1:{port}"))
}

/// Bake both compression fold chains from the live service config. Folding
/// reads its address and policy from the template, so a reconfigure must
/// re-bake instead of editing node config in place.
pub fn bake_fold_chains(
    registries: &wf_resource::registry::ResourceRegistries,
    service: &Arc<CodeContextService>,
) {
    let chains = [
        wf_resource::predefined::workflow::create_fold_summary_workflow_with_service(
            None, service,
        ),
        wf_resource::predefined::workflow::create_fold_summary_repress_workflow_with_service(
            None, service,
        ),
    ];
    for template in chains {
        if let Err(e) = registries.upsert_workflow_template(template) {
            warn!("code-context fold chain bake skipped: {e}");
        }
    }
}

/// Queue the workspace project resolution and, when needed, the first index.
/// Nothing here blocks bootstrap; failures stay observable in project state
/// and are retried lazily by the next consumer.
pub fn queue_project_supply(service: &Arc<CodeContextService>) {
    if !service.supplies_project() {
        return;
    }
    let service = Arc::clone(service);
    tokio::spawn(async move {
        if let Err(e) = service.supply().await {
            warn!("Code-context project supply deferred: {e}");
        }
    });
}

async fn probe_advisory(service: &CodeContextService) {
    match service.probe().await {
        Ok(report) if !report.healthy => {
            warn!(
                "Code-context service at {} is degraded at startup (vectors={}, bm25={}, embedding={}); retrieval may skip",
                service.endpoint().unwrap_or_else(|| "unknown".into()),
                report.vectors,
                report.bm25,
                report.embedding
            );
        }
        Ok(_) => {}
        Err(e) => {
            warn!("Code-context service at startup probe failed: {e}");
        }
    }
}

impl crate::bootstrap::runtime::Runtime {
    /// Apply a new code-context config to the live runtime.
    ///
    /// The single seam for hot reload: policy changes take effect on the next
    /// call, an address or launch-line change stops the old managed process
    /// and starts a new one, and both fold chains are re-baked from the same
    /// effective config. The trigger (file change or management endpoint) is
    /// deliberately absent: it arrives with the general config reload, so this
    /// section never grows a second mechanism of its own.
    ///
    /// Folding is read-only node configuration for a running execution, so the
    /// caller must not reconfigure while a compression chain is in flight.
    pub async fn reconfigure_code_context(
        &mut self,
        config: CodeContextConfig,
    ) -> crate::error::RuntimeResult<()> {
        use crate::error::RuntimeError;

        let Some(service) = self.code_context_service.clone() else {
            return Err(RuntimeError::Config(
                "code-context service is not attached to this runtime".into(),
            ));
        };
        let outcome = service.apply_config(config.clone()).await;
        if let Some(reason) = outcome.reason.as_deref() {
            info!("Code-context reconfigure requires a managed restart: {reason}");
        }
        if outcome.restart_managed {
            if let Some(sidecar) = self.code_context_sidecar.take() {
                sidecar.shutdown().await;
            }
            let endpoint = managed_endpoint(&config)
                .map_err(|e| RuntimeError::Config(format!("code-context reconfigure: {e}")))?;
            service.set_managed_endpoint(endpoint);
            let Some(root) = service.workspace_root().map(Path::to_path_buf) else {
                return Err(RuntimeError::Config(
                    "code-context reconfigure needs a workspace root to start the managed process"
                        .into(),
                ));
            };
            let sidecar = start_managed_service(&service, &root)
                .await
                .map_err(|e| RuntimeError::Config(format!("code-context reconfigure: {e}")))?;
            let budget = config.transport.managed_startup_timeout_ms;
            if let Err(e) = sidecar.wait_ready(budget).await {
                warn!("Reconfigured code-context service not ready yet: {e}");
            }
            self.code_context_sidecar = Some(sidecar);
        } else if config.enabled {
            probe_advisory(&service).await;
        }
        bake_fold_chains(&self.registries, &service);
        queue_project_supply(&service);
        self.tools.code_context = Some(service.config());
        info!("Code-context reconfigured: {:?}", service.status());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::Registry;
    use wf_integration::{FoldPolicy, ProjectPolicy, ProjectState, ServiceTransport};
    use wf_resource::registry::ResourceRegistries;

    fn external_config(base_url: &str, max_tokens: usize) -> CodeContextConfig {
        CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some(base_url.to_string()),
                ..Default::default()
            },
            fold: FoldPolicy {
                max_tokens,
                ..Default::default()
            },
            project: ProjectPolicy::default(),
            ..Default::default()
        }
    }

    fn fold_node_config(registries: &ResourceRegistries, workflow_id: &str) -> serde_json::Value {
        let template = registries.workflows.get(workflow_id).expect("chain baked");
        template
            .definition
            .nodes
            .iter()
            .find(|node| node.id == wf_resource::predefined::workflow::FOLD_SUMMARY_FOLD_NODE_ID)
            .and_then(|node| node.config.clone())
            .expect("fold node config")
    }

    #[tokio::test]
    async fn reconfigure_rebakes_both_fold_chains() {
        let registries = ResourceRegistries::new();
        let service = Arc::new(CodeContextService::external(external_config(
            "http://a:1",
            2000,
        )));
        bake_fold_chains(&registries, &service);
        for workflow in [
            wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID,
            wf_resource::predefined::workflow::FOLD_SUMMARY_REPRESS_WORKFLOW_ID,
        ] {
            let node = fold_node_config(&registries, workflow);
            assert_eq!(node["service_base_url"], serde_json::json!("http://a:1"));
            assert_eq!(node["max_tokens"], serde_json::json!(2000));
        }

        let outcome = service
            .apply_config(external_config("http://b:2", 512))
            .await;
        assert!(!outcome.restart_managed);
        bake_fold_chains(&registries, &service);
        for workflow in [
            wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID,
            wf_resource::predefined::workflow::FOLD_SUMMARY_REPRESS_WORKFLOW_ID,
        ] {
            let node = fold_node_config(&registries, workflow);
            assert_eq!(node["service_base_url"], serde_json::json!("http://b:2"));
            assert_eq!(node["max_tokens"], serde_json::json!(512));
        }
        assert!(
            matches!(
                service.project_state(),
                ProjectState::NotSupplied
            ),
            "external supply keeps the project unattributed"
        );
    }

    #[tokio::test]
    async fn disabling_the_service_reports_that_it_stays_baked() {
        let registries = ResourceRegistries::new();
        let service = Arc::new(CodeContextService::external(external_config(
            "http://a:1",
            2000,
        )));
        bake_fold_chains(&registries, &service);
        let mut disabled = external_config("http://a:1", 2000);
        disabled.enabled = false;
        let outcome = service.apply_config(disabled).await;
        assert!(!outcome.restart_managed);
        assert!(service.endpoint().is_none());
        bake_fold_chains(&registries, &service);
        let node = fold_node_config(
            &registries,
            wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID,
        );
        assert!(
            node.get("service_base_url").is_none(),
            "a disabled service must bake no remote address: {node}"
        );
    }
}
