use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::collections::HashMap;
use std::future::Future;

#[derive(Debug, Clone, Default)]
pub struct WorkflowListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub name_filter: Option<String>,
    pub type_filter: Option<String>,
}

impl From<WorkflowListOptions> for QueryFilter {
    fn from(opts: WorkflowListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.type_filter {
            filter.add_op(FilterOp::Eq("type".into(), value));
        }
        if let Some(value) = opts.name_filter {
            filter.add_op(FilterOp::Eq("name".into(), value));
        }
        filter
    }
}

pub trait WorkflowStorageAdapter:
    BaseStorageAdapter<wf_types::WorkflowDefinition, WorkflowListOptions>
{
    fn update_metadata<'a>(
        &'a self,
        id: &'a str,
        metadata: &'a HashMap<String, serde_json::Value>,
    ) -> impl Future<Output = Result<(), StorageError>> + Send + 'a;

    fn save_version<'a>(
        &'a self,
        workflow_id: &'a str,
        version: &'a str,
        template: &'a wf_types::WorkflowDefinition,
    ) -> impl Future<Output = Result<(), StorageError>> + Send + 'a;

    fn list_versions<'a>(
        &'a self,
        workflow_id: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::WorkflowDefinition>, StorageError>> + Send + 'a;

    fn load_version<'a>(
        &'a self,
        workflow_id: &'a str,
        version: &'a str,
    ) -> impl Future<Output = Result<Option<wf_types::WorkflowDefinition>, StorageError>> + Send + 'a;

    fn delete_version<'a>(
        &'a self,
        workflow_id: &'a str,
        version: &'a str,
    ) -> impl Future<Output = Result<bool, StorageError>> + Send + 'a;
}
