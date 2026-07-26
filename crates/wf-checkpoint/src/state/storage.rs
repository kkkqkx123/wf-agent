use crate::error::CheckpointError;
use crate::serializer::{CheckpointCodec, CheckpointSerializer};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use wf_storage::domain::store::{QueryFilter, Store};
use wf_types::checkpoint::CheckpointType;
use wf_types::storage::CheckpointStorageMetadata;

pub struct StorageBackedStateManager<S, T> {
    storage: Arc<S>,
    _marker: std::marker::PhantomData<T>,
}

impl<S, T> StorageBackedStateManager<S, T>
where
    S: Store,
{
    pub fn new(storage: Arc<S>) -> Self {
        Self {
            storage,
            _marker: std::marker::PhantomData,
        }
    }

    fn build_metadata(
        &self,
        id: &str,
        entity_type: &str,
        entity_id: &str,
        checkpoint_type: CheckpointType,
        timestamp: i64,
    ) -> Value {
        serde_json::json!({
            "id": id,
            "entityType": entity_type,
            "entityId": entity_id,
            "checkpointType": checkpoint_type,
            "timestamp": timestamp,
            "status": "completed",
        })
    }
}

#[async_trait]
impl<S, T> super::CheckpointStateManager for StorageBackedStateManager<S, T>
where
    S: Store + Send + Sync,
    T: Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    type Checkpoint = T;

    async fn save(
        &self,
        checkpoint: &Self::Checkpoint,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<(), CheckpointError> {
        let id = extract_field_as_str(checkpoint, "id")?;
        let checkpoint_type = extract_checkpoint_type(checkpoint)?;
        let timestamp = extract_field_as_i64(checkpoint, "timestamp")?;

        let data = CheckpointSerializer::serialize(checkpoint, CheckpointCodec::Json)?;
        let metadata =
            self.build_metadata(&id, entity_type, entity_id, checkpoint_type, timestamp);

        self.storage
            .save(&id, &data, &metadata)
            .await
            .map_err(CheckpointError::Storage)
    }

    async fn load(&self, id: &str) -> Result<Option<Self::Checkpoint>, CheckpointError> {
        match self.storage.load(id).await.map_err(CheckpointError::Storage)? {
            Some((data, _)) => {
                let checkpoint = CheckpointSerializer::auto_deserialize(&data)?;
                Ok(Some(checkpoint))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &str) -> Result<bool, CheckpointError> {
        let exists = self.storage.exists(id).await.map_err(CheckpointError::Storage)?;
        if exists {
            self.storage.delete(id).await.map_err(CheckpointError::Storage)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> Result<Vec<CheckpointStorageMetadata>, CheckpointError> {
        let filter = QueryFilter::new().with_field("entityId", entity_id);

        let entries = self
            .storage
            .list(Some(&filter))
            .await
            .map_err(CheckpointError::Storage)?;

        let mut results: Vec<CheckpointStorageMetadata> = entries
            .into_iter()
            .map(|(id, meta)| {
                let entity_type = meta
                    .get("entityType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let cp_type = meta
                    .get("checkpointType")
                    .and_then(|v| v.as_str())
                    .map(|s| match s {
                        "delta" => CheckpointType::Delta,
                        _ => CheckpointType::Full,
                    })
                    .unwrap_or(CheckpointType::Full);

                let timestamp = meta.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
                let status = meta
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                CheckpointStorageMetadata {
                    id,
                    entity_type,
                    entity_id: entity_id.to_string(),
                    checkpoint_type: cp_type,
                    timestamp,
                    status,
                }
            })
            .collect();

        results.sort_by_key(|m| m.timestamp);
        Ok(results)
    }

    async fn get_latest(
        &self,
        entity_id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
        let mut all = self.list_by_entity(entity_id).await?;
        Ok(all.pop())
    }

    async fn cleanup(
        &self,
        entity_id: &str,
        max_count: Option<u32>,
    ) -> Result<u64, CheckpointError> {
        let max = match max_count {
            Some(0) => return Ok(0),
            Some(n) => n as usize,
            None => return Ok(0),
        };

        let all = self.list_by_entity(entity_id).await?;
        if all.len() <= max {
            return Ok(0);
        }

        let to_remove: Vec<_> = all.iter().take(all.len() - max).collect();
        let mut deleted = 0u64;

        for meta in &to_remove {
            if self.delete(&meta.id).await? {
                deleted += 1;
            }
        }

        Ok(deleted)
    }
}

fn extract_field_as_str<T: Serialize>(
    value: &T,
    field: &str,
) -> Result<String, CheckpointError> {
    let json = serde_json::to_value(value).map_err(|e| {
        CheckpointError::Serialization(format!("failed to serialize for field {}: {}", field, e))
    })?;
    json.get(field)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| CheckpointError::Validation {
            reason: format!("missing field: {}", field),
        })
}

fn extract_field_as_i64<T: Serialize>(value: &T, field: &str) -> Result<i64, CheckpointError> {
    let json = serde_json::to_value(value).map_err(|e| {
        CheckpointError::Serialization(format!("failed to serialize for field {}: {}", field, e))
    })?;
    json.get(field).and_then(|v| v.as_i64()).ok_or_else(|| {
        CheckpointError::Validation {
            reason: format!("missing field: {}", field),
        }
    })
}

fn extract_checkpoint_type<T: Serialize>(value: &T) -> Result<CheckpointType, CheckpointError> {
    let json = serde_json::to_value(value).map_err(|e| {
        CheckpointError::Serialization(format!("failed to serialize for type extraction: {}", e))
    })?;
    match json.get("type").and_then(|v| v.as_str()) {
        Some("delta") => Ok(CheckpointType::Delta),
        _ => Ok(CheckpointType::Full),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::CheckpointStateManager;
    use std::sync::Arc;
    use wf_storage::store::memory::MemoryStorage;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    struct TestCheckpoint {
        id: String,
        checkpoint_type: Option<String>,
        entity_id: String,
        timestamp: i64,
        data: String,
    }

    fn make_storage() -> Arc<MemoryStorage> {
        Arc::new(MemoryStorage::new("test"))
    }

    #[tokio::test]
    async fn save_and_load() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<MemoryStorage, TestCheckpoint>::new(storage);

        let cp = TestCheckpoint {
            id: "cp-1".to_string(),
            checkpoint_type: None,
            entity_id: "exec-1".to_string(),
            timestamp: 1000,
            data: "snapshot".to_string(),
        };

        mgr.save(&cp, "test", "exec-1").await.unwrap();
        let loaded = mgr.load("cp-1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().data, "snapshot");
    }

    #[tokio::test]
    async fn load_missing() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<MemoryStorage, TestCheckpoint>::new(storage);
        let loaded = mgr.load("nonexistent").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn delete_existing() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<MemoryStorage, TestCheckpoint>::new(storage);

        let cp = TestCheckpoint {
            id: "cp-1".to_string(),
            checkpoint_type: None,
            entity_id: "exec-1".to_string(),
            timestamp: 1000,
            data: "x".to_string(),
        };

        mgr.save(&cp, "test", "exec-1").await.unwrap();
        assert!(mgr.delete("cp-1").await.unwrap());
        assert!(!mgr.delete("cp-1").await.unwrap());
    }

    #[tokio::test]
    async fn list_by_entity_filters_correctly() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<MemoryStorage, TestCheckpoint>::new(storage);

        let cp1 = TestCheckpoint {
            id: "cp-1".to_string(),
            checkpoint_type: None,
            entity_id: "exec-1".to_string(),
            timestamp: 1000,
            data: "x".to_string(),
        };
        let cp2 = TestCheckpoint {
            id: "cp-2".to_string(),
            checkpoint_type: None,
            entity_id: "exec-2".to_string(),
            timestamp: 2000,
            data: "y".to_string(),
        };

        mgr.save(&cp1, "test", "exec-1").await.unwrap();
        mgr.save(&cp2, "test", "exec-2").await.unwrap();

        let list = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "cp-1");
    }

    #[tokio::test]
    async fn cleanup_removes_oldest() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<MemoryStorage, TestCheckpoint>::new(storage);

        for i in 0..5 {
            let cp = TestCheckpoint {
                id: format!("cp-{}", i),
                checkpoint_type: None,
                entity_id: "exec-1".to_string(),
                timestamp: i as i64 * 1000,
                data: format!("data-{}", i),
            };
            mgr.save(&cp, "test", "exec-1").await.unwrap();
        }

        let deleted = mgr.cleanup("exec-1", Some(2)).await.unwrap();
        assert_eq!(deleted, 3);

        let remaining = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(remaining.len(), 2);
    }
}
