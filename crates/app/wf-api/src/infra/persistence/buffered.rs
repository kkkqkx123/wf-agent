use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::core::{PersistenceHealth, PersistenceLayer};
use crate::infra::error::ApiResult;
use crate::infra::events::EventQueryOptions;
use wf_types::events::BaseEvent;

/// Defaults for the buffered wrapper.
const DEFAULT_EVENT_BUFFER_SIZE: usize = 256;
const DEFAULT_SNAPSHOT_BUFFER_SIZE: usize = 64;
const DEFAULT_METRIC_BUFFER_SIZE: usize = 64;
const DEFAULT_FLUSH_INTERVAL_MS: u64 = 5000;
/// Default capacity of the bounded write channel: headroom above the three
/// water levels (256 + 64 + 64) so a slow backend rarely drops writes.
const DEFAULT_QUEUE_CAPACITY: usize = 1024;

/// Write request enqueued on the buffered layer's channel. The channel gives
/// natural batching (a single flusher drains many writes per wake-up); data
/// ops are best-effort (dropped when the bounded queue is full, counted on
/// `dropped`), while `Flush`/`Shutdown` are control ops that must always
/// arrive.
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

/// Buffered persistence layer: writes land on a bounded `mpsc` channel
/// (capacity [`DEFAULT_QUEUE_CAPACITY`], configurable) and are drained by a
/// single flusher task that batches them into the inner backend when a buffer
/// fills or on a time interval.
/// Queries hit the inner backend; `pending_writes` reports the un-persisted
/// backlog (queued + in-flight).
pub struct BufferedPersistenceLayer {
    inner: Arc<dyn PersistenceLayer>,
    /// Write entry point. `None` until `initialize` spawns the flusher.
    tx: Mutex<Option<mpsc::Sender<WriteOp>>>,
    event_buffer_size: usize,
    snapshot_buffer_size: usize,
    metric_buffer_size: usize,
    queue_capacity: usize,
    flush_interval: Duration,
    flush_handle: Mutex<Option<JoinHandle<()>>>,
    initialized: Mutex<bool>,
    /// Records enqueued but not yet persisted (flusher-local backlog).
    pending: Arc<AtomicUsize>,
    /// Records sitting in the channel awaiting the flusher.
    queued: Arc<AtomicUsize>,
    /// Data writes dropped because the bounded queue was full.
    dropped: Arc<std::sync::atomic::AtomicU64>,
}

impl BufferedPersistenceLayer {
    pub fn new(inner: Arc<dyn PersistenceLayer>) -> Self {
        Self {
            inner,
            tx: Mutex::new(None),
            event_buffer_size: DEFAULT_EVENT_BUFFER_SIZE,
            snapshot_buffer_size: DEFAULT_SNAPSHOT_BUFFER_SIZE,
            metric_buffer_size: DEFAULT_METRIC_BUFFER_SIZE,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            flush_interval: Duration::from_millis(DEFAULT_FLUSH_INTERVAL_MS),
            flush_handle: Mutex::new(None),
            initialized: Mutex::new(false),
            pending: Arc::new(AtomicUsize::new(0)),
            queued: Arc::new(AtomicUsize::new(0)),
            dropped: Arc::new(std::sync::atomic::AtomicU64::new(0)),
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

    /// Capacity of the bounded write channel (default
    /// [`DEFAULT_QUEUE_CAPACITY`]). Data writes beyond the capacity are
    /// dropped (newest first) and counted on the `health()` report; control
    /// ops (`Flush`/`Shutdown`) always wait for capacity.
    pub fn with_queue_capacity(mut self, capacity: usize) -> Self {
        self.queue_capacity = capacity;
        self
    }

    /// Enqueue a data write, best-effort: dropped (and counted) when the
    /// bounded queue is full, keeping producers non-blocking.
    fn try_enqueue(&self, op: WriteOp) {
        let guard = wf_common::lock::lock_ok(self.tx.lock());
        let Some(tx) = guard.as_ref() else {
            return;
        };
        match tx.try_send(op) {
            Ok(()) => {
                self.queued.fetch_add(1, Ordering::Relaxed);
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {}
        }
    }

    /// Enqueue a flush and wait for the in-flight backlog to drain so a
    /// following `clear_*` sees a clean backend. Bounded by `flush_interval`;
    /// on a persistent backend failure the batch stays buffered and is
    /// retried on the next tick. `Flush` is a control op and always waits
    /// for capacity.
    async fn flush_and_wait(&self) {
        let maybe_tx = wf_common::lock::lock_ok(self.tx.lock()).clone();
        if let Some(tx) = maybe_tx.as_ref() {
            let _ = tx.send(WriteOp::Flush).await;
        }
        let deadline = tokio::time::Instant::now() + self.flush_interval;
        while self.pending_writes() != 0 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    fn spawn_flusher(&self, mut rx: mpsc::Receiver<WriteOp>) -> JoinHandle<()> {
        let inner = self.inner.clone();
        let event_limit = self.event_buffer_size;
        let snapshot_limit = self.snapshot_buffer_size;
        let metric_limit = self.metric_buffer_size;
        let flush_interval = self.flush_interval;
        let pending = self.pending.clone();
        let queued = self.queued.clone();
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
                            queued.fetch_sub(1, Ordering::Relaxed);
                            pending.fetch_add(1, Ordering::Relaxed);
                        }
                        Some(WriteOp::Snapshot(key, value)) => {
                            snapshots.push((key, value));
                            queued.fetch_sub(1, Ordering::Relaxed);
                            pending.fetch_add(1, Ordering::Relaxed);
                        }
                        Some(WriteOp::Metric(key, value)) => {
                            metrics.push((key, value));
                            queued.fetch_sub(1, Ordering::Relaxed);
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
                        just_failed = !flush_batch(
                            &*inner,
                            &mut events,
                            &mut snapshots,
                            &mut metrics,
                            &pending,
                        )
                        .await;
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
                    just_failed =
                        !flush_batch(&*inner, &mut events, &mut snapshots, &mut metrics, &pending)
                            .await;
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

#[async_trait::async_trait]
impl PersistenceLayer for BufferedPersistenceLayer {
    fn name(&self) -> &str {
        "buffered"
    }

    async fn flush(&self) -> ApiResult<()> {
        self.flush_and_wait().await;
        Ok(())
    }

    async fn initialize(&self) -> ApiResult<()> {
        if *wf_common::lock::lock_ok(self.initialized.lock()) {
            return Ok(());
        }
        self.inner.initialize().await?;
        let (tx, rx) = mpsc::channel(self.queue_capacity);
        let handle = self.spawn_flusher(rx);
        *wf_common::lock::lock_ok(self.tx.lock()) = Some(tx);
        *wf_common::lock::lock_ok(self.flush_handle.lock()) = Some(handle);
        *wf_common::lock::lock_ok(self.initialized.lock()) = true;
        Ok(())
    }

    async fn shutdown(&self) -> ApiResult<()> {
        if !*wf_common::lock::lock_ok(self.initialized.lock()) {
            return Ok(());
        }
        // Two-phase shutdown: flush in-flight data first, then ask the flusher
        // to flush once more and exit. The flusher never blocks shutdown.
        // Both are control ops and wait for capacity so they always arrive.
        let maybe_tx = wf_common::lock::lock_ok(self.tx.lock()).clone();
        if let Some(tx) = maybe_tx.as_ref() {
            let _ = tx.send(WriteOp::Flush).await;
            let _ = tx.send(WriteOp::Shutdown).await;
        }
        let handle = wf_common::lock::lock_ok(self.flush_handle.lock()).take();
        if let Some(handle) = handle {
            let _ = tokio::time::timeout(self.flush_interval, handle).await;
        }
        self.inner.shutdown().await?;
        *wf_common::lock::lock_ok(self.initialized.lock()) = false;
        Ok(())
    }

    fn pending_writes(&self) -> usize {
        self.pending.load(Ordering::Relaxed) + self.queued.load(Ordering::Relaxed)
    }

    async fn save_event(&self, event: &BaseEvent) -> ApiResult<()> {
        self.try_enqueue(WriteOp::Event(event.clone()));
        Ok(())
    }

    async fn save_events(&self, events: &[BaseEvent]) -> ApiResult<()> {
        for event in events {
            self.try_enqueue(WriteOp::Event(event.clone()));
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
        self.try_enqueue(WriteOp::Snapshot(key.to_string(), snapshot.clone()));
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
        self.try_enqueue(WriteOp::Metric(key.to_string(), value.clone()));
        Ok(())
    }

    async fn query_metrics(&self, key_prefix: &str) -> ApiResult<Vec<(String, Value)>> {
        self.inner.query_metrics(key_prefix).await
    }

    fn health(&self) -> PersistenceHealth {
        let dropped = self.dropped.load(std::sync::atomic::Ordering::Relaxed);
        PersistenceHealth {
            healthy: true,
            storage: self.inner.name().to_string(),
            pending_writes: self.pending_writes(),
            message: (dropped > 0).then(|| format!("{dropped} writes dropped (queue full)")),
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

    #[async_trait::async_trait]
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
            self.snapshots
                .lock()
                .unwrap()
                .push((key.into(), value.clone()));
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
            self.metrics
                .lock()
                .unwrap()
                .push((key.into(), value.clone()));
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

            event_name: None,
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

    #[tokio::test]
    async fn bounded_queue_drops_newest_writes_and_counts_them() {
        let inner = Arc::new(RecordingLayer::default());
        let buffered = BufferedPersistenceLayer::new(inner.clone()).with_queue_capacity(2);
        // No flusher: the queue fills and stays full, so overflow must be
        // dropped (newest first) and counted, never blocking the producer.
        let (tx, _rx) = mpsc::channel(2);
        *buffered.tx.lock().unwrap() = Some(tx);

        buffered.save_event(&make_event()).await.unwrap();
        buffered.save_event(&make_event()).await.unwrap();
        buffered.save_event(&make_event()).await.unwrap();

        assert_eq!(buffered.queued.load(Ordering::Relaxed), 2);
        assert_eq!(buffered.dropped.load(Ordering::Relaxed), 1);
        assert_eq!(buffered.pending_writes(), 2);
        let health = buffered.health();
        assert!(
            health
                .message
                .as_deref()
                .is_some_and(|m| m.contains("dropped")),
            "health must surface dropped writes: {:?}",
            health
        );
    }

    #[tokio::test]
    async fn queued_writes_are_counted_before_dequeue() {
        // `pending_writes` must include writes still in the channel, so
        // `flush_and_wait` cannot return while the channel is backed up.
        let inner = Arc::new(RecordingLayer::default());
        let buffered = BufferedPersistenceLayer::new(inner.clone()).with_queue_capacity(4);
        let (tx, _rx) = mpsc::channel(4);
        *buffered.tx.lock().unwrap() = Some(tx);

        buffered.save_event(&make_event()).await.unwrap();
        buffered.save_event(&make_event()).await.unwrap();
        assert_eq!(buffered.pending_writes(), 2);
    }
}
