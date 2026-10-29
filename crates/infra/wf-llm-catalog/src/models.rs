//! Model-metadata side of the built-in catalog.
//!
//! The data itself lives in `wf-pricing` (embedded LiteLLM table); this
//! module only re-exports the resolver and adds small query helpers so
//! hosts depend on one catalog crate instead of reaching into pricing
//! directly.

pub use wf_pricing::ModelMetadataResolver;

use llm_types::llm::ModelMetadata;

/// Metadata for one model name, if the embedded table knows it.
pub fn model_metadata(model: &str) -> Option<ModelMetadata> {
    ModelMetadataResolver::from_embedded().metadata_for(model)
}

/// Metadata for every model in `models`, in input order. Unknown models
/// are skipped rather than padded with `None`, so callers can tell which
/// names resolved.
pub fn model_metadata_for(models: &[&str]) -> Vec<ModelMetadata> {
    let resolver = ModelMetadataResolver::from_embedded();
    models
        .iter()
        .filter_map(|model| resolver.metadata_for(model))
        .collect()
}