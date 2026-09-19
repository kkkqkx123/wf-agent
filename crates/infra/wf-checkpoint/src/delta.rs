//! Delta computation for structured execution-state snapshots.
//!
//! Distinct from layertwine's `engine::diff`, which operates on file text
//! (line-level deltas for the file-edit history engine). This module diffs
//! workflow/agent state snapshots (`Message`, variables, node results) and
//! restores state from delta chains; the two layers share no logic.

pub mod calculator;
pub mod diff;
pub mod restorer;

pub use calculator::{AgentDiffCalculator, WorkflowDiffCalculator};
pub use diff::{CheckpointLoader, DeltaRestorer, DiffCalculator};
pub use restorer::GenericDeltaRestorer;
