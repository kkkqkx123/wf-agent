//! Predefined shell tools: definitions + background shell engine.
//!
//! Tools: execute_command (stateless), backend_shell / shell_output /
//! shell_kill / shell_send_input / shell_resize / get_or_create_shell /
//! execute_in_session / release_sessions_for_task (stateful background shell
//! sessions). Each tool lives in its own file; the shared background shell
//! engine lives in `wf_shell::engine`.

pub mod backend_shell;
pub mod execute_command;
pub mod execute_in_session;
pub mod get_or_create_shell;
pub mod release_sessions_for_task;
pub mod shell_kill;
pub mod shell_output;
pub mod shell_resize;
pub mod shell_send_input;

pub use backend_shell::BACKEND_SHELL;
pub use execute_command::EXECUTE_COMMAND;
pub use execute_in_session::EXECUTE_IN_SESSION;
pub use get_or_create_shell::GET_OR_CREATE_SHELL;
pub use release_sessions_for_task::RELEASE_SESSIONS_FOR_TASK;
pub use shell_kill::SHELL_KILL;
pub use shell_output::SHELL_OUTPUT;
pub use shell_resize::SHELL_RESIZE;
pub use shell_send_input::SHELL_SEND_INPUT;

use std::sync::Arc;

use super::schema::ToolDefinition;
use crate::error::ToolResult;
use crate::registry::ToolRegistry;
use wf_shell::config::ShellToolConfig;
use wf_shell::engine::BackgroundShellStore;

/// All shell tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[
    &EXECUTE_COMMAND,
    &BACKEND_SHELL,
    &SHELL_OUTPUT,
    &SHELL_KILL,
    &SHELL_SEND_INPUT,
    &SHELL_RESIZE,
    &GET_OR_CREATE_SHELL,
    &EXECUTE_IN_SESSION,
    &RELEASE_SESSIONS_FOR_TASK,
];

/// Register shell handlers: execute_command (stateless) plus the background
/// shell stateful factories.
pub fn register(registry: &ToolRegistry, config: &ShellToolConfig) -> ToolResult<()> {
    execute_command::register(registry, config)?;

    let store = Arc::new(BackgroundShellStore::from_config(config));
    backend_shell::register(registry, &store)?;
    shell_output::register(registry, &store)?;
    shell_kill::register(registry, &store)?;
    shell_send_input::register(registry, &store)?;
    shell_resize::register(registry, &store)?;
    get_or_create_shell::register(registry, &store)?;
    execute_in_session::register(registry, &store)?;
    release_sessions_for_task::register(registry, &store)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::trait_def::ToolExecutionContext;
    use wf_shell::event_sink::ShellEventSink;

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

        // Wait briefly for the command to finish writing output.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let output = registry
            .execute_tool(
                "shell_output",
                &serde_json::json!({ "session_id": session_id }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(output.success);
        let text = output
            .result
            .and_then(|v| v.get("output").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
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
        std::thread::sleep(std::time::Duration::from_millis(150));
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
        std::thread::sleep(std::time::Duration::from_millis(300));
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

    fn make_options() -> wf_types::tool::ToolExecutionOptions {
        wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        }
    }

    /// Registry wired with all shell tool factories and tool definitions.
    fn shell_registry(config: &ShellToolConfig) -> ToolRegistry {
        let registry = ToolRegistry::new();
        register(&registry, config).unwrap();
        for def in [
            EXECUTE_COMMAND.tool_def(),
            BACKEND_SHELL.tool_def(),
            SHELL_OUTPUT.tool_def(),
            SHELL_KILL.tool_def(),
            SHELL_SEND_INPUT.tool_def(),
            SHELL_RESIZE.tool_def(),
            GET_OR_CREATE_SHELL.tool_def(),
            EXECUTE_IN_SESSION.tool_def(),
            RELEASE_SESSIONS_FOR_TASK.tool_def(),
        ] {
            registry.register_tool(def);
        }
        registry
    }

    async fn spawn_session(
        registry: &ToolRegistry,
        params: &serde_json::Value,
        ctx: &ToolExecutionContext,
    ) -> String {
        let options = make_options();
        let backend = registry
            .execute_tool("backend_shell", params, &options, ctx)
            .await
            .unwrap();
        backend
            .result
            .unwrap()
            .get("session_id")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap()
    }

    /// Full output of a session (via shell_output with all=true).
    async fn read_full_output(
        registry: &ToolRegistry,
        session_id: &str,
        ctx: &ToolExecutionContext,
    ) -> String {
        let options = make_options();
        registry
            .execute_tool(
                "shell_output",
                &serde_json::json!({ "session_id": session_id }),
                &options,
                ctx,
            )
            .await
            .unwrap()
            .result
            .and_then(|v| v.get("output").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default()
    }

    /// Poll the session output until `needle` appears or the timeout elapses.
    async fn wait_for_output(
        registry: &ToolRegistry,
        session_id: &str,
        ctx: &ToolExecutionContext,
        needle: &str,
    ) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let mut last = String::new();
        while std::time::Instant::now() < deadline {
            last = read_full_output(registry, session_id, ctx).await;
            if last.contains(needle) {
                return last;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        last
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
        std::thread::sleep(std::time::Duration::from_millis(100));

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

    #[cfg(feature = "pty")]
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

    #[cfg(feature = "pty")]
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

    #[cfg(feature = "pty")]
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

    #[cfg(feature = "pty")]
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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let st = registry
                .execute_tool(
                    "shell_output",
                    &serde_json::json!({ "session_id": session_id }),
                    &options,
                    &ctx,
                )
                .await
                .unwrap();
            if st.result.unwrap()["status"] == "busy" {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "session never became busy"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let events = loop {
            let events = sink.events.lock().unwrap().clone();
            if events
                .iter()
                .any(|e| e.starts_with(&format!("terminated:{}:{}", session_id, "exec-events")))
                || std::time::Instant::now() >= deadline
            {
                break events;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(
            events
                .iter()
                .any(|e| e.starts_with(&format!("created:{}:false:", session_id))),
            "events: {:?}",
            events
        );
        assert!(
            events.iter().any(
                |e| e == &format!("started:{}:{}:printf 'x\\ny\\n'", session_id, "exec-events")
            ),
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
}
