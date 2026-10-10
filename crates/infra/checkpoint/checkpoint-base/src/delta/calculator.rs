//! Diff calculators for structured execution-state snapshots.
//!
//! The workflow and agent snapshots have no shared diff logic left beyond
//! the delta wire shape, so each calculator owns its own branches. Test
//! builders shared by both live in `test_support`.

mod agent;
#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod test_support;
mod workflow;
#[cfg(test)]
mod workflow_tests;

pub use agent::AgentDiffCalculator;
pub use workflow::{WorkflowDiffCalculator, VARIABLE_DELETED_MARKER_KEY};
