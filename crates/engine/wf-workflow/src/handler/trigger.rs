//! In-graph synchronous trigger actions (message-node scope).
//!
//! This module has no relation to the event-driven trigger listener in
//! `crate::trigger`: it runs a `TriggerAction` set synchronously inside a
//! workflow node (message nodes consume trigger messages through this path).
//! Rejected actions follow the shared `TriggerAction::supported_in` matrix:
//! nested agent execution is refused because message nodes have no parent
//! agent-loop session anchor for input snapshot or write-back, and
//! cold-start actions (`ExecuteWorkflow` / `ExecuteAgent`) are refused because
//! message nodes always run inside an execution and cannot start a fresh run.
//!
//! Module split (each stage owns one concern):
//!
//! - [`runner`]: sandbox script execution abstraction (`ScriptRunner`).
//! - [`context`]: per-execution `TriggerContext` and its builders.
//! - [`events`]: event-bus emission helpers for trigger actions.
//! - [`subworkflow`]: `ExecuteTriggeredSubworkflow` action execution.
//! - [`script_exec`]: `ExecuteScript` action execution (legacy + routed).
//! - [`coordinator`]: `TriggerCoordinator` dispatch and result shaping.

pub mod context;
pub mod coordinator;
pub mod events;
pub mod runner;
pub mod script_exec;
pub mod subworkflow;

pub use context::TriggerContext;
pub use coordinator::TriggerCoordinator;
pub use runner::{SandboxScriptRunner, ScriptRunner};

#[cfg(test)]
mod tests;
