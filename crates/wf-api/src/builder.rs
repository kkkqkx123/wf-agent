//! Typed builders for workflow definitions.
//!
//! Mirrors the TS `WorkflowBuilder` / `NodeBuilder` contract (see
//! `packages/sdk/api/workflow/builders`) without the OOP scaffolding:
//! construction phases are tracked in the type system with `PhantomData`, so
//! invalid states (a node without a type, a workflow being built before any
//! node exists) are unrepresentable. A workflow built through
//! [`WorkflowBuilder`] is validated with the full graph validator before it
//! can be saved.

pub mod node;
pub mod workflow;

pub use node::{NoType, NodeBuilder, Typed};
pub use workflow::{Building, Empty, WorkflowBuilder};
