//! Workflow node-level iteration analysis.

mod llm;
mod optimization;
mod path;
mod records;
mod stats;
#[cfg(test)]
mod tests;
mod tool_dependencies;
mod types;

pub use types::*;

pub use llm::get_llm_reasoning_path;
pub use optimization::get_optimization_opportunities;
pub use path::get_execution_path;
pub use records::{get_execution_node_analyses, get_node_analysis};
pub use stats::{get_failed_nodes, get_node_execution_stats, get_node_executions_by_type};
pub use tool_dependencies::get_tool_dependency_chain;
