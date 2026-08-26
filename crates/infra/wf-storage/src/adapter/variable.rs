use std::future::Future;

use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct VariableListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub scope_filter: Option<String>,
    pub execution_id_filter: Option<String>,
}

impl From<VariableListOptions> for QueryFilter {
    fn from(opts: VariableListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.scope_filter {
            filter.add_op(FilterOp::Eq("scope".into(), value));
        }
        if let Some(value) = opts.execution_id_filter {
            filter.add_op(FilterOp::Eq("executionId".into(), value));
        }
        filter
    }
}

pub trait VariableStorageAdapter:
    BaseStorageAdapter<wf_types::VariableStorageMetadata, VariableListOptions>
{
    /// Load one variable by its deterministic composite id.
    fn get_by_scope<'a>(
        &'a self,
        execution_id: Option<&'a str>,
        scope: &'a str,
        name: &'a str,
    ) -> impl Future<Output = Result<Option<wf_types::VariableStorageMetadata>, StorageError>> + Send + 'a;

    fn list_by_execution<'a>(
        &'a self,
        execution_id: &'a str,
        options: Option<VariableListOptions>,
    ) -> impl Future<Output = Result<Vec<wf_types::VariableStorageMetadata>, StorageError>> + Send + 'a;

    fn list_by_scope<'a>(
        &'a self,
        scope: &'a str,
        options: Option<VariableListOptions>,
    ) -> impl Future<Output = Result<Vec<wf_types::VariableStorageMetadata>, StorageError>> + Send + 'a;

    /// Delete every variable of an execution scope.
    fn delete_by_execution<'a>(
        &'a self,
        execution_id: &'a str,
    ) -> impl Future<Output = Result<u64, StorageError>> + Send + 'a;
}
