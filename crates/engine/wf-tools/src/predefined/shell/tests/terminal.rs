//! Terminal-facing shell tests: graceful kill, resize errors and the pty
//! backend (roundtrip, resize, CRLF normalization, pipe fallback).

use super::*;
use crate::executor::trait_def::ToolExecutionContext;

#[tokio::test]
async fn test_shell_kill_graceful_returns_quickly() {
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-graceful".into());
    let options = make_options();

    let session_id = spawn_session(
        &registry,
        &serde_json::json!({ "command": "sleep 30" }),
        &ctx,
    )
    .await;
    wait_for_status(&registry, &session_id, &ctx, "busy").await;

    let start = std::time::Instant::now();
    let killed = registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id, "graceful": true }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert!(killed.success);
    assert_eq!(killed.result.unwrap()["killed"], serde_json::json!(true));
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "graceful kill took too long: {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_shell_resize_pipe_session_errors() {
    let config = ShellToolConfig {
        pty_enabled: false,
        ..Default::default()
    };
    let registry = shell_registry(&config);
    let ctx = ToolExecutionContext::new("exec-resize-pipe".into());
    let options = make_options();

    let session_id = spawn_session(
        &registry,
        &serde_json::json!({ "command": "sleep 5" }),
        &ctx,
    )
    .await;
    let result = registry
        .execute_tool(
            "shell_resize",
            &serde_json::json!({ "session_id": session_id, "rows": 10, "cols": 20 }),
            &options,
            &ctx,
        )
        .await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("does not use a PTY"));

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
async fn test_pty_interactive_read_roundtrip() {
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-pty-read".into());
    let options = make_options();

    let params = serde_json::json!({
        "command": "printf 'name: '; read n; echo \"hi $n\"",
        "interactive": true,
    });
    let backend = registry
        .execute_tool("backend_shell", &params, &options, &ctx)
        .await
        .unwrap();
    let result = backend.result.unwrap();
    assert_eq!(result["mode"], serde_json::json!("pty"));
    let session_id = result
        .get("session_id")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap();

    let prompt = wait_for_output(&registry, &session_id, &ctx, "name:").await;
    assert!(prompt.contains("name:"), "no prompt in: {}", prompt);

    let sent = registry
        .execute_tool(
            "shell_send_input",
            &serde_json::json!({ "session_id": session_id, "input": "world" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert!(sent.success);

    let output = wait_for_output(&registry, &session_id, &ctx, "hi world").await;
    assert!(output.contains("hi world"), "output was: {}", output);

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
async fn test_pty_resize_changes_terminal_size() {
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-pty-resize".into());
    let options = make_options();

    let params = serde_json::json!({
        "command": "stty size; read x; stty size",
        "interactive": true,
        "rows": 12,
        "cols": 34,
    });
    let backend = registry
        .execute_tool("backend_shell", &params, &options, &ctx)
        .await
        .unwrap();
    let result = backend.result.unwrap();
    assert_eq!(result["mode"], serde_json::json!("pty"));
    let session_id = result
        .get("session_id")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap();

    let initial = wait_for_output(&registry, &session_id, &ctx, "12 34").await;
    assert!(initial.contains("12 34"), "initial size: {}", initial);

    let resized = registry
        .execute_tool(
            "shell_resize",
            &serde_json::json!({ "session_id": session_id, "rows": 20, "cols": 45 }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert!(resized.success);

    let _ = registry
        .execute_tool(
            "shell_send_input",
            &serde_json::json!({ "session_id": session_id, "input": "go" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();

    let resized_out = wait_for_output(&registry, &session_id, &ctx, "20 45").await;
    assert!(
        resized_out.contains("20 45"),
        "resized size: {}",
        resized_out
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

#[tokio::test]
async fn test_pty_crlf_normalization() {
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-pty-crlf".into());
    let options = make_options();

    let session_id = spawn_session(
        &registry,
        &serde_json::json!({ "command": "echo hello-pty", "interactive": true }),
        &ctx,
    )
    .await;

    let output = wait_for_output(&registry, &session_id, &ctx, "hello-pty").await;
    assert!(output.contains("hello-pty\n"), "output: {:?}", output);
    assert!(!output.contains('\r'), "CR not normalized: {:?}", output);

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
async fn test_pty_disabled_falls_back_to_pipe() {
    let config = ShellToolConfig {
        pty_enabled: false,
        ..Default::default()
    };
    let registry = shell_registry(&config);
    let ctx = ToolExecutionContext::new("exec-pty-fallback".into());
    let options = make_options();

    let backend = registry
        .execute_tool(
            "backend_shell",
            &serde_json::json!({ "command": "echo hi", "interactive": true }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let result = backend.result.unwrap();
    assert_eq!(result["mode"], serde_json::json!("pipe"));
    let session_id = result
        .get("session_id")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap();

    let _ = registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &options,
            &ctx,
        )
        .await;
}
