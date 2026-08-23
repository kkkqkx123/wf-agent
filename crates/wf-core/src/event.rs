use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use tokio::sync::broadcast;

use wf_types::events::{BaseEvent, EventType};

use crate::error::EventError;
use crate::registry::{
    BatchRegistry, ConcurrentRegistry, MutableRegistry, Registry, RegistryResult,
};

const DEFAULT_CAPACITY: usize = 1024;

/// Number of most recent events kept for search/observability. Bounded so
/// unbounded publish rates never grow memory.
const DEFAULT_RECENT_LIMIT: usize = 512;

/// Multi-channel event bus.
///
/// Supports both a general-purpose broadcast channel and per-event-type
/// typed channels, so subscribers that only care about a specific event
/// type avoid the overhead of filtering.
pub struct EventBus {
    /// General-purpose channel: every published event is forwarded here.
    sender: broadcast::Sender<BaseEvent>,
    /// Per-event-type channels: subscribers using `subscribe_typed` only
    /// receive events of that type, avoiding local filtering overhead.
    typed_channels: Mutex<HashMap<EventType, broadcast::Sender<BaseEvent>>>,
    /// Ring buffer of the most recent published events, serving the unified
    /// search's `event` type and lightweight observability queries.
    recent: Mutex<VecDeque<BaseEvent>>,
    recent_limit: usize,
    /// Default capacity for per-event-type channels.
    typed_capacity: usize,
}

pub struct EventBusBuilder {
    capacity: usize,
    recent_limit: usize,
    typed_capacity: usize,
}

pub struct Subscription {
    receiver: broadcast::Receiver<BaseEvent>,
}

/// A subscription scoped to a specific event type.
///
/// The caller can also subscribe to the general channel and filter locally;
/// this type exists so the `subscribe_typed` API is self-documenting.
pub struct TypedSubscription {
    receiver: broadcast::Receiver<BaseEvent>,
}

#[derive(Debug, Clone)]
pub struct StateTransition {
    pub from: String,
    pub to: String,
    pub timestamp: i64,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        Self {
            sender: broadcast::channel(capacity).0,
            typed_channels: Mutex::new(HashMap::new()),
            recent: Mutex::new(VecDeque::with_capacity(DEFAULT_RECENT_LIMIT)),
            recent_limit: DEFAULT_RECENT_LIMIT,
            typed_capacity: capacity,
        }
    }

    /// Build a bus with an explicit recent-event history size.
    pub fn with_recent_limit(capacity: usize, recent_limit: usize) -> Self {
        Self {
            sender: broadcast::channel(capacity).0,
            typed_channels: Mutex::new(HashMap::new()),
            recent: Mutex::new(VecDeque::with_capacity(recent_limit)),
            recent_limit,
            typed_capacity: capacity,
        }
    }

    /// Build a bus with explicit capacities for both the general channel
    /// and per-event-type channels.
    pub fn with_typed_capacity(capacity: usize, typed_capacity: usize) -> Self {
        Self {
            sender: broadcast::channel(capacity).0,
            typed_channels: Mutex::new(HashMap::new()),
            recent: Mutex::new(VecDeque::with_capacity(DEFAULT_RECENT_LIMIT)),
            recent_limit: DEFAULT_RECENT_LIMIT,
            typed_capacity,
        }
    }

    pub fn builder() -> EventBusBuilder {
        EventBusBuilder {
            capacity: DEFAULT_CAPACITY,
            recent_limit: DEFAULT_RECENT_LIMIT,
            typed_capacity: DEFAULT_CAPACITY,
        }
    }

    /// Subscribe to **all** events on the bus (general channel).
    pub fn subscribe(&self) -> Subscription {
        Subscription {
            receiver: self.sender.subscribe(),
        }
    }

    /// Subscribe to events of a specific type only.
    ///
    /// The first call for a given `EventType` lazily creates the typed
    /// channel. Events published to this channel are also published to the
    /// general channel, so subscribers using `subscribe()` still receive
    /// them.
    pub fn subscribe_typed(&self, event_type: EventType) -> TypedSubscription {
        let mut channels = wf_common::lock::lock_ok(self.typed_channels.lock());
        let sender = channels
            .entry(event_type)
            .or_insert_with(|| broadcast::channel(self.typed_capacity).0);
        TypedSubscription {
            receiver: sender.subscribe(),
        }
    }

    pub fn publish(&self, event: BaseEvent) -> Result<usize, EventError> {
        self.record_recent(event.clone());
        // Publish to the general channel.
        //
        // `send` returns `Err(SendError)` when there are **zero** receivers
        // on the channel.  This is a normal condition — callers may create
        // typed-only subscriptions (`subscribe_typed`) without subscribing
        // to the general channel.  We treat zero receivers as success with
        // a count of 0 rather than propagating an error.
        let n = self.sender.send(event.clone()).unwrap_or(0);
        // Also publish to the typed channel (if one exists for this type).
        let channels = wf_common::lock::lock_ok(self.typed_channels.lock());
        if let Some(tx) = channels.get(&event.r#type) {
            let _ = tx.send(event);
        }
        Ok(n)
    }

    /// Publish an event and log a warning on failure instead of silently
    /// dropping it. `context` is a caller-provided description (e.g.
    /// `"workflow={} node={}"`) used to locate the failed publish in logs.
    pub fn publish_logged(&self, event: BaseEvent, context: &str) -> Result<usize, EventError> {
        let result = self.publish(event.clone());
        if let Err(ref err) = result {
            tracing::warn!(
                event_type = ?event.r#type,
                execution_id = ?event.execution_id,
                context,
                error = ?err,
                "event publish failed"
            );
        }
        result
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Number of events sent but not yet received by any subscriber
    /// (backlog depth of the broadcast channel).
    pub fn queue_len(&self) -> usize {
        self.sender.len()
    }

    /// Maximum number of events retained in the recent-event history.
    pub fn recent_limit(&self) -> usize {
        self.recent_limit
    }

    /// Most recent published events, newest first, up to `recent_limit`.
    pub fn recent_events(&self) -> Vec<BaseEvent> {
        wf_common::lock::lock_ok(self.recent.lock())
            .iter()
            .rev()
            .cloned()
            .collect()
    }

    fn record_recent(&self, event: BaseEvent) {
        let mut recent = wf_common::lock::lock_ok(self.recent.lock());
        if recent.len() == self.recent_limit {
            recent.pop_front();
        }
        recent.push_back(event);
    }
}

impl EventBusBuilder {
    pub fn capacity(mut self, cap: usize) -> Self {
        self.capacity = cap;
        self
    }

    pub fn recent_limit(mut self, limit: usize) -> Self {
        self.recent_limit = limit;
        self
    }

    pub fn typed_capacity(mut self, cap: usize) -> Self {
        self.typed_capacity = cap;
        self
    }

    pub fn build(self) -> EventBus {
        EventBus::with_typed_capacity(self.capacity, self.typed_capacity)
    }
}

impl Subscription {
    pub async fn recv(&mut self) -> Result<BaseEvent, EventError> {
        Ok(self.receiver.recv().await?)
    }

    pub fn try_recv(&mut self) -> Result<BaseEvent, EventError> {
        Ok(self.receiver.try_recv()?)
    }
}

impl TypedSubscription {
    pub async fn recv(&mut self) -> Result<BaseEvent, EventError> {
        Ok(self.receiver.recv().await?)
    }

    pub fn try_recv(&mut self) -> Result<BaseEvent, EventError> {
        Ok(self.receiver.try_recv()?)
    }
}

// ── Registry Event Listener ──

/// Listens for registry state-change events.
///
/// Registered via [`ObservableRegistry::add_listener`] and called
/// synchronously on each register / unregister operation.
#[async_trait]
pub trait RegistryEventListener: Send + Sync {
    async fn on_registered(&self, key: &str);
    async fn on_unregistered(&self, key: &str);
}

/// A registry wrapper that notifies [`RegistryEventListener`]s on each
/// mutation.
///
/// Delegates all reads/writes to the inner [`ConcurrentRegistry`] and
/// implements [`Registry`], [`MutableRegistry`], and [`BatchRegistry`] so
/// it can be used as a drop-in replacement.
pub struct ObservableRegistry<T: Send + Sync> {
    inner: Arc<ConcurrentRegistry<T>>,
    listeners: Vec<Arc<dyn RegistryEventListener>>,
    notify_error_count: Arc<AtomicUsize>,
}

impl<T: Send + Sync> ObservableRegistry<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ConcurrentRegistry::new()),
            listeners: Vec::new(),
            notify_error_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(ConcurrentRegistry::with_capacity(capacity)),
            listeners: Vec::new(),
            notify_error_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Register a listener that will be notified on every mutation.
    pub fn add_listener(&mut self, listener: Arc<dyn RegistryEventListener>) {
        self.listeners.push(listener);
    }

    /// Number of listener notification errors since last reset.
    pub fn notify_error_count(&self) -> usize {
        self.notify_error_count.load(Ordering::Relaxed)
    }

    fn dispatch_registered(&self, key: &str) {
        let key = key.to_string();
        let listeners = self.listeners.clone();
        tokio::spawn(async move {
            for listener in &listeners {
                listener.on_registered(&key).await;
            }
        });
    }

    fn dispatch_unregistered(&self, key: &str) {
        let key = key.to_string();
        let listeners = self.listeners.clone();
        tokio::spawn(async move {
            for listener in &listeners {
                listener.on_unregistered(&key).await;
            }
        });
    }
}

impl<T: Send + Sync> Default for ObservableRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync> Registry<T> for ObservableRegistry<T> {
    fn get(&self, key: &str) -> Option<Arc<T>> {
        self.inner.get(key)
    }

    fn has(&self, key: &str) -> bool {
        self.inner.has(key)
    }

    fn list(&self) -> Vec<String> {
        self.inner.list()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<T: Send + Sync> MutableRegistry<T> for ObservableRegistry<T> {
    fn register(&self, key: String, item: Arc<T>) -> RegistryResult<()> {
        let result = self.inner.register(key.clone(), item);
        if result.is_ok() {
            self.dispatch_registered(&key);
        }
        result
    }

    fn register_or_replace(&self, key: String, item: Arc<T>) -> Option<Arc<T>> {
        let result = self.inner.register_or_replace(key.clone(), item);
        self.dispatch_registered(&key);
        result
    }

    fn unregister(&self, key: &str) -> Option<Arc<T>> {
        let result = self.inner.unregister(key);
        if result.is_some() {
            self.dispatch_unregistered(key);
        }
        result
    }

    fn clear(&self) {
        self.inner.clear();
    }
}

impl<T: Send + Sync> BatchRegistry<T> for ObservableRegistry<T> {
    fn register_batch(&self, items: Vec<(String, Arc<T>)>) -> RegistryResult<()> {
        self.inner.register_batch(items)
    }

    fn unregister_batch(&self, keys: &[String]) -> Vec<Option<Arc<T>>> {
        self.inner.unregister_batch(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::events::{BaseEvent, EventType};

    fn make_event(execution_id: Option<&str>, event_type: EventType) -> BaseEvent {
        BaseEvent {
            id: "test-id".to_string(),
            r#type: event_type,
            timestamp: 0,
            workflow_id: None,
            execution_id: execution_id.map(|s| s.to_string()),
            agent_loop_id: None,

            event_name: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn test_publish_and_receive() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();

        let event = make_event(None, EventType::Heartbeat);
        bus.publish(event.clone()).unwrap();

        let received = sub.recv().await.unwrap();
        assert_eq!(received.r#type, EventType::Heartbeat);
    }

    #[tokio::test]
    async fn test_subscribe_global_receives_all() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();

        bus.publish(make_event(None, EventType::Heartbeat)).unwrap();
        bus.publish(make_event(None, EventType::NodeStarted))
            .unwrap();

        let r1 = sub.recv().await.unwrap();
        let r2 = sub.recv().await.unwrap();
        assert_eq!(r1.r#type, EventType::Heartbeat);
        assert_eq!(r2.r#type, EventType::NodeStarted);
    }

    #[tokio::test]
    async fn test_typed_subscription_receives_only_one_type() {
        let bus = EventBus::new(16);
        let mut typed = bus.subscribe_typed(EventType::Heartbeat);

        bus.publish(make_event(None, EventType::Heartbeat)).unwrap();
        bus.publish(make_event(None, EventType::NodeStarted))
            .unwrap();
        bus.publish(make_event(None, EventType::Heartbeat)).unwrap();

        // Typed subscription should only receive Heartbeat events.
        let r1 = typed.recv().await.unwrap();
        assert_eq!(r1.r#type, EventType::Heartbeat);
        let r2 = typed.recv().await.unwrap();
        assert_eq!(r2.r#type, EventType::Heartbeat);
    }

    #[tokio::test]
    async fn test_typed_subscription_does_not_receive_other_types() {
        let bus = EventBus::new(16);
        let mut typed = bus.subscribe_typed(EventType::NodeStarted);

        bus.publish(make_event(None, EventType::Heartbeat)).unwrap();

        // The typed subscription should not receive Heartbeat; try_recv
        // should return Empty.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(typed.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_global_still_receives_all_with_typed_subscribers() {
        let bus = EventBus::new(16);
        let mut global = bus.subscribe();
        let _typed = bus.subscribe_typed(EventType::Heartbeat);

        bus.publish(make_event(None, EventType::Heartbeat)).unwrap();
        bus.publish(make_event(None, EventType::NodeStarted))
            .unwrap();

        let r1 = global.recv().await.unwrap();
        assert_eq!(r1.r#type, EventType::Heartbeat);
        let r2 = global.recv().await.unwrap();
        assert_eq!(r2.r#type, EventType::NodeStarted);
    }

    #[tokio::test]
    async fn test_builder() {
        let bus = EventBus::builder().capacity(64).build();
        assert_eq!(bus.receiver_count(), 0);
    }

    #[tokio::test]
    async fn test_try_recv_lagged() {
        let bus = EventBus::new(1);
        let mut sub = bus.subscribe();

        bus.publish(make_event(None, EventType::Heartbeat)).unwrap();
        bus.publish(make_event(None, EventType::NodeStarted))
            .unwrap();

        let result = sub.try_recv();
        assert!(matches!(result, Err(EventError::Lagged(1))));
    }

    #[tokio::test]
    async fn recent_events_are_retained_newest_first() {
        let bus = EventBus::new(16);
        let _sub = bus.subscribe();
        bus.publish(make_event(Some("exec-1"), EventType::Heartbeat))
            .unwrap();
        bus.publish(make_event(Some("exec-2"), EventType::NodeStarted))
            .unwrap();
        bus.publish(make_event(Some("exec-3"), EventType::NodeCompleted))
            .unwrap();

        let recent = bus.recent_events();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].execution_id.as_deref(), Some("exec-3"));
        assert_eq!(recent[2].execution_id.as_deref(), Some("exec-1"));
    }

    #[tokio::test]
    async fn recent_events_are_bounded() {
        let bus = EventBus::with_recent_limit(16, 3);
        let _sub = bus.subscribe();
        for i in 0..10 {
            bus.publish(make_event(Some(&format!("exec-{i}")), EventType::Heartbeat))
                .unwrap();
        }
        let recent = bus.recent_events();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].execution_id.as_deref(), Some("exec-9"));
        assert_eq!(recent[2].execution_id.as_deref(), Some("exec-7"));
    }
}
