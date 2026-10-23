//! `wf-client`: generated HTTP client for `wf-server`.
//!
//! The progenitor-generated API lives in `generated` (built from the
//! committed `wf-server/openapi.json` snapshot via `build.rs`; never edit the
//! output by hand). Hand-written layers add connection setup, envelope
//! unwrapping, and SSE streaming:
//!
//! - [`WfClient`] / [`ClientConfig`] (`client.rs`): strong-type-first
//!   helpers over `generated`, plus one documented JSON escape hatch.
//! - [`ClientError`] (`error.rs`): shared status and envelope mapping.
//! - [`SseFrame`] (`sse.rs`): streaming endpoints the generator skips.

#[allow(clippy::all, clippy::pedantic, clippy::nursery, dead_code)]
pub(crate) mod generated {
    include!(concat!(env!("OUT_DIR"), "/codegen.rs"));
}

pub mod client;
pub mod error;
#[allow(dead_code)]
pub(crate) mod normalize;
pub mod remote;
pub mod sse;

pub use client::{ClientConfig, WfClient, DEFAULT_BASE_URL};
pub use error::{ClientError, ClientResult};
pub use remote::{resolve_base_url, tunnel_base_url_from_env, SshTunnelConfig};
pub use sse::{endpoints, SseFrame};

/// The fully-typed progenitor client and its model namespace, for call sites
/// that want builder-style operations instead of the JSON escape hatch.
pub use generated::{types as models, Client as GeneratedClient};
