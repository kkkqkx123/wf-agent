use serde_json::Value;

use super::core::{PersistenceHealth, PersistenceLayer};
use crate::infra::error::ApiResult;
use crate::infra::events::EventQueryOptions;
use wf_types::events::BaseEvent;

/// Persistence layer that discards every write (default when no backend is
/// configured). Keeps the API surface functional while events stay in the
/// bounded `EventBus` window. Every discarded write is counted and surfaced
/// in `health()` so a silently-dropping sink is observable.
pub struct NoOpPersistenceLayer {
    discarded: std::sync::atomic::AtomicU64,
}

impl Default for NoOpPersistenceLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl NoOpPersistenceLayer {
    pub fn new() -> Self {
        Self {
            discarded: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn count_discarded(&self, amount: u64) {
        self.discarded
            .fetch_add(amount, std::sync::atomic::Ordering::Relaxed);
    }
}

#[async_trait::async_trait]
impl PersistenceLayer for NoOpPersistenceLayer {
    fn name(&self) -> &str {
        "noop"
    }

    async fn initialize(&self) -> ApiResult<()> {
        Ok(())
    }

    async fn shutdown(&self) -> ApiResult<()> {
        Ok(())
    }

    fn pending_writes(&self) -> usize {
        0
    }

    async fn save_event(&self, _event: &BaseEvent) -> ApiResult<()> {
        self.count_discarded(1);
        Ok(())
    }

    async fn save_events(&self, events: &[BaseEvent]) -> ApiResult<()> {
        self.count_discarded(events.len() as u64);
        Ok(())
    }

    async fn query_events(&self, _options: &EventQueryOptions) -> ApiResult<Vec<BaseEvent>> {
        Ok(Vec::new())
    }

    async fn count_events(&self, _options: &EventQueryOptions) -> ApiResult<usize> {
        Ok(0)
    }

    async fn clear_events(&self) -> ApiResult<()> {
        Ok(())
    }

    async fn save_snapshot(&self, _key: &str, _snapshot: &Value) -> ApiResult<()> {
        self.count_discarded(1);
        Ok(())
    }

    async fn load_snapshot(&self, _key: &str) -> ApiResult<Option<Value>> {
        Ok(None)
    }

    async fn list_snapshots(&self, _prefix: &str) -> ApiResult<Vec<(String, Value)>> {
        Ok(Vec::new())
    }

    async fn clear_snapshots(&self, _prefix: &str) -> ApiResult<()> {
        Ok(())
    }

    async fn save_metric(&self, _key: &str, _value: &Value) -> ApiResult<()> {
        self.count_discarded(1);
        Ok(())
    }

    async fn query_metrics(&self, _key_prefix: &str) -> ApiResult<Vec<(String, Value)>> {
        Ok(Vec::new())
    }

    fn health(&self) -> PersistenceHealth {
        PersistenceHealth {
            healthy: true,
            storage: "noop".into(),
            pending_writes: 0,
            message: Some(format!(
                "no-op persistence backend; {} writes discarded",
                self.discarded.load(std::sync::atomic::Ordering::Relaxed)
            )),
        }
    }
}
