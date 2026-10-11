//! Session lifecycle tests: get-or-create reuse, execute_in_session
//! semantics and release_sessions_for_task.

use super::*;
use crate::executor::trait_def::ToolExecutionContext;

#[tokio::test]
async fn test_get_or_create_shell_reuse() {
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-reuse".into());
    let options = make_options();

    let first = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/reuse-a" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let first_result = first.result.unwrap();
    assert_eq!(first_result["reused"], serde_json::json!(false));
    let first_id = first_result["session_id"].as_str().unwrap().to_string();

    // Same cwd + same task (defaults to execution id): reused.
    let second = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/reuse-a" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let second_result = second.result.unwrap();
    assert_eq!(second_result["reused"], serde_json::json!(true));
    assert_eq!(second_result["session_id"], serde_json::json!(first_id));
    assert_eq!(second_result["status"], serde_json::json!("idle"));

    // Different cwd: new session.
    let other = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/reuse-b" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let other_result = other.result.unwrap();
    assert_eq!(other_result["reused"], serde_json::json!(false));
    assert_ne!(other_result["session_id"], serde_json::json!(first_id));
}

#[tokio::test]
async fn test_get_or_create_shell_custom_task_id() {
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-reuse-task".into());
    let options = make_options();

    let created = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/reuse-t", "task_id": "task-99" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let result = created.result.unwrap();
    assert_eq!(result["task_id"], serde_json::json!("task-99"));
    assert_eq!(result["reused"], serde_json::json!(false));
}

#[tokio::test]
async fn test_execute_in_session_multiple_commands() {
    std::fs::create_dir_all("/tmp/session-cmds").unwrap();
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-session-commands".into());
    let options = make_options();

    let created = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/session-cmds" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let session_id = created.result.unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let first = registry
        .execute_tool(
            "execute_in_session",
            &serde_json::json!({ "session_id": session_id, "command": "echo a" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let first_result = first.result.unwrap();
    assert_eq!(first_result["success"], serde_json::json!(true));
    assert!(first_result["output"].as_str().unwrap().contains("a"));

    let second = registry
        .execute_tool(
            "execute_in_session",
            &serde_json::json!({ "session_id": session_id, "command": "echo b" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let second_result = second.result.unwrap();
    assert_eq!(second_result["success"], serde_json::json!(true));
    assert!(second_result["output"].as_str().unwrap().contains("b"));

    // Session output accumulates both commands; status is idle again.
    let output = read_full_output(&registry, &session_id, &ctx).await;
    assert!(
        output.contains("a") && output.contains("b"),
        "output: {}",
        output
    );
    let status = registry
        .execute_tool(
            "shell_output",
            &serde_json::json!({ "session_id": session_id, "all": false }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(status.result.unwrap()["status"], serde_json::json!("idle"));

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
async fn test_execute_in_session_missing_session() {
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-session-missing".into());
    let options = make_options();
    let result = registry
        .execute_tool(
            "execute_in_session",
            &serde_json::json!({ "session_id": "nope", "command": "echo hi" }),
            &options,
            &ctx,
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_in_session_busy_rejected() {
    std::fs::create_dir_all("/tmp/session-busy").unwrap();
    let registry = Arc::new(shell_registry(&ShellToolConfig::default()));
    let ctx = ToolExecutionContext::new("exec-session-busy".into());
    let options = make_options();
    let created = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/session-busy" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let session_id = created.result.unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Run a blocking command on another thread to make the session busy.
    let busy_registry = registry.clone();
    let busy_ctx = ToolExecutionContext::new("exec-session-busy".into());
    let busy_options = make_options();
    let sid = session_id.clone();
    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            busy_registry
                .execute_tool(
                    "execute_in_session",
                    &serde_json::json!({ "session_id": sid, "command": "sleep 2", "timeout": 10000 }),
                    &busy_options,
                    &busy_ctx,
                )
                .await
                .unwrap()
        })
    });

    // Wait until the session is busy before attempting a second command.
    wait_for_status(&registry, &session_id, &ctx, "busy").await;

    let result = registry
        .execute_tool(
            "execute_in_session",
            &serde_json::json!({ "session_id": session_id, "command": "echo hi" }),
            &options,
            &ctx,
        )
        .await;
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("busy"),
        "expected a busy error"
    );

    handle.join().unwrap();
    let _ = registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &options,
            &ctx,
        )
        .await;
}

/// A long blocking `execute_in_session` must not occupy a tokio worker
/// thread: an async timer scheduled concurrently fires well before the
/// blocking command finishes (the call runs on the tokio blocking pool).
#[tokio::test]
async fn test_execute_in_session_does_not_occupy_worker() {
    std::fs::create_dir_all("/tmp/session-worker").unwrap();
    let registry = Arc::new(shell_registry(&ShellToolConfig::default()));
    let ctx = ToolExecutionContext::new("exec-worker".into());
    let options = make_options();

    let created = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/session-worker" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let session_id = created.result.unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let busy_registry = registry.clone();
    let busy_ctx = ToolExecutionContext::new("exec-worker".into());
    let busy_options = make_options();
    let sid = session_id.clone();
    let start = std::time::Instant::now();
    let handle = tokio::spawn(async move {
        busy_registry
            .execute_tool(
                "execute_in_session",
                &serde_json::json!({ "session_id": sid, "command": "sleep 2", "timeout": 10000 }),
                &busy_options,
                &busy_ctx,
            )
            .await
            .unwrap()
    });

    // A timer must fire while the blocking command is still running; if
    // `execute_in_session` had blocked a worker thread this would wait
    // until the command finished (~2s).
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        start.elapsed() < std::time::Duration::from_millis(1000),
        "tokio worker was occupied: {:?}",
        start.elapsed()
    );

    let result = handle.await.unwrap();
    assert_eq!(result.result.unwrap()["success"], serde_json::json!(true));
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
async fn test_release_sessions_for_task() {
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-release".into());
    let options = make_options();

    let created = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/release-a", "task_id": "rel-1" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let session_id = created.result.unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/release-b", "task_id": "rel-1" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();

    // Release (not terminate): sessions remain and become reusable.
    let released = registry
        .execute_tool(
            "release_sessions_for_task",
            &serde_json::json!({ "task_id": "rel-1" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(released.result.unwrap()["released"], serde_json::json!(2));

    // The released session is reusable by cwd for another task.
    let reused = registry
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/release-a", "task_id": "rel-2" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let reused_result = reused.result.unwrap();
    assert_eq!(reused_result["reused"], serde_json::json!(true));
    assert_eq!(reused_result["session_id"], serde_json::json!(session_id));

    // Terminate: sessions are removed.
    let terminated = registry
        .execute_tool(
            "release_sessions_for_task",
            &serde_json::json!({ "task_id": "rel-2", "terminate": true }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(terminated.result.unwrap()["released"], serde_json::json!(1));
    let result = registry
        .execute_tool(
            "shell_output",
            &serde_json::json!({ "session_id": session_id }),
            &options,
            &ctx,
        )
        .await;
    assert!(result.is_err(), "session should be gone after terminate");
}
