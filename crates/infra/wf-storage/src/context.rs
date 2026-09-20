use crate::adapter::adapter_impls::{
    AgentDraftStorage, AgentExecutionStorage, AgentLoopStorage, AgentProfileStorage,
    AgentTemplateStorage, CheckpointStorage, MessageStorage, MetricsStorage, NodeTemplateStorage,
    ScriptStorage, TaskStorage, ToolDefinitionStorage, ToolStorage, TriggerExecutionStorage,
    TriggerTemplateStorage, UserInteractionStorage, VariableStorage, WorkflowDraftStorage,
    WorkflowExecutionStorage, WorkflowStorage,
};
use crate::backend::StorageBackend;
use crate::decorator::cache::{CacheConfig, CachingStore};
use crate::decorator::instrumented::{InstrumentedStore, StorageMetrics, StorageMetricsSnapshot};
use crate::domain::store::{CrossTableOperation, StoreExt, StoreOperation};
use crate::error::StorageError;
use crate::store::memory::MemoryStorage;
use crate::store::postgres::PostgresStorage;
use crate::store::sqlite::SqliteStorage;
use sqlx::PgPool;
use sqlx::SqlitePool;

macro_rules! make_backend {
    ($variant:ident, $name:expr) => {
        StorageBackend::$variant(InstrumentedStore::new(MemoryStorage::new($name)))
    };
}

/// Single source of truth for every entity store in the context.
///
/// Each entry provides the identifier variant, the context field name, the
/// physical table name, and the adapter type. The macro derives the
/// identifier enum, the table mapping, the context struct, all constructors,
/// backend lookup, backend iteration, metric aggregation, and clearing from
/// this one list, so adding an entity means adding one line. The declaration
/// order doubles as the global lock order for memory cross-store batches.
macro_rules! define_storage_entities {
    ($( $variant:ident, $field:ident, $table:literal, $adapter:ident ),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum EntityStoreId {
            $($variant),*
        }

        impl EntityStoreId {
            /// Physical table (or memory partition) name. Constructors and the
            /// atomic batch coordinator share this mapping so the name exists once.
            pub fn table(self) -> &'static str {
                match self {
                    $(Self::$variant => $table),*
                }
            }

            /// Logical store name for diagnostics and reports. Derived from
            /// the context field name so the diagnostic list can never drift
            /// from the registered entities.
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($field)),*
                }
            }
        }

        pub struct StorageContext {
            $(pub $field: $adapter<StorageBackend>),*,
            sqlite_pool: Option<SqlitePool>,
            pg_pool: Option<PgPool>,
        }

        impl StorageContext {
            pub fn new_memory() -> Self {
                Self {
                    $($field: $adapter::new(make_backend!(Memory, $table))),*,
                    sqlite_pool: None,
                    pg_pool: None,
                }
            }

            /// Create a temporary Sqlite-backed storage context for tests.
            /// Returns the context and the database file path; the caller should
            /// delete the file when done.
            pub async fn new_test_sqlite() -> Result<(Self, std::path::PathBuf), StorageError> {
                let dir = std::env::temp_dir();
                let path = dir.join(format!("wf-test-{}.db", uuid::Uuid::new_v4()));
                let path_str = path.to_string_lossy().to_string();
                let ctx = Self::new_sqlite(&path_str, CacheConfig::default()).await?;
                Ok((ctx, path))
            }

            /// Open a Sqlite-backed context. Every entity store shares one
            /// connection pool and wraps its storage in an entity cache built
            /// from `cache`. Callers that do not tune the cache pass
            /// `CacheConfig::default()`.
            pub async fn new_sqlite(
                path: &str,
                cache: CacheConfig,
            ) -> Result<Self, StorageError> {
                let pool = SqliteStorage::create_pool(path).await?;
                Ok(Self {
                    $($field: $adapter::new(StorageBackend::Sqlite(InstrumentedStore::new(
                        CachingStore::new(
                            SqliteStorage::with_pool(pool.clone(), $table).await?,
                            cache,
                        ),
                    )))),*,
                    sqlite_pool: Some(pool),
                    pg_pool: None,
                })
            }

            /// Open a PostgreSQL-backed context. Every entity store shares one
            /// connection pool and wraps its storage in an entity cache built
            /// from `cache`. Callers that do not tune the cache pass
            /// `CacheConfig::default()`.
            pub async fn new_postgres(
                connection_string: &str,
                cache: CacheConfig,
            ) -> Result<Self, StorageError> {
                let pool = crate::util::pool::create_pg_pool(connection_string).await?;
                Ok(Self {
                    $($field: $adapter::new(StorageBackend::Postgres(InstrumentedStore::new(
                        CachingStore::new(
                            PostgresStorage::with_pool(pool.clone(), $table).await?,
                            cache,
                        ),
                    )))),*,
                    sqlite_pool: None,
                    pg_pool: Some(pool),
                })
            }
        }

        impl StorageContext {
            /// Borrow every backend in declaration order. Used for metric
            /// aggregation, clearing, and future diagnostics so callers stop
            /// maintaining a second hand-written entity list.
            pub fn all_backends(&self) -> Vec<&StorageBackend> {
                vec![$(self.$field.store()),*]
            }

            /// Borrow every backend paired with its identifier, in
            /// declaration order. Diagnostics iterates this instead of a
            /// hand-written subset so new entities are probed automatically.
            pub fn named_backends(&self) -> Vec<(EntityStoreId, &StorageBackend)> {
                vec![$( (EntityStoreId::$variant, self.$field.store()) ),*]
            }

            fn backend_of(&self, id: EntityStoreId) -> &StorageBackend {
                match id {
                    $(EntityStoreId::$variant => self.$field.store()),*
                }
            }

            /// Aggregate operation counters of every store backend (save/load/delete
            /// /list/exists/clear/batch), exported to the metrics sampler.
            pub fn ops_snapshot(&self) -> StorageMetrics {
                let mut total = StorageMetrics::default();
                for backend in self.all_backends() {
                    total = total.accumulate(backend.op_metrics());
                }
                total
            }

            /// Per-entity operation snapshots in declaration order, for
            /// diagnostics that must locate load per store instead of only a
            /// context-wide total.
            pub fn ops_snapshot_by_entity(&self) -> Vec<(EntityStoreId, StorageMetricsSnapshot)> {
                self.named_backends()
                    .iter()
                    .map(|(id, backend)| (*id, backend.op_metrics().snapshot()))
                    .collect()
            }

            /// Clear every entity store in the context. Used for test reset and
            /// runtime teardown so newly added entities cannot be missed by callers
            /// clearing stores one by one.
            ///
            /// Full cleanup runs atomically: Sqlite and PostgreSQL issue every
            /// delete inside one transaction on the shared pool, and the memory
            /// backend locks all stores in registry order before clearing. When
            /// no shared pool is available the stores are cleared sequentially as
            /// a fallback. The internal schema version rows are removed like the
            /// per-table `clear` does, so a fresh version row is written on the
            /// next open.
            pub async fn clear_all(&self) -> Result<(), StorageError> {
                if let Some(pool) = self.sqlite_pool.as_ref() {
                    let tables: Vec<&str> = self
                        .named_backends()
                        .iter()
                        .map(|(id, _)| id.table())
                        .collect();
                    SqliteStorage::clear_cross_table(pool, &tables).await?;
                    self.invalidate_all_cached();
                    return Ok(());
                }

                if let Some(pool) = self.pg_pool.as_ref() {
                    let tables: Vec<&str> = self
                        .named_backends()
                        .iter()
                        .map(|(id, _)| id.table())
                        .collect();
                    PostgresStorage::clear_cross_table(pool, &tables).await?;
                    self.invalidate_all_cached();
                    return Ok(());
                }

                {
                    let stores: Vec<&MemoryStorage> = self
                        .named_backends()
                        .iter()
                        .filter_map(|(_, backend)| backend.memory_storage())
                        .collect();
                    return MemoryStorage::clear_cross_store(&stores).await;
                }
            }

            /// Clear every entity cache without touching durable storage. A
            /// full cleanup that runs through `clear_cross_table` bypasses the
            /// per-store `clear`, so caches must be dropped explicitly.
            fn invalidate_all_cached(&self) {
                for backend in self.all_backends() {
                    backend.invalidate_all_cached();
                }
            }
        }
    };
}

define_storage_entities!(
    Workflow,
    workflow,
    "workflow",
    WorkflowStorage,
    WorkflowDraft,
    workflow_draft,
    "workflow_draft",
    WorkflowDraftStorage,
    WorkflowExecution,
    workflow_execution,
    "execution",
    WorkflowExecutionStorage,
    Checkpoint,
    checkpoint,
    "checkpoint",
    CheckpointStorage,
    Task,
    task,
    "task",
    TaskStorage,
    AgentLoop,
    agent_loop,
    "agent_loop",
    AgentLoopStorage,
    AgentExecution,
    agent_execution,
    "agent_execution",
    AgentExecutionStorage,
    AgentProfile,
    agent_profile,
    "agent_profile",
    AgentProfileStorage,
    AgentTemplate,
    agent_template,
    "agent_template",
    AgentTemplateStorage,
    AgentDraft,
    agent_draft,
    "agent_draft",
    AgentDraftStorage,
    TriggerTemplate,
    trigger_template,
    "trigger_template",
    TriggerTemplateStorage,
    TriggerExecution,
    trigger_execution,
    "trigger_execution",
    TriggerExecutionStorage,
    UserInteraction,
    user_interaction,
    "user_interaction",
    UserInteractionStorage,
    Tool,
    tool,
    "tool",
    ToolStorage,
    ToolDefinition,
    tool_definition,
    "tool_definition",
    ToolDefinitionStorage,
    Script,
    script,
    "script",
    ScriptStorage,
    NodeTemplate,
    node_template,
    "node_template",
    NodeTemplateStorage,
    Metrics,
    metrics,
    "metrics",
    MetricsStorage,
    Message,
    message,
    "message",
    MessageStorage,
    Variable,
    variable,
    "variable",
    VariableStorage,
);

/// One operation of a cross-entity atomic batch: which entity store it
/// targets plus the save/delete to run.
#[derive(Debug, Clone)]
pub struct AtomicOperation {
    pub target: EntityStoreId,
    pub operation: StoreOperation,
}

impl AtomicOperation {
    pub fn new(target: EntityStoreId, operation: StoreOperation) -> Self {
        Self { target, operation }
    }
}

/// View atomic operations as cross-table operations against physical table
/// names. Shared by the Sqlite and PostgreSQL coordinators so the mapping
/// from entity stores to tables exists exactly once.
fn cross_operations(operations: &[AtomicOperation]) -> Vec<CrossTableOperation<'_>> {
    operations
        .iter()
        .map(|op| CrossTableOperation {
            table: op.target.table(),
            operation: &op.operation,
        })
        .collect()
}

impl StorageContext {
    /// Apply operations spanning several entity stores atomically: every
    /// operation lands or none does. Sqlite and PostgreSQL run the whole
    /// batch in one transaction on the shared pool; the memory backend locks
    /// the involved stores in registry order. A batch that targets a single
    /// store delegates to the normal per-store path so decorator behavior
    /// (metrics, cache invalidation) is preserved.
    pub async fn apply_atomic(&self, operations: &[AtomicOperation]) -> Result<(), StorageError> {
        if operations.is_empty() {
            return Ok(());
        }
        let first = operations[0].target;
        if operations.iter().all(|op| op.target == first) {
            let single: Vec<StoreOperation> =
                operations.iter().map(|op| op.operation.clone()).collect();
            return self.backend_of(first).apply_batch(&single).await;
        }

        if let Some(pool) = self.sqlite_pool.as_ref() {
            let cross = cross_operations(operations);
            let start = std::time::Instant::now();
            let result = SqliteStorage::apply_cross_table(pool, &cross).await;
            self.record_atomic_metrics(operations, start.elapsed().as_millis() as u64);
            result?;
            self.invalidate_after_atomic(operations);
            return Ok(());
        }

        if let Some(pool) = self.pg_pool.as_ref() {
            let cross = cross_operations(operations);
            let start = std::time::Instant::now();
            let result = PostgresStorage::apply_cross_table(pool, &cross).await;
            self.record_atomic_metrics(operations, start.elapsed().as_millis() as u64);
            result?;
            self.invalidate_after_atomic(operations);
            return Ok(());
        }

        {
            use std::collections::BTreeMap;
            let mut grouped: BTreeMap<EntityStoreId, Vec<StoreOperation>> = BTreeMap::new();
            for op in operations {
                grouped
                    .entry(op.target)
                    .or_default()
                    .push(op.operation.clone());
            }
            let mut groups = Vec::with_capacity(grouped.len());
            for (target, ops) in &grouped {
                let store = self.backend_of(*target).memory_storage().ok_or_else(|| {
                    StorageError::General {
                        operation: "apply_atomic".into(),
                        message: "entity store is not memory-backed".into(),
                        source: None,
                    }
                })?;
                groups.push((store, ops.as_slice()));
            }
            let start = std::time::Instant::now();
            let result = MemoryStorage::apply_cross_store(&groups).await;
            self.record_atomic_metrics(operations, start.elapsed().as_millis() as u64);
            result
        }
    }

    /// Record one batch observation per involved backend, mirroring the
    /// per-store path (`InstrumentedStore::apply_batch` records even on
    /// failure, so counters stay comparable across both paths).
    fn record_atomic_metrics(&self, operations: &[AtomicOperation], elapsed_ms: u64) {
        use std::collections::BTreeMap;
        let mut bytes_by_target: BTreeMap<EntityStoreId, u64> = BTreeMap::new();
        for op in operations {
            let bytes = match &op.operation {
                StoreOperation::Save(item) => item.data.len() as u64,
                StoreOperation::Delete(_) => 0,
            };
            *bytes_by_target.entry(op.target).or_default() += bytes;
        }
        for (target, bytes) in &bytes_by_target {
            self.backend_of(*target)
                .op_metrics()
                .batch
                .record(elapsed_ms, *bytes);
        }
    }

    /// Drop cached rows touched by a cross-table batch. Only called after a
    /// successful commit; a failed transaction changes nothing, so caches
    /// stay valid without invalidation.
    ///
    /// Convention for every coordinator path: any operation that bypasses the
    /// per-store write path must supplement cache invalidation after commit.
    /// The single-store path inherits invalidation from the caching decorator;
    /// cross-table batches call this, and full cleanup calls
    /// `invalidate_all_cached`.
    fn invalidate_after_atomic(&self, operations: &[AtomicOperation]) {
        for op in operations {
            let backend = self.backend_of(op.target);
            match &op.operation {
                StoreOperation::Save(item) => backend.invalidate_cached(&item.id),
                StoreOperation::Delete(id) => backend.invalidate_cached(id),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::store::Store;

    fn seed_id(id: EntityStoreId) -> String {
        format!("seed-{}", id.table())
    }

    async fn seed_all(ctx: &StorageContext) {
        for (id, backend) in ctx.named_backends() {
            let seed = seed_id(id);
            backend
                .save(
                    &seed,
                    b"payload",
                    &serde_json::json!({"entityType": "test"}),
                )
                .await
                .unwrap();
            // Populate the entity cache where one exists so a missed
            // invalidation cannot be hidden by an empty cache.
            backend.load(&seed).await.unwrap();
        }
    }

    async fn assert_all_empty(ctx: &StorageContext) {
        for (id, backend) in ctx.named_backends() {
            assert!(
                backend.list(None).await.unwrap().is_empty(),
                "store {} still has rows after clear_all",
                id.name()
            );
            assert!(
                !backend.exists(&seed_id(id)).await.unwrap(),
                "store {} still reports the seeded row after clear_all",
                id.name()
            );
        }
    }

    #[tokio::test]
    async fn test_memory_clear_all_empties_every_store() {
        let ctx = StorageContext::new_memory();
        seed_all(&ctx).await;
        ctx.clear_all().await.unwrap();
        assert_all_empty(&ctx).await;
    }

    #[tokio::test]
    async fn test_sqlite_clear_all_empties_every_store() {
        let (ctx, path) = StorageContext::new_test_sqlite().await.unwrap();
        seed_all(&ctx).await;
        ctx.clear_all().await.unwrap();
        assert_all_empty(&ctx).await;
        drop(ctx);
        std::fs::remove_file(&path).ok();
    }
}
