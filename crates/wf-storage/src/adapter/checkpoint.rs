use std::collections::HashMap;
use crate::domain::store::QueryFilter;
use crate::error::StorageError;
use crate::adapter::base::BaseStorageAdapter;

#[derive(Debug, Clone, Default)]
pub struct CheckpointListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub entity_type_filter: Option<String>,
    pub entity_id_filter: Option<String>,
}

impl From<CheckpointListOptions> for QueryFilter {
    fn from(opts: CheckpointListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(ety) = opts.entity_type_filter {
            filter.entity_type = Some(ety);
        }
        if let Some(eid) = opts.entity_id_filter {
            filter.fields.insert("entityId".to_string(), eid);
        }
        filter
    }
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
