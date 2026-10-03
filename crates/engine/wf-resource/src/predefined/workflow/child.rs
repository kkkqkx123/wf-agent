//! Child-only workflow templates: graphs designed to run as a nested
//! execution (a `SUBGRAPH` child, an `EMBED_GRAPH` inline expansion, or the
//! compression runner), never as a directly executed top-level workflow.
//!
//! Two invocation channels live here, kept as siblings on purpose:
//! `prefetch` is a plain `Start`/`End` child over a JSON input object, so it
//! satisfies both the `SUBGRAPH` and the `EMBED_GRAPH` constraints; the
//! `fold_summary` chain is a message-channel child (`StartFromMessage` /
//! `ContinueFromMessage` plus `triggered_subworkflow_config`) served only by
//! the compression runner. `summary_stage` is not a template at all, only the
//! node-fragment helpers the fold chain is built from.

pub mod fold_summary;
pub mod prefetch;

mod summary_stage;
