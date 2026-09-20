//! Predefined shell tools: definitions + background shell engine.
//!
//! Tools: execute_command (stateless), backend_shell / shell_output /
//! shell_kill / shell_send_input / shell_resize / get_or_create_shell /
//! execute_in_session / release_sessions_for_task (stateful background shell
//! sessions). Each tool lives in its own file; the shared background shell
//! engine lives in `wf_shell::engine`.

pub mod backend_shell;
pub mod checkpoint_handle;
pub mod execute_command;
pub mod execute_in_session;
pub mod get_or_create_shell;
pub mod release_sessions_for_task;
pub mod session_observe;
pub mod shell_kill;
pub mod shell_output;
pub mod shell_resize;
pub mod shell_send_input;
pub mod shell_wait;

pub use backend_shell::BACKEND_SHELL;
pub use execute_command::EXECUTE_COMMAND;
pub use execute_in_session::EXECUTE_IN_SESSION;
pub use get_or_create_shell::GET_OR_CREATE_SHELL;
pub use release_sessions_for_task::RELEASE_SESSIONS_FOR_TASK;
pub use shell_kill::SHELL_KILL;
pub use shell_output::SHELL_OUTPUT;
pub use shell_resize::SHELL_RESIZE;
pub use shell_send_input::SHELL_SEND_INPUT;
pub use shell_wait::SHELL_WAIT;

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
    &SHELL_WAIT,
    &SHELL_KILL,
    &SHELL_SEND_INPUT,
    &SHELL_RESIZE,
    &GET_OR_CREATE_SHELL,
    &EXECUTE_IN_SESSION,
    &RELEASE_SESSIONS_FOR_TASK,
];

/// Register shell handlers: execute_command (stateless) plus the background
/// shell stateful factories.
///
/// A [`session_observe::SessionLifecycleForwarder`] is installed as the
/// store's lifecycle sink so background process exits that happen without
/// any further tool call (store monitor thread) still close scope sampling
/// for the owning execution. Tool-level notifications remain the primary
/// path; the forwarder is the safety net for natural exits.
pub fn register(registry: &ToolRegistry, config: &ShellToolConfig) -> ToolResult<()> {
    execute_command::register(registry, config)?;

    let forwarder: session_observe::SharedSessionForwarder =
        Arc::new(session_observe::SessionLifecycleForwarder::new());
    let mut store_config = config.clone();
    store_config.lifecycle_sink = Some(forwarder.clone());
    let store = Arc::new(BackgroundShellStore::from_config(&store_config));
    backend_shell::register(registry, &store, &forwarder)?;
    shell_output::register(registry, &store)?;
    shell_wait::register(registry, &store)?;
    shell_kill::register(registry, &store, &forwarder)?;
    shell_send_input::register(registry, &store)?;
    shell_resize::register(registry, &store)?;
    get_or_create_shell::register(registry, &store, &forwarder)?;
    execute_in_session::register(registry, &store, &forwarder)?;
    release_sessions_for_task::register(registry, &store, &forwarder)?;

    Ok(())
}

#[cfg(test)]
mod tests;
