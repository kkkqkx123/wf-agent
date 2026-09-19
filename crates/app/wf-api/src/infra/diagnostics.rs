use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use wf_storage::backend::StorageBackend;
use wf_storage::context::StorageContext;
use wf_storage::domain::store::Store;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;
use crate::infra::events::EventQueryOptions;

/// Health of a single storage adapter.
#[derive(Debug, Clone, Serialize)]
pub struct StoreDiagnostic {
    pub name: &'static str,
    pub entries: u64,
    pub healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Aggregate storage diagnostics report.
#[derive(Debug, Clone, Serialize)]
pub struct StorageDiagnosticReport {
    pub stores: Vec<StoreDiagnostic>,
    pub total_entries: u64,
    pub healthy: bool,
}

/// Storage diagnostics: probe every backend registered in the shared
/// `StorageContext` for availability and entry counts. The probed set comes
/// from the context registry itself, so newly added entities are covered
/// without touching this module.
///
/// A probe failure (e.g. a broken backend) degrades that store to
/// `healthy: false` with the error text instead of failing the whole report.
pub async fn health(ctx: &ApiContext) -> ApiResult<StorageDiagnosticReport> {
    health_for(&ctx.storage).await
}

async fn health_for(storage: &Arc<StorageContext>) -> ApiResult<StorageDiagnosticReport> {
    let mut stores = Vec::new();
    let mut total_entries = 0u64;
    let mut healthy = true;

    for (id, backend) in storage.named_backends() {
        let diagnostic = probe_backend(id.name(), backend).await;
        total_entries += diagnostic.entries;
        healthy &= diagnostic.healthy;
        stores.push(diagnostic);
    }

    Ok(StorageDiagnosticReport {
        stores,
        total_entries,
        healthy,
    })
}

async fn probe_backend(name: &'static str, backend: &StorageBackend) -> StoreDiagnostic {
    match backend.count(None).await {
        Ok(entries) => StoreDiagnostic {
            name,
            entries,
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

/// Full storage diagnostics report: per-store probes plus persistence-layer
/// status.
#[derive(Debug, Clone, Serialize)]
pub struct StorageDiagnosticsReport {
    pub stores: Vec<StoreDiagnostic>,
    pub total_entries: u64,
    pub healthy: bool,
    pub persistence_storage: String,
    pub persistence_healthy: bool,
    pub persisted_events: u64,
}

/// Storage diagnostics: storage adapters + persistence layer.
pub async fn diagnose(ctx: &ApiContext) -> ApiResult<StorageDiagnosticsReport> {
    let report = health(ctx).await?;
    let persistence = ctx.persistence.health();
    let persisted_events = ctx
        .persistence
        .count_events(&EventQueryOptions::default())
        .await?;
    Ok(StorageDiagnosticsReport {
        stores: report.stores,
        total_entries: report.total_entries,
        healthy: report.healthy,
        persistence_storage: persistence.storage,
        persistence_healthy: persistence.healthy,
        persisted_events: persisted_events as u64,
    })
}

/// Per-store entry counts.
pub async fn item_counts(ctx: &ApiContext) -> ApiResult<BTreeMap<String, u64>> {
    let report = health(ctx).await?;
    Ok(report
        .stores
        .into_iter()
        .map(|store| (store.name.to_string(), store.entries))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn reports_healthy_empty_stores() {
        let ctx = make_ctx();
        let report = health(&ctx).await.unwrap();
        assert!(report.healthy);
        assert_eq!(report.total_entries, 0);
        assert_eq!(report.stores.len(), 20);
        assert!(report.stores.iter().all(|s| s.healthy));
    }

    #[tokio::test]
    async fn counts_existing_entries() {
        let ctx = make_ctx();
        let task = wf_types::TaskStorageMetadata {
            id: "t-1".into(),
            task_type: "x".into(),
            status: "pending".into(),
            execution_id: None,
            instance_id: None,
            created_at: 1,
            updated_at: 1,
        };
        ctx.storage.task.save(&task).await.unwrap();

        let report = health(&ctx).await.unwrap();
        let task_diag = report.stores.iter().find(|s| s.name == "task").unwrap();
        assert_eq!(task_diag.entries, 1);
        assert_eq!(report.total_entries, 1);
    }
}
