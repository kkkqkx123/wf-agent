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

#[tokio::test]
async fn shell_wait_returns_background_branch_when_preempted() {
    let execution = format!("exec-bg-preempt-{}", wf_common::generate_id());
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ctx_for(&execution);
    let session_id = super::spawn_session(
        &registry,
        &serde_json::json!({ "command": "sleep 30" }),
        &ctx,
    )
    .await;
    registry
        .execute_tool(
            "shell_background",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let waited = registry
        .execute_tool(
            "shell_wait",
            &serde_json::json!({ "session_id": session_id, "timeout": 5 }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let value = waited.result.unwrap();
    assert_eq!(value["backgrounded"], serde_json::json!(true));
    assert_eq!(value["timed_out"], serde_json::json!(false));
    assert!(value["guidance"].as_str().unwrap().contains("background"));
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
async fn shell_background_all_moves_execution_tasks() {
    let execution = format!("exec-bg-all-{}", wf_common::generate_id());
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ctx_for(&execution);
    let first = super::spawn_session(
        &registry,
        &serde_json::json!({ "command": "sleep 30" }),
        &ctx,
    )
    .await;
    let second = super::spawn_session(
        &registry,
        &serde_json::json!({ "command": "sleep 30" }),
        &ctx,
    )
    .await;
    let moved = registry
        .execute_tool(
            "shell_background_all",
            &serde_json::json!({ "execution_id": execution }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let value = moved.result.unwrap();
    assert_eq!(value["count"], serde_json::json!(2));
    assert_eq!(value["backgrounded"], serde_json::json!(true));
    for session_id in [&first, &second] {
        let _ = registry
            .execute_tool(
                "shell_kill",
                &serde_json::json!({ "session_id": session_id }),
                &make_options(),
                &ctx,
            )
            .await;
    }
}

#[tokio::test]
async fn shell_ack_confirms_settled_tasks_only() {
    let execution = format!("exec-bg-ack-{}", wf_common::generate_id());
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ctx_for(&execution);
    let session_id = super::spawn_session(
        &registry,
        &serde_json::json!({ "command": "sleep 30" }),
        &ctx,
    )
    .await;
    let running_ack = registry
        .execute_tool(
            "shell_ack",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(
        running_ack.result.unwrap()["acknowledged"],
        serde_json::json!(false)
    );
    registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let settled_ack = registry
        .execute_tool(
            "shell_ack",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(
        settled_ack.result.unwrap()["acknowledged"],
        serde_json::json!(true)
    );
}

#[tokio::test]
async fn shell_tasks_global_listing_includes_settled() {
    let execution = format!("exec-bg-global-{}", wf_common::generate_id());
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ctx_for(&execution);
    let session_id = super::spawn_session(
        &registry,
        &serde_json::json!({ "command": "sleep 30" }),
        &ctx,
    )
    .await;
    let running = registry
        .execute_tool("shell_tasks", &serde_json::json!({}), &make_options(), &ctx)
        .await
        .unwrap();
    assert!(running.result.unwrap()["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .any(|task| task["session_id"] == serde_json::json!(session_id)));
    registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    let without_settled = registry
        .execute_tool("shell_tasks", &serde_json::json!({}), &make_options(), &ctx)
        .await
        .unwrap();
    assert!(!without_settled.result.unwrap()["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .any(|task| task["session_id"] == serde_json::json!(session_id)));
    let with_settled = registry
        .execute_tool(
            "shell_tasks",
            &serde_json::json!({ "include_settled": true }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();
    assert!(with_settled.result.unwrap()["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .any(|task| task["session_id"] == serde_json::json!(session_id)));
    let _ = registry
        .execute_tool(
            "shell_ack",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await;
}

#[tokio::test]
async fn shell_output_reports_absolute_cursor() {
    let execution = format!("exec-bg-cursor-{}", wf_common::generate_id());
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
    let full = registry
        .execute_tool(
            "shell_output",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap()
        .result
        .unwrap();
    let total = full["total_written"].as_u64().expect("total cursor");
    assert_eq!(full["cursor"].as_u64(), Some(total));
    assert_eq!(full["window_end"].as_u64(), Some(total));
    let window = registry
        .execute_tool(
            "shell_output",
            &serde_json::json!({ "session_id": session_id, "start": 0, "max_bytes": 2 }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap()
        .result
        .unwrap();
    assert_eq!(window["window_end"].as_u64(), Some(2));
    let incremental = registry
        .execute_tool(
            "shell_output",
            &serde_json::json!({ "session_id": session_id, "all": false }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap()
        .result
        .unwrap();
    assert_eq!(incremental["cursor"].as_u64(), Some(total));
    let _ = registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &make_options(),
            &ctx,
        )
        .await;
}
