//! Application-facing workflow execution API, split by concern. This file
//! keeps the module root so external paths
//! (`crate::workflow::workflow_execution::...`) stay unchanged.
//!
//! - `lifecycle`: execute / stream / pause / resume / cancel / status
//! - `checkpoint`: checkpoint create / restore and snapshot building
//! - `graph`: definition-to-graph conversion and validation entry
//! - `summary`: execution list digests

mod checkpoint;
mod graph;
mod lifecycle;
#[cfg(test)]
mod tests;
mod summary;

pub use checkpoint::{create_checkpoint, restore_and_resume, restore_checkpoint, RestoredCheckpoint};
pub use graph::{definition_to_graph, resolve_graph};
pub use lifecycle::{
    cancel, execute, pause, resume, status, stream, ExecuteWorkflowParams,
    DEFAULT_EXECUTION_TIMEOUT_MS,
};
pub use summary::{execution_summaries, ExecutionSummary};

pub use crate::workflow::composition::{
    apply_workflow_config_defaults, empty_options, resolve_options as merge_workflow_options,
};
