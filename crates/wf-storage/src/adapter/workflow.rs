use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::collections::HashMap;

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
    async fn update_metadata(
        &self,
        id: &str,
        metadata: &HashMap<String, serde_json::Value>,
    ) -> Result<(), StorageError>;

    async fn save_version(
        &self,
        workflow_id: &str,
        version: &str,
        template: &wf_types::WorkflowDefinition,
    ) -> Result<(), StorageError>;

    async fn list_versions(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<wf_types::WorkflowDefinition>, StorageError>;

    async fn load_version(
        &self,
        workflow_id: &str,
        version: &str,
    ) -> Result<Option<wf_types::WorkflowDefinition>, StorageError>;

    async fn delete_version(&self, workflow_id: &str, version: &str) -> Result<bool, StorageError>;
}
