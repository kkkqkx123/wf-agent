use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use wf_storage::backend::StorageBackend;
use wf_storage::domain::store::{BatchStore, Maintainable, QueryFilter, Store};
use wf_types::events::BaseEvent;

use crate::error::ApiResult;
use crate::events::EventQueryOptions;

/// Keyspace prefixes used to namespace records inside the KV backend.
const EVENT_KEY_PREFIX: &str = "persistence/event/";
const SNAPSHOT_KEY_PREFIX: &str = "persistence/snapshot/";
const METRIC_KEY_PREFIX: &str = "persistence/metric/";

/// Health snapshot of a persistence layer.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PersistenceHealth {
    pub healthy: bool,
    pub storage: String,
    pub pending_writes: usize,
    pub message: Option<String>,
}

/// Unified persistence abstraction over events, execution state snapshots and
/// metrics (TS `PersistenceLayer` counterpart).
///
/// Backends are interchangeable (memory, sqlite, no-op); the buffered wrapper
/// adds async flush on top of any backend. `wf-api` reads through this layer
/// for event history / timeline / stats queries that must survive the bounded
/// in-memory `EventBus` window.
#[async_trait]
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

/// Persistence layer that discards every write (default when no backend is
/// configured). Keeps the API surface functional while events stay in the
/// bounded `EventBus` window.
pub struct NoOpPersistenceLayer;

#[async_trait]
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
        Ok(())
    }

    async fn save_events(&self, _events: &[BaseEvent]) -> ApiResult<()> {
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
            message: Some("no-op persistence backend".into()),
        }
    }
}

/// KV-store-backed persistence layer. The store is a
/// [`StorageBackend`] so the same adapter serves both the in-memory and the
/// SQLite backends (`StorageBackend::Sqlite`), avoiding a direct sqlx
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

    /// SQLite-backed layer; enabled with the `sqlite` feature.
    #[cfg(feature = "sqlite")]
    pub fn sqlite(path: &str) -> ApiResult<Self> {
        let store = wf_storage::store::sqlite::SqliteStorage::new(path, "persistence")?;
        Ok(Self {
            store: StorageBackend::Sqlite(
                wf_storage::decorator::instrumented::InstrumentedStore::new(store),
            ),
            name: "sqlite".into(),
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

#[async_trait]
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
        Ok(super::events::filter_events(events, options))
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

/// Defaults for the buffered wrapper.
const DEFAULT_EVENT_BUFFER_SIZE: usize = 256;
const DEFAULT_SNAPSHOT_BUFFER_SIZE: usize = 64;
const DEFAULT_METRIC_BUFFER_SIZE: usize = 64;
const DEFAULT_FLUSH_INTERVAL_MS: u64 = 5000;

/// Buffered persistence layer: writes land in in-memory buffers and are
/// flushed to the inner backend when a buffer fills or on a time interval
/// (TS `BufferedPersistenceLayer` counterpart). Queries hit the inner
/// backend; `pending_writes` reports the buffered backlog.
pub struct BufferedPersistenceLayer {
    inner: Arc<dyn PersistenceLayer>,
    event_buffer: Arc<Mutex<Vec<BaseEvent>>>,
    snapshot_buffer: Arc<Mutex<Vec<(String, Value)>>>,
    metric_buffer: Arc<Mutex<Vec<(String, Value)>>>,
    event_buffer_size: usize,
    snapshot_buffer_size: usize,
    metric_buffer_size: usize,
    flush_interval: Duration,
    shutdown_tx: Mutex<Option<watch::Sender<bool>>>,
    flush_handle: Mutex<Option<JoinHandle<()>>>,
    initialized: Mutex<bool>,
}

impl BufferedPersistenceLayer {
    pub fn new(inner: Arc<dyn PersistenceLayer>) -> Self {
        Self {
            inner,
            event_buffer: Arc::new(Mutex::new(Vec::new())),
            snapshot_buffer: Arc::new(Mutex::new(Vec::new())),
            metric_buffer: Arc::new(Mutex::new(Vec::new())),
            event_buffer_size: DEFAULT_EVENT_BUFFER_SIZE,
            snapshot_buffer_size: DEFAULT_SNAPSHOT_BUFFER_SIZE,
            metric_buffer_size: DEFAULT_METRIC_BUFFER_SIZE,
            flush_interval: Duration::from_millis(DEFAULT_FLUSH_INTERVAL_MS),
            shutdown_tx: Mutex::new(None),
            flush_handle: Mutex::new(None),
            initialized: Mutex::new(false),
        }
    }

    pub fn with_event_buffer_size(mut self, size: usize) -> Self {
        self.event_buffer_size = size;
        self
    }

    pub fn with_snapshot_buffer_size(mut self, size: usize) -> Self {
        self.snapshot_buffer_size = size;
        self
    }

    pub fn with_metric_buffer_size(mut self, size: usize) -> Self {
        self.metric_buffer_size = size;
        self
    }

    pub fn with_flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = interval;
        self
    }

    fn spawn_flusher(&self) {
        let mut handle = lock_ok(self.flush_handle.lock());
        if handle.is_some() {
            return;
        }
        let (tx, rx) = watch::channel(false);
        *lock_ok(self.shutdown_tx.lock()) = Some(tx);

        let inner = self.inner.clone();
        let event_buffer = self.event_buffer.clone();
        let snapshot_buffer = self.snapshot_buffer.clone();
        let metric_buffer = self.metric_buffer.clone();
        let interval = self.flush_interval;
        let mut rx = rx;
        let task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {}
                    changed = rx.changed() => {
                        let _ = changed;
                        if *rx.borrow() {
                            break;
                        }
                    }
                }
                let events = drain(&event_buffer);
                let snapshots = drain(&snapshot_buffer);
                let metrics = drain(&metric_buffer);
                let mut empty = events.is_empty() && snapshots.is_empty() && metrics.is_empty();
                if empty {
                    continue;
                }
                if let Err(err) = write_batch(&*inner, &events, &snapshots, &metrics).await {
                    tracing::warn!(target: "wf_api", error = %err, "persistence flush failed; re-buffering");
                    // Re-buffer so a transient backend failure does not lose data.
                    lock_ok(event_buffer.lock()).splice(0..0, events);
                    lock_ok(snapshot_buffer.lock()).splice(0..0, snapshots);
                    lock_ok(metric_buffer.lock()).splice(0..0, metrics);
                    empty = false;
                }
                if empty && *rx.borrow() {
                    break;
                }
            }
            let _ = write_batch(
                &*inner,
                &drain(&event_buffer),
                &drain(&snapshot_buffer),
                &drain(&metric_buffer),
            )
            .await;
        });
        *handle = Some(task);
    }
}

async fn write_batch(
    layer: &dyn PersistenceLayer,
    events: &[BaseEvent],
    snapshots: &[(String, Value)],
    metrics: &[(String, Value)],
) -> ApiResult<()> {
    if !events.is_empty() {
        layer.save_events(events).await?;
    }
    for (key, value) in snapshots {
        layer.save_snapshot(key, value).await?;
    }
    for (key, value) in metrics {
        layer.save_metric(key, value).await?;
    }
    Ok(())
}

/// Drain `buffer` without panicking on a poisoned lock (a panicked holder is
/// unrecoverable anyway; the buffered data is still taken).
fn drain<T>(buffer: &Mutex<Vec<T>>) -> Vec<T> {
    std::mem::take(&mut *buffer.lock().unwrap_or_else(|p| p.into_inner()))
}

/// Recover from a poisoned lock rather than panicking library callers.
fn lock_ok<T>(result: std::sync::LockResult<T>) -> T {
    result.unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[async_trait]
impl PersistenceLayer for BufferedPersistenceLayer {
    fn name(&self) -> &str {
        "buffered"
    }

    async fn initialize(&self) -> ApiResult<()> {
        if *lock_ok(self.initialized.lock()) {
            return Ok(());
        }
        self.inner.initialize().await?;
        self.spawn_flusher();
        *lock_ok(self.initialized.lock()) = true;
        Ok(())
    }

    async fn shutdown(&self) -> ApiResult<()> {
        if !*lock_ok(self.initialized.lock()) {
            return Ok(());
        }
        let shutdown_signal = lock_ok(self.shutdown_tx.lock()).take();
        if let Some(tx) = shutdown_signal {
            let _ = tx.send(true);
        }
        let handle = lock_ok(self.flush_handle.lock()).take();
        if let Some(handle) = handle {
            let _ = tokio::time::timeout(self.flush_interval, handle).await;
        }
        // Final flush of anything buffered after the task exited.
        let events = drain(&self.event_buffer);
        let snapshots = drain(&self.snapshot_buffer);
        let metrics = drain(&self.metric_buffer);
        let _ = write_batch(&*self.inner, &events, &snapshots, &metrics).await;
        self.inner.shutdown().await?;
        *lock_ok(self.initialized.lock()) = false;
        Ok(())
    }

    fn pending_writes(&self) -> usize {
        lock_ok(self.event_buffer.lock()).len()
            + lock_ok(self.snapshot_buffer.lock()).len()
            + lock_ok(self.metric_buffer.lock()).len()
    }

    async fn save_event(&self, event: &BaseEvent) -> ApiResult<()> {
        let mut buffer = lock_ok(self.event_buffer.lock());
        buffer.push(event.clone());
        let full = buffer.len() >= self.event_buffer_size;
        if full {
            let drained = std::mem::take(&mut *buffer);
            let inner = self.inner.clone();
            tokio::spawn(async move {
                let _ = inner.save_events(&drained).await;
            });
        }
        Ok(())
    }

    async fn save_events(&self, events: &[BaseEvent]) -> ApiResult<()> {
        let mut buffer = lock_ok(self.event_buffer.lock());
        buffer.extend_from_slice(events);
        Ok(())
    }

    async fn query_events(&self, options: &EventQueryOptions) -> ApiResult<Vec<BaseEvent>> {
        self.inner.query_events(options).await
    }

    async fn count_events(&self, options: &EventQueryOptions) -> ApiResult<usize> {
        self.inner.count_events(options).await
    }

    async fn clear_events(&self) -> ApiResult<()> {
        lock_ok(self.event_buffer.lock()).clear();
        self.inner.clear_events().await
    }

    async fn save_snapshot(&self, key: &str, snapshot: &Value) -> ApiResult<()> {
        let mut buffer = lock_ok(self.snapshot_buffer.lock());
        buffer.push((key.to_string(), snapshot.clone()));
        if buffer.len() >= self.snapshot_buffer_size {
            let drained = std::mem::take(&mut *buffer);
            let inner = self.inner.clone();
            tokio::spawn(async move {
                for (key, value) in drained {
                    let _ = inner.save_snapshot(&key, &value).await;
                }
            });
        }
        Ok(())
    }

    async fn load_snapshot(&self, key: &str) -> ApiResult<Option<Value>> {
        self.inner.load_snapshot(key).await
    }

    async fn list_snapshots(&self, prefix: &str) -> ApiResult<Vec<(String, Value)>> {
        self.inner.list_snapshots(prefix).await
    }

    async fn clear_snapshots(&self, prefix: &str) -> ApiResult<()> {
        lock_ok(self.snapshot_buffer.lock()).clear();
        self.inner.clear_snapshots(prefix).await
    }

    async fn save_metric(&self, key: &str, value: &Value) -> ApiResult<()> {
        let mut buffer = lock_ok(self.metric_buffer.lock());
        buffer.push((key.to_string(), value.clone()));
        if buffer.len() >= self.metric_buffer_size {
            let drained = std::mem::take(&mut *buffer);
            let inner = self.inner.clone();
            tokio::spawn(async move {
                for (key, value) in drained {
                    let _ = inner.save_metric(&key, &value).await;
                }
            });
        }
        Ok(())
    }

    async fn query_metrics(&self, key_prefix: &str) -> ApiResult<Vec<(String, Value)>> {
        self.inner.query_metrics(key_prefix).await
    }

    fn health(&self) -> PersistenceHealth {
        PersistenceHealth {
            healthy: true,
            storage: self.inner.name().to_string(),
            pending_writes: self.pending_writes(),
            message: None,
        }
    }
}
