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
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    #[tokio::test]
    async fn reports_healthy_empty_stores() {
        let ctx = make_ctx();
        let report = health(&ctx).await.unwrap();
        assert!(report.healthy);
        assert_eq!(report.total_entries, 0);
        assert_eq!(report.stores.len(), ctx.storage.named_backends().len());
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

/// Code-context diagnostic view.
///
/// One read answers "can the service work right now, and if not, why". The
/// view is assembled from cached live state only: no probe, no breaker
/// mutation, no outbound request. It carries no credential, no error stack and
/// no filesystem path.
#[derive(Debug, Clone, Serialize)]
pub struct CodeContextDiagnostic {
    /// Whether the service is switched on in the effective config.
    pub enabled: bool,
    /// Whether any channel is usable right now.
    pub usable: bool,
    /// Single-line verdict for operators.
    pub summary: String,
    /// Full cached snapshot (transport, health, indexes, supervisor).
    pub service: wf_integration::CodeContextStatus,
}

/// Code-context diagnostics: cached service snapshot plus a derived verdict.
///
/// A service that was never wired reports as disabled instead of failing the
/// call, so the endpoint stays usable on hosts without the feature.
pub fn code_context(ctx: &ApiContext) -> CodeContextDiagnostic {
    let Some(service) = ctx.code_context() else {
        return CodeContextDiagnostic {
            enabled: false,
            usable: false,
            summary: "code-context service is not attached to this runtime".to_string(),
            service: wf_integration::CodeContextService::disabled().status(),
        };
    };
    let status = service.status();
    CodeContextDiagnostic {
        enabled: status.enabled,
        usable: status.availability.fold
            || status.availability.navigation
            || status.availability.keyword
            || status.availability.semantic,
        summary: summarize(&status),
        service: status,
    }
}

/// Derive the operator-facing verdict. Ordered by blocker severity so the
/// first true condition is the one that explains the outage.
fn summarize(status: &wf_integration::CodeContextStatus) -> String {
    if !status.enabled {
        return "service is disabled".to_string();
    }
    if let Some(reason) = status.startup_error.as_deref() {
        return format!("managed process did not start: {reason}");
    }
    if let Some(reason) = status.credential_error.as_deref() {
        return format!("credential problem: {reason}");
    }
    if let Some(capability) = status.capabilities.as_ref() {
        if !capability.compatible {
            return format!(
                "service is incompatible: missing {}",
                capability.missing.join(", ")
            );
        }
    }
    if let Some(sidecar) = status.sidecar.as_ref() {
        if !sidecar.phase.is_ready() {
            let detail = sidecar
                .last_error
                .as_deref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default();
            return format!(
                "managed process {:?} after {} restarts{detail}",
                sidecar.phase, sidecar.restarts
            );
        }
    }
    match status.health {
        Some(health) if !health.reachable => {
            return "service is unreachable".to_string();
        }
        Some(health) if !health.healthy => {
            let mut down = Vec::new();
            if !health.vectors {
                down.push("vectors");
            }
            if !health.bm25 {
                down.push("bm25");
            }
            if !health.embedding {
                down.push("embedding");
            }
            return format!("service is degraded: {} unreachable", down.join(", "));
        }
        _ => {}
    }
    if status.project.state == "failed" {
        return format!(
            "project supply failed: {}",
            status.project.reason.as_deref().unwrap_or("unknown reason")
        );
    }
    if status.project.state == "indexing" {
        return format!(
            "project {} is indexing the workspace ({} ms so far)",
            status.project.project_id.unwrap_or_default(),
            status.project.indexing_for_ms.unwrap_or_default()
        );
    }
    if let Some(breaker) = status.breaker.as_ref() {
        if breaker.phase != wf_integration::BreakerPhase::Closed {
            return format!(
                "circuit {:?}, retry in {} ms",
                breaker.phase, breaker.retry_in_ms
            );
        }
    }
    format!(
        "service is usable (project {} state: {})",
        status
            .project
            .project_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "none".to_string()),
        status.project.state
    )
}

#[cfg(test)]
mod code_context_tests {
    use super::*;
    use wf_integration::{CodeContextConfig, CodeContextService, ServiceTransport};
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn ctx_with(service: Option<CodeContextService>) -> Arc<ApiContext> {
        let ctx = ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        );
        Arc::new(match service {
            Some(service) => ctx.with_code_context(Arc::new(service)),
            None => ctx,
        })
    }

    #[tokio::test]
    async fn unattached_service_reports_as_disabled() {
        let ctx = ctx_with(None);
        let view = code_context(&ctx);
        assert!(!view.enabled);
        assert!(!view.usable);
        assert!(view.summary.contains("not attached"), "{}", view.summary);
    }

    #[tokio::test]
    async fn disabled_service_reports_as_disabled() {
        let ctx = ctx_with(Some(CodeContextService::disabled()));
        let view = code_context(&ctx);
        assert!(!view.enabled);
        assert!(!view.usable);
        assert!(view.summary.contains("disabled"), "{}", view.summary);
    }

    #[tokio::test]
    async fn missing_credential_is_reported_without_echoing_it() {
        let ctx = ctx_with(Some(CodeContextService::external(CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some("http://127.0.0.1:9".into()),
                auth_token_env: Some("WF_CCE_DIAG_MISSING_TOKEN".into()),
                ..Default::default()
            },
            ..Default::default()
        })));
        let view = code_context(&ctx);
        assert!(view.enabled);
        assert!(!view.usable);
        assert!(view.summary.contains("credential"), "{}", view.summary);
        let rendered = serde_json::to_string(&view).expect("serializes");
        assert!(!rendered.contains("Bearer"), "{rendered}");
    }

    #[tokio::test]
    async fn enabled_service_without_a_verdict_is_usable() {
        let ctx = ctx_with(Some(CodeContextService::external(CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some("http://127.0.0.1:9".into()),
                ..Default::default()
            },
            ..Default::default()
        })));
        let view = code_context(&ctx);
        assert!(view.enabled);
        assert!(view.usable);
        assert!(view.summary.contains("usable"), "{}", view.summary);
    }
}
