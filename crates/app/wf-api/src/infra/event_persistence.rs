//! Event persistence bridge: subscribes to the shared `EventBus` and
//! persists every engine event through the persistence layer.
//!
//! Engine-side publishers (workflow / agent / node / fork / shell /
//! interruption) all publish to the shared bus directly and never touch
//! `wf-api`'s `PersistenceLayer` (which uses `ApiResult`, a `wf-api` type).
//! This watcher mirrors [`wf_core::event_bridge::EventMetricsBridge`]: one
//! subscriber picks up every bus event and forwards it to the buffered
//! persistence layer, so the ~120 engine event types reach durable storage
//! with zero changes to the emitting crates.

use std::sync::Arc;

use wf_core::error::EventError;
use wf_core::event::EventBus;

use crate::infra::persistence::PersistenceLayer;

/// Watches the shared event bus and persists every event.
pub struct EventPersistenceBridge {
    persistence: Arc<dyn PersistenceLayer>,
}

impl EventPersistenceBridge {
    pub fn new(persistence: Arc<dyn PersistenceLayer>) -> Self {
        Self { persistence }
    }

    /// Subscribe to the bus and persist events in the background.
    ///
    /// The buffered layer's `save_event` only enqueues (non-blocking), so
    /// this task keeps up with the bus under normal load. A `Lagged` result
    /// means the broadcast channel dropped events before this subscriber
    /// could receive them — surfaced as a warning, consistent with
    /// `EventMetricsBridge`. `ChannelClosed` (all senders gone) exits.
    pub fn spawn(self, bus: Arc<EventBus>) -> tokio::task::JoinHandle<()> {
        let mut subscription = bus.subscribe();
        tokio::spawn(async move {
            loop {
                match subscription.recv().await {
                    Ok(event) => {
                        if let Err(err) = self.persistence.save_event(&event).await {
                            tracing::warn!(
                                event_id = %event.id,
                                event_type = %event.r#type.as_str(),
                                error = %err,
                                "event persistence bridge failed to save event"
                            );
                        }
                    }
                    Err(EventError::Lagged(skipped)) => {
                        // The bus dropped events this subscriber could not
                        // keep up with; the persisted history is missing them.
                        tracing::warn!(
                            skipped = skipped,
                            "event persistence bridge lagged and skipped events"
                        );
                        continue;
                    }
                    Err(EventError::ChannelClosed) => break,
                    Err(_) => continue,
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::persistence::PersistenceLayer;
    use std::sync::Arc;
    use wf_types::events::{BaseEvent, EventType};

    fn event(execution_id: &str, event_type: EventType) -> BaseEvent {
        BaseEvent {
            id: wf_common::generate_id(),
            r#type: event_type,
            timestamp: wf_common::now(),
            workflow_id: None,
            execution_id: Some(execution_id.to_string()),
            agent_loop_id: None,

            event_name: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn persists_events_published_on_the_bus() {
        let persistence: Arc<dyn PersistenceLayer> =
            Arc::new(crate::infra::persistence::StorePersistenceLayer::memory());
        let bridge = EventPersistenceBridge::new(persistence.clone());
        let bus = Arc::new(EventBus::new(16));
        let _task = bridge.spawn(bus.clone());
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        bus.publish(event("exec-1", EventType::NodeStarted))
            .unwrap();
        bus.publish(event("exec-1", EventType::NodeCompleted))
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let stored = persistence
            .query_events(&crate::infra::events::EventQueryOptions::default())
            .await
            .unwrap();
        assert_eq!(stored.len(), 2);
        let types: Vec<_> = stored.iter().map(|e| &e.r#type).collect();
        assert!(types.contains(&&EventType::NodeStarted));
        assert!(types.contains(&&EventType::NodeCompleted));
    }

    #[tokio::test]
    async fn exits_when_the_bus_closes() {
        let persistence: Arc<dyn PersistenceLayer> =
            Arc::new(crate::infra::persistence::StorePersistenceLayer::memory());
        let bridge = EventPersistenceBridge::new(persistence.clone());
        let bus = Arc::new(EventBus::new(16));
        let task = bridge.spawn(bus.clone());
        drop(bus);
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), task).await;
        assert!(result.is_ok(), "bridge must exit when the bus is closed");
    }
}
