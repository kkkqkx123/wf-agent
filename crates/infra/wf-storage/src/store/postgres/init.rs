use serde_json::Value;
use sqlx::PgPool;

use crate::domain::indexes::{index_name, EntityIndexes};
use crate::domain::keys::{schema_version_key, SCHEMA_VERSION, SCHEMA_VERSION_EXCLUDE_PATTERN};
use crate::domain::store::{BatchItem, StoreOperation};
use crate::error::StorageError;
use crate::util::pool::create_pg_pool;

#[derive(Debug, Clone)]
pub struct PostgresStorage {
    pub(crate) pool: PgPool,
    pub(crate) table_name: String,
}

impl PostgresStorage {
    pub async fn new(
        connection_string: &str,
        table_name: &str,
        indexes: EntityIndexes,
    ) -> Result<Self, StorageError> {
        let pool = create_pg_pool(connection_string).await?;
        Self::with_pool(pool, table_name, indexes).await
    }

    /// Create one declared index, reporting a failure without failing the
    /// open: the table stays usable without it, and the next open retries.
    async fn create_index(pool: &PgPool, table_name: &str, name: &str, sql: &str) {
        if let Err(e) = sqlx::query(sql).execute(pool).await {
            tracing::warn!(
                table = table_name,
                index = name,
                error = %e,
                "failed to create metadata index (table functional without it)"
            );
        }
    }

    pub async fn with_pool(
        pool: PgPool,
        table_name: &str,
        indexes: EntityIndexes,
    ) -> Result<Self, StorageError> {
        let create_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                data BYTEA NOT NULL,
                metadata JSONB NOT NULL,
                hash TEXT NOT NULL,
                data_size INTEGER NOT NULL,
                compressed BOOLEAN NOT NULL DEFAULT FALSE,
                created_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL
            )",
            table_name
        );
        sqlx::query(&create_sql).execute(&pool).await.map_err(|e| {
            StorageError::Initialization {
                backend: "postgres".into(),
                message: format!("Failed to create table '{}'", table_name),
                source: Some(Box::new(e)),
            }
        })?;

        // Equality keys serve `metadata->>'key' = value`; the default btree
        // operator class is what makes that a range probe.
        for key in indexes.equality {
            let name = index_name(table_name, key);
            let sql = format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}((metadata->>'{}'))",
                name, table_name, key
            );
            Self::create_index(&pool, table_name, &name, &sql).await;
        }

        // text_pattern_ops is what lets a btree index actually serve the
        // prefix scan these keys are read with; without it Postgres would
        // sequence-scan the table on every one.
        for key in indexes.prefix {
            let name = index_name(table_name, key);
            let sql = format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}((metadata->>'{}') text_pattern_ops)",
                name, table_name, key
            );
            Self::create_index(&pool, table_name, &name, &sql).await;
        }

        // The declared numeric key backs range filters and ordering, so it is
        // indexed as a number rather than as text: a text order puts `"10"`
        // before `"9"` and cannot serve a numeric comparison.
        if let Some(key) = indexes.numeric {
            let name = index_name(table_name, key);
            let sql = format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(((metadata->>'{}')::float8))",
                name, table_name, key
            );
            Self::create_index(&pool, table_name, &name, &sql).await;
        }

        // Schema version check: insert on first open, reject on mismatch.
        let version_key = schema_version_key(table_name);
        let check_sql = format!("SELECT metadata FROM {} WHERE id = $1", table_name);
        let existing: Option<(Value,)> = sqlx::query_as(&check_sql)
            .bind(&version_key)
            .fetch_optional(&pool)
            .await
            .map_err(|e| StorageError::Initialization {
                backend: "postgres".into(),
                message: format!("Failed to check schema version for '{}'", table_name),
                source: Some(Box::new(e)),
            })?;
        match existing {
            Some((meta,)) => {
                let stored = meta.get("version").and_then(|v| v.as_i64()).unwrap_or(0);
                if stored != SCHEMA_VERSION {
                    return Err(StorageError::StateError {
                        expected: format!("schema v{}", SCHEMA_VERSION),
                        actual: format!("schema v{}", stored),
                    });
                }
            }
            None => {
                let now = chrono::Utc::now().timestamp_millis();
                let version_meta = serde_json::json!({"version": SCHEMA_VERSION});
                let empty_hash = crate::util::hash::compute_hash(b"");
                let insert_sql = format!(
                    "INSERT INTO {} (id, data, metadata, hash, data_size, compressed, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    table_name
                );
                sqlx::query(&insert_sql)
                    .bind(&version_key)
                    .bind(b"" as &[u8])
                    .bind(&version_meta)
                    .bind(&empty_hash)
                    .bind(0i64)
                    .bind(false)
                    .bind(now)
                    .bind(now)
                    .execute(&pool)
                    .await
                    .map_err(|e| StorageError::Initialization {
                        backend: "postgres".into(),
                        message: format!("Failed to write schema version for '{}'", table_name),
                        source: Some(Box::new(e)),
                    })?;
            }
        }

        Ok(Self {
            pool,
            table_name: table_name.to_string(),
        })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    pub async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        let now = chrono::Utc::now().timestamp_millis();
        let sql = format!(
            "UPDATE {} SET metadata = jsonb_set(metadata, '{{status}}', $1::jsonb), updated_at = $2 WHERE id = $3",
            self.table_name
        );
        sqlx::query(&sql)
            .bind(serde_json::json!(status))
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "update_status".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(())
    }

    /// Apply operations targeting several tables inside one transaction.
    /// Tables come from the storage context registry, never from external
    /// input. Crate-visible only; cache invalidation is the caller's job.
    pub(crate) async fn apply_cross_table(
        pool: &PgPool,
        operations: &[crate::domain::store::CrossTableOperation<'_>],
    ) -> Result<(), StorageError> {
        if operations.is_empty() {
            return Ok(());
        }
        let mut tx = pool.begin().await.map_err(|e| StorageError::General {
            operation: "apply_cross_table".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;

        let now = chrono::Utc::now().timestamp_millis();
        for operation in operations {
            match operation.operation {
                StoreOperation::Save(item) => {
                    exec_save(&mut *tx, operation.table, item, now).await?;
                }
                StoreOperation::Delete(id) => {
                    exec_delete(&mut *tx, operation.table, id).await?;
                }
            }
        }

        tx.commit().await.map_err(|e| StorageError::General {
            operation: "apply_cross_table".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;
        Ok(())
    }

    /// Delete every business row of the given tables inside one transaction.
    /// Tables come from the storage context registry, never from external
    /// input. Internal schema version rows are preserved so the version guard
    /// stays active without reopening. Crate-visible only; cache invalidation
    /// is the caller's job.
    pub(crate) async fn clear_cross_table(
        pool: &PgPool,
        tables: &[&str],
    ) -> Result<(), StorageError> {
        if tables.is_empty() {
            return Ok(());
        }
        let mut tx = pool.begin().await.map_err(|e| StorageError::General {
            operation: "clear_cross_table".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;

        for table in tables {
            let sql = format!(
                "DELETE FROM {} WHERE id NOT LIKE '{}'",
                table, SCHEMA_VERSION_EXCLUDE_PATTERN
            );
            sqlx::query(&sql)
                .execute(&mut *tx)
                .await
                .map_err(|e| StorageError::General {
                    operation: "clear_cross_table".into(),
                    message: e.to_string(),
                    source: Some(Box::new(e)),
                })?;
        }

        tx.commit().await.map_err(|e| StorageError::General {
            operation: "clear_cross_table".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;
        Ok(())
    }
}

/// Translates a QueryFilter into a complete SELECT statement.
/// Field names come from a fixed metadata schema, so interpolation is safe.
/// The filter is first normalized through the shared compiled plan; only
/// placeholder style and JSON operators are PostgreSQL-specific. Mirror any
/// semantic change in the Sqlite renderer.
pub async fn exec_save<'e, E>(
    executor: E,
    table: &str,
    item: &BatchItem,
    now: i64,
) -> Result<(), StorageError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let hash = crate::util::hash::compute_hash(&item.data);
    let data_size = item.data_size();
    let compressed = item.compressed();
    let sql = format!(
        "INSERT INTO {} (id, data, metadata, hash, data_size, compressed, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (id) DO UPDATE SET
            data = EXCLUDED.data,
            metadata = EXCLUDED.metadata,
            hash = EXCLUDED.hash,
            data_size = EXCLUDED.data_size,
            compressed = EXCLUDED.compressed,
            updated_at = EXCLUDED.updated_at",
        table
    );
    sqlx::query(&sql)
        .bind(&item.id)
        .bind(&item.data)
        .bind(&item.metadata)
        .bind(&hash)
        .bind(data_size)
        .bind(compressed)
        .bind(now)
        .bind(now)
        .execute(executor)
        .await
        .map_err(|e| StorageError::General {
            operation: "apply_batch.save".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;
    Ok(())
}

/// Execute one delete inside a transaction or pool executor (see `exec_save`).
pub async fn exec_delete<'e, E>(executor: E, table: &str, id: &str) -> Result<(), StorageError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let sql = format!("DELETE FROM {} WHERE id = $1", table);
    sqlx::query(&sql)
        .bind(id)
        .execute(executor)
        .await
        .map_err(|e| StorageError::General {
            operation: "apply_batch.delete".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;
    Ok(())
}
