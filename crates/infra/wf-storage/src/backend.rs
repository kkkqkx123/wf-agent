use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::decorator::cache::{CacheConfig, CachingStore};
use crate::decorator::instrumented::{InstrumentedStore, StorageMetrics};
use crate::domain::indexes::EntityIndexes;
use crate::domain::store::{
    BatchItem, CrossTableOperation, Maintainable, QueryFilter, Store, StoreExt, StoreOperation,
};
use crate::error::StorageError;
use crate::revision::{
    component_from_delete, component_from_row, source_for_table, ComponentUpdate, RevisionStore,
};
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
            Self::Memory { store: $inner, .. } => $call,
            Self::Sqlite { store: $inner, .. } => $call,
            Self::Postgres { store: $inner, .. } => $call,
        }
    };
}

/// Store backend with per-operation instrumentation: every variant counts
/// save/load/delete/list/exists/clear/batch calls, latency and bytes so the
/// runtime can export storage load as metrics.
///
/// Only the Sqlite variant layers an entity cache (`CachingStore`) over the
/// pool: it runs in a single process with a shared pool, so local
/// invalidation on every write path keeps the cache coherent. PostgreSQL
/// serves multi-process deployments where a process-local cache would stay
/// stale across writers, so it reads straight through to the database. The
/// memory variant deliberately has no cache layer: it already lives in
/// memory, so a second cache would only add a copy without benefit. The
/// variant-special-cased methods below follow the same reasoning and are not
/// a candidate for flattening.
///
/// A backend holds one table only. Entity tables owned by a `StorageContext`
/// share one pool and participate in cross-entity atomic batches; a backend
/// built here stands alone and never joins those batches.
#[derive(Debug, Clone)]
pub enum StorageBackend {
    Memory {
        store: InstrumentedStore<MemoryStorage>,
        revision: Option<Arc<RevisionStore>>,
    },
    Sqlite {
        store: InstrumentedStore<CachingStore<SqliteStorage>>,
        revision: Option<Arc<RevisionStore>>,
    },
    Postgres {
        store: InstrumentedStore<PostgresStorage>,
        revision: Option<Arc<RevisionStore>>,
    },
}

impl StorageBackend {
    pub fn new_memory() -> Self {
        Self::Memory {
            store: InstrumentedStore::new(MemoryStorage::new("default")),
            revision: None,
        }
    }

    /// Open a standalone Sqlite backend with the entity cache enabled (default cache
    /// configuration: 1000 entries / 300s TTL). The backend owns its pool and
    /// serves one table only, for standalone single-table use such as event snapshots.
    /// Entity tables must use `StorageContext` instead so they share a pool
    /// and join atomic batches. `indexes` is the metadata
    /// keys this table's reads are built on, declared with the entity.
    pub async fn new_sqlite(
        path: &str,
        table_name: &str,
        indexes: EntityIndexes,
    ) -> Result<Self, StorageError> {
        let store = SqliteStorage::new(path, table_name, indexes).await?;
        Ok(Self::Sqlite {
            store: InstrumentedStore::new(CachingStore::new(store, CacheConfig::default())),
            revision: None,
        })
    }

    /// Open a standalone PostgreSQL backend without a process-local cache.
    /// Every read goes to the database so concurrent processes observe each
    /// other's committed writes. The backend owns its pool and serves one
    /// table only, for standalone single-table use such as event snapshots.
    /// Entity tables must use `StorageContext` instead so they share a pool and
    /// join atomic batches. `indexes` is the metadata keys this table's reads
    /// are built on, declared with the entity.
    pub async fn new_postgres(
        connection_string: &str,
        table_name: &str,
        indexes: EntityIndexes,
    ) -> Result<Self, StorageError> {
        let store = PostgresStorage::new(connection_string, table_name, indexes).await?;
        Ok(Self::Postgres {
            store: InstrumentedStore::new(store),
            revision: None,
        })
    }

    /// Operation counters for this backend (the instrumentation wrapper is
    /// always present).
    pub fn op_metrics(&self) -> &StorageMetrics {
        dispatch!(self, |s| s.metrics())
    }

    /// Drop one cached record without touching durable storage. Only the
    /// cross-table coordinator calls this after a batch that bypasses the
    /// per-backend write path. No-op for backends without a cache layer.
    pub(crate) fn invalidate_cached(&self, id: &str) {
        match self {
            Self::Memory { .. } => {}
            Self::Postgres { .. } => {}
            Self::Sqlite { store, .. } => store.inner().cache().invalidate(id),
        }
    }

    /// Drop every cached record without touching durable storage. Only the
    /// context-wide cleanup calls this after a clear that bypasses the
    /// per-backend clear path. No-op for backends without a cache layer.
    pub(crate) fn invalidate_all_cached(&self) {
        match self {
            Self::Memory { .. } => {}
            Self::Postgres { .. } => {}
            Self::Sqlite { store, .. } => store.inner().cache().clear(),
        }
    }

    /// Borrow the memory store behind this backend, if it is one. Used by
    /// the cross-entity atomic batch coordinator.
    pub fn memory_storage(&self) -> Option<&MemoryStorage> {
        match self {
            Self::Memory { store, .. } => Some(store.inner()),
            _ => None,
        }
    }

    /// Test support: corrupt one payload byte of an in-memory record without
    /// updating its hash (simulates on-disk corruption). No-op for the other
    /// backends.
    #[doc(hidden)]
    pub async fn corrupt_payload(&self, id: &str, offset: usize, value: u8) -> bool {
        match self {
            Self::Memory { store, .. } => store.corrupt_payload(id, offset, value).await,
            _ => false,
        }
    }

    /// Attach the revision table owner to this backend. Execution and
    /// checkpoint tables use it to advance revisions on every write.
    pub fn with_revision(mut self, revision: Arc<RevisionStore>) -> Self {
        match &mut self {
            Self::Memory { revision: slot, .. }
            | Self::Sqlite { revision: slot, .. }
            | Self::Postgres { revision: slot, .. } => *slot = Some(revision),
        }
        self
    }

    pub fn revision(&self) -> Option<&Arc<RevisionStore>> {
        match self {
            Self::Memory { revision, .. }
            | Self::Sqlite { revision, .. }
            | Self::Postgres { revision, .. } => revision.as_ref(),
        }
    }

    /// Physical table served by this backend.
    pub fn table_name(&self) -> &str {
        match self {
            Self::Memory { store, .. } => store.inner().name(),
            Self::Sqlite { store, .. } => store.inner().inner().table_name(),
            Self::Postgres { store, .. } => store.inner().table_name(),
        }
    }

    /// Apply one group of operations on this table and one group on another
    /// table in a single transaction. Both handles must come from the same
    /// storage context and therefore use the same backend implementation.
    pub async fn apply_cross_backend(
        &self,
        other: &StorageBackend,
        operations: &[StoreOperation],
        other_operations: &[StoreOperation],
    ) -> Result<(), StorageError> {
        let left_table = self.table_name();
        let right_table = other.table_name();
        let cross_ops: Vec<CrossTableOperation<'_>> = operations
            .iter()
            .map(|operation| CrossTableOperation {
                table: left_table,
                operation,
            })
            .chain(
                other_operations
                    .iter()
                    .map(|operation| CrossTableOperation {
                        table: right_table,
                        operation,
                    }),
            )
            .collect();
        match (self, other) {
            (Self::Sqlite { store: left, .. }, Self::Sqlite { store: right, .. }) => {
                SqliteStorage::apply_cross_table(left.inner().inner().pool(), &cross_ops).await?;
                left.inner().cache().clear();
                right.inner().cache().clear();
            }
            (Self::Postgres { store: left, .. }, Self::Postgres { store: right, .. }) => {
                let _ = right;
                PostgresStorage::apply_cross_table(left.inner().pool(), &cross_ops).await?;
            }
            (Self::Memory { store: left, .. }, Self::Memory { store: right, .. }) => {
                let left_mem = left.inner();
                let right_mem = right.inner();
                if left_mem.name() <= right_mem.name() {
                    MemoryStorage::apply_cross_store(&[
                        (left_mem, operations),
                        (right_mem, other_operations),
                    ])
                    .await?;
                } else {
                    MemoryStorage::apply_cross_store(&[
                        (right_mem, other_operations),
                        (left_mem, operations),
                    ])
                    .await?;
                }
            }
            _ => {
                return Err(StorageError::General {
                    operation: "apply_cross_backend".to_string(),
                    message: "storage backends do not share a transaction".to_string(),
                    source: None,
                });
            }
        }
        Ok(())
    }
}

fn row_operation(id: &str, data: &[u8], metadata: &Value) -> StoreOperation {
    StoreOperation::Save(BatchItem::new(
        id.to_string(),
        data.to_vec(),
        metadata.clone(),
    ))
}

async fn delete_update(
    backend: &StorageBackend,
    id: &str,
) -> Result<Option<ComponentUpdate>, StorageError> {
    let Some((_, metadata)) = backend.load(id).await? else {
        return Ok(None);
    };
    Ok(component_from_delete(backend.table_name(), id, &metadata))
}

#[async_trait]
impl Store for StorageBackend {
    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError> {
        if let Some(revisions) = self.revision() {
            if let Some(update) = component_from_row(self.table_name(), id, data, metadata)? {
                revisions
                    .advance_components(
                        self,
                        &[update],
                        &[row_operation(id, data, metadata)],
                        source_for_table(self.table_name()),
                        "save",
                    )
                    .await?;
                return Ok(());
            }
        }
        dispatch!(self, |s| s.save(id, data, metadata).await)
    }

    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, Value)>, StorageError> {
        dispatch!(self, |s| s.load(id).await)
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        if let Some(revisions) = self.revision() {
            if let Ok(Some(update)) = delete_update(self, id).await {
                revisions
                    .advance_components(
                        self,
                        &[update],
                        &[StoreOperation::Delete(id.to_string())],
                        source_for_table(self.table_name()),
                        "delete",
                    )
                    .await?;
                return Ok(());
            }
        }
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
        if let Some(revisions) = self.revision() {
            revisions.clear().await?;
        }
        dispatch!(self, |s| s.clear().await)
    }
}

#[async_trait]
impl StoreExt for StorageBackend {
    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        if let Some(revisions) = self.revision() {
            // A corrupt payload cannot supply a component hash, but it can
            // still be marked through the native metadata update path.
            if let Ok(Some((data, mut metadata))) = self.load(id).await {
                if let Some(object) = metadata.as_object_mut() {
                    object.insert("status".to_string(), Value::String(status.to_string()));
                }
                if let Some(update) = component_from_row(self.table_name(), id, &data, &metadata)? {
                    revisions
                        .advance_components(
                            self,
                            &[update],
                            &[row_operation(id, &data, &metadata)],
                            source_for_table(self.table_name()),
                            "status_update",
                        )
                        .await?;
                    return Ok(());
                }
            }
        }
        dispatch!(self, |s| s.update_status(id, status).await)
    }

    async fn apply_batch(&self, operations: &[StoreOperation]) -> Result<(), StorageError> {
        let Some(revisions) = self.revision() else {
            return dispatch!(self, |s| s.apply_batch(operations).await);
        };
        let mut updates = Vec::with_capacity(operations.len());
        for operation in operations {
            match operation {
                StoreOperation::Save(item) => {
                    if let Some(update) =
                        component_from_row(self.table_name(), &item.id, &item.data, &item.metadata)?
                    {
                        updates.push(update);
                    }
                }
                StoreOperation::Delete(id) => {
                    if let Ok(Some(update)) = delete_update(self, id).await {
                        updates.push(update);
                    }
                }
            }
        }
        if updates.is_empty() {
            return dispatch!(self, |s| s.apply_batch(operations).await);
        }
        revisions
            .advance_components(
                self,
                &updates,
                operations,
                source_for_table(self.table_name()),
                "batch",
            )
            .await?;
        Ok(())
    }

    async fn count_by_field(
        &self,
        field: &str,
    ) -> Result<std::collections::HashMap<String, u64>, StorageError> {
        dispatch!(self, |s| s.count_by_field(field).await)
    }

    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        let operations: Vec<StoreOperation> = items
            .iter()
            .map(|item| StoreOperation::Save(item.clone()))
            .collect();
        self.apply_batch(&operations).await
    }

    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<u8>, Value)>, StorageError> {
        dispatch!(self, |s| s.load_batch(ids).await)
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        let operations: Vec<StoreOperation> = ids
            .iter()
            .map(|id| StoreOperation::Delete(id.clone()))
            .collect();
        self.apply_batch(&operations).await
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

    async fn flush(&self) -> Result<(), StorageError> {
        dispatch!(self, |s| s.flush().await)
    }
}
