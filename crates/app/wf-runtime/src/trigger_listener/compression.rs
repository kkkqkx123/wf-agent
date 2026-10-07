//! The engine's builtin `CONTEXT_COMPRESSION_REQUESTED` signal path.
//!
//! Two responsibilities share this module, split across two types on purpose:
//! `CompressionService` is the thin hook adapter (parse the signal, skip
//! empty snapshots, translate it into a trigger-side request and hand it
//! over, then return); `CompressionPipeline` is the trigger-side runner
//! owning the whole execution (retry loop, degraded fallback, write-back
//! orchestration, terminal events, trigger-state and ledger audit). No
//! execution policy lives in the hook adapter: the emitting execution blocks
//! on its version anchor until the pipeline lands the compression, and a
//! terminal failure either lands a visibly degraded window (the
//! `partial_summary` policy declared by the summary workflow resource) or
//! stops the emitter for external handling.
//!
//! Handoff has two construction-time modes (see `CompressionDispatch` in
//! `service.rs`): direct spawn (tests and listener-less embedding) and the
//! routed mode used in production, where the adapter publishes a
//! snapshot-carrying routed copy of the signal and the listener matches it
//! against the builtin template (`registry.rs`) whose reserved action routes
//! to `CompressionPipeline::run_routed`. User templates never participate in
//! either mode.

mod pipeline;
mod policy;
mod registry;
mod service;
mod signal;
#[cfg(test)]
mod tests;

pub(crate) use pipeline::CompressionPipeline;
pub use pipeline::COMPRESSION_HANDLED_CAPACITY;
pub use policy::CompressionPolicy;
#[cfg(test)]
pub(crate) use registry::builtin_compression_template;
pub(crate) use registry::CompressionRoutedRegistry;
pub use registry::BUILTIN_COMPRESSION_TEMPLATE_NAME;
pub use service::{CompressionService, COMPRESSION_SERVICE_HANDLER_NAME};
