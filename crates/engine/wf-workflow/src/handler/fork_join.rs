pub mod branch;
pub mod events;
pub mod fork;
pub mod graph;
pub mod join;
pub mod merge;

pub use fork::ForkHandler;
pub use graph::{find_fork_by_path, find_fork_node};
pub use join::JoinHandler;

/// Variable-map key carrying one fork's branch results to its JOIN.
///
/// Branches run in child coordinators whose node outputs never reach the
/// parent graph, so the fork publishes the aggregated results under this key
/// (keyed by fork node id) and the JOIN reads it back. Prefer the live
/// [`wf_execution_shared::fork::ForkRegistry`] where available; this key is
/// the compatibility fallback (e.g. JOIN reached without a fork after a
/// checkpoint resume).
pub fn fork_output_key(fork_id: &str) -> String {
    format!("__fork_outputs_{fork_id}")
}
