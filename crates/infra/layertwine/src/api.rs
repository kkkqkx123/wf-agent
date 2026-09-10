//! In-process access layer for Layertwine operations.
//!
//! Provides the `ApiService` facade over the storage engine for
//! same-process callers (notably `wf-checkpoint`) and the test suite.
//! There are no network transports: HTTP, gRPC, CLI and the standalone
//! binary were removed when Layertwine became an infra-only library crate.

pub mod service;
pub mod types;

pub use service::{ApiService, ServiceConfig};
pub use types::*;
