use std::collections::HashMap;
use crate::error::StorageError;
use crate::adapter::base::BaseStorageAdapter;

#[derive(Debug, Clone, Default)]
pub struct CheckpointListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub entity_type_filter: Option<String>,
    pub entity_id_filter: Option<String>,
}

pub trait CheckpointStorageAdapter:
    BaseStorageAdapter<wf_types::Checkpoint, CheckpointListOptions>
{
    async fn list_by_entity(
        &self,
        entity_id: &str,
        entity_type: &str,
    ) -> Result<Vec<wf_types::Checkpoint>, StorageError>;

    async fn get_latest_by_entity(
        &self,
        entity_id: &str,
        entity_type: &str,
    ) -> Result<Option<wf_types::Checkpoint>, StorageError>;

    async fn delete_by_entity(
        &self,
        entity_id: &str,
        entity_type: &str,
    ) -> Result<u64, StorageError>;

    async fn get_entity_metadata(
        &self,
        entity_id: &str,
    ) -> Result<Option<HashMap<String, serde_json::Value>>, StorageError>;

    async fn set_entity_metadata(
        &self,
        entity_id: &str,
        metadata: &HashMap<String, serde_json::Value>,
    ) -> Result<(), StorageError>;
}
