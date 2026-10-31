//! Shell event streaming tests: an in-memory event sink observed through
//! shell_wait completion and shell_output event reads.

use super::*;
use crate::executor::trait_def::ToolExecutionContext;
use wf_shell::event_sink::ShellEventSink;

/// In-memory sink capturing shell events (test-only).
#[derive(Default)]
struct TestEventSink {
    events: std::sync::Mutex<Vec<String>>,
}

impl ShellEventSink for TestEventSink {
    fn on_session_created(&self, session_id: &str, reused: bool, task_id: Option<&str>) {
        self.events.lock().unwrap().push(format!(
            "created:{}:{}:{}",
            session_id,
            reused,
            task_id.unwrap_or("")
        ));
    }

    fn on_command_started(&self, session_id: &str, task_id: Option<&str>, command: &str) {
        self.events.lock().unwrap().push(format!(
            "started:{}:{}:{}",
            session_id,
            task_id.unwrap_or(""),
            command
        ));
    }

    fn on_output(&self, session_id: &str, task_id: Option<&str>, line: &str) {
        self.events.lock().unwrap().push(format!(
            "output:{}:{}:{}",
            session_id,
            task_id.unwrap_or(""),
            line
        ));
    }

    fn on_command_completed(
        &self,
        session_id: &str,
        task_id: Option<&str>,
        command: &str,
        exit_code: Option<i32>,
        success: bool,
    ) {
        self.events.lock().unwrap().push(format!(
            "completed:{}:{}:{}:{:?}:{}",
            session_id,
            task_id.unwrap_or(""),
            command,
            exit_code,
            success
        ));
    }

    fn on_session_terminated(&self, session_id: &str, task_id: Option<&str>) {
        self.events.lock().unwrap().push(format!(
            "terminated:{}:{}",
            session_id,
            task_id.unwrap_or("")
        ));
    }
}

#[tokio::test]
async fn test_shell_wait_completion_and_pattern() {
    let registry = shell_registry(&ShellToolConfig::default());
    registry.register_tool(SHELL_WAIT.tool_def());
    let ctx = ToolExecutionContext::new("exec-wait".into());
    let options = make_options();

    let session_id = spawn_session(
        &registry,
        &serde_json::json!({ "command": "printf 'waiting\\n'; sleep 0.3; printf 'done-marker\\n'" }),
        &ctx,
    )
    .await;

    let matched = registry
        .execute_tool(
            "shell_wait",
            &serde_json::json!({ "session_id": session_id, "pattern": "done-marker", "timeout": 8 }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let matched_value = matched.result.unwrap();
    assert_eq!(matched_value["matched"], serde_json::json!(true));
    assert_eq!(matched_value["timed_out"], serde_json::json!(false));

    let completed = registry
        .execute_tool(
            "shell_wait",
            &serde_json::json!({ "session_id": session_id, "timeout": 8 }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let completed_value = completed.result.unwrap();
    assert_eq!(completed_value["timed_out"], serde_json::json!(false));

    let missing = registry
        .execute_tool(
            "shell_wait",
            &serde_json::json!({ "session_id": "nope", "timeout": 10 }),
            &options,
            &ctx,
        )
        .await;
    assert!(missing.is_err());

    let _ = registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &options,
            &ctx,
        )
        .await;
}

#[tokio::test]
async fn test_shell_output_events_via_sink() {
    std::fs::create_dir_all("/tmp/events").unwrap();
    let sink = Arc::new(TestEventSink::default());
    let config = ShellToolConfig {
        output_event_enabled: true,
        event_sink: Some(sink.clone()),
        ..Default::default()
    };
    let registry = shell_registry(&config);
    let ctx = ToolExecutionContext::new("exec-events".into());
    let options = make_options();

    let created = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/events" }),
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
            &serde_json::json!({ "session_id": session_id, "command": "printf 'x\\ny\\n'" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(executed.result.unwrap()["success"], serde_json::json!(true));

    let _ = registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &options,
            &ctx,
        )
        .await;

    // Events are delivered on a background dispatch thread; wait for the
    // terminated event (queued after kill) to arrive, which implies the
    // rest have been flushed by the execute_in_session path.
    wf_common::poll_until(
        std::time::Duration::from_millis(10),
        std::time::Duration::from_secs(8),
        || async {
            sink.events
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.starts_with(&format!("terminated:{}:{}", session_id, "exec-events")))
        },
    )
    .await;
    let events = sink.events.lock().unwrap().clone();
    assert!(
        events
            .iter()
            .any(|e| e.starts_with(&format!("created:{}:false:", session_id))),
        "events: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| e == &format!("started:{}:{}:printf 'x\\ny\\n'", session_id, "exec-events")),
        "events: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| e == &format!("output:{}:{}:x", session_id, "exec-events")),
        "events: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| e == &format!("output:{}:{}:y", session_id, "exec-events")),
        "events: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| e.starts_with(&format!("completed:{}:{}:", session_id, "exec-events"))),
        "events: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| e.starts_with(&format!("terminated:{}:{}", session_id, "exec-events"))),
        "events: {:?}",
        events
    );
}
