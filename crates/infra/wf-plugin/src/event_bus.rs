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
