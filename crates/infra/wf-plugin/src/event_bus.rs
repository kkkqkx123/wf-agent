use tokio::sync::broadcast;

use crate::error::{PluginError, PluginResult};
use crate::events::PluginEvent;

const DEFAULT_CHANNEL_CAPACITY: usize = 256;

pub struct PluginEventBus {
    sender: broadcast::Sender<PluginEvent>,
}

pub struct PluginEventSubscription {
    receiver: broadcast::Receiver<PluginEvent>,
}

impl PluginEventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe(&self) -> PluginEventSubscription {
        PluginEventSubscription {
            receiver: self.sender.subscribe(),
        }
    }

    pub fn publish(&self, event: PluginEvent) -> PluginResult<usize> {
        self.sender
            .send(event)
            .map_err(|e| PluginError::Internal(format!("event bus send failed: {}", e)))
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for PluginEventBus {
    fn default() -> Self {
        Self::new(DEFAULT_CHANNEL_CAPACITY)
    }
}

impl PluginEventSubscription {
    pub async fn recv(&mut self) -> PluginResult<PluginEvent> {
        self.receiver
            .recv()
            .await
            .map_err(|e| PluginError::Internal(format!("event bus recv failed: {}", e)))
    }

    pub fn try_recv(&mut self) -> PluginResult<PluginEvent> {
        self.receiver
            .try_recv()
            .map_err(|e| PluginError::Internal(format!("event bus try_recv failed: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_reaches_subscribed_receivers() {
        let bus = PluginEventBus::new(8);
        let mut sub_a = bus.subscribe();
        let mut sub_b = bus.subscribe();
        assert_eq!(bus.receiver_count(), 2);

        let n = bus
            .publish(PluginEvent::Activated {
                plugin_id: "p1".into(),
            })
            .unwrap();
        // Only pre-existing receivers get the event.
        assert_eq!(n, 2);
        assert!(matches!(
            sub_a.recv().await.unwrap(),
            PluginEvent::Activated { .. }
        ));
        assert!(matches!(
            sub_b.try_recv().unwrap(),
            PluginEvent::Activated { .. }
        ));
    }

    #[tokio::test]
    async fn subscriber_added_after_publish_does_not_receive() {
        let bus = PluginEventBus::new(8);
        // An active receiver keeps the channel open during publish.
        let _keepalive = bus.subscribe();
        bus.publish(PluginEvent::Discovered {
            plugin_id: "p1".into(),
        })
        .unwrap();
        let mut late = bus.subscribe();
        assert!(late.try_recv().is_err());
    }

    #[tokio::test]
    async fn lagged_receiver_reports_error() {
        let bus = PluginEventBus::new(1);
        let mut sub = bus.subscribe();
        // Capacity 1: publishing twice overflows the lagged receiver.
        bus.publish(PluginEvent::Discovered {
            plugin_id: "a".into(),
        })
        .unwrap();
        bus.publish(PluginEvent::Discovered {
            plugin_id: "b".into(),
        })
        .unwrap();
        assert!(sub.recv().await.is_err());
    }

    #[test]
    fn event_type_and_payload_mapping() {
        let activated = PluginEvent::Activated {
            plugin_id: "p1".into(),
        };
        assert_eq!(activated.event_type(), crate::events::PLUGIN_ACTIVATED);
        assert_eq!(activated.payload()["plugin_id"], "p1");

        let loaded = PluginEvent::Loaded {
            plugin_id: "p1".into(),
            version: "1.2.3".into(),
        };
        assert_eq!(loaded.payload()["version"], "1.2.3");

        let err = PluginEvent::Error {
            plugin_id: "p1".into(),
            error: "boom".into(),
        };
        assert_eq!(err.event_type(), crate::events::PLUGIN_ERROR);
        assert_eq!(err.payload()["error"], "boom");
    }
}
