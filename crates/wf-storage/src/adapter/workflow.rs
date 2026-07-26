use std::collections::HashMap;
use crate::error::StorageError;
use crate::adapter::base::BaseStorageAdapter;

#[derive(Debug, Clone, Default)]
pub struct WorkflowListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub name_filter: Option<String>,
    pub type_filter: Option<String>,
}

pub trait WorkflowStorageAdapter: BaseStorageAdapter<wf_types::WorkflowDefinition, WorkflowListOptions> {
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

    async fn delete_version(
        &self,
        workflow_id: &str,
        version: &str,
    ) -> Result<bool, StorageError>;
}
