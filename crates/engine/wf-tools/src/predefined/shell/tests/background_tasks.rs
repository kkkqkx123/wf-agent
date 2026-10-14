//! Background task tests: registry wiring through backend_shell,
//! shell_background handoff and shell_tasks listing.

use super::super::*;
use super::{make_options, shell_registry};
use crate::executor::trait_def::ToolExecutionContext;

fn ctx_for(execution: &str) -> ToolExecutionContext {
    ToolExecutionContext::new(execution.into())
}

#[tokio::test]
async fn backend_shell_registers_a_running_task() {
    let execution = format!("exec-bg-reg-{}", wf_common::generate_id());
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ctx_for(&execution);
    let session_id = super::spawn_session(
        &registry,
        &serde_json::json!({ "command": "sleep 30" }),
        &ctx,
    )
    .await;
    let listed = registry
        .execute_tool(
            "shell_tasks",
            &serde_json::json!({ "execution_id": execution }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let tasks = listed.result.unwrap()["tasks"].clone();
    let found = tasks
        .as_array()
        .expect("tasks array")
        .iter()
        .find(|task| task["session_id"] == serde_json::json!(session_id))
        .expect("spawned session tracked")
        .clone();
    assert_eq!(found["status"], serde_json::json!("running"));
    assert_eq!(found["backgrounded"], serde_json::json!(false));
    let _ = registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn shell_background_marks_task_and_kill_cancels() {
    let execution = format!("exec-bg-hand-{}", wf_common::generate_id());
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ctx_for(&execution);
    let session_id = super::spawn_session(
        &registry,
        &serde_json::json!({ "command": "sleep 30" }),
        &ctx,
    )
    .await;
    let moved = registry
        .execute_tool(
            "shell_background",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let moved = moved.result.unwrap();
    assert_eq!(moved["backgrounded"], serde_json::json!(true));
    assert!(moved["guidance"].as_str().unwrap().contains("background"));
    let listed = registry
        .execute_tool(
            "shell_tasks",
            &serde_json::json!({ "execution_id": execution }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let found = listed.result.unwrap()["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .find(|task| task["session_id"] == serde_json::json!(session_id))
        .expect("task listed")
        .clone();
    assert_eq!(found["backgrounded"], serde_json::json!(true));
    registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let listed = registry
        .execute_tool(
            "shell_tasks",
            &serde_json::json!({ "execution_id": execution, "include_settled": true }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let found = listed.result.unwrap()["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .find(|task| task["session_id"] == serde_json::json!(session_id))
        .expect("cancelled task kept")
        .clone();
    assert_eq!(found["status"], serde_json::json!("cancelled"));
}

#[tokio::test]
async fn finished_command_settles_the_task() {
    let execution = format!("exec-bg-settle-{}", wf_common::generate_id());
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ctx_for(&execution);
    let created = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({}),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let session_id = created.result.unwrap()["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    registry
        .execute_tool(
            "execute_in_session",
            &serde_json::json!({ "session_id": session_id, "command": "echo hi" }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let listed = registry
        .execute_tool(
            "shell_tasks",
            &serde_json::json!({ "execution_id": execution, "include_settled": true }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let found = listed.result.unwrap()["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .find(|task| task["session_id"] == serde_json::json!(session_id))
        .expect("settled task kept")
        .clone();
    assert_eq!(found["status"], serde_json::json!("completed"));
    let running = registry
        .execute_tool(
            "shell_tasks",
            &serde_json::json!({ "execution_id": execution }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    assert!(running.result.unwrap()["tasks"]
        .as_array()
        .expect("tasks array")
        .is_empty());
    let _ = registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await;
}

#[tokio::test]
async fn shell_background_rejects_idle_sessions() {
    let execution = format!("exec-bg-idle-{}", wf_common::generate_id());
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ctx_for(&execution);
    let created = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({}),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let session_id = created.result.unwrap()["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    let moved = registry
        .execute_tool(
            "shell_background",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await;
    assert!(moved.is_err());
    let _ = registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await;
}
