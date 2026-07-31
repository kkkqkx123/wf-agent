use std::collections::HashMap;

use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct UserInteractionListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub execution_id_filter: Option<String>,
    pub status_filter: Option<String>,
    pub interaction_type_filter: Option<String>,
}

impl From<UserInteractionListOptions> for QueryFilter {
    fn from(opts: UserInteractionListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.execution_id_filter {
            filter.add_op(FilterOp::Eq("executionId".into(), value));
        }
        if let Some(value) = opts.status_filter {
            filter.add_op(FilterOp::Eq("status".into(), value));
        }
        if let Some(value) = opts.interaction_type_filter {
            filter.add_op(FilterOp::Eq("interactionType".into(), value));
        }
        filter
    }
}

pub trait UserInteractionStorageAdapter:
    BaseStorageAdapter<wf_types::UserInteractionStorageMetadata, UserInteractionListOptions>
{
    async fn list_by_execution(
        &self,
        execution_id: &str,
    ) -> Result<Vec<wf_types::UserInteractionStorageMetadata>, StorageError>;

    async fn list_by_status(
        &self,
        status: &str,
    ) -> Result<Vec<wf_types::UserInteractionStorageMetadata>, StorageError>;

    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError>;
}
