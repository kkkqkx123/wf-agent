//! EventBus bridge for shell session events.
//!
//! Implements [`ShellEventSink`] and forwards each shell lifecycle/output
//! event to the shared `wf_core::EventBus` as a `wf_types::events::BaseEvent`
//! (payload carried in `BaseEvent.metadata`). The bridge lives in
//! `wf-runtime` (an upper crate) so `wf-tools` keeps its strict dependency
//! DAG and never depends on `wf-core`.

use std::collections::HashMap;
use std::sync::Arc;

use wf_core::event::EventBus;
use wf_shell::event_sink::ShellEventSink;
use wf_types::events::{BaseEvent, EventType};

/// Sink that forwards shell session events to the shared EventBus.
///
/// Publishing is best-effort: a full or closed broadcast channel only logs a
/// warning and never blocks the caller.
pub struct ShellEventBusBridge {
    bus: Arc<EventBus>,
}

impl ShellEventBusBridge {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self { bus }
    }
}

impl ShellEventSink for ShellEventBusBridge {
    fn on_session_created(&self, session_id: &str, reused: bool, task_id: Option<&str>) {
        let mut metadata = shell_metadata(session_id, task_id);
        metadata.insert("reused".to_string(), serde_json::json!(reused));
        self.publish(EventType::ShellSessionCreated, task_id, metadata);
    }

    fn on_command_started(&self, session_id: &str, task_id: Option<&str>, command: &str) {
        let mut metadata = shell_metadata(session_id, task_id);
        metadata.insert("command".to_string(), serde_json::json!(command));
        self.publish(EventType::ShellCommandStarted, task_id, metadata);
    }

    fn on_output(&self, session_id: &str, task_id: Option<&str>, line: &str) {
        let mut metadata = shell_metadata(session_id, task_id);
        metadata.insert("line".to_string(), serde_json::json!(line));
        self.publish(EventType::ShellOutputReceived, task_id, metadata);
    }

    fn on_command_completed(
        &self,
        session_id: &str,
        task_id: Option<&str>,
        command: &str,
        exit_code: Option<i32>,
        success: bool,
    ) {
        let mut metadata = shell_metadata(session_id, task_id);
        metadata.insert("command".to_string(), serde_json::json!(command));
        metadata.insert("exit_code".to_string(), serde_json::json!(exit_code));
        metadata.insert("success".to_string(), serde_json::json!(success));
        self.publish(EventType::ShellCommandCompleted, task_id, metadata);
    }

    fn on_session_terminated(&self, session_id: &str, task_id: Option<&str>) {
        self.publish(
            EventType::ShellSessionTerminated,
            task_id,
            shell_metadata(session_id, task_id),
        );
    }
}

impl ShellEventBusBridge {
    fn publish(
        &self,
        event_type: EventType,
        task_id: Option<&str>,
        metadata: HashMap<String, serde_json::Value>,
    ) {
        let event = BaseEvent {
            id: wf_common::id::generate_id(),
            r#type: event_type,
            timestamp: wf_common::time::now(),
            workflow_id: None,
            execution_id: task_id.map(String::from),
            agent_loop_id: None,

            event_name: None,
            metadata: Some(metadata),
        };
        if let Err(err) = self.bus.publish(event) {
            tracing::warn!("Failed to publish shell event: {}", err);
        }
    }
}

fn shell_metadata(session_id: &str, task_id: Option<&str>) -> HashMap<String, serde_json::Value> {
    let mut metadata = HashMap::new();
    metadata.insert("session_id".to_string(), serde_json::json!(session_id));
    if let Some(task_id) = task_id {
        metadata.insert("task_id".to_string(), serde_json::json!(task_id));
    }
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_core::event::EventBus;
    use wf_shell::config::ShellToolConfig;

    fn bridge_with_bus() -> (Arc<EventBus>, ShellEventBusBridge) {
        let bus = Arc::new(EventBus::new(64));
        (bus.clone(), ShellEventBusBridge::new(bus))
    }

    #[test]
    fn test_bridge_forwards_session_created() {
        let (bus, bridge) = bridge_with_bus();
        let mut sub = bus.subscribe();
        bridge.on_session_created("shell-1", false, Some("exec-1"));
        let event = sub.try_recv().unwrap();
        assert_eq!(event.r#type, EventType::ShellSessionCreated);
        assert_eq!(event.execution_id.as_deref(), Some("exec-1"));
        let metadata = event.metadata.unwrap();
        assert_eq!(metadata["session_id"], serde_json::json!("shell-1"));
        assert_eq!(metadata["task_id"], serde_json::json!("exec-1"));
        assert_eq!(metadata["reused"], serde_json::json!(false));
    }

    #[test]
    fn test_bridge_forwards_output_lines() {
        let (bus, bridge) = bridge_with_bus();
        let mut sub = bus.subscribe();
        bridge.on_output("shell-1", Some("exec-1"), "line-x");
        bridge.on_output("shell-1", Some("exec-1"), "line-y");
        let first = sub.try_recv().unwrap();
        assert_eq!(first.r#type, EventType::ShellOutputReceived);
        assert_eq!(first.metadata.unwrap()["line"], serde_json::json!("line-x"));
        let second = sub.try_recv().unwrap();
        assert_eq!(
            second.metadata.unwrap()["line"],
            serde_json::json!("line-y")
        );
    }

    #[test]
    fn test_bridge_forwards_command_completed() {
        let (bus, bridge) = bridge_with_bus();
        let mut sub = bus.subscribe();
        bridge.on_command_completed("shell-1", Some("exec-1"), "echo a", Some(0), true);
        let event = sub.try_recv().unwrap();
        assert_eq!(event.r#type, EventType::ShellCommandCompleted);
        let metadata = event.metadata.unwrap();
        assert_eq!(metadata["command"], serde_json::json!("echo a"));
        assert_eq!(metadata["exit_code"], serde_json::json!(0));
        assert_eq!(metadata["success"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn test_bridge_end_to_end_with_shell_tools() {
        let bus = Arc::new(EventBus::new(64));
        let mut sub = bus.subscribe();
        let bridge = ShellEventBusBridge::new(bus.clone());

        let config = ShellToolConfig {
            output_event_enabled: true,
            event_sink: Some(Arc::new(bridge)),
            ..Default::default()
        };
        let registry = wf_tools::registry::ToolRegistry::new();
        wf_tools::predefined::shell::register(&registry, &config).unwrap();
        for def in [
            wf_tools::predefined::shell::EXECUTE_COMMAND.tool_def(),
            wf_tools::predefined::shell::BACKEND_SHELL.tool_def(),
            wf_tools::predefined::shell::SHELL_OUTPUT.tool_def(),
            wf_tools::predefined::shell::SHELL_KILL.tool_def(),
            wf_tools::predefined::shell::SHELL_SEND_INPUT.tool_def(),
            wf_tools::predefined::shell::SHELL_RESIZE.tool_def(),
            wf_tools::predefined::shell::GET_OR_CREATE_SHELL.tool_def(),
            wf_tools::predefined::shell::EXECUTE_IN_SESSION.tool_def(),
            wf_tools::predefined::shell::RELEASE_SESSIONS_FOR_TASK.tool_def(),
        ] {
            registry.register_tool(def);
        }
        let ctx =
            wf_tools::executor::trait_def::ToolExecutionContext::new("exec-bridge-e2e".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };
        std::fs::create_dir_all("/tmp/bridge-e2e").unwrap();

        let created = registry
            .execute_tool(
                "get_or_create_shell",
                &serde_json::json!({ "cwd": "/tmp/bridge-e2e" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        let session_id = created.result.unwrap()["session_id"]
            .as_str()
            .unwrap()
            .to_string();

        let executed = registry
            .execute_tool(
                "execute_in_session",
                &serde_json::json!({ "session_id": session_id, "command": "printf 'a\\nb\\n'" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(executed.result.unwrap()["success"], serde_json::json!(true));

        let mut types = Vec::new();
        while let Ok(event) = sub.try_recv() {
            types.push(event.r#type);
        }
        assert!(types.contains(&EventType::ShellSessionCreated));
        assert!(types.contains(&EventType::ShellCommandStarted));
        assert!(types.contains(&EventType::ShellOutputReceived));
        assert!(types.contains(&EventType::ShellCommandCompleted));
        assert_eq!(
            types
                .iter()
                .filter(|t| **t == EventType::ShellOutputReceived)
                .count(),
            2,
            "two output lines expected, got {:?}",
            types
        );

        let _ = registry
            .execute_tool(
                "shell_kill",
                &serde_json::json!({ "session_id": session_id }),
                &options,
                &ctx,
            )
            .await;
    }
}
