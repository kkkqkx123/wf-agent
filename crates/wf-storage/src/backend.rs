use async_trait::async_trait;
use serde_json::Value;

use crate::decorator::instrumented::{InstrumentedStore, StorageMetrics};
use crate::domain::store::{BatchItem, BatchStore, Maintainable, QueryFilter, Store};
use crate::error::StorageError;
use crate::store::memory::MemoryStorage;
#[cfg(feature = "postgres")]
use crate::store::postgres::PostgresStorage;
#[cfg(feature = "sqlite")]
use crate::store::sqlite::SqliteStorage;

/// Store backend with per-operation instrumentation: every variant counts
/// save/load/delete/list/exists/clear/batch calls, latency and bytes so the
/// runtime can export storage load as metrics.
#[derive(Debug, Clone)]
pub enum StorageBackend {
    Memory(InstrumentedStore<MemoryStorage>),
    #[cfg(feature = "sqlite")]
    Sqlite(InstrumentedStore<SqliteStorage>),
    #[cfg(feature = "postgres")]
    Postgres(InstrumentedStore<PostgresStorage>),
}

impl StorageBackend {
    pub fn new_memory() -> Self {
        Self::Memory(InstrumentedStore::new(MemoryStorage::new("default")))
    }

    /// Operation counters for this backend (the instrumentation wrapper is
    /// always present).
    pub fn op_metrics(&self) -> &StorageMetrics {
        match self {
            Self::Memory(s) => s.metrics(),
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.metrics(),
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.metrics(),
        }
    }
}

#[async_trait]
impl Store for StorageBackend {
    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError> {
        match self {
            Self::Memory(s) => s.save(id, data, metadata).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.save(id, data, metadata).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.save(id, data, metadata).await,
        }
    }

    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, Value)>, StorageError> {
        match self {
            Self::Memory(s) => s.load(id).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.load(id).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.load(id).await,
        }
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        match self {
            Self::Memory(s) => s.delete(id).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.delete(id).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.delete(id).await,
        }
    }

    async fn list(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(String, Value)>, StorageError> {
        match self {
            Self::Memory(s) => s.list(filter).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.list(filter).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.list(filter).await,
        }
    }

    async fn list_data(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(Vec<u8>, Value)>, StorageError> {
        match self {
            Self::Memory(s) => s.list_data(filter).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.list_data(filter).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.list_data(filter).await,
        }
    }

    async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        match self {
            Self::Memory(s) => s.exists(id).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.exists(id).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.exists(id).await,
        }
    }

    async fn clear(&self) -> Result<(), StorageError> {
        match self {
            Self::Memory(s) => s.clear().await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.clear().await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.clear().await,
        }
    }
}

#[async_trait]
impl BatchStore for StorageBackend {
    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        match self {
            Self::Memory(s) => s.save_batch(items).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.save_batch(items).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.save_batch(items).await,
        }
    }

    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<u8>, Value)>, StorageError> {
        match self {
            Self::Memory(s) => s.load_batch(ids).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.load_batch(ids).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.load_batch(ids).await,
        }
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        match self {
            Self::Memory(s) => s.delete_batch(ids).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.delete_batch(ids).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.delete_batch(ids).await,
        }
    }
}

#[async_trait]
impl Maintainable for StorageBackend {
    async fn vacuum(&self) -> Result<(), StorageError> {
        match self {
            Self::Memory(s) => s.vacuum().await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.vacuum().await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.vacuum().await,
        }
    }

    async fn checkpoint(&self) -> Result<(), StorageError> {
        match self {
            Self::Memory(s) => s.checkpoint().await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.checkpoint().await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.checkpoint().await,
        }
    }

    async fn sync(&self) -> Result<(), StorageError> {
        match self {
            Self::Memory(s) => s.sync().await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.sync().await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.sync().await,
        }
    }
}
