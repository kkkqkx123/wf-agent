use std::collections::HashMap;

use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct TriggerExecutionListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub trigger_name_filter: Option<String>,
    pub execution_id_filter: Option<String>,
    pub workflow_id_filter: Option<String>,
    pub success_filter: Option<bool>,
}

impl From<TriggerExecutionListOptions> for QueryFilter {
    fn from(opts: TriggerExecutionListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.trigger_name_filter {
            filter.add_op(FilterOp::Eq("triggerName".into(), value));
        }
        if let Some(value) = opts.execution_id_filter {
            filter.add_op(FilterOp::Eq("executionId".into(), value));
        }
        if let Some(value) = opts.workflow_id_filter {
            filter.add_op(FilterOp::Eq("workflowId".into(), value));
        }
        if let Some(value) = opts.success_filter {
            filter.add_op(FilterOp::Eq("success".into(), value.to_string()));
        }
        filter
    }
}

pub trait TriggerExecutionStorageAdapter:
    BaseStorageAdapter<wf_types::TriggerExecutionStorageMetadata, TriggerExecutionListOptions>
{
    async fn list_by_trigger(
        &self,
        trigger_name: &str,
    ) -> Result<Vec<wf_types::TriggerExecutionStorageMetadata>, StorageError>;

    async fn list_by_execution(
        &self,
        execution_id: &str,
    ) -> Result<Vec<wf_types::TriggerExecutionStorageMetadata>, StorageError>;

    async fn list_by_workflow(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<wf_types::TriggerExecutionStorageMetadata>, StorageError>;

    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError>;

    async fn cleanup(&self, older_than: i64) -> Result<u64, StorageError>;
}
