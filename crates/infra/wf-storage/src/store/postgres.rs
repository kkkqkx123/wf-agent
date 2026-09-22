use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;

use crate::domain::keys::{schema_version_key, SCHEMA_VERSION, SCHEMA_VERSION_EXCLUDE_PATTERN};
use crate::domain::store::{
    prefix_like_pattern, BatchItem, FilterBindValue, FilterCondition, Maintainable, QueryFilter,
    Store, StoreExt, StoreOperation,
};
use crate::error::StorageError;
use crate::util::pool::create_pg_pool;

#[derive(Debug, Clone)]
pub struct PostgresStorage {
    pool: PgPool,
    table_name: String,
}

impl PostgresStorage {
    pub async fn new(connection_string: &str, table_name: &str) -> Result<Self, StorageError> {
        let pool = create_pg_pool(connection_string).await?;
        Self::with_pool(pool, table_name).await
    }

    pub async fn with_pool(pool: PgPool, table_name: &str) -> Result<Self, StorageError> {
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

        let idx1 = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_entity_type ON {}((metadata->>'entityType'))",
            table_name, table_name
        );
        if let Err(e) = sqlx::query(&idx1).execute(&pool).await {
            tracing::warn!(
                table = table_name,
                error = %e,
                "failed to create entityType index (table functional without it)"
            );
        }

        let idx2 = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_status ON {}((metadata->>'status'))",
            table_name, table_name
        );
        if let Err(e) = sqlx::query(&idx2).execute(&pool).await {
            tracing::warn!(
                table = table_name,
                error = %e,
                "failed to create status index (table functional without it)"
            );
        }

        let idx3 = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_execution ON {}((metadata->>'executionId'))",
            table_name, table_name
        );
        if let Err(e) = sqlx::query(&idx3).execute(&pool).await {
            tracing::warn!(
                table = table_name,
                error = %e,
                "failed to create executionId index (table functional without it)"
            );
        }

        let idx4 = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_entity ON {}((metadata->>'entityId'))",
            table_name, table_name
        );
        if let Err(e) = sqlx::query(&idx4).execute(&pool).await {
            tracing::warn!(
                table = table_name,
                error = %e,
                "failed to create entityId index (table functional without it)"
            );
        }

        let idx_ts = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_timestamp ON {}(((metadata->>'timestamp')::float8))",
            table_name, table_name
        );
        if let Err(e) = sqlx::query(&idx_ts).execute(&pool).await {
            tracing::warn!(
                table = table_name,
                error = %e,
                "failed to create timestamp index (table functional without it)"
            );
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

    /// Truncate the given tables inside one transaction. Tables come from
    /// the storage context registry, never from external input. The internal
    /// schema version rows are removed as well, matching the per-table
    /// `clear` semantics. Crate-visible only; cache invalidation is the
    /// caller's job.
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
            let sql = format!("TRUNCATE TABLE {}", table);
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
fn build_select_sql(
    filter: Option<&QueryFilter>,
    table_name: &str,
    select_columns: &str,
) -> (String, Vec<FilterBindValue>) {
    let plan = filter.map(|f| f.compile());
    let mut sql = format!("SELECT {} FROM {}", select_columns, table_name);
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<FilterBindValue> = Vec::new();

    // Exclude internal schema version records from application queries.
    conditions.push(format!("id NOT LIKE '{}'", SCHEMA_VERSION_EXCLUDE_PATTERN));

    if let Some(p) = plan.as_ref() {
        for op in &p.conditions {
            match op {
                FilterCondition::Eq(key, value) => {
                    conditions.push(format!("metadata->>'{}' = ${}", key, params.len() + 1));
                    params.push(FilterBindValue::S(value.clone()));
                }
                FilterCondition::IdPrefix(prefix) => {
                    conditions.push(format!("id LIKE ${} ESCAPE '\\'", params.len() + 1));
                    params.push(FilterBindValue::S(prefix_like_pattern(prefix)));
                }
                FilterCondition::Prefix(key, prefix) => {
                    conditions.push(format!(
                        "metadata->>'{}' LIKE ${} ESCAPE '\\'",
                        key,
                        params.len() + 1
                    ));
                    params.push(FilterBindValue::S(prefix_like_pattern(prefix)));
                }
                FilterCondition::Lt(key, value) => {
                    conditions.push(format!(
                        "(jsonb_typeof(metadata->'{}') = 'number' AND (metadata->>'{}')::float8 < ${})",
                        key,
                        key,
                        params.len() + 1
                    ));
                    params.push(FilterBindValue::I(*value));
                }
                FilterCondition::Gt(key, value) => {
                    conditions.push(format!(
                        "(jsonb_typeof(metadata->'{}') = 'number' AND (metadata->>'{}')::float8 > ${})",
                        key,
                        key,
                        params.len() + 1
                    ));
                    params.push(FilterBindValue::I(*value));
                }
                FilterCondition::Between(key, start, end) => {
                    conditions.push(format!(
                        "(jsonb_typeof(metadata->'{}') = 'number' AND (metadata->>'{}')::float8 >= ${} AND (metadata->>'{}')::float8 <= ${})",
                        key,
                        key,
                        params.len() + 1,
                        key,
                        params.len() + 2
                    ));
                    params.push(FilterBindValue::I(*start));
                    params.push(FilterBindValue::I(*end));
                }
                FilterCondition::In(key, values) => {
                    if values.is_empty() {
                        conditions.push("false".into());
                    } else {
                        let placeholders: Vec<String> = (0..values.len())
                            .map(|i| format!("${}", params.len() + i + 1))
                            .collect();
                        conditions.push(format!(
                            "metadata->>'{}' IN ({})",
                            key,
                            placeholders.join(", ")
                        ));
                        params.extend(values.iter().cloned().map(FilterBindValue::S));
                    }
                }
            }
        }
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    if let Some((key, descending)) = plan.as_ref().and_then(|p| p.order_by.clone()) {
        // Numeric-aware ordering: numeric metadata values sort by their
        // numeric value, everything else falls back to NULL (excluded from
        // numeric comparison) so a cast can never fail the query. NULLS LAST
        // keeps the direction predictable for both ASC and DESC; the trailing
        // id follows the primary direction so ties resolve to creation order
        // on both newest-first and oldest-first queries.
        let direction = if descending { "DESC" } else { "ASC" };
        sql.push_str(&format!(
            " ORDER BY (CASE WHEN jsonb_typeof(metadata->'{}') = 'number' THEN (metadata->>'{}')::float8 END) {} NULLS LAST, id {}",
            key,
            key,
            direction,
            direction
        ));
    }
    if let Some(limit) = plan.as_ref().and_then(|p| p.limit) {
        sql.push_str(&format!(" LIMIT {}", limit));
    }
    if let Some(offset) = plan.as_ref().and_then(|p| p.offset) {
        sql.push_str(&format!(" OFFSET {}", offset));
    }

    (sql, params)
}

#[async_trait]
impl Store for PostgresStorage {
    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError> {
        let now = chrono::Utc::now().timestamp_millis();
        let hash = crate::util::hash::compute_hash(data);
        let data_size = data.len() as i64;
        let compressed = crate::domain::store::metadata_compressed(metadata);

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
            self.table_name
        );
        sqlx::query(&sql)
            .bind(id)
            .bind(data)
            .bind(metadata)
            .bind(&hash)
            .bind(data_size)
            .bind(compressed)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "save".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, Value)>, StorageError> {
        let sql = format!(
            "SELECT data, metadata, hash FROM {} WHERE id = $1",
            self.table_name
        );
        let result: Option<(Vec<u8>, Value, String)> = sqlx::query_as(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "load".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;

        match result {
            Some((data, metadata, hash)) => {
                crate::util::hash::verify_integrity(id, &data, &hash)?;
                Ok(Some((data, metadata)))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        let sql = format!("DELETE FROM {} WHERE id = $1", self.table_name);
        sqlx::query(&sql)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "delete".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(())
    }

    async fn list(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(String, Value)>, StorageError> {
        let (sql, params) = build_select_sql(filter, &self.table_name, "id, metadata");
        let mut query = sqlx::query_as::<_, (String, Value)>(&sql);
        for param in &params {
            match param {
                FilterBindValue::S(s) => query = query.bind(s),
                FilterBindValue::I(i) => query = query.bind(*i),
            }
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "list".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;

        Ok(rows)
    }

    async fn list_data(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(Vec<u8>, Value)>, StorageError> {
        let (sql, params) = build_select_sql(filter, &self.table_name, "id, data, metadata, hash");
        let mut query = sqlx::query_as::<_, (String, Vec<u8>, Value, String)>(&sql);
        for param in &params {
            match param {
                FilterBindValue::S(s) => query = query.bind(s),
                FilterBindValue::I(i) => query = query.bind(*i),
            }
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "list_data".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;

        rows.into_iter()
            .map(|(id, data, metadata, hash)| {
                crate::util::hash::verify_integrity(&id, &data, &hash)?;
                Ok((data, metadata))
            })
            .collect()
    }

    async fn count(&self, filter: Option<&QueryFilter>) -> Result<u64, StorageError> {
        // Counting reports total matches: ordering and pagination are
        // stripped so a page-sized filter still counts the whole set.
        let stripped = filter.map(|f| f.stripped_for_count());
        let (sql, params) = build_select_sql(stripped.as_ref(), &self.table_name, "1");
        let sql = format!("SELECT COUNT(*) FROM ({}) AS filtered", sql);
        let mut query = sqlx::query_scalar::<_, i64>(&sql);
        for param in &params {
            match param {
                FilterBindValue::S(s) => query = query.bind(s),
                FilterBindValue::I(i) => query = query.bind(*i),
            }
        }
        let count = query
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "count".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(count as u64)
    }

    async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        let sql = format!("SELECT 1 FROM {} WHERE id = $1 LIMIT 1", self.table_name);
        let result: Option<(i32,)> = sqlx::query_as(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "exists".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(result.is_some())
    }

    async fn clear(&self) -> Result<(), StorageError> {
        let sql = format!("TRUNCATE TABLE {}", self.table_name);
        sqlx::query(&sql)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "clear".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(())
    }
}

/// Execute one upsert inside a transaction or pool executor. Shared by the
/// single-table batch and the cross-table atomic batch so both paths write
/// identical rows.
async fn exec_save<'e, E>(
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
async fn exec_delete<'e, E>(executor: E, table: &str, id: &str) -> Result<(), StorageError>
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

#[async_trait]
impl StoreExt for PostgresStorage {
    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        PostgresStorage::update_status(self, id, status).await
    }

    async fn count_by_field(
        &self,
        field: &str,
    ) -> Result<std::collections::HashMap<String, u64>, StorageError> {
        // Internal schema version rows are excluded like in every other
        // application query.
        let sql = format!(
            "SELECT metadata->>'{}' AS k, COUNT(*) AS c FROM {} WHERE id NOT LIKE '{}' GROUP BY k",
            field, self.table_name, SCHEMA_VERSION_EXCLUDE_PATTERN
        );
        let rows: Vec<(Option<String>, i64)> = sqlx::query_as(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "count_by_field".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(rows
            .into_iter()
            .filter_map(|(key, count)| key.map(|k| (k, count as u64)))
            .collect())
    }

    async fn apply_batch(&self, operations: &[StoreOperation]) -> Result<(), StorageError> {
        if operations.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(|e| StorageError::General {
            operation: "apply_batch".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;

        let now = chrono::Utc::now().timestamp_millis();
        for operation in operations {
            match operation {
                StoreOperation::Save(item) => {
                    exec_save(&mut *tx, &self.table_name, item, now).await?;
                }
                StoreOperation::Delete(id) => {
                    exec_delete(&mut *tx, &self.table_name, id).await?;
                }
            }
        }

        tx.commit().await.map_err(|e| StorageError::General {
            operation: "apply_batch".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;
        Ok(())
    }

    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<u8>, Value)>, StorageError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT id, data, metadata, hash FROM {} WHERE id = ANY($1)",
            self.table_name
        );
        let rows = sqlx::query_as::<_, (String, Vec<u8>, Value, String)>(&sql)
            .bind(ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "load_batch".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        rows.into_iter()
            .map(|(id, data, metadata, hash)| {
                crate::util::hash::verify_integrity(&id, &data, &hash)?;
                Ok((id, data, metadata))
            })
            .collect()
    }

    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        if items.is_empty() {
            return Ok(());
        }
        // Chunked UNNEST transfers inside one transaction: the batch stays
        // atomic however large it grows.
        let mut tx = self.pool.begin().await.map_err(|e| StorageError::General {
            operation: "save_batch".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;
        let now = chrono::Utc::now().timestamp_millis();
        for chunk in items.chunks(1000) {
            let ids: Vec<&str> = chunk.iter().map(|i| i.id.as_str()).collect();
            let datas: Vec<&[u8]> = chunk.iter().map(|i| i.data.as_slice()).collect();
            let metadatas: Vec<&Value> = chunk.iter().map(|i| &i.metadata).collect();
            let hashes: Vec<String> = chunk
                .iter()
                .map(|i| crate::util::hash::compute_hash(&i.data))
                .collect();
            let hashes_ref: Vec<&str> = hashes.iter().map(|h| h.as_str()).collect();
            let sizes: Vec<i64> = chunk.iter().map(|i| i.data_size()).collect();
            let compresseds: Vec<bool> = chunk.iter().map(|i| i.compressed()).collect();
            let nows: Vec<i64> = (0..chunk.len()).map(|_| now).collect();

            let sql = format!(
                "INSERT INTO {} (id, data, metadata, hash, data_size, compressed, created_at, updated_at)
                 SELECT * FROM UNNEST($1::text[], $2::bytea[], $3::jsonb[], $4::text[], $5::int8[], $6::boolean[], $7::bigint[], $8::bigint[])
                 ON CONFLICT (id) DO UPDATE SET
                    data = EXCLUDED.data,
                    metadata = EXCLUDED.metadata,
                    hash = EXCLUDED.hash,
                    data_size = EXCLUDED.data_size,
                    compressed = EXCLUDED.compressed,
                    updated_at = EXCLUDED.updated_at",
                self.table_name
            );

            sqlx::query(&sql)
                .bind(&ids)
                .bind(&datas)
                .bind(&metadatas)
                .bind(&hashes_ref)
                .bind(&sizes)
                .bind(&compresseds)
                .bind(&nows)
                .bind(&nows)
                .execute(&mut *tx)
                .await
                .map_err(|e| StorageError::General {
                    operation: "save_batch".into(),
                    message: e.to_string(),
                    source: Some(Box::new(e)),
                })?;
        }
        tx.commit().await.map_err(|e| StorageError::General {
            operation: "save_batch".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;
        Ok(())
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        if ids.is_empty() {
            return Ok(());
        }
        let sql = format!("DELETE FROM {} WHERE id = ANY($1)", self.table_name);
        sqlx::query(&sql)
            .bind(ids)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "delete_batch".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(())
    }
}

#[async_trait]
impl Maintainable for PostgresStorage {
    // PostgreSQL has no per-table checkpoint a regular user can trigger:
    // the CHECKPOINT command requires superuser privileges and applies to
    // the whole cluster. WAL advancement is handled internally by the
    // server, so this is a no-op like `sync`.
    async fn wal_checkpoint(&self) -> Result<(), StorageError> {
        Ok(())
    }

    async fn vacuum(&self) -> Result<(), StorageError> {
        // Per-table vacuum: a bare VACUUM ANALYZE would process every table
        // in the database. The table name comes from the entity registry,
        // never from external input, so interpolation is safe.
        let sql = format!("VACUUM (ANALYZE) {}", self.table_name);
        sqlx::query(&sql)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "vacuum".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(())
    }

    async fn sync(&self) -> Result<(), StorageError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::pool::sanitize_connection_string;

    #[tokio::test]
    async fn test_postgres_pool_sanitization() {
        let conn = "postgres://user:secret@localhost:5432/testdb";
        let sanitized = sanitize_connection_string(conn);
        assert!(!sanitized.contains("secret"));
        assert!(sanitized.contains("***"));
        assert!(sanitized.contains("localhost"));
        assert!(sanitized.contains("testdb"));
    }

    #[tokio::test]
    async fn test_postgres_new_with_invalid_url() {
        let result = PostgresStorage::new("not-a-url", "test").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_build_select_sql_numeric_guards() {
        let filter = QueryFilter::new()
            .with_field_lt("timestamp", 100)
            .with_order_by("timestamp", true)
            .with_limit(10);
        let (sql, params) = build_select_sql(Some(&filter), "checkpoint", "id, metadata");

        // Numeric predicates must not fail on non-numeric values.
        assert!(sql.contains("jsonb_typeof(metadata->'timestamp') = 'number'"));
        // float8 keeps integer and floating-point metadata comparable.
        assert!(sql.contains("::float8 <"));
        assert!(!sql.contains("::bigint"));
        // Ordering must fall back safely instead of casting blindly.
        assert!(sql.contains("CASE WHEN jsonb_typeof"));
        assert!(sql.contains("NULLS LAST"));
        // The id tie-break follows the primary direction so ties resolve to
        // creation order on both newest-first and oldest-first queries.
        assert!(sql.contains(", id DESC"));
        assert_eq!(params.len(), 1);

        let filter = QueryFilter::new()
            .with_field_lt("timestamp", 100)
            .with_order_by("timestamp", false)
            .with_limit(10);
        let (sql, _) = build_select_sql(Some(&filter), "checkpoint", "id, metadata");
        assert!(sql.contains(", id ASC"));

        let filter = QueryFilter::new().with_field_in("entityId", vec!["a".into()]);
        let (sql, params) = build_select_sql(Some(&filter), "checkpoint", "id, metadata");
        assert!(sql.contains("metadata->>'entityId' IN"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_build_select_sql_prefix_uses_like() {
        let filter = QueryFilter::new().with_id_prefix("wf-");
        let (sql, params) = build_select_sql(Some(&filter), "checkpoint", "id, metadata");
        assert!(sql.contains("id LIKE $1 ESCAPE"));
        assert!(!sql.contains("substr"));
        assert_eq!(params.len(), 1);
        assert_eq!(
            params[0],
            crate::domain::store::FilterBindValue::S("wf-%".into())
        );

        let filter = QueryFilter::new().with_field_prefix("name", "100%");
        let (sql, params) = build_select_sql(Some(&filter), "checkpoint", "id, metadata");
        assert!(sql.contains("metadata->>'name' LIKE $1 ESCAPE"));
        assert_eq!(params.len(), 1);
        assert_eq!(
            params[0],
            crate::domain::store::FilterBindValue::S("100\\%%".into())
        );
    }
}
