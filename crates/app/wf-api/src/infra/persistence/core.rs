use serde_json::Value;

use crate::infra::error::ApiResult;
use crate::infra::events::EventQueryOptions;
use wf_types::events::BaseEvent;

/// Health snapshot of a persistence layer.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PersistenceHealth {
    pub healthy: bool,
    pub storage: String,
    pub pending_writes: usize,
    pub message: Option<String>,
}

/// Unified persistence abstraction over events, execution state snapshots and
/// metrics.
///
/// Backends are interchangeable (memory, sqlite, no-op); the buffered wrapper
/// adds async flush on top of any backend. `wf-api` reads through this layer
/// for event history / timeline / stats queries that must survive the bounded
/// in-memory `EventBus` window.
#[async_trait::async_trait]
pub trait PersistenceLayer: Send + Sync {
    /// Backend identifier (for diagnostics and health reports).
    fn name(&self) -> &str;

    /// Open the backend and (for buffered layers) start the async flush task.
    async fn initialize(&self) -> ApiResult<()>;

    /// Flush pending writes and close the backend.
    async fn shutdown(&self) -> ApiResult<()>;

    /// Number of records buffered in memory awaiting flush.
    fn pending_writes(&self) -> usize;

    // ---------- events ----------

    async fn save_event(&self, event: &BaseEvent) -> ApiResult<()>;

    /// Batch save; backends may implement atomically.
    async fn save_events(&self, events: &[BaseEvent]) -> ApiResult<()>;

    /// Query persisted events matching the options, oldest first.
    async fn query_events(&self, options: &EventQueryOptions) -> ApiResult<Vec<BaseEvent>>;

    async fn count_events(&self, options: &EventQueryOptions) -> ApiResult<usize>;

    async fn clear_events(&self) -> ApiResult<()>;

    /// Flush pending writes to the backend. No-op for non-buffered layers.
    async fn flush(&self) -> ApiResult<()> {
        Ok(())
    }

    // ---------- execution state snapshots ----------

    async fn save_snapshot(&self, key: &str, snapshot: &Value) -> ApiResult<()>;

    async fn load_snapshot(&self, key: &str) -> ApiResult<Option<Value>>;

    /// List snapshots whose key starts with `prefix`, newest first.
    async fn list_snapshots(&self, prefix: &str) -> ApiResult<Vec<(String, Value)>>;

    async fn clear_snapshots(&self, prefix: &str) -> ApiResult<()>;

    // ---------- metrics ----------

    async fn save_metric(&self, key: &str, value: &Value) -> ApiResult<()>;

    async fn query_metrics(&self, key_prefix: &str) -> ApiResult<Vec<(String, Value)>>;

    fn health(&self) -> PersistenceHealth;
}
