use async_trait::async_trait;
use serde_json::Value;

use crate::decorator::cache::{CacheConfig, CachingStore};
use crate::decorator::instrumented::{InstrumentedStore, StorageMetrics};
use crate::domain::store::{BatchItem, Maintainable, QueryFilter, Store, StoreExt, StoreOperation};
use crate::error::StorageError;
use crate::store::memory::MemoryStorage;
use crate::store::postgres::PostgresStorage;
use crate::store::sqlite::SqliteStorage;

/// Forward a call to the inner store of whichever backend variant is held.
/// All variants implement the same store traits; only the concrete wrapper
/// types differ, so a macro keeps the forwarding in one place instead of
/// repeating one match block per trait method.
macro_rules! dispatch {
    ($this:expr, |$inner:ident| $call:expr) => {
        match $this {
            Self::Memory($inner) => $call,
            Self::Sqlite($inner) => $call,
            Self::Postgres($inner) => $call,
        }
    };
}

/// Store backend with per-operation instrumentation: every variant counts
/// save/load/delete/list/exists/clear/batch calls, latency and bytes so the
/// runtime can export storage load as metrics.
///
/// The Sqlite and PostgreSQL variants layer an entity cache (`CachingStore`)
/// over the pool — durable backends benefit most from read caching and every
/// write path invalidates the affected ids, so the cache cannot serve stale
/// data. The memory variant deliberately has no cache layer: it already lives
/// in memory, so a second cache would only add a copy without benefit. The
/// variant-special-cased methods below follow the same reasoning and are not
/// a candidate for flattening.
///
/// A backend holds one table only. Entity tables owned by a `StorageContext`
/// share one pool and participate in cross-entity atomic batches; a backend
/// built here stands alone and never joins those batches.
#[derive(Debug, Clone)]
pub enum StorageBackend {
    Memory(InstrumentedStore<MemoryStorage>),
    Sqlite(InstrumentedStore<CachingStore<SqliteStorage>>),
    Postgres(InstrumentedStore<CachingStore<PostgresStorage>>),
}

impl StorageBackend {
    pub fn new_memory() -> Self {
        Self::Memory(InstrumentedStore::new(MemoryStorage::new("default")))
    }

    /// Open a Sqlite backend with the entity cache enabled (default cache
    /// configuration: 1000 entries / 300s TTL). The backend owns its pool and
    /// serves one table; entity tables should use `StorageContext` instead so
    /// they share a pool and join atomic batches.
    pub async fn new_sqlite(path: &str, table_name: &str) -> Result<Self, StorageError> {
        let store = SqliteStorage::new(path, table_name).await?;
        Ok(Self::Sqlite(InstrumentedStore::new(CachingStore::new(
            store,
            CacheConfig::default(),
        ))))
    }

    /// Open a PostgreSQL backend with the entity cache enabled, mirroring
    /// `new_sqlite`. The backend owns its pool and serves one table; entity
    /// tables should use `StorageContext` instead so they share a pool and
    /// join atomic batches.
    pub async fn new_postgres(
        connection_string: &str,
        table_name: &str,
    ) -> Result<Self, StorageError> {
        let store = PostgresStorage::new(connection_string, table_name).await?;
        Ok(Self::Postgres(InstrumentedStore::new(CachingStore::new(
            store,
            CacheConfig::default(),
        ))))
    }

    /// Operation counters for this backend (the instrumentation wrapper is
    /// always present).
    pub fn op_metrics(&self) -> &StorageMetrics {
        dispatch!(self, |s| s.metrics())
    }

    /// Drop one cached record without touching durable storage, used after a
    /// cross-table atomic batch that bypasses the per-backend write path.
    /// No-op for backends without a cache layer.
    pub fn invalidate_cached(&self, id: &str) {
        match self {
            Self::Memory(_) => {}
            Self::Sqlite(s) => s.inner().cache().invalidate(id),
            Self::Postgres(s) => s.inner().cache().invalidate(id),
        }
    }

    /// Drop every cached record without touching durable storage, used after
    /// a full cleanup that bypasses the per-backend clear path. No-op for
    /// backends without a cache layer.
    pub fn invalidate_all_cached(&self) {
        match self {
            Self::Memory(_) => {}
            Self::Sqlite(s) => s.inner().cache().clear(),
            Self::Postgres(s) => s.inner().cache().clear(),
        }
    }

    /// Borrow the memory store behind this backend, if it is one. Used by
    /// the cross-entity atomic batch coordinator.
    pub fn memory_storage(&self) -> Option<&MemoryStorage> {
        match self {
            Self::Memory(s) => Some(s.inner()),
            _ => None,
        }
    }

    /// Test support: corrupt one payload byte of an in-memory record without
    /// updating its hash (simulates on-disk corruption). No-op for the other
    /// backends.
    #[doc(hidden)]
    pub async fn corrupt_payload(&self, id: &str, offset: usize, value: u8) -> bool {
        match self {
            Self::Memory(s) => s.corrupt_payload(id, offset, value).await,
            _ => false,
        }
    }
}

#[async_trait]
impl Store for StorageBackend {
    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError> {
        dispatch!(self, |s| s.save(id, data, metadata).await)
    }

    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, Value)>, StorageError> {
        dispatch!(self, |s| s.load(id).await)
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        dispatch!(self, |s| s.delete(id).await)
    }

    async fn list(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(String, Value)>, StorageError> {
        dispatch!(self, |s| s.list(filter).await)
    }

    async fn list_data(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(Vec<u8>, Value)>, StorageError> {
        dispatch!(self, |s| s.list_data(filter).await)
    }

    async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        dispatch!(self, |s| s.exists(id).await)
    }

    async fn count(&self, filter: Option<&QueryFilter>) -> Result<u64, StorageError> {
        dispatch!(self, |s| s.count(filter).await)
    }

    async fn clear(&self) -> Result<(), StorageError> {
        dispatch!(self, |s| s.clear().await)
    }
}

#[async_trait]
impl StoreExt for StorageBackend {
    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        dispatch!(self, |s| s.update_status(id, status).await)
    }

    async fn apply_batch(&self, operations: &[StoreOperation]) -> Result<(), StorageError> {
        dispatch!(self, |s| s.apply_batch(operations).await)
    }

    async fn count_by_field(
        &self,
        field: &str,
    ) -> Result<std::collections::HashMap<String, u64>, StorageError> {
        dispatch!(self, |s| s.count_by_field(field).await)
    }

    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        dispatch!(self, |s| s.save_batch(items).await)
    }

    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<u8>, Value)>, StorageError> {
        dispatch!(self, |s| s.load_batch(ids).await)
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        dispatch!(self, |s| s.delete_batch(ids).await)
    }
}

#[async_trait]
impl Maintainable for StorageBackend {
    async fn vacuum(&self) -> Result<(), StorageError> {
        dispatch!(self, |s| s.vacuum().await)
    }

    async fn wal_checkpoint(&self) -> Result<(), StorageError> {
        dispatch!(self, |s| s.wal_checkpoint().await)
    }

    async fn sync(&self) -> Result<(), StorageError> {
        dispatch!(self, |s| s.sync().await)
    }
}
