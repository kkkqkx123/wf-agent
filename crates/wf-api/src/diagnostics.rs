use std::sync::Arc;

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::context::StorageContext;

use crate::context::ApiContext;
use crate::error::ApiResult;

/// Health of a single storage adapter.
#[derive(Debug, Clone, Serialize)]
pub struct StoreDiagnostic {
    pub name: &'static str,
    pub entries: u64,
    pub healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Aggregate storage diagnostics report (TS `StorageDiagnosticsAPI`
/// counterpart).
#[derive(Debug, Clone, Serialize)]
pub struct StorageDiagnosticReport {
    pub stores: Vec<StoreDiagnostic>,
    pub total_entries: u64,
    pub healthy: bool,
}

/// Storage diagnostics: probe every adapter in the shared `StorageContext`
/// for availability and entry counts.
///
/// A probe failure (e.g. a broken backend) degrades that store to
/// `healthy: false` with the error text instead of failing the whole report.
pub struct StorageDiagnosticsApi {
    ctx: Arc<ApiContext>,
}

impl StorageDiagnosticsApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    pub async fn health(&self) -> ApiResult<StorageDiagnosticReport> {
        self.health_for(&self.ctx.storage).await
    }

    async fn health_for(
        &self,
        storage: &Arc<StorageContext>,
    ) -> ApiResult<StorageDiagnosticReport> {
        let mut stores = Vec::new();
        let mut total_entries = 0u64;
        let mut healthy = true;

        macro_rules! probe {
            ($name:literal, $store:expr) => {{
                let diagnostic = probe_store($name, $store).await;
                total_entries += diagnostic.entries;
                healthy &= diagnostic.healthy;
                stores.push(diagnostic);
            }};
        }

        probe!("workflow", &storage.workflow);
        probe!("workflow_execution", &storage.workflow_execution);
        probe!("checkpoint", &storage.checkpoint);
        probe!("task", &storage.task);
        probe!("agent_loop", &storage.agent_loop);
        probe!("agent_execution", &storage.agent_execution);
        probe!("agent_profile", &storage.agent_profile);
        probe!("file_checkpoint", &storage.file_checkpoint);
        probe!("trigger", &storage.trigger);
        probe!("trigger_execution", &storage.trigger_execution);
        probe!("user_interaction", &storage.user_interaction);
        probe!("tool", &storage.tool);
        probe!("script", &storage.script);
        probe!("node_template", &storage.node_template);
        probe!("hook_template", &storage.hook_template);
        probe!("message", &storage.message);
        probe!("variable", &storage.variable);

        Ok(StorageDiagnosticReport {
            stores,
            total_entries,
            healthy,
        })
    }
}

async fn probe_store<E, F, A>(name: &'static str, adapter: &A) -> StoreDiagnostic
where
    A: BaseStorageAdapter<E, F> + Sync,
    E: Send + Sync,
    F: Send + Sync,
{
    match adapter.list(None).await {
        Ok(entries) => StoreDiagnostic {
            name,
            entries: entries.len() as u64,
            healthy: true,
            error: None,
        },
        Err(err) => StoreDiagnostic {
            name,
            entries: 0,
            healthy: false,
            error: Some(err.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn reports_healthy_empty_stores() {
        let ctx = make_ctx();
        let api = StorageDiagnosticsApi::new(ctx);
        let report = api.health().await.unwrap();
        assert!(report.healthy);
        assert_eq!(report.total_entries, 0);
        assert_eq!(report.stores.len(), 17);
        assert!(report.stores.iter().all(|s| s.healthy));
    }

    #[tokio::test]
    async fn counts_existing_entries() {
        let ctx = make_ctx();
        let task = wf_types::TaskStorageMetadata {
            id: "t-1".into(),
            task_type: "x".into(),
            status: "pending".into(),
            created_at: 1,
            updated_at: 1,
        };
        ctx.storage.task.save(&task).await.unwrap();

        let api = StorageDiagnosticsApi::new(ctx);
        let report = api.health().await.unwrap();
        let task_diag = report.stores.iter().find(|s| s.name == "task").unwrap();
        assert_eq!(task_diag.entries, 1);
        assert_eq!(report.total_entries, 1);
    }
}
