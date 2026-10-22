use serde_json::Value;
use sqlx::SqlitePool;

use crate::domain::indexes::{index_name, EntityIndexes};
use crate::domain::keys::{schema_version_key, SCHEMA_VERSION, SCHEMA_VERSION_EXCLUDE_PATTERN};
use crate::domain::store::{
    BatchItem, StoreOperation,
};
use crate::error::StorageError;

#[derive(Debug, Clone)]
pub struct SqliteStorage {
    pub(crate) pool: SqlitePool,
    pub(crate) table_name: String,
}

impl SqliteStorage {
    pub async fn new(
        path: &str,
        table_name: &str,
        indexes: EntityIndexes,
    ) -> Result<Self, StorageError> {
        let pool = Self::create_pool(path).await?;
        Self::with_pool(pool, table_name, indexes).await
    }

    pub async fn create_pool(path: &str) -> Result<SqlitePool, StorageError> {
        crate::util::pool::create_sqlite_pool(path).await
    }

    pub async fn with_pool(
        pool: SqlitePool,
        table_name: &str,
        indexes: EntityIndexes,
    ) -> Result<Self, StorageError> {
        let create_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                data BLOB NOT NULL,
                metadata TEXT NOT NULL,
                hash TEXT NOT NULL,
                data_size INTEGER NOT NULL,
                compressed BOOLEAN NOT NULL DEFAULT FALSE,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            table_name
        );
        sqlx::query(&create_sql).execute(&pool).await.map_err(|e| {
            StorageError::Initialization {
                backend: "sqlite".into(),
                message: format!("Failed to create table '{}'", table_name),
                source: Some(Box::new(e)),
            }
        })?;

        // Declared text indexes use the same boolean-normalized expression as
        // the query renderer so the planner can match them. Databases created
        // before the normalization carry the plain CAST version under the same
        // name; drop the stale definition once so it is rebuilt below.
        for key in indexes.equality.iter().chain(indexes.prefix) {
            let name = index_name(table_name, key);
            let check: Option<(Option<String>,)> =
                sqlx::query_as("SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1")
                    .bind(&name)
                    .fetch_optional(&pool)
                    .await
                    .map_err(|e| StorageError::Initialization {
                        backend: "sqlite".into(),
                        message: format!("Failed to inspect index '{}'", name),
                        source: Some(Box::new(e)),
                    })?;
            if let Some((Some(sql),)) = check {
                if !sql.contains("CASE WHEN") {
                    sqlx::query(&format!("DROP INDEX {}", name))
                        .execute(&pool)
                        .await
                        .map_err(|e| StorageError::Initialization {
                            backend: "sqlite".into(),
                            message: format!("Failed to drop stale index '{}'", name),
                            source: Some(Box::new(e)),
                        })?;
                }
            }
            let create = format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}({})",
                name,
                table_name,
                super::query::metadata_text_expr(key)
            );
            sqlx::query(&create).execute(&pool).await.ok();
        }

        // The declared numeric key backs range filters and ordering, so it is
        // indexed as a number rather than as text: a text order puts `"10"`
        // before `"9"` and cannot serve a numeric comparison.
        if let Some(key) = indexes.numeric {
            let name = index_name(table_name, key);
            let create = format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(json_extract(metadata, '$.{}'))",
                name, table_name, key
            );
            sqlx::query(&create).execute(&pool).await.ok();
        }

        // Schema version check: insert on first open, reject on mismatch.
        let version_key = schema_version_key(table_name);
        let check_sql = format!("SELECT metadata FROM {} WHERE id = ?1", table_name);
        let existing: Option<(String,)> = sqlx::query_as(&check_sql)
            .bind(&version_key)
            .fetch_optional(&pool)
            .await
            .map_err(|e| StorageError::Initialization {
                backend: "sqlite".into(),
                message: format!("Failed to check schema version for '{}'", table_name),
                source: Some(Box::new(e)),
            })?;
        match existing {
            Some((meta_str,)) => {
                let meta: Value =
                    serde_json::from_str(&meta_str).map_err(|e| StorageError::Initialization {
                        backend: "sqlite".into(),
                        message: format!(
                            "Failed to parse schema version metadata for '{}'",
                            table_name
                        ),
                        source: Some(Box::new(e)),
                    })?;
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
                let insert_sql = format!(
                    "INSERT INTO {} (id, data, metadata, hash, data_size, compressed, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    table_name
                );
                let version_meta = serde_json::json!({"version": SCHEMA_VERSION});
                let meta_str = serde_json::to_string(&version_meta)?;
                let empty_hash = crate::util::hash::compute_hash(b"");
                sqlx::query(&insert_sql)
                    .bind(&version_key)
                    .bind(b"" as &[u8])
                    .bind(&meta_str)
                    .bind(&empty_hash)
                    .bind(0i64)
                    .bind(false)
                    .bind(now)
                    .bind(now)
                    .execute(&pool)
                    .await
                    .map_err(|e| StorageError::Initialization {
                        backend: "sqlite".into(),
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

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    pub async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        let now = chrono::Utc::now().timestamp_millis();
        let sql = format!(
            "UPDATE {} SET metadata = json_set(metadata, '$.status', ?1), updated_at = ?2 WHERE id = ?3",
            self.table_name
        );
        sqlx::query(&sql)
            .bind(status)
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
        pool: &SqlitePool,
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
        pool: &SqlitePool,
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


/// Execute one upsert inside a transaction or pool executor. Shared by the
/// single-table batch and the cross-table atomic batch so both paths write
/// identical rows.
pub async fn exec_save<'e, E>(
    executor: E,
    table: &str,
    item: &BatchItem,
    now: i64,
) -> Result<(), StorageError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let hash = crate::util::hash::compute_hash(&item.data);
    let data_size = item.data_size();
    let metadata_str = item.metadata_json()?;
    let compressed = item.compressed();
    let sql = format!(
        "INSERT INTO {} (id, data, metadata, hash, data_size, compressed, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (id) DO UPDATE SET
            data = excluded.data,
            metadata = excluded.metadata,
            hash = excluded.hash,
            data_size = excluded.data_size,
            compressed = excluded.compressed,
            updated_at = excluded.updated_at",
        table
    );
    sqlx::query(&sql)
        .bind(&item.id)
        .bind(&item.data)
        .bind(&metadata_str)
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
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let sql = format!("DELETE FROM {} WHERE id = ?1", table);
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

