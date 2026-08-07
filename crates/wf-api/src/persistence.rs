use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;
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
        self.discarded.fetch_add(amount, std::sync::atomic::Ordering::Relaxed);
    }
}

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
    pub async fn sqlite(path: &str) -> ApiResult<Self> {
        let store = wf_storage::store::sqlite::SqliteStorage::new(path, "persistence").await?;
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

/// Write request enqueued on the buffered layer's channel. The channel gives
/// natural batching (a single flusher drains many writes per wake-up) plus
/// backpressure-free ingestion; `Flush`/`Shutdown` drive the shutdown path.
enum WriteOp {
    Event(BaseEvent),
    Snapshot(String, Value),
    Metric(String, Value),
    /// Force an immediate flush of the flusher's batch (used by `clear_*` and
    /// the first stage of `shutdown`).
    Flush,
    /// Flush the remaining batch and exit the flusher task (second stage of
    /// `shutdown`).
    Shutdown,
}

/// Buffered persistence layer: writes land on an unbounded `mpsc` channel and
/// are drained by a single flusher task that batches them into the inner
/// backend when a buffer fills or on a time interval (TS
/// `BufferedPersistenceLayer` counterpart). Queries hit the inner backend;
/// `pending_writes` reports the un-persisted backlog.
pub struct BufferedPersistenceLayer {
    inner: Arc<dyn PersistenceLayer>,
    /// Write entry point. `None` until `initialize` spawns the flusher.
    tx: Mutex<Option<mpsc::UnboundedSender<WriteOp>>>,
    event_buffer_size: usize,
    snapshot_buffer_size: usize,
    metric_buffer_size: usize,
    flush_interval: Duration,
    flush_handle: Mutex<Option<JoinHandle<()>>>,
    initialized: Mutex<bool>,
    /// Records enqueued but not yet persisted (flusher-local backlog).
    pending: Arc<AtomicUsize>,
}

impl BufferedPersistenceLayer {
    pub fn new(inner: Arc<dyn PersistenceLayer>) -> Self {
        Self {
            inner,
            tx: Mutex::new(None),
            event_buffer_size: DEFAULT_EVENT_BUFFER_SIZE,
            snapshot_buffer_size: DEFAULT_SNAPSHOT_BUFFER_SIZE,
            metric_buffer_size: DEFAULT_METRIC_BUFFER_SIZE,
            flush_interval: Duration::from_millis(DEFAULT_FLUSH_INTERVAL_MS),
            flush_handle: Mutex::new(None),
            initialized: Mutex::new(false),
            pending: Arc::new(AtomicUsize::new(0)),
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

    /// Enqueue a flush and wait for the in-flight backlog to drain so a
    /// following `clear_*` sees a clean backend. Bounded by `flush_interval`;
    /// on a persistent backend failure the batch stays buffered and is
    /// retried on the next tick.
    async fn flush_and_wait(&self) {
        if let Some(tx) = lock_ok(self.tx.lock()).as_ref() {
            let _ = tx.send(WriteOp::Flush);
        }
        let deadline = tokio::time::Instant::now() + self.flush_interval;
        while self.pending.load(Ordering::Relaxed) != 0 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    fn spawn_flusher(
        inner: Arc<dyn PersistenceLayer>,
        mut rx: mpsc::UnboundedReceiver<WriteOp>,
        event_limit: usize,
        snapshot_limit: usize,
        metric_limit: usize,
        flush_interval: Duration,
        pending: Arc<AtomicUsize>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            // First tick after `flush_interval`: writes landing right after
            // `initialize` are batched rather than flushed by the initial
            // immediate tick.
            let mut ticker = tokio::time::interval_at(
                tokio::time::Instant::now() + flush_interval,
                flush_interval,
            );
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut events: Vec<BaseEvent> = Vec::new();
            let mut snapshots: Vec<(String, Value)> = Vec::new();
            let mut metrics: Vec<(String, Value)> = Vec::new();
            let mut just_failed = false;

            loop {
                let mut ticked = false;
                tokio::select! {
                    _ = ticker.tick() => ticked = true,
                    msg = rx.recv() => match msg {
                        None => break,
                        Some(WriteOp::Event(event)) => {
                            events.push(event);
                            pending.fetch_add(1, Ordering::Relaxed);
                        }
                        Some(WriteOp::Snapshot(key, value)) => {
                            snapshots.push((key, value));
                            pending.fetch_add(1, Ordering::Relaxed);
                        }
                        Some(WriteOp::Metric(key, value)) => {
                            metrics.push((key, value));
                            pending.fetch_add(1, Ordering::Relaxed);
                        }
                        Some(WriteOp::Flush) => {
                            just_failed = !flush_batch(&*inner, &mut events, &mut snapshots, &mut metrics, &pending).await;
                        }
                        Some(WriteOp::Shutdown) => {
                            let _ = flush_batch(&*inner, &mut events, &mut snapshots, &mut metrics, &pending).await;
                            break;
                        }
                    },
                }

                if ticked {
                    // Periodic flush; also the retry cadence after a failure.
                    if !events.is_empty() || !snapshots.is_empty() || !metrics.is_empty() {
                        just_failed = !flush_batch(&*inner, &mut events, &mut snapshots, &mut metrics, &pending).await;
                    }
                    continue;
                }
                if just_failed {
                    // Back off until the next tick instead of busy-looping
                    // against a persistently failing backend.
                    continue;
                }
                if events.len() >= event_limit
                    || snapshots.len() >= snapshot_limit
                    || metrics.len() >= metric_limit
                {
                    just_failed = !flush_batch(&*inner, &mut events, &mut snapshots, &mut metrics, &pending).await;
                }
            }
        })
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

/// Flush the flusher-local batch. On success clears the batch and decrements
/// `pending`; on failure the batch is kept (`pending` unchanged) for the next
/// tick / `Shutdown` retry, mirroring the old re-buffer semantics.
async fn flush_batch(
    inner: &dyn PersistenceLayer,
    events: &mut Vec<BaseEvent>,
    snapshots: &mut Vec<(String, Value)>,
    metrics: &mut Vec<(String, Value)>,
    pending: &AtomicUsize,
) -> bool {
    if events.is_empty() && snapshots.is_empty() && metrics.is_empty() {
        return true;
    }
    match write_batch(inner, events, snapshots, metrics).await {
        Ok(()) => {
            let count = events.len() + snapshots.len() + metrics.len();
            events.clear();
            snapshots.clear();
            metrics.clear();
            pending.fetch_sub(count, Ordering::Relaxed);
            true
        }
        Err(err) => {
            tracing::warn!(target: "wf_api", error = %err, "persistence flush failed; re-buffering for retry");
            false
        }
    }
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
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = Self::spawn_flusher(
            self.inner.clone(),
            rx,
            self.event_buffer_size,
            self.snapshot_buffer_size,
            self.metric_buffer_size,
            self.flush_interval,
            self.pending.clone(),
        );
        *lock_ok(self.tx.lock()) = Some(tx);
        *lock_ok(self.flush_handle.lock()) = Some(handle);
        *lock_ok(self.initialized.lock()) = true;
        Ok(())
    }

    async fn shutdown(&self) -> ApiResult<()> {
        if !*lock_ok(self.initialized.lock()) {
            return Ok(());
        }
        // Two-phase shutdown: flush in-flight data first, then ask the flusher
        // to flush once more and exit. The flusher never blocks shutdown.
        if let Some(tx) = lock_ok(self.tx.lock()).as_ref() {
            let _ = tx.send(WriteOp::Flush);
            let _ = tx.send(WriteOp::Shutdown);
        }
        let handle = lock_ok(self.flush_handle.lock()).take();
        if let Some(handle) = handle {
            let _ = tokio::time::timeout(self.flush_interval, handle).await;
        }
        self.inner.shutdown().await?;
        *lock_ok(self.initialized.lock()) = false;
        Ok(())
    }

    fn pending_writes(&self) -> usize {
        self.pending.load(Ordering::Relaxed)
    }

    async fn save_event(&self, event: &BaseEvent) -> ApiResult<()> {
        if let Some(tx) = lock_ok(self.tx.lock()).as_ref() {
            let _ = tx.send(WriteOp::Event(event.clone()));
        }
        Ok(())
    }

    async fn save_events(&self, events: &[BaseEvent]) -> ApiResult<()> {
        if let Some(tx) = lock_ok(self.tx.lock()).as_ref() {
            for event in events {
                let _ = tx.send(WriteOp::Event(event.clone()));
            }
        }
        Ok(())
    }

    async fn query_events(&self, options: &EventQueryOptions) -> ApiResult<Vec<BaseEvent>> {
        self.inner.query_events(options).await
    }

    async fn count_events(&self, options: &EventQueryOptions) -> ApiResult<usize> {
        self.inner.count_events(options).await
    }

    async fn clear_events(&self) -> ApiResult<()> {
        self.flush_and_wait().await;
        self.inner.clear_events().await
    }

    async fn save_snapshot(&self, key: &str, snapshot: &Value) -> ApiResult<()> {
        if let Some(tx) = lock_ok(self.tx.lock()).as_ref() {
            let _ = tx.send(WriteOp::Snapshot(key.to_string(), snapshot.clone()));
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
        self.flush_and_wait().await;
        self.inner.clear_snapshots(prefix).await
    }

    async fn save_metric(&self, key: &str, value: &Value) -> ApiResult<()> {
        if let Some(tx) = lock_ok(self.tx.lock()).as_ref() {
            let _ = tx.send(WriteOp::Metric(key.to_string(), value.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingLayer {
        events: std::sync::Mutex<Vec<BaseEvent>>,
        snapshots: std::sync::Mutex<Vec<(String, Value)>>,
        metrics: std::sync::Mutex<Vec<(String, Value)>>,
    }

    #[async_trait]
    impl PersistenceLayer for RecordingLayer {
        fn name(&self) -> &str {
            "recording"
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
        async fn save_event(&self, event: &BaseEvent) -> ApiResult<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
        async fn save_events(&self, events: &[BaseEvent]) -> ApiResult<()> {
            self.events.lock().unwrap().extend_from_slice(events);
            Ok(())
        }
        async fn query_events(&self, _options: &EventQueryOptions) -> ApiResult<Vec<BaseEvent>> {
            Ok(self.events.lock().unwrap().clone())
        }
        async fn count_events(&self, _options: &EventQueryOptions) -> ApiResult<usize> {
            Ok(self.events.lock().unwrap().len())
        }
        async fn clear_events(&self) -> ApiResult<()> {
            self.events.lock().unwrap().clear();
            Ok(())
        }
        async fn save_snapshot(&self, key: &str, value: &Value) -> ApiResult<()> {
            self.snapshots.lock().unwrap().push((key.into(), value.clone()));
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
        async fn save_metric(&self, key: &str, value: &Value) -> ApiResult<()> {
            self.metrics.lock().unwrap().push((key.into(), value.clone()));
            Ok(())
        }
        async fn query_metrics(&self, _prefix: &str) -> ApiResult<Vec<(String, Value)>> {
            Ok(Vec::new())
        }
        fn health(&self) -> PersistenceHealth {
            PersistenceHealth {
                healthy: true,
                storage: "recording".into(),
                pending_writes: 0,
                message: None,
            }
        }
    }

    fn make_event() -> BaseEvent {
        BaseEvent {
            id: "evt".into(),
            r#type: wf_types::events::EventType::NodeStarted,
            timestamp: 1,
            workflow_id: Some("wf".into()),
            execution_id: Some("exec".into()),
            agent_loop_id: None,
            metadata: None,
        }
    }

    fn buffered_with(limit: usize) -> (Arc<RecordingLayer>, Arc<BufferedPersistenceLayer>) {
        let inner = Arc::new(RecordingLayer::default());
        let buffered = Arc::new(
            BufferedPersistenceLayer::new(inner.clone())
                .with_event_buffer_size(limit)
                .with_flush_interval(Duration::from_millis(10_000)),
        );
        (inner, buffered)
    }

    #[tokio::test]
    async fn shutdown_flushes_everything() {
        let (inner, buffered) = buffered_with(1024);
        buffered.initialize().await.unwrap();
        for _ in 0..5 {
            buffered.save_event(&make_event()).await.unwrap();
        }
        buffered
            .save_snapshot("s1", &serde_json::json!({"x": 1}))
            .await
            .unwrap();
        buffered
            .save_metric("m1", &serde_json::json!({"n": 1}))
            .await
            .unwrap();
        buffered.shutdown().await.unwrap();
        assert_eq!(inner.events.lock().unwrap().len(), 5);
        assert_eq!(inner.snapshots.lock().unwrap().len(), 1);
        assert_eq!(inner.metrics.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn water_level_triggers_immediate_flush() {
        let (inner, buffered) = buffered_with(3);
        buffered.initialize().await.unwrap();
        buffered.save_event(&make_event()).await.unwrap();
        buffered.save_event(&make_event()).await.unwrap();
        buffered.save_event(&make_event()).await.unwrap();
        // Water level reached: the flusher flushes without waiting for the
        // interval or shutdown.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while inner.events.lock().unwrap().len() < 3 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(inner.events.lock().unwrap().len(), 3);
        buffered.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn pending_writes_tracks_backlog() {
        let (inner, buffered) = buffered_with(1024);
        buffered.initialize().await.unwrap();
        for _ in 0..4 {
            buffered.save_event(&make_event()).await.unwrap();
        }
        // Below the water level and well inside the flush interval: every
        // write is counted until a flush actually happens.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while buffered.pending_writes() != 4 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(buffered.pending_writes(), 4);
        buffered.shutdown().await.unwrap();
        assert_eq!(buffered.pending_writes(), 0);
        assert_eq!(inner.events.lock().unwrap().len(), 4);
    }
}
