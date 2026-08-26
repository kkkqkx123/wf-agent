use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::collections::HashMap;
use std::future::Future;

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
    fn list_by_entity<'a>(
        &'a self,
        entity_id: &'a str,
        entity_type: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::Checkpoint>, StorageError>> + Send + 'a;

    fn get_latest_by_entity<'a>(
        &'a self,
        entity_id: &'a str,
        entity_type: &'a str,
    ) -> impl Future<Output = Result<Option<wf_types::Checkpoint>, StorageError>> + Send + 'a;

    fn delete_by_entity<'a>(
        &'a self,
        entity_id: &'a str,
        entity_type: &'a str,
    ) -> impl Future<Output = Result<u64, StorageError>> + Send + 'a;

    fn get_entity_metadata<'a>(
        &'a self,
        entity_id: &'a str,
    ) -> impl Future<Output = Result<Option<HashMap<String, serde_json::Value>>, StorageError>> + Send + 'a;

    fn set_entity_metadata<'a>(
        &'a self,
        entity_id: &'a str,
        metadata: &'a HashMap<String, serde_json::Value>,
    ) -> impl Future<Output = Result<(), StorageError>> + Send + 'a;

    fn list_by_entities_with_metadata<'a>(
        &'a self,
        entity_ids: &'a [String],
        entity_type: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::Checkpoint>, StorageError>> + Send + 'a;
}
