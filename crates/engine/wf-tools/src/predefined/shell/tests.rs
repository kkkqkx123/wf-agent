//! Shell tool tests grouped by topic: backend shell, terminal backends,
//! session lifecycle and event streaming.

use super::*;
use crate::executor::trait_def::ToolExecutionContext;

mod backend_shell;
mod background_tasks;
mod events;
mod session;
mod terminal;

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
        SHELL_WAIT.tool_def(),
        SHELL_KILL.tool_def(),
        SHELL_SEND_INPUT.tool_def(),
        SHELL_RESIZE.tool_def(),
        GET_OR_CREATE_SHELL.tool_def(),
        EXECUTE_IN_SESSION.tool_def(),
        RELEASE_SESSIONS_FOR_TASK.tool_def(),
        SHELL_TASKS.tool_def(),
        SHELL_BACKGROUND.tool_def(),
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

/// Poll the session status until it becomes `status` or the timeout elapses.
async fn wait_for_status(
    registry: &ToolRegistry,
    session_id: &str,
    ctx: &ToolExecutionContext,
    status: &str,
) {
    let options = make_options();
    wf_common::poll_until(
        std::time::Duration::from_millis(50),
        std::time::Duration::from_secs(5),
        || async {
            registry
                .execute_tool(
                    "shell_output",
                    &serde_json::json!({ "session_id": session_id }),
                    &options,
                    ctx,
                )
                .await
                .ok()
                .and_then(|result| result.result)
                .and_then(|value| {
                    value
                        .get("status")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                })
                .as_deref()
                == Some(status)
        },
    )
    .await;
}

/// Poll the session output until `needle` appears or the timeout elapses.
async fn wait_for_output(
    registry: &ToolRegistry,
    session_id: &str,
    ctx: &ToolExecutionContext,
    needle: &str,
) -> String {
    wf_common::poll_until(
        std::time::Duration::from_millis(100),
        std::time::Duration::from_secs(8),
        || async {
            read_full_output(registry, session_id, ctx)
                .await
                .contains(needle)
        },
    )
    .await;
    read_full_output(registry, session_id, ctx).await
}
