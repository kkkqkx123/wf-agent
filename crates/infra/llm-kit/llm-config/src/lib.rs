// Configuration layer: profile, provider and model-catalog registries plus
// request merging.
pub mod catalog;
pub mod merge;
pub mod profile;
pub mod provider;

pub use catalog::{ModelCatalog, DEFAULT_MODELS_JSON_PATH, DEFAULT_MODELS_PATH};
pub use merge::merge_request;
pub use profile::ProfileManager;
pub use provider::{apply_provider_defaults, ProviderDefinitionRegistry};
