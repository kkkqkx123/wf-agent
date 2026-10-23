//! Execution graph / decision-point analysis.

mod alternatives;
mod analysis;
mod efficiency;
mod paths;
mod probability;
mod recording;
#[cfg(test)]
mod tests;
mod types;

pub use types::*;

pub use alternatives::get_alternative_paths;
pub use analysis::analyze;
pub(crate) use analysis::resolve_graph;
pub use efficiency::{analyze_efficiency, get_slow_nodes};
pub use paths::{analyze_decision_points, enumerate_paths, reachable_nodes};
pub use probability::get_path_probability_analysis;
pub use recording::{clear_execution_data, record_execution_graph};
