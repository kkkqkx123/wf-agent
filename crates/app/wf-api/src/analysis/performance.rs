//! Per-execution performance profiling.

mod analyze;
mod bottlenecks;
mod compare;
mod helpers;
mod iteration;
#[cfg(test)]
mod tests;
mod profile;
mod summary;
mod types;

pub use types::*;

pub use analyze::analyze_performance;
pub use profile::profile;
pub use summary::get_performance_summary;
pub use bottlenecks::identify_bottlenecks;
pub use iteration::get_iteration_comparison;
pub use compare::compare;
