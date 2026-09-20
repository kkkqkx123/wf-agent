//! Composition boundary: caller intent plus registered defaults.
//!
//! Execution APIs stay pure executors. Every composition entry point (HTTP
//! handlers, CLI frontends, builders) resolves through this module so agent,
//! workflow, hook, trigger and node defaults share one caller-wins rule.

pub mod agent;
pub mod hook;
pub mod node;
pub mod trigger;
pub mod workflow;
