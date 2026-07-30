use std::collections::HashMap;

use crate::adapter::base::BaseStorageAdapter;
use crate::domain::QueryFilter;
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
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(name) = opts.trigger_name_filter {
            filter.fields.insert("triggerName".to_string(), name);
        }
        if let Some(eid) = opts.execution_id_filter {
            filter.fields.insert("executionId".to_string(), eid);
        }
        if let Some(wid) = opts.workflow_id_filter {
            filter.fields.insert("workflowId".to_string(), wid);
        }
        if let Some(success) = opts.success_filter {
            filter
                .fields
                .insert("success".to_string(), success.to_string());
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

    async fn get_stats(
        &self,
    ) -> Result<HashMap<String, u64>, StorageError>;

    async fn cleanup(&self, older_than: i64) -> Result<u64, StorageError>;
}
