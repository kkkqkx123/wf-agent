use std::collections::HashMap;
use std::future::Future;

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
    fn list_by_execution<'a>(
        &'a self,
        execution_id: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::UserInteractionStorageMetadata>, StorageError>>
           + Send
           + 'a;

    fn list_by_status<'a>(
        &'a self,
        status: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::UserInteractionStorageMetadata>, StorageError>>
           + Send
           + 'a;

    fn get_stats<'a>(
        &'a self,
    ) -> impl Future<Output = Result<HashMap<String, u64>, StorageError>> + Send + 'a;
}
