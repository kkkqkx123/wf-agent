//! Unified cross-resource search over workflows, executions, tasks,
//! checkpoints, events and agent loops.
//!
//! Types are searched in parallel; a failure of one resource type degrades to
//! empty results for that type instead of failing the whole search. Results
//! are collected first and then sorted, so the outcome never depends on
//! registry iteration order.

mod agent_loops;
mod checkpoints;
mod events;
mod executions;
mod messages;
mod orchestrator;
mod scoring;
mod tasks;
#[cfg(test)]
mod tests;
mod types;
mod workflows;

pub use orchestrator::search;
pub use types::{SearchOptions, SearchResourceType, SearchResult, SearchResultItem};
