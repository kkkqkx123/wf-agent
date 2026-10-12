//! `wf-client`: generated HTTP client for `wf-server`.
//!
//! The progenitor-generated API lives in [`generated`] (built from the
//! committed `wf-server/openapi.json` snapshot via `build.rs`; never edit the
//! output by hand). Hand-written layers add connection setup, envelope
//! unwrapping, and SSE streaming:
//!
//! - [`WfClient`] / [`ClientConfig`] (`client.rs`)
//! - [`ClientError`] (`error.rs`)
//! - [`SseFrame`] (`sse.rs`)
//! - [`normalize`] (3.1 to 3.0 transform, shared with `build.rs`)

#[allow(clippy::all, clippy::pedantic, clippy::nursery, dead_code)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/codegen.rs"));
}

pub mod client;
pub mod error;
pub mod normalize;
pub mod sse;

pub use client::{ClientConfig, WfClient, DEFAULT_AGENT, DEFAULT_BASE_URL, DEFAULT_MODEL};
pub use error::{ClientError, ClientResult};
pub use sse::{endpoints, SseFrame};

/// The fully-typed progenitor client and its model namespace, for call sites
/// that want builder-style operations instead of the JSON escape hatch.
pub use generated::{Client as GeneratedClient, types as models};
