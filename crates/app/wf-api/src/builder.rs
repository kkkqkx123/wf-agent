//! Typed builders for workflow definitions.
//!
//! Construction phases are tracked in the type system with `PhantomData`, so
//! invalid states (a node without a type, a workflow being built before any
//! node exists) are unrepresentable. A workflow built through
//! [`WorkflowBuilder`] is validated with the full graph validator before it
//! can be saved.

pub mod agent;
pub mod execution;
pub mod node;
pub mod template;
pub mod workflow;

pub use agent::{
    AgentDefinitionBuilder, AgentExecutionBuilder, AgentHookBuilder, AgentLoopConfigBuilder,
    AgentToolConfigBuilder,
};
pub use execution::{ExecutionBuilder, ExecutionResult};
pub use node::{NoType, NodeBuilder, Typed};
pub use template::{NodeTemplateBuilder, TriggerTemplateBuilder};
pub use workflow::{Building, Empty, WorkflowBuilder};
