//! Trigger types.
//!
//! "Trigger" refers to the **event-driven trigger** only: event → template
//! matching → action execution (`TriggerTemplate` / `TriggerCondition` /
//! `TriggerAction`). The listener core lives in `wf-workflow`; concrete
//! actions are executed by runners in `wf-runtime`.
//!
//! Related but distinct concepts (kept separate, do not confuse with
//! triggers):
//!
//! - **hook**: `wf_types::hook` vocabulary plus the synchronous fire
//!   pipeline (`wf-execution-shared`); a fire publishes one `HOOK_TRIGGERED`
//!   audit event that trigger templates may match. Handlers observe at every
//!   point and may gate only the two wired points (`BEFORE_EXECUTE` /
//!   `BEFORE_TOOL_CALL`) by returning `Veto`; trigger actions themselves
//!   always run asynchronously after the fire and never block execution;
//! - **approval**: the front-gating mechanism for tool calls; trigger
//!   actions always run after the fact and cannot gate;
//!
//! - **message node**: `START_FROM_MESSAGE` / `CONTINUE_FROM_MESSAGE`
//!   workflow nodes (wf-workflow `handler/message_node.rs`) that consume a
//!   trigger message and execute the same `TriggerAction` set synchronously
//!   in-node;
//! - **checkpoint timing**: `CheckpointTiming` (wf-types `checkpoint`
//!   module), a checkpoint *timing* concept unrelated to events.
pub mod config;
pub mod execution;
pub mod schedule;
pub mod scope;
pub mod template;

pub use config::*;
pub use execution::*;
pub use schedule::*;
pub use scope::*;
pub use template::*;
