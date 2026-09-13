//! Event-bus plumbing for the trigger listener: which event types are worth
//! subscribing to, and the fan-in that merges those subscriptions into the
//! single stream the listener loop reads.
//!
//! Kept separate from the listener so the subscription policy (typed channels
//! per event type, general-channel fallback, lag reporting) lives in one
//! place.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{debug, warn};
use wf_core::error::EventError;
use wf_core::event::{Subscription, TypedSubscription};
use wf_core::EventBus;
use wf_types::events::{BaseEvent, EventCategory, EventType};
use wf_types::trigger::TriggerTemplate;

/// Event types that at least one registered template can match, deduplicated
/// in registration order. A template whose `event_type` does not parse into a
/// known [`EventType`] forces the general-channel fallback (empty result) so
/// a template registered around load-time validation (which rejects unknown
/// types) keeps its previous delivery behavior instead of silently missing.
/// There is no wildcard subscription: an empty result always means a bypassed
/// validation, never an explicit opt-in.
pub(crate) fn subscribed_types(templates: &[TriggerTemplate]) -> Vec<EventType> {
    let mut seen = HashSet::new();
    let mut types = Vec::new();
    for template in templates {
        let Some(condition) = &template.condition else {
            continue;
        };
        if !seen.insert(condition.event_type.clone()) {
            continue;
        }
        match condition.event_type.parse::<EventType>() {
            Ok(event_type) => types.push(event_type),
            Err(e) => {
                warn!(
                    "Trigger template event_type '{}' is not a known event type: {}; falling back to the general event channel",
                    condition.event_type, e
                );
                return Vec::new();
            }
        }
    }
    types
}

/// One event subscription that the fan-in can drain, shared by the general
/// and the typed channel.
#[async_trait]
trait EventStream: Send {
    async fn next_event(&mut self) -> Result<BaseEvent, EventError>;
}

#[async_trait]
impl EventStream for Subscription {
    async fn next_event(&mut self) -> Result<BaseEvent, EventError> {
        self.recv().await
    }
}

#[async_trait]
impl EventStream for TypedSubscription {
    async fn next_event(&mut self) -> Result<BaseEvent, EventError> {
        self.recv().await
    }
}

/// One stream merging every event channel the listener cares about.
///
/// A forwarder task per subscription copies events into the shared channel,
/// so the listener loop reads a single source regardless of how many event
/// types are involved. Dropping the fan-in closes the channel and ends the
/// forwarders.
pub(crate) struct EventFanIn {
    /// Held so the forwarders always have a live peer; the fan-in is closed
    /// by dropping the whole value.
    _sender: mpsc::UnboundedSender<BaseEvent>,
    receiver: mpsc::UnboundedReceiver<BaseEvent>,
}

impl EventFanIn {
    /// Subscribe `bus` to `interested_types` (typed channels) or to the
    /// general channel when the list is empty, and spawn the forwarders.
    pub(crate) fn new(bus: &Arc<EventBus>, interested_types: &[EventType]) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel::<BaseEvent>();
        if interested_types.is_empty() {
            spawn_forwarder(bus.subscribe(), sender.clone(), None);
        } else {
            for event_type in interested_types {
                spawn_forwarder(
                    bus.subscribe_typed(event_type.clone()),
                    sender.clone(),
                    Some(event_type.clone()),
                );
            }
        }
        Self {
            _sender: sender,
            receiver,
        }
    }

    /// Next event, or `None` once the fan-in has been closed.
    pub(crate) async fn recv(&mut self) -> Option<BaseEvent> {
        self.receiver.recv().await
    }
}

/// Copy one subscription into the fan-in channel until it ends. `event_type`
/// labels the lag report: observable types only lose notifications, so a
/// debug line is enough, while a request/mutated type dropping events is
/// worth a warning.
fn spawn_forwarder<S: EventStream + 'static>(
    mut subscription: S,
    sender: mpsc::UnboundedSender<BaseEvent>,
    event_type: Option<EventType>,
) {
    tokio::spawn(async move {
        loop {
            match subscription.next_event().await {
                Ok(event) => {
                    if sender.send(event).is_err() {
                        break;
                    }
                }
                Err(EventError::Lagged(_)) => {
                    let label = event_type.as_ref().map(|t| t.as_str()).unwrap_or("general");
                    match event_type.as_ref().map(EventType::category) {
                        Some(EventCategory::Observable) => {
                            debug!(
                                "TriggerEventListener lagged behind observable event type {}",
                                label
                            );
                        }
                        _ => {
                            warn!(
                                "TriggerEventListener lagged behind request/mutated event type {}",
                                label
                            );
                        }
                    }
                    continue;
                }
                Err(_) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::trigger::test_fixtures::{base_event, event_template};

    #[test]
    fn subscribed_types_are_deduplicated_in_registration_order() {
        let templates = vec![
            event_template("a", "NODE_COMPLETED", 0),
            event_template("b", "NODE_COMPLETED", 0),
            event_template("c", "NODE_FAILED", 0),
        ];
        let types = subscribed_types(&templates);
        assert_eq!(
            types.iter().map(|t| t.as_str()).collect::<Vec<_>>(),
            vec!["NODE_COMPLETED", "NODE_FAILED"]
        );
    }

    #[test]
    fn unparseable_event_type_falls_back_to_the_general_channel() {
        let mut odd = event_template("odd", "NODE_COMPLETED", 0);
        odd.condition.as_mut().expect("condition").event_type = "NOT_A_TYPE".to_string();
        let templates = vec![event_template("a", "NODE_COMPLETED", 0), odd];
        assert!(subscribed_types(&templates).is_empty());
    }

    #[tokio::test]
    async fn typed_fan_in_delivers_only_subscribed_types() {
        let bus = Arc::new(EventBus::new(64));
        // Subscriptions are created inside `new`, so the forwarders already
        // see anything published afterwards.
        let mut fan_in = EventFanIn::new(&bus, &[EventType::NodeCompleted]);
        bus.publish(base_event(EventType::NodeFailed, "e1"))
            .unwrap();
        bus.publish(base_event(EventType::NodeCompleted, "e1"))
            .unwrap();
        let event = fan_in.recv().await.expect("typed channel event");
        assert_eq!(event.r#type.as_str(), "NODE_COMPLETED");
    }

    #[tokio::test]
    async fn general_fan_in_delivers_every_type() {
        let bus = Arc::new(EventBus::new(64));
        let mut fan_in = EventFanIn::new(&bus, &[]);
        bus.publish(base_event(EventType::NodeCompleted, "e1"))
            .unwrap();
        bus.publish(base_event(EventType::NodeFailed, "e1"))
            .unwrap();
        let mut seen = Vec::new();
        for _ in 0..2 {
            let event = fan_in.recv().await.expect("general channel event");
            seen.push(event.r#type.as_str().to_string());
        }
        seen.sort();
        assert_eq!(seen, vec!["NODE_COMPLETED", "NODE_FAILED"]);
    }
}
