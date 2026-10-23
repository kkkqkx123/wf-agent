//! Per-execution performance profiling.

mod analyze;
mod bottlenecks;
mod compare;
mod helpers;
mod iteration;
mod profile;
mod summary;
#[cfg(test)]
mod tests;
mod types;

pub use types::*;

pub use analyze::analyze_performance;
pub use bottlenecks::identify_bottlenecks;
pub use compare::compare;
pub use iteration::get_iteration_comparison;
pub use profile::profile;
pub use summary::get_performance_summary;
