use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::StorageError;

pub trait Entity: Serialize + DeserializeOwned + Send + Sync {
    type Metadata: Serialize + DeserializeOwned + Send + Sync + Clone;

    fn entity_id(&self) -> &str;
    fn entity_type() -> &'static str;
    fn metadata(&self) -> Self::Metadata;

    fn to_bytes(&self) -> Result<Vec<u8>, StorageError> {
        serde_json::to_vec(self).map_err(StorageError::from)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, StorageError> {
        serde_json::from_slice(bytes).map_err(StorageError::from)
    }
}
