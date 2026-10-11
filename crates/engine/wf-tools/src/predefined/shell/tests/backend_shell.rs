//! Backend shell tests: denied-command rejection, session lifecycle,
//! incremental output reads, output filters and send-input.

use super::*;
use crate::executor::trait_def::ToolExecutionContext;

#[tokio::test]
async fn test_backend_shell_denied_command_rejected() {
    let config = ShellToolConfig {
        denied_commands: Some(vec!["danger".into()]),
        ..Default::default()
    };
    let registry = shell_registry(&config);
    let ctx = ToolExecutionContext::new("exec-denied".into());
    let options = make_options();
    let result = registry
        .execute_tool(
            "backend_shell",
            &serde_json::json!({ "command": "danger --all" }),
            &options,
            &ctx,
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejected by shell policy"),
        "backend_shell must honor the deny policy at the spawn entry"
    );
}

#[tokio::test]
async fn test_backend_shell_lifecycle() {
    let registry = ToolRegistry::new();
    register(&registry, &ShellToolConfig::default()).unwrap();

    let ctx = ToolExecutionContext::new("exec-1".into());
    let options = wf_types::tool::ToolExecutionOptions {
        timeout: None,
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    };

    let tool = BACKEND_SHELL.tool_def();
    registry.register_tool(tool.clone());
    registry.register_tool(SHELL_OUTPUT.tool_def());
    registry.register_tool(SHELL_KILL.tool_def());

    let result = registry
        .execute_tool(
            "backend_shell",
            &serde_json::json!({ "command": "echo hello-backend" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert!(result.success);
    let session_id = result
        .result
        .and_then(|v| v.get("session_id").cloned())
        .and_then(|v| v.as_str().map(String::from))
        .unwrap();

    // Wait for the command to finish writing output.
    let text = wait_for_output(&registry, &session_id, &ctx, "hello-backend").await;
    assert!(text.contains("hello-backend"), "output was: {}", text);

    let killed = registry
        .execute_tool(
            "shell_kill",
            &serde_json::json!({ "session_id": session_id }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert!(killed.success);
    assert_eq!(killed.result.unwrap()["killed"], serde_json::json!(true));
}

#[tokio::test]
async fn test_shell_output_incremental_read() {
    let registry = ToolRegistry::new();
    register(&registry, &ShellToolConfig::default()).unwrap();
    registry.register_tool(BACKEND_SHELL.tool_def());
    registry.register_tool(SHELL_OUTPUT.tool_def());
    registry.register_tool(SHELL_KILL.tool_def());

    let ctx = ToolExecutionContext::new("exec-1".into());
    let options = wf_types::tool::ToolExecutionOptions {
        timeout: None,
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    };

    let backend = registry
        .execute_tool(
            "backend_shell",
            &serde_json::json!({ "command": "printf 'first\\n'; sleep 0.2; printf 'second\\n'" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let session_id = backend
        .result
        .unwrap()
        .get("session_id")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap();

    // Allow the first chunk to be captured before the incremental read.
    wait_for_output(&registry, &session_id, &ctx, "first").await;
    let out1 = registry
        .execute_tool(
            "shell_output",
            &serde_json::json!({ "session_id": session_id, "all": false }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let new1 = out1
        .result
        .unwrap()
        .get("new_output")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    assert!(
        new1.contains("first"),
        "first read should contain 'first': {}",
        new1
    );

    // Wait for the second chunk, then read incrementally again.
    wait_for_output(&registry, &session_id, &ctx, "second").await;
    let out2 = registry
        .execute_tool(
            "shell_output",
            &serde_json::json!({ "session_id": session_id, "all": false }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let new2 = out2
        .result
        .unwrap()
        .get("new_output")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    assert!(
        new2.contains("second"),
        "second read should contain 'second': {}",
        new2
    );

    // A full read returns everything seen so far.
    let full = registry
        .execute_tool(
            "shell_output",
            &serde_json::json!({ "session_id": session_id }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let full_text = full
        .result
        .unwrap()
        .get("output")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    assert!(full_text.contains("first") && full_text.contains("second"));

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
async fn test_backend_shell_send_input() {
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-send-input".into());
    let options = make_options();

    let session_id = spawn_session(
        &registry,
        &serde_json::json!({ "command": "read x; echo \"got:$x\"" }),
        &ctx,
    )
    .await;

    let sent = registry
        .execute_tool(
            "shell_send_input",
            &serde_json::json!({ "session_id": session_id, "input": "hello" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert!(sent.success, "send_input failed: {:?}", sent.error);

    let output = wait_for_output(&registry, &session_id, &ctx, "got:hello").await;
    assert!(output.contains("got:hello"), "output was: {}", output);

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
async fn test_shell_send_input_missing_session() {
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-send-input-missing".into());
    let options = make_options();
    let result = registry
        .execute_tool(
            "shell_send_input",
            &serde_json::json!({ "session_id": "nope", "input": "hi" }),
            &options,
            &ctx,
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_shell_output_filter() {
    let registry = shell_registry(&ShellToolConfig::default());
    let ctx = ToolExecutionContext::new("exec-filter".into());
    let options = make_options();

    let session_id = spawn_session(
        &registry,
        &serde_json::json!({ "command": "printf 'alpha\\nbeta\\ngamma\\n'" }),
        &ctx,
    )
    .await;

    let output = wait_for_output(&registry, &session_id, &ctx, "gamma").await;
    assert!(output.contains("alpha"), "output was: {}", output);

    let filtered = registry
        .execute_tool(
            "shell_output",
            &serde_json::json!({ "session_id": session_id, "filter": "beta" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let filtered_text = filtered
        .result
        .unwrap()
        .get("output")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    assert!(
        filtered_text.contains("beta"),
        "filtered: {}",
        filtered_text
    );
    assert!(
        !filtered_text.contains("alpha"),
        "filtered: {}",
        filtered_text
    );

    // An invalid regex is ignored (all lines kept).
    let invalid = registry
        .execute_tool(
            "shell_output",
            &serde_json::json!({ "session_id": session_id, "filter": "([invalid" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    let invalid_text = invalid
        .result
        .unwrap()
        .get("output")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    assert!(
        invalid_text.contains("alpha"),
        "invalid filter: {}",
        invalid_text
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
