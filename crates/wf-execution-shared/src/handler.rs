//! Node handler contract shared across execution engines.
//!
//! The workflow engine (`wf-workflow`) implements one `NodeHandler` per node
//! type and registers the implementations in a [`NodeHandlerRegistry`] which
//! is carried through [`NodeExecutionContext`] so nested executions
//! (subgraphs, fork branches, triggered sub-workflows) resolve the handlers
//! of the same engine.
//!
//! The registry is strongly typed: the shared context stores the concrete
//! registry instead of a `Box<dyn Any>` carrier, so a missing registry is the
//! only runtime failure mode (any other mismatch fails at compile time).
//! Engines convert their internal error types into [`ExecutionSharedError`]
//! at this trait boundary (see `wf_workflow::error` for the workflow engine's
//! conversion).

use std::collections::HashMap;

use async_trait::async_trait;
use wf_types::node::StaticNodeType;

use crate::context::{NodeExecutionContext, NodeExecutionResult};
use crate::error::ExecutionSharedResult;

/// Execute one node of an execution engine.
#[async_trait]
pub trait NodeHandler: Send + Sync {
    fn node_type(&self) -> StaticNodeType;

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> ExecutionSharedResult<NodeExecutionResult>;
}

/// Strongly typed handler registry: maps node types to their handlers.
///
/// Stored in [`NodeExecutionContext::handler_registry`] and shared with
/// nested executions (the `Arc` allows cheap cloning into child execution
/// contexts).
pub type NodeHandlerRegistry = HashMap<StaticNodeType, Box<dyn NodeHandler>>;
