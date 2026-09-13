//! Event-driven trigger subsystem (engine-owned listener core).
//!
//! The listener loop lives here because both workflow and agent events flow
//! through the same `EventBus`, but the concrete action runners live in
//! `wf-runtime` (`TriggerActionRouter`: sub-workflow / agent / creation /
//! in-context). `wf-agent` owns only the child-execution target
//! (`TriggeredAgentExecutionManager`); it never runs this listener directly.
//!
//! Grouped here: the listener orchestration loop and the stages it wires
//! together, the ports through which the machinery reaches runtime services,
//! the reserved trigger protocol contract, and the records a firing leaves
//! behind for checkpoint audit.
//!
//! - [`listener`]: the orchestration loop (event loop, dispatch loop, action
//!   spawn);
//! - [`matcher`]: which templates are candidates for one event;
//! - [`arbiter`]: which candidates win their competition scope;
//! - [`governor`]: whether a winner may fire right now (re-entrancy guard
//!   and `max_triggers` budget);
//! - [`subscription`]: the event-bus fan-in feeding the loop;
//! - [`ports`]: the runtime services the listener is assembled from;
//! - [`internal`]: reserved protocol variables and internal signal helpers;
//! - [`states`]: per-execution trigger firing records for checkpoint audit.

pub mod arbiter;
pub mod governor;
pub mod internal;
pub mod listener;
pub mod matcher;
pub mod ports;
pub mod states;
pub mod subscription;
#[cfg(test)]
pub(crate) mod test_fixtures;

pub use listener::TriggerEventListener;
pub use ports::{SubworkflowRunner, TriggerActionRunner, TriggerTemplateRegistry};
pub use states::{TriggerStateRecord, TriggerStateRegistry};
