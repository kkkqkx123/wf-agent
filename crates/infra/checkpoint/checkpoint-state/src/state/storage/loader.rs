//! Raw payload and metadata loading used while restoring delta chains.
//!
//! Restoration needs the untouched stored bytes of a checkpoint (its own
//! serialization) rather than the deserialized value the manager returns, so
//! this reads the row directly without decoding or decompressing it.

use checkpoint_base::delta::CheckpointLoader;
use checkpoint_base::error::CheckpointError;
use wf_storage::domain::store::Store;
use wf_types::storage::CheckpointStorageMetadata;

#[async_trait::async_trait]
impl<T: Send + Sync> CheckpointLoader for super::StorageBackedStateManager<T> {
    async fn load_checkpoint_data(&self, id: &str) -> Result<Option<Vec<u8>>, CheckpointError> {
        self.storage
            .load(id)
            .await
            .map(|entry| entry.map(|(data, _)| data))
            .map_err(CheckpointError::Storage)
    }

    async fn load_metadata(
        &self,
        id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
        match self
            .storage
            .load(id)
            .await
            .map_err(CheckpointError::Storage)?
        {
            Some((_, meta)) => {
                let entity_id = meta
                    .get("entityId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                Ok(Some(super::metadata::parse_storage_metadata(
                    id, &entity_id, &meta,
                )?))
            }
            None => Ok(None),
        }
    }
}
