//! Agent domain query APIs: execution, loop registry, graph, checkpoints,
//! error analysis, performance and variables.

#[allow(clippy::module_inception)]
pub mod agent;
pub mod agent_checkpoint;
pub mod agent_config;
pub mod agent_draft;
pub mod agent_error_analysis;
pub mod agent_execution;
pub mod agent_execution_registry;
pub mod agent_graph;
pub mod agent_loop_registry;
pub mod agent_message;
pub mod agent_performance;
pub mod agent_user_interaction;
pub mod agent_variable;
