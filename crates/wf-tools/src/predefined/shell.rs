//! Predefined shell tools: definitions + background shell engine.
//!
//! Tools: execute_command (stateless), backend_shell / shell_output /
//! shell_kill (stateful background shell sessions). Each tool lives in its
//! own file; the shared background shell engine lives in [`engine`].

pub mod backend_shell;
pub mod engine;
pub mod execute_command;
pub mod shell_kill;
pub mod shell_output;

pub use backend_shell::BACKEND_SHELL;
pub use engine::BackgroundShellStore;
pub use execute_command::EXECUTE_COMMAND;
pub use shell_kill::SHELL_KILL;
pub use shell_output::SHELL_OUTPUT;

use std::sync::Arc;

use super::schema::ToolDefinition;
use crate::error::ToolResult;
use crate::registry::ToolRegistry;
use crate::shell::ShellToolConfig;

/// All shell tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&EXECUTE_COMMAND, &BACKEND_SHELL, &SHELL_OUTPUT, &SHELL_KILL];

/// Register shell handlers: execute_command (stateless) plus the background
/// shell stateful factories.
pub fn register(registry: &ToolRegistry, config: &ShellToolConfig) -> ToolResult<()> {
    execute_command::register(registry, config)?;

    let store = Arc::new(BackgroundShellStore::new(config.workspace_dir.clone()));
    backend_shell::register(registry, &store)?;
    shell_output::register(registry, &store)?;
    shell_kill::register(registry, &store)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::trait_def::ToolExecutionContext;

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
}
