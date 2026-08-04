//! Execution event bus: pub/sub for execution state changes and events,
//! aligned with the TS `ExecutionEventBus`
//! (packages/sdk/shared/events/execution-event-bus.ts).
//!
//! Subscribers are registered per event type or catch-all (`*`). Handler
//! panics and errors are caught and routed to registered error handlers so a
//! faulty subscriber never breaks the execution flow.

use std::sync::Arc;
use wf_types::execution::{ExecutionEvent, ExecutionEventType};

type Handler = Arc<dyn Fn(&ExecutionEvent) + Send + Sync>;
type AnyHandler = Arc<dyn Fn(ExecutionEventType, &ExecutionEvent) + Send + Sync>;
type ErrorHandler = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone, Default)]
pub struct ExecutionEventBus {
    handlers: Arc<dashmap::DashMap<ExecutionEventType, Vec<Handler>>>,
    wildcard: Arc<std::sync::RwLock<Vec<AnyHandler>>>,
    error_handlers: Arc<std::sync::RwLock<Vec<ErrorHandler>>>,
}

impl ExecutionEventBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe to events of a specific type. Returns an unsubscribe
    /// function, mirroring the TS `on(type, handler)` return value.
    pub fn on(
        &self,
        event_type: ExecutionEventType,
        handler: impl Fn(&ExecutionEvent) + Send + Sync + 'static,
    ) -> impl FnOnce() + 'static {
        let handler: Handler = Arc::new(handler);
        self.handlers
            .entry(event_type)
            .or_default()
            .push(handler.clone());
        let handlers = self.handlers.clone();
        move || {
            if let Some(mut list) = handlers.get_mut(&event_type) {
                list.retain(|h| !Arc::ptr_eq(h, &handler));
            }
        }
    }

    /// Subscribe to all events (catch-all handler).
    pub fn on_any(
        &self,
        handler: impl Fn(ExecutionEventType, &ExecutionEvent) + Send + Sync + 'static,
    ) -> impl FnOnce() + 'static {
        let handler: AnyHandler = Arc::new(handler);
        self.wildcard.write().unwrap().push(handler.clone());
        let wildcard = self.wildcard.clone();
        move || {
            wildcard
                .write()
                .unwrap()
                .retain(|h| !Arc::ptr_eq(h, &handler));
        }
    }

    /// Subscribe to handler errors (a subscriber panicked or an error handler
    /// raised an error).
    pub fn on_error(
        &self,
        handler: impl Fn(&str) + Send + Sync + 'static,
    ) -> impl FnOnce() + 'static {
        let handler: ErrorHandler = Arc::new(handler);
        self.error_handlers.write().unwrap().push(handler.clone());
        let error_handlers = self.error_handlers.clone();
        move || {
            error_handlers
                .write()
                .unwrap()
                .retain(|h| !Arc::ptr_eq(h, &handler));
        }
    }

    /// Publish an event to all subscribed handlers (type-specific first,
    /// then catch-all), aligned with TS `publish`.
    pub fn publish(&self, event: &ExecutionEvent) {
        let event_type = event.event_type();
        let type_handlers: Vec<Handler> = self
            .handlers
            .get(&event_type)
            .map(|list| list.clone())
            .unwrap_or_default();
        let all_handlers = self.wildcard.read().unwrap().clone();

        for handler in &type_handlers {
            self.invoke(|| handler(event));
        }
        for handler in &all_handlers {
            self.invoke(|| handler(event_type, event));
        }
    }

    /// Publish a state-changed event (convenience constructor).
    pub fn publish_state_changed(
        &self,
        execution_id: &str,
        previous_status: Option<&str>,
        new_status: &str,
        changes: Option<serde_json::Map<String, serde_json::Value>>,
    ) {
        self.publish(&ExecutionEvent::StateChanged(
            wf_types::execution::ExecutionStateChangedEvent {
                execution_id: execution_id.to_string(),
                timestamp: chrono::Utc::now().timestamp_millis(),
                previous_status: previous_status.map(String::from),
                new_status: new_status.to_string(),
                changes,
            },
        ));
    }

    /// Total number of registered handlers (all types + wildcard).
    pub fn handler_count(&self) -> usize {
        let typed: usize = self.handlers.iter().map(|entry| entry.value().len()).sum();
        typed + self.wildcard.read().unwrap().len()
    }

    /// Number of handlers for a specific event type (or wildcard count when
    /// `None` is passed with a wildcard).
    pub fn handler_count_for(&self, event_type: ExecutionEventType) -> usize {
        self.handlers
            .get(&event_type)
            .map(|list| list.len())
            .unwrap_or(0)
    }

    /// Remove all subscribers (testing / reset).
    pub fn clear(&self) {
        self.handlers.clear();
        self.wildcard.write().unwrap().clear();
        self.error_handlers.write().unwrap().clear();
    }

    fn invoke(&self, f: impl FnOnce()) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        if let Err(panic) = result {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic in event handler".to_string());
            self.handle_error(&message);
        }
    }

    fn handle_error(&self, message: &str) {
        tracing::warn!(error = %message, "execution event handler error");
        let handlers = self.error_handlers.read().unwrap().clone();
        for handler in &handlers {
            handler(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wf_types::execution::{
        ExecutionEventType, ExecutionStateChangedEvent, IterationCompletedEvent,
    };

    #[test]
    fn typed_handler_receives_matching_events_only() {
        let bus = ExecutionEventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count2 = count.clone();
        let _unsub = bus.on(ExecutionEventType::StateChanged, move |_| {
            count2.fetch_add(1, Ordering::SeqCst);
        });

        bus.publish(&ExecutionEvent::StateChanged(ExecutionStateChangedEvent {
            execution_id: "exec-1".to_string(),
            timestamp: 0,
            previous_status: None,
            new_status: "running".to_string(),
            changes: None,
        }));
        bus.publish(&ExecutionEvent::IterationCompleted(
            IterationCompletedEvent {
                execution_id: "exec-1".to_string(),
                timestamp: 0,
                iteration: 1,
                result: None,
            },
        ));

        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(bus.handler_count(), 1);
        assert_eq!(bus.handler_count_for(ExecutionEventType::StateChanged), 1);
    }

    #[test]
    fn wildcard_handler_receives_all_events_with_type() {
        let bus = ExecutionEventBus::new();
        let types: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let types2 = types.clone();
        let _unsub_any = bus.on_any(move |t, _| {
            types2.lock().unwrap().push(format!("{:?}", t));
        });

        bus.publish(&ExecutionEvent::StateChanged(ExecutionStateChangedEvent {
            execution_id: "e".to_string(),
            timestamp: 0,
            previous_status: None,
            new_status: "running".to_string(),
            changes: None,
        }));
        bus.publish(&ExecutionEvent::IterationCompleted(
            IterationCompletedEvent {
                execution_id: "e".to_string(),
                timestamp: 0,
                iteration: 2,
                result: None,
            },
        ));

        let got = types.lock().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], "StateChanged");
        assert_eq!(got[1], "IterationCompleted");
    }

    #[test]
    fn unsubscribe_removes_handler() {
        let bus = ExecutionEventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count2 = count.clone();
        let unsub = bus.on(ExecutionEventType::StateChanged, move |_| {
            count2.fetch_add(1, Ordering::SeqCst);
        });

        bus.publish_state_changed("e", None, "running", None);
        unsub();
        bus.publish_state_changed("e", None, "completed", None);

        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn panicking_handler_does_not_break_bus() {
        let bus = ExecutionEventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count2 = count.clone();
        let _unsub = bus.on(ExecutionEventType::StateChanged, move |_| {
            count2.fetch_add(1, Ordering::SeqCst);
            panic!("boom");
        });

        bus.publish_state_changed("e", None, "running", None);
        bus.publish_state_changed("e", None, "completed", None);

        assert_eq!(
            count.load(Ordering::SeqCst),
            2,
            "second publish still works"
        );
    }

    #[test]
    fn clear_removes_all_handlers() {
        let bus = ExecutionEventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count2 = count.clone();
        let _unsub = bus.on(ExecutionEventType::StateChanged, move |_| {
            count2.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(bus.handler_count(), 1);

        bus.clear();
        assert_eq!(bus.handler_count(), 0);
        bus.publish_state_changed("e", None, "running", None);
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
}
