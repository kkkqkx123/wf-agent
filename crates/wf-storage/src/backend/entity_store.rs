use std::marker::PhantomData;
use serde::{Serialize, de::DeserializeOwned};
use crate::error::StorageError;
use crate::adapter::base::{BaseStorageAdapter, ListOptions};

pub struct EntityStore<I, T> {
    inner: I,
    _marker: PhantomData<T>,
}

impl<I, T> EntityStore<I, T> {
    pub fn new(inner: I) -> Self {
        Self { inner, _marker: PhantomData }
    }
}

impl<I, T> BaseStorageAdapter<T, ListOptions> for EntityStore<I, T>
where
    I: BaseStorageAdapter<serde_json::Value, ListOptions> + Send + Sync,
    T: Serialize + DeserializeOwned + Send + Sync,
{
    async fn initialize(&self) -> Result<(), StorageError> {
        self.inner.initialize().await
    }

    async fn close(&self) -> Result<(), StorageError> {
        self.inner.close().await
    }

    async fn save(&self, entity: &T) -> Result<(), StorageError> {
        let value = serde_json::to_value(entity)?;
        self.inner.save(&value).await
    }

    async fn load(&self, id: &str) -> Result<Option<T>, StorageError> {
        let value = self.inner.load(id).await?;
        value
            .map(|v| serde_json::from_value(v))
            .transpose()
            .map_err(Into::into)
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> {
        self.inner.delete(id).await
    }

    async fn list(
        &self,
        _opts: Option<ListOptions>,
    ) -> Result<Vec<T>, StorageError> {
        let values = self.inner.list(None).await?;
        values
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(Into::into))
            .collect()
    }

    async fn clear(&self) -> Result<(), StorageError> {
        self.inner.clear().await
    }
}
