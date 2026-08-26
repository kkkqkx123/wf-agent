use async_trait::async_trait;
use serde_json::Value;

use crate::decorator::cache::{CacheConfig, CachingStore};
use crate::decorator::instrumented::{InstrumentedStore, StorageMetrics};
use crate::domain::store::{
    BatchItem, BatchStore, Maintainable, QueryFilter, Store, StoreOperation,
};
use crate::error::StorageError;
use crate::store::memory::MemoryStorage;
#[cfg(feature = "postgres")]
use crate::store::postgres::PostgresStorage;
#[cfg(feature = "sqlite")]
use crate::store::sqlite::SqliteStorage;

/// Store backend with per-operation instrumentation: every variant counts
/// save/load/delete/list/exists/clear/batch calls, latency and bytes so the
/// runtime can export storage load as metrics.
///
/// The Sqlite variant additionally layers an entity cache (`CachingStore`)
/// over the pool — the durable backend benefits most from read caching and
/// every write path invalidates the affected ids, so the cache cannot serve
/// stale data.
#[derive(Debug, Clone)]
pub enum StorageBackend {
    Memory(InstrumentedStore<MemoryStorage>),
    #[cfg(feature = "sqlite")]
    Sqlite(InstrumentedStore<CachingStore<SqliteStorage>>),
    #[cfg(feature = "postgres")]
    Postgres(InstrumentedStore<PostgresStorage>),
}

impl StorageBackend {
    pub fn new_memory() -> Self {
        Self::Memory(InstrumentedStore::new(MemoryStorage::new("default")))
    }

    /// Open a Sqlite backend with the entity cache enabled (default cache
    /// configuration: 1000 entries / 300s TTL).
    #[cfg(feature = "sqlite")]
    pub async fn new_sqlite(path: &str, table_name: &str) -> Result<Self, StorageError> {
        let store = SqliteStorage::new(path, table_name).await?;
        Ok(Self::Sqlite(InstrumentedStore::new(CachingStore::new(
            store,
            CacheConfig::default(),
        ))))
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

    /// Test support: corrupt one payload byte of an in-memory record without
    /// updating its hash (simulates on-disk corruption). No-op for the other
    /// backends.
    #[doc(hidden)]
    pub async fn corrupt_payload(&self, id: &str, offset: usize, value: u8) -> bool {
        match self {
            Self::Memory(s) => s.corrupt_payload(id, offset, value).await,
            #[cfg(any(feature = "sqlite", feature = "postgres"))]
            _ => false,
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

    async fn count(&self, filter: Option<&QueryFilter>) -> Result<u64, StorageError> {
        match self {
            Self::Memory(s) => s.count(filter).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.count(filter).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.count(filter).await,
        }
    }

    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        match self {
            Self::Memory(s) => s.update_status(id, status).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.update_status(id, status).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.update_status(id, status).await,
        }
    }

    async fn apply_batch(&self, operations: &[StoreOperation]) -> Result<(), StorageError> {
        match self {
            Self::Memory(s) => s.apply_batch(operations).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(s) => s.apply_batch(operations).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(s) => s.apply_batch(operations).await,
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
