//! Host-side bridge for plugin-contributed retrieval extensions.
//!
//! Registration state lives here so the runtime bridge can sync manager
//! entries without the pipeline depending on the plugin engine. Every
//! suggestion is validated and any failure falls back to the built-in path.
//!
//! Vector encodings are typed: plugins emit a named schema with a vector
//! kind and a shape-valid payload, which the ingest and query paths use for
//! real writes and searches. Vector and keyword backend selection, embedding,
//! rerank, chunking, fusion and rewrite choices stay host-owned.
//!
//! Retrieval extensions are advisory: metadata extractors suggest ingest
//! metadata, chunk splitters suggest content pieces, route selectors choose
//! retrieval scopes, post-fusion shaping reshapes fused hits and eval judges
//! score answers. Pipelines consume them through explicit config names
//! (`metadata_extractor`, `chunk_splitter`, `route_selector`,
//! `postprocessor`); unset names keep built-in behavior. Eval judges stay
//! host-called through the judge helper since retrieval has no answer text.
//! Only the native backend dispatches retrieval extensions; Lua/Wasm hosts
//! expose no retrieval channel.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use keyword_contracts::AnalyzerRegistry;
use vector_contracts::{validate_named_data, VectorData, VectorKind, VectorSpaceSpec};
use wf_plugin_sdk::{
    PluginChunkSplitter, PluginEvalJudge, PluginMetadataExtractor, PluginPostprocessor,
    PluginRouteSelector, PluginTextAnalyzer, PluginVectorEncoding,
};

use crate::error::Result;
use crate::retriever::RetrievedChunk;

mod analyzer;
mod encoding;
mod eval;
mod ingest;
mod retrieval;
mod routing;

pub use analyzer::{register_plugin_analyzer, unregister_plugin_analyzer};
pub use encoding::{encode_named_with_registered, encode_with_registered};
pub use eval::judge_with_registered;
pub use ingest::{extract_with_registered, split_with_registered};
pub use retrieval::apply_registered_postprocessor;
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

/// Host-owned registry for retrieval extensions.
///
/// Hosts hold one instance per configuration scope and pass it into pipelines.
/// All lookups and executions stay on the instance; no process-wide state is
/// consulted.
#[derive(Default)]
pub struct RetrievalExtensions {
    analyzers: Arc<AnalyzerRegistry>,
    vector_encodings: Mutex<HashMap<String, Arc<dyn PluginVectorEncoding>>>,
    metadata_extractors: Mutex<HashMap<String, Arc<dyn PluginMetadataExtractor>>>,
    chunk_splitters: Mutex<HashMap<String, Arc<dyn PluginChunkSplitter>>>,
    route_selectors: Mutex<HashMap<String, Arc<dyn PluginRouteSelector>>>,
    postprocessors: Mutex<HashMap<String, Arc<dyn PluginPostprocessor>>>,
    eval_judges: Mutex<HashMap<String, Arc<dyn PluginEvalJudge>>>,
}

impl RetrievalExtensions {
    /// Empty registry with no extensions registered.
    pub fn new() -> Self {
        Self::default()
    }

    /// Analyzer registry belonging to this host scope.
    pub fn analyzer_registry(&self) -> Arc<AnalyzerRegistry> {
        self.analyzers.clone()
    }

    /// Finds an analyzer registered in this host scope.
    pub fn find_analyzer(&self, name: &str) -> Option<keyword_contracts::CustomAnalyzer> {
        self.analyzers.find(name)
    }

    /// Runs the configured selector once and applies its fallback rules.
    /// Unknown names fail; execution errors and empty suggestions keep every
    /// candidate so callers can apply their own secondary policy.
    pub fn select(
        &self,
        name: Option<&str>,
        query: &str,
        candidates: &[String],
    ) -> Result<Vec<String>> {
        let Some(name) = name.filter(|name| !name.trim().is_empty()) else {
            return Ok(candidates.to_vec());
        };
        let Some(selector) = self.find_route_selector(name) else {
            return Err(crate::error::RagError::InvalidInput(format!(
                "unknown route selector '{name}'"
            )));
        };
        let selected = match routing::poll_route_selector(selector.as_ref(), query, candidates) {
            Ok(selected) => selected,
            Err(error) => {
                tracing::warn!(selector = %name, error = %error, "route selector failed; keeping all targets");
                return Ok(candidates.to_vec());
            }
        };
        let kept: Vec<String> = selected
            .into_iter()
            .filter(|item| candidates.iter().any(|candidate| candidate == item))
            .collect();
        if kept.is_empty() {
            return Ok(candidates.to_vec());
        }
        Ok(kept)
    }

    /// Registers a vector encoding, replacing any prior entry.
    pub fn register_vector_encoding(&self, name: &str, encoding: Arc<dyn PluginVectorEncoding>) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.vector_encodings.lock() {
            guard.insert(trimmed.to_string(), encoding);
        }
    }

    /// Removes a vector encoding.
    pub fn unregister_vector_encoding(&self, name: &str) {
        if let Ok(mut guard) = self.vector_encodings.lock() {
            guard.remove(name.trim());
        }
    }

    /// Finds a vector encoding by name.
    pub fn find_vector_encoding(&self, name: &str) -> Option<Arc<dyn PluginVectorEncoding>> {
        self.vector_encodings
            .lock()
            .ok()
            .and_then(|guard| guard.get(name.trim()).cloned())
    }

    /// Clears vector encodings.
    pub fn clear_vector_encodings(&self) {
        if let Ok(mut guard) = self.vector_encodings.lock() {
            guard.clear();
        }
    }

    /// Registers a metadata extractor, replacing any prior entry.
    pub fn register_metadata_extractor(
        &self,
        name: &str,
        extractor: Arc<dyn PluginMetadataExtractor>,
    ) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.metadata_extractors.lock() {
            guard.insert(trimmed.to_string(), extractor);
        }
    }

    /// Removes a metadata extractor.
    pub fn unregister_metadata_extractor(&self, name: &str) {
        if let Ok(mut guard) = self.metadata_extractors.lock() {
            guard.remove(name.trim());
        }
    }

    /// Finds a metadata extractor by name.
    pub fn find_metadata_extractor(&self, name: &str) -> Option<Arc<dyn PluginMetadataExtractor>> {
        self.metadata_extractors
            .lock()
            .ok()
            .and_then(|guard| guard.get(name.trim()).cloned())
    }

    /// Clears metadata extractors.
    pub fn clear_metadata_extractors(&self) {
        if let Ok(mut guard) = self.metadata_extractors.lock() {
            guard.clear();
        }
    }

    /// Registers a chunk splitter, replacing any prior entry.
    pub fn register_chunk_splitter(&self, name: &str, splitter: Arc<dyn PluginChunkSplitter>) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.chunk_splitters.lock() {
            guard.insert(trimmed.to_string(), splitter);
        }
    }

    /// Removes a chunk splitter.
    pub fn unregister_chunk_splitter(&self, name: &str) {
        if let Ok(mut guard) = self.chunk_splitters.lock() {
            guard.remove(name.trim());
        }
    }

    /// Finds a chunk splitter by name.
    pub fn find_chunk_splitter(&self, name: &str) -> Option<Arc<dyn PluginChunkSplitter>> {
        self.chunk_splitters
            .lock()
            .ok()
            .and_then(|guard| guard.get(name.trim()).cloned())
    }

    /// Clears chunk splitters.
    pub fn clear_chunk_splitters(&self) {
        if let Ok(mut guard) = self.chunk_splitters.lock() {
            guard.clear();
        }
    }

    /// Registers a route selector, replacing any prior entry.
    pub fn register_route_selector(&self, name: &str, selector: Arc<dyn PluginRouteSelector>) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.route_selectors.lock() {
            guard.insert(trimmed.to_string(), selector);
        }
    }

    /// Removes a route selector.
    pub fn unregister_route_selector(&self, name: &str) {
        if let Ok(mut guard) = self.route_selectors.lock() {
            guard.remove(name.trim());
        }
    }

    /// Finds a route selector by name.
    pub fn find_route_selector(&self, name: &str) -> Option<Arc<dyn PluginRouteSelector>> {
        self.route_selectors
            .lock()
            .ok()
            .and_then(|guard| guard.get(name.trim()).cloned())
    }

    /// Clears route selectors.
    pub fn clear_route_selectors(&self) {
        if let Ok(mut guard) = self.route_selectors.lock() {
            guard.clear();
        }
    }

    /// Registers a post-fusion shaping step, replacing any prior entry.
    pub fn register_postprocessor(&self, name: &str, postprocessor: Arc<dyn PluginPostprocessor>) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.postprocessors.lock() {
            guard.insert(trimmed.to_string(), postprocessor);
        }
    }

    /// Removes a post-fusion shaping step.
    pub fn unregister_postprocessor(&self, name: &str) {
        if let Ok(mut guard) = self.postprocessors.lock() {
            guard.remove(name.trim());
        }
    }

    /// Finds a post-fusion shaping step by name.
    pub fn find_postprocessor(&self, name: &str) -> Option<Arc<dyn PluginPostprocessor>> {
        self.postprocessors
            .lock()
            .ok()
            .and_then(|guard| guard.get(name.trim()).cloned())
    }

    /// Clears post-fusion shaping steps.
    pub fn clear_postprocessors(&self) {
        if let Ok(mut guard) = self.postprocessors.lock() {
            guard.clear();
        }
    }

    /// Registers an eval judge, replacing any prior entry.
    pub fn register_eval_judge(&self, name: &str, judge: Arc<dyn PluginEvalJudge>) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.eval_judges.lock() {
            guard.insert(trimmed.to_string(), judge);
        }
    }

    /// Removes an eval judge.
    pub fn unregister_eval_judge(&self, name: &str) {
        if let Ok(mut guard) = self.eval_judges.lock() {
            guard.remove(name.trim());
        }
    }

    /// Finds an eval judge by name, including built-in judges.
    pub fn find_eval_judge(&self, name: &str) -> Option<Arc<dyn PluginEvalJudge>> {
        let trimmed = name.trim();
        if let Some(judge) = self
            .eval_judges
            .lock()
            .ok()
            .and_then(|guard| guard.get(trimmed).cloned())
        {
            return Some(judge);
        }
        match trimmed {
            crate::builtin_judge::BUILTIN_HEURISTIC_JUDGE => {
                Some(Arc::new(crate::builtin_judge::HeuristicJudge))
            }
            _ => None,
        }
    }

    /// Clears eval judges.
    pub fn clear_eval_judges(&self) {
        if let Ok(mut guard) = self.eval_judges.lock() {
            guard.clear();
        }
    }
}

#[cfg(test)]
mod test;
