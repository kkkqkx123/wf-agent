use super::PluginEngine;
use crate::event_bus::PluginEventSubscription;
use crate::events::PluginEvent;

impl PluginEngine {
    pub fn subscribe(&self) -> PluginEventSubscription {
        self.plugin_event_bus.subscribe()
    }

    pub(crate) fn publish(&self, event: PluginEvent) {
        let _ = self.plugin_event_bus.publish(event.clone());
        let base_event = plugin_event_to_base(&event);
        if let Some(ref bus) = self.event_bus {
            let _ = bus.publish(base_event);
        }
    }

    /// Spawn the per-plugin event dispatch task: subscribes the plugin event
    /// bus and forwards every event to the plugin's event-handler
    /// contributions. The handle is stored so `deactivate` can abort it —
    /// the subscription teardown is symmetric with activation.
    pub(crate) fn start_event_dispatch(&self, plugin_id: &str) {
        if self.contribution_manager.all_event_handlers().is_empty() {
            return;
        }
        let subscription = self.plugin_event_bus.subscribe();
        let manager = self.contribution_manager.clone();
        let owned_plugin_id = plugin_id.to_owned();
        let handle = tokio::spawn(async move {
            let mut subscription = subscription;
            let plugin_id = owned_plugin_id;
            loop {
                match subscription.recv().await {
                    Ok(event) => {
                        let data = crate::contributions::PluginEventData {
                            event_type: event.event_type().to_string(),
                            data: event.payload(),
                        };
                        for handler in manager.get_event_handlers(data.event_type.as_str()) {
                            if let Err(e) = handler.handle(data.clone()).await {
                                tracing::warn!(
                                    plugin_id = %plugin_id,
                                    event_type = %data.event_type,
                                    "plugin event handler failed: {}",
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        // `PluginEventSubscription::recv` maps Lagged/Closed
                        // onto `PluginError`; a closed bus (engine dropped)
                        // ends the dispatch loop, other errors just log.
                        tracing::warn!(
                            plugin_id = %plugin_id,
                            "plugin event dispatch stopped: {}",
                            e
                        );
                        break;
                    }
                }
            }
        });
        self.event_tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(plugin_id.to_owned(), handle);
    }

    /// Abort the per-plugin event dispatch task (if any).
    pub(crate) fn stop_event_dispatch(&self, plugin_id: &str) {
        if let Some(handle) = self
            .event_tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(plugin_id)
        {
            handle.abort();
        }
    }
}

pub(crate) fn plugin_event_to_base(event: &PluginEvent) -> wf_types::events::BaseEvent {
    use std::collections::HashMap;
    use wf_types::events::{BaseEvent, EventType};

    let (etype, meta): (EventType, Option<HashMap<String, serde_json::Value>>) = match event {
        PluginEvent::Discovered { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:discovered".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Loading { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:loading".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Loaded { plugin_id, version } => (
            EventType::Heartbeat,
            Some(HashMap::from([
                (
                    "plugin:loaded".into(),
                    serde_json::Value::String(plugin_id.clone()),
                ),
                ("version".into(), serde_json::Value::String(version.clone())),
            ])),
        ),
        PluginEvent::Activating { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:activating".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Activated { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:activated".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Deactivating { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:deactivating".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Deactivated { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:deactivated".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Error { plugin_id, error } => (
            EventType::Error,
            Some(HashMap::from([
                (
                    "plugin:error".into(),
                    serde_json::Value::String(plugin_id.clone()),
                ),
                ("error".into(), serde_json::Value::String(error.clone())),
            ])),
        ),
        PluginEvent::ConfigChanged { plugin_id, config } => (
            EventType::Heartbeat,
            Some(HashMap::from([
                (
                    "plugin:config-changed".into(),
                    serde_json::Value::String(plugin_id.clone()),
                ),
                ("config".into(), config.clone()),
            ])),
        ),
    };
    BaseEvent {
        id: uuid_or_fallback(),
        r#type: etype,
        timestamp: chrono::Utc::now().timestamp_millis(),
        workflow_id: None,
        execution_id: None,
        agent_loop_id: None,

        event_name: None,
        metadata: meta,
    }
}

pub(crate) fn uuid_or_fallback() -> String {
    format!(
        "plugin-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    )
}
