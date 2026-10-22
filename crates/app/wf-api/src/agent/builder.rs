//! Typed agent builders.
//!
//! Construction phases are tracked in the type system with `PhantomData`, so
//! incomplete configurations (a tool config with no tools, a hook without a
//! hook type, an unnamed definition) are unrepresentable. Each phase
//! constructor preserves the invariant, so the type never claims a completed
//! phase it has not actually reached.
//!
//! The builders produce the static artifact types consumed by the engines:
//! [`AvailableTools`], [`AgentHookConfig`],
//! agent definitions and loop configs. Value builders are pure; the
//! execution builder drives a loop through
//! [`crate::agent::agent_execution`].




/// Marker: no model has been assigned to the agent loop config.
#[derive(Debug)]
pub struct LoopEmpty;
/// Marker: a model has been assigned to the agent loop config.
#[derive(Debug)]
pub struct LoopConfigured;
/// Marker: the agent definition has no name yet.
#[derive(Debug)]
pub struct DefUnnamed;
/// Marker: the agent definition has a name.
#[derive(Debug)]
pub struct DefNamed;
/// Marker: the hook has no hook type yet.
#[derive(Debug)]
pub struct HookNoType;
/// Marker: the hook has a hook type.
#[derive(Debug)]
pub struct HookTyped;
/// Marker: the tool config has no tools yet.
#[derive(Debug)]
pub struct ToolEmpty;
/// Marker: the tool config has at least one tool.
#[derive(Debug)]
pub struct ToolBuilt;

mod definition;
mod execution;
mod hook;
#[cfg(test)]
mod tests;
mod tool;
mod loop_config;

pub use definition::AgentDefinitionBuilder;
pub use execution::AgentExecutionBuilder;
pub use hook::AgentHookBuilder;
pub use loop_config::AgentLoopConfigBuilder;
pub use tool::AgentToolConfigBuilder;
