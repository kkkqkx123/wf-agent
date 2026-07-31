use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct CheckpointListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub entity_type_filter: Option<String>,
    pub entity_id_filter: Option<String>,
}

impl From<CheckpointListOptions> for QueryFilter {
    fn from(opts: CheckpointListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.entity_type_filter {
            filter.add_op(FilterOp::Eq("entityType".into(), value));
        }
        if let Some(value) = opts.entity_id_filter {
            filter.add_op(FilterOp::Eq("entityId".into(), value));
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

    async fn list_by_entities_with_metadata(
        &self,
        entity_ids: &[String],
        entity_type: &str,
    ) -> Result<Vec<wf_types::Checkpoint>, StorageError>;
}
