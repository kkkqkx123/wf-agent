pub mod agent;
pub mod metrics;
pub mod task;
pub mod workflow;

use std::sync::Arc;

use wf_resource::registrar::Registries;
use wf_resource::starter::BundleRegistry;
use wf_storage::context::StorageContext;
use wf_storage::error::StorageError;

pub use wf_storage::adapter::base::ListOptions;
pub use wf_storage::domain::QueryFilter;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("Not found: {entity_type} [{id}]")]
    NotFound { entity_type: String, id: String },
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Already exists: {entity_type} [{id}]")]
    AlreadyExists { entity_type: String, id: String },
}

pub struct ApiContext {
    pub storage: StorageContext,
    pub registries: Arc<Registries>,
    pub bundles: Arc<BundleRegistry>,
}

impl ApiContext {
    pub fn new(storage: StorageContext, registries: Arc<Registries>, bundles: Arc<BundleRegistry>) -> Self {
        Self { storage, registries, bundles }
    }
}

pub(crate) fn not_found(entity_type: &str, id: &str) -> ApiError {
    ApiError::NotFound {
        entity_type: entity_type.to_string(),
        id: id.to_string(),
    }
}
