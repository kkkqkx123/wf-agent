use serde_json::Value;
use wf_storage::backend::StorageBackend;
use wf_storage::domain::store::{Maintainable, QueryFilter, Store, StoreExt};
use wf_storage::domain::EntityIndexes;

use super::core::{PersistenceHealth, PersistenceLayer};
use crate::infra::error::ApiResult;
use crate::infra::events::EventQueryOptions;
use wf_types::events::BaseEvent;

/// Keyspace prefixes used to namespace records inside the KV backend.
pub(crate) const EVENT_KEY_PREFIX: &str = "persistence/event/";
pub(crate) const SNAPSHOT_KEY_PREFIX: &str = "persistence/snapshot/";
pub(crate) const METRIC_KEY_PREFIX: &str = "persistence/metric/";

/// KV-store-backed persistence layer. The store is a
/// [`StorageBackend`] so the same adapter serves both the in-memory and the
/// Sqlite backends (`StorageBackend::Sqlite`), avoiding a direct sqlx
/// dependency in `wf-api`.
///
/// Records are stored as JSON blobs under a namespaced key prefix; event
/// queries list the prefix and filter in memory (bounded by the store size).
pub struct StorePersistenceLayer {
    store: StorageBackend,
    name: String,
}

impl StorePersistenceLayer {
    pub fn memory() -> Self {
        Self {
            store: StorageBackend::new_memory(),
            name: "memory".into(),
        }
    }

    /// Sqlite-backed layer sharing the configured database file.
    pub async fn sqlite(path: &str) -> ApiResult<Self> {
        let store = wf_storage::backend::StorageBackend::new_sqlite(
            path,
            "persistence",
            EntityIndexes::NONE,
        )
        .await?;
        Ok(Self {
            store,
            name: "sqlite".into(),
        })
    }

    /// PostgreSQL-backed layer sharing the configured database.
    pub async fn postgres(connection_string: &str) -> ApiResult<Self> {
        let store = wf_storage::backend::StorageBackend::new_postgres(
            connection_string,
            "persistence",
            EntityIndexes::NONE,
        )
        .await?;
        Ok(Self {
            store,
            name: "postgres".into(),
        })
    }

    fn event_key(id: &str) -> String {
        format!("{EVENT_KEY_PREFIX}{id}")
    }

    fn snapshot_key(key: &str) -> String {
        format!("{SNAPSHOT_KEY_PREFIX}{key}")
    }

    fn metric_key(key: &str) -> String {
        format!("{METRIC_KEY_PREFIX}{key}")
    }

    fn strip(prefix: &str, key: &str) -> String {
        key.strip_prefix(prefix).unwrap_or(key).to_string()
    }
}

#[async_trait::async_trait]
impl PersistenceLayer for StorePersistenceLayer {
    fn name(&self) -> &str {
        &self.name
    }

    async fn initialize(&self) -> ApiResult<()> {
        Ok(())
    }

    async fn shutdown(&self) -> ApiResult<()> {
        let _ = self.store.sync().await;
        Ok(())
    }

    fn pending_writes(&self) -> usize {
        0
    }

    async fn save_event(&self, event: &BaseEvent) -> ApiResult<()> {
        self.save_events(std::slice::from_ref(event)).await
    }

    async fn save_events(&self, events: &[BaseEvent]) -> ApiResult<()> {
        for event in events {
            let payload = serde_json::to_vec(event)?;
            let metadata = serde_json::json!({
                "type": event.r#type.as_str(),
                "timestamp": event.timestamp,
                "execution_id": event.execution_id.as_deref(),
                "workflow_id": event.workflow_id.as_deref(),
                "agent_loop_id": event.agent_loop_id.as_deref(),
            });
            self.store
                .save(&Self::event_key(&event.id), &payload, &metadata)
                .await?;
        }
        Ok(())
    }

    async fn query_events(&self, options: &EventQueryOptions) -> ApiResult<Vec<BaseEvent>> {
        let filter = QueryFilter::new().with_id_prefix(EVENT_KEY_PREFIX);
        let ids: Vec<String> = self
            .store
            .list(Some(&filter))
            .await?
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let records = self.store.load_batch(&ids).await?;
        let mut events = Vec::new();
        for (_, payload, _) in records {
            if let Ok(event) = serde_json::from_slice::<BaseEvent>(&payload) {
                events.push(event);
            }
        }
        events.sort_by_key(|e| e.timestamp);
        Ok(super::super::events::filter_events(events, options))
    }

    async fn count_events(&self, options: &EventQueryOptions) -> ApiResult<usize> {
        Ok(self.query_events(options).await?.len())
    }

    async fn clear_events(&self) -> ApiResult<()> {
        let filter = QueryFilter::new().with_id_prefix(EVENT_KEY_PREFIX);
        let ids: Vec<String> = self
            .store
            .list(Some(&filter))
            .await?
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        self.store.delete_batch(&ids).await?;
        Ok(())
    }

    async fn save_snapshot(&self, key: &str, snapshot: &Value) -> ApiResult<()> {
        let payload = serde_json::to_vec(snapshot)?;
        self.store
            .save(&Self::snapshot_key(key), &payload, &Value::Null)
            .await?;
        Ok(())
    }

    async fn load_snapshot(&self, key: &str) -> ApiResult<Option<Value>> {
        let Some((payload, _)) = self.store.load(&Self::snapshot_key(key)).await? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(&payload)?))
    }

    async fn list_snapshots(&self, prefix: &str) -> ApiResult<Vec<(String, Value)>> {
        let full_prefix = format!("{SNAPSHOT_KEY_PREFIX}{prefix}");
        let filter = QueryFilter::new().with_id_prefix(&full_prefix);
        let ids: Vec<String> = self
            .store
            .list(Some(&filter))
            .await?
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let records = self.store.load_batch(&ids).await?;
        let mut snapshots = Vec::new();
        for (id, payload, _) in records {
            if let Ok(value) = serde_json::from_slice::<Value>(&payload) {
                snapshots.push((Self::strip(SNAPSHOT_KEY_PREFIX, &id), value));
            }
        }
        snapshots.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(snapshots)
    }

    async fn clear_snapshots(&self, prefix: &str) -> ApiResult<()> {
        let full_prefix = format!("{SNAPSHOT_KEY_PREFIX}{prefix}");
        let filter = QueryFilter::new().with_id_prefix(&full_prefix);
        let ids: Vec<String> = self
            .store
            .list(Some(&filter))
            .await?
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        self.store.delete_batch(&ids).await?;
        Ok(())
    }

    async fn save_metric(&self, key: &str, value: &Value) -> ApiResult<()> {
        let payload = serde_json::to_vec(value)?;
        self.store
            .save(&Self::metric_key(key), &payload, &Value::Null)
            .await?;
        Ok(())
    }

    async fn query_metrics(&self, key_prefix: &str) -> ApiResult<Vec<(String, Value)>> {
        let full_prefix = format!("{METRIC_KEY_PREFIX}{key_prefix}");
        let filter = QueryFilter::new().with_id_prefix(&full_prefix);
        let ids: Vec<String> = self
            .store
            .list(Some(&filter))
            .await?
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let records = self.store.load_batch(&ids).await?;
        let mut metrics = Vec::new();
        for (id, payload, _) in records {
            if let Ok(value) = serde_json::from_slice::<Value>(&payload) {
                metrics.push((Self::strip(METRIC_KEY_PREFIX, &id), value));
            }
        }
        metrics.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(metrics)
    }

    fn health(&self) -> PersistenceHealth {
        PersistenceHealth {
            healthy: true,
            storage: self.name.clone(),
            pending_writes: 0,
            message: None,
        }
    }
}
