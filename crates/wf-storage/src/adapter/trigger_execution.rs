use std::collections::HashMap;
use std::future::Future;

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
    fn list_by_trigger<'a>(
        &'a self,
        trigger_name: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::TriggerExecutionStorageMetadata>, StorageError>>
           + Send
           + 'a;

    fn list_by_execution<'a>(
        &'a self,
        execution_id: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::TriggerExecutionStorageMetadata>, StorageError>>
           + Send
           + 'a;

    fn list_by_workflow<'a>(
        &'a self,
        workflow_id: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::TriggerExecutionStorageMetadata>, StorageError>>
           + Send
           + 'a;

    fn get_stats<'a>(
        &'a self,
    ) -> impl Future<Output = Result<HashMap<String, u64>, StorageError>> + Send + 'a;

    fn cleanup<'a>(
        &'a self,
        older_than: i64,
    ) -> impl Future<Output = Result<u64, StorageError>> + Send + 'a;
}
