//! Host-side bridge for plugin-contributed retrieval extensions.
//!
//! Registration state lives here so the runtime bridge can sync manager
//! entries without the pipeline depending on the plugin engine. Every
//! suggestion is validated and any failure falls back to the built-in path.
//!
//! Vector encodings are typed: plugins emit a named schema with a vector
//! kind and a shape-valid payload, which the ingest and query paths use for
//! real writes and searches. Snapshot policies are
//! reservation-only declarations; execution stays host-owned and no
//! pipeline reads them. Vector and keyword backend selection, embedding,
//! rerank, chunking, fusion and rewrite choices stay host-owned.
//!
//! Retrieval extensions are advisory: metadata extractors suggest ingest
//! metadata, chunk splitters suggest content pieces, route selectors choose
//! retrieval scopes, postprocessors reshape fused hits and eval judges score
//! answers. Pipelines consume them through explicit config names
//! (`metadata_extractor`, `chunk_splitter`, `route_selector`,
//! `postprocessor`); unset names keep built-in behavior. Eval judges stay
//! host-called through the judge helper since retrieval has no answer text.
//! Only the native backend dispatches retrieval extensions; Lua/Wasm hosts
//! expose no retrieval channel.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

use vector_contracts::{validate_named_data, VectorData, VectorKind, VectorSpaceSpec};
use wf_plugin_sdk::{
    PluginChunkSplitter, PluginEvalJudge, PluginMetadataExtractor, PluginPostprocessor,
    PluginRetrievalAdvisor, PluginRouteSelector, PluginTextAnalyzer, PluginVectorEncoding,
};

use crate::retriever::RetrievedChunk;

mod analyzer;
mod encoding;
mod eval;
mod ingest;
mod retrieval;
mod routing;
mod snapshot;

pub use analyzer::{register_plugin_analyzer, unregister_plugin_analyzer};
pub use encoding::{
    clear_vector_encodings, encode_named_with_registered, encode_with_registered,
    find_vector_encoding, register_vector_encoding, unregister_vector_encoding,
};
pub use eval::{
    clear_eval_judges, find_eval_judge, judge_with_registered, register_eval_judge,
    unregister_eval_judge, JudgeScore,
};
pub use ingest::{
    clear_chunk_splitters, clear_metadata_extractors, extract_with_registered, find_chunk_splitter,
    find_metadata_extractor, register_chunk_splitter, register_metadata_extractor,
    split_with_registered, unregister_chunk_splitter, unregister_metadata_extractor, SplitPiece,
};
pub use retrieval::{
    apply_registered_advisor, apply_registered_postprocessor, apply_retrieval_advisor,
    clear_postprocessors, clear_retrieval_advisors, find_postprocessor, find_retrieval_advisor,
    register_postprocessor, register_retrieval_advisor, unregister_postprocessor,
    unregister_retrieval_advisor,
};
pub(crate) use routing::poll_route_selector;
pub use routing::{
    apply_registered_selector, clear_route_selectors, find_route_selector, register_route_selector,
    unregister_route_selector,
};
pub use snapshot::{
    clear_snapshot_policies, find_snapshot_policy, is_valid_snapshot_policy,
    register_snapshot_policy, unregister_snapshot_policy,
};

/// Protocol version sent with every retrieval advisor input.
pub const RETRIEVAL_ADVISOR_PROTOCOL_VERSION: u32 = 1;
/// Protocol version sent with every route selector input.
pub const ROUTE_SELECTOR_PROTOCOL_VERSION: u32 = 1;
/// Protocol version sent with every postprocessor input.
pub const POSTPROCESSOR_PROTOCOL_VERSION: u32 = 1;
/// Protocol version sent with every eval judge input.
pub const EVAL_JUDGE_PROTOCOL_VERSION: u32 = 1;
/// Schema every single-space vector encoder output must declare.
pub const VECTOR_ENCODING_SCHEMA: &str = "vector-encoding/v1";
/// Schema of an encoder returning one vector per named space.
pub const NAMED_VECTOR_ENCODING_SCHEMA: &str = "named-vector-encoding/v1";

/// Typed output of a named-space vector encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedEncodedVector {
    /// Validated vector for each declared named space.
    pub spaces: BTreeMap<String, VectorData>,
    /// Encoder name declared for each space by the plugin output.
    pub encoders: BTreeMap<String, String>,
    /// Schema declared by the plugin output.
    pub schema: String,
    /// Registered encoder that produced the payload.
    pub encoder: String,
}

/// Typed vector encoding produced by a registered encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodedVector {
    /// Kind declared by the encoder and expected by the caller.
    pub kind: VectorKind,
    /// Shape-validated vector payload.
    pub data: VectorData,
    /// Schema declared by the encoder output.
    pub schema: String,
    /// Name of the encoder that produced the payload.
    pub encoder: String,
}

/// Serializes tests that mutate the process-wide plugin registries. A
/// parallel test's `clear_*` otherwise races another test's `register_*`
/// plus in-flight use and flakes the suite.
#[cfg(test)]
pub(crate) async fn test_registry_guard() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

#[cfg(test)]
mod test;
