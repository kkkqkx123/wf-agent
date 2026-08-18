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
//! - **message node**: `START_FROM_MESSAGE` / `CONTINUE_FROM_MESSAGE`
//!   workflow nodes (wf-workflow `handler/message_node.rs`) that consume a
//!   trigger message and execute the same `TriggerAction` set synchronously
//!   in-node;
//! - **checkpoint timing**: `CheckpointTiming` (wf-types `checkpoint`
//!   module), a checkpoint *timing* concept unrelated to events.
pub mod config;
pub mod execution;
pub mod template;

pub use config::*;
pub use execution::*;
pub use template::*;
