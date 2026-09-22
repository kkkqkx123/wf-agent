use async_trait::async_trait;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::domain::keys::{schema_version_key, SCHEMA_VERSION, SCHEMA_VERSION_EXCLUDE_PATTERN};
use crate::domain::store::{
    prefix_like_pattern, BatchItem, FilterBindValue, FilterCondition, Maintainable, QueryFilter,
    Store, StoreExt, StoreOperation,
};
use crate::error::StorageError;

/// Normalizes a metadata value to its text representation, mirroring
/// PostgreSQL's `metadata->>'key'` operator: strings as-is, numbers in
/// canonical decimal form, booleans as 'true' / 'false'. Sqlite's
/// `json_extract` yields 1/0 for booleans, but `json_type` still reports
/// 'true' / 'false', so the boolean arms restore the text form while every
/// other type keeps the plain cast.
fn metadata_text_expr(key: &str) -> String {
    format!(
        "(CASE WHEN json_type(metadata, '$.{0}') = 'true' THEN 'true' \
         WHEN json_type(metadata, '$.{0}') = 'false' THEN 'false' \
         ELSE CAST(json_extract(metadata, '$.{0}') AS TEXT) END)",
        key
    )
}

/// True when the metadata value is a JSON number, mirroring PostgreSQL's
/// `jsonb_typeof(...) = 'number'` guard so that numeric predicates never
/// match non-numeric values.
fn is_numeric_expr(key: &str) -> String {
    format!("json_type(metadata, '$.{}') IN ('integer', 'real')", key)
}

/// Translates a QueryFilter into a complete SELECT statement.
/// Field names come from a fixed metadata schema, so interpolation is safe.
/// The filter is first normalized through the shared compiled plan; only
/// placeholder style and JSON operators are Sqlite-specific. Mirror any
/// semantic change in the PostgreSQL renderer.
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
                    conditions.push(format!("{} = ?", metadata_text_expr(key)));
                    params.push(FilterBindValue::S(value.clone()));
                }
                FilterCondition::IdPrefix(prefix) => {
                    conditions.push("id LIKE ? ESCAPE '\\'".into());
                    params.push(FilterBindValue::S(prefix_like_pattern(prefix)));
                }
                FilterCondition::Prefix(key, prefix) => {
                    conditions.push(format!("{} LIKE ? ESCAPE '\\'", metadata_text_expr(key)));
                    params.push(FilterBindValue::S(prefix_like_pattern(prefix)));
                }
                FilterCondition::Lt(key, value) => {
                    conditions.push(format!(
                        "({} AND json_extract(metadata, '$.{}') < ?)",
                        is_numeric_expr(key),
                        key
                    ));
                    params.push(FilterBindValue::I(*value));
                }
                FilterCondition::Gt(key, value) => {
                    conditions.push(format!(
                        "({} AND json_extract(metadata, '$.{}') > ?)",
                        is_numeric_expr(key),
                        key
                    ));
                    params.push(FilterBindValue::I(*value));
                }
                FilterCondition::Between(key, start, end) => {
                    conditions.push(format!(
                        "({} AND json_extract(metadata, '$.{}') >= ? AND json_extract(metadata, '$.{}') <= ?)",
                        is_numeric_expr(key),
                        key,
                        key
                    ));
                    params.push(FilterBindValue::I(*start));
                    params.push(FilterBindValue::I(*end));
                }
                FilterCondition::In(key, values) => {
                    if values.is_empty() {
                        conditions.push("0 = 1".into());
                    } else {
                        let placeholders: Vec<String> =
                            (0..values.len()).map(|_| "?".into()).collect();
                        conditions.push(format!(
                            "{} IN ({})",
                            metadata_text_expr(key),
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
        // Numeric-aware ordering matching PostgreSQL: numeric values sort by
        // their numeric value and always come first, everything else (missing
        // keys and non-numeric values) sorts last in both directions. The
        // leading flag column emulates PostgreSQL's `NULLS LAST`; the
        // trailing id follows the primary direction so ties resolve to
        // creation order on both newest-first and oldest-first queries.
        let direction = if descending { "DESC" } else { "ASC" };
        sql.push_str(&format!(
            " ORDER BY (CASE WHEN {} THEN 0 ELSE 1 END) ASC, json_extract(metadata, '$.{}') {}, id {}",
            is_numeric_expr(&key),
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

#[derive(Debug, Clone)]
pub struct SqliteStorage {
    pool: SqlitePool,
    table_name: String,
}

impl SqliteStorage {
    pub async fn new(path: &str, table_name: &str) -> Result<Self, StorageError> {
        let pool = Self::create_pool(path).await?;
        Self::with_pool(pool, table_name).await
    }

    pub async fn create_pool(path: &str) -> Result<SqlitePool, StorageError> {
        crate::util::pool::create_sqlite_pool(path).await
    }

    pub async fn with_pool(pool: SqlitePool, table_name: &str) -> Result<Self, StorageError> {
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

        // Metadata text indexes use the same boolean-normalized expression as
        // the query renderer so the planner can match them. Databases created
        // before the normalization carry the plain CAST version under the same
        // name; drop the stale definition once so it is rebuilt below.
        for (suffix, key) in [
            ("entity_type", "entityType"),
            ("status", "status"),
            ("execution", "executionId"),
            ("entity", "entityId"),
        ] {
            let name = format!("idx_{}_{}", table_name, suffix);
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
                metadata_text_expr(key)
            );
            sqlx::query(&create).execute(&pool).await.ok();
        }

        let idx_ts = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_timestamp ON {}(json_extract(metadata, '$.timestamp'))",
            table_name, table_name
        );
        sqlx::query(&idx_ts).execute(&pool).await.ok();

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

    /// Delete every row of the given tables inside one transaction. Tables
    /// come from the storage context registry, never from external input.
    /// The internal schema version rows are removed as well, matching the
    /// per-table `clear` semantics. Crate-visible only; cache invalidation
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
            let sql = format!("DELETE FROM {}", table);
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

#[async_trait]
impl Store for SqliteStorage {
    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError> {
        let now = chrono::Utc::now().timestamp_millis();
        let hash = crate::util::hash::compute_hash(data);
        let data_size = data.len() as i64;
        let metadata_str = serde_json::to_string(metadata)?;
        let compressed = crate::domain::store::metadata_compressed(metadata);

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
            self.table_name
        );
        sqlx::query(&sql)
            .bind(id)
            .bind(data)
            .bind(&metadata_str)
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
            "SELECT data, metadata, hash FROM {} WHERE id = ?1",
            self.table_name
        );
        let result: Option<(Vec<u8>, String, String)> = sqlx::query_as(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "load".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;

        match result {
            Some((data, metadata_str, hash)) => {
                crate::util::hash::verify_integrity(id, &data, &hash)?;
                let metadata: Value = serde_json::from_str(&metadata_str)?;
                Ok(Some((data, metadata)))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        let sql = format!("DELETE FROM {} WHERE id = ?1", self.table_name);
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
        let mut query = sqlx::query_as::<_, (String, String)>(&sql);
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

        rows.into_iter()
            .map(|(id, metadata_str)| {
                let metadata: Value = serde_json::from_str(&metadata_str)?;
                Ok((id, metadata))
            })
            .collect()
    }

    async fn list_data(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(Vec<u8>, Value)>, StorageError> {
        let (sql, params) = build_select_sql(filter, &self.table_name, "id, data, metadata, hash");
        let mut query = sqlx::query_as::<_, (String, Vec<u8>, String, String)>(&sql);
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
            .map(|(id, data, metadata_str, hash)| {
                crate::util::hash::verify_integrity(&id, &data, &hash)?;
                let metadata: Value = serde_json::from_str(&metadata_str)?;
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
        let sql = format!("SELECT 1 FROM {} WHERE id = ?1 LIMIT 1", self.table_name);
        let result: Option<(i64,)> = sqlx::query_as(&sql)
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
        let sql = format!("DELETE FROM {}", self.table_name);
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
async fn exec_delete<'e, E>(executor: E, table: &str, id: &str) -> Result<(), StorageError>
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

#[async_trait]
impl StoreExt for SqliteStorage {
    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        SqliteStorage::update_status(self, id, status).await
    }

    async fn count_by_field(
        &self,
        field: &str,
    ) -> Result<std::collections::HashMap<String, u64>, StorageError> {
        // Internal schema version rows are excluded like in every other
        // application query; the boolean-normalized text expression keeps
        // grouped keys identical to the Eq filter vocabulary.
        let sql = format!(
            "SELECT {} AS k, COUNT(*) AS c FROM {} WHERE id NOT LIKE '{}' GROUP BY k",
            metadata_text_expr(field),
            self.table_name,
            SCHEMA_VERSION_EXCLUDE_PATTERN
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
        // Chunked so large id sets stay under the SQLite variable limit.
        let mut results = Vec::new();
        for chunk in ids.chunks(500) {
            let placeholders: Vec<String> = (0..chunk.len()).map(|_| "?".to_string()).collect();
            let sql = format!(
                "SELECT id, data, metadata, hash FROM {} WHERE id IN ({})",
                self.table_name,
                placeholders.join(", ")
            );
            let mut query = sqlx::query_as::<_, (String, Vec<u8>, String, String)>(&sql);
            for id in chunk {
                query = query.bind(id);
            }
            let rows = query
                .fetch_all(&self.pool)
                .await
                .map_err(|e| StorageError::General {
                    operation: "load_batch".into(),
                    message: e.to_string(),
                    source: Some(Box::new(e)),
                })?;
            for (id, data, metadata_str, hash) in rows {
                crate::util::hash::verify_integrity(&id, &data, &hash)?;
                let metadata: Value = serde_json::from_str(&metadata_str)?;
                results.push((id, data, metadata));
            }
        }
        Ok(results)
    }

    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        if items.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(|e| StorageError::General {
            operation: "save_batch".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;

        // Eight bound values per row: chunks of 100 stay under the SQLite
        // variable limit with margin.
        for chunk in items.chunks(100) {
            let mut sql = format!(
                "INSERT INTO {} (id, data, metadata, hash, data_size, compressed, created_at, updated_at) VALUES ",
                self.table_name
            );
            let placeholders: Vec<String> = (0..chunk.len())
                .map(|_| "(?, ?, ?, ?, ?, ?, ?, ?)".to_string())
                .collect();
            sql.push_str(&placeholders.join(", "));
            sql.push_str(
                " ON CONFLICT (id) DO UPDATE SET
                    data = excluded.data,
                    metadata = excluded.metadata,
                    hash = excluded.hash,
                    data_size = excluded.data_size,
                    compressed = excluded.compressed,
                    updated_at = excluded.updated_at",
            );

            let now = chrono::Utc::now().timestamp_millis();
            let mut query = sqlx::query(&sql);
            for item in chunk {
                let hash = crate::util::hash::compute_hash(&item.data);
                let metadata_str = item.metadata_json()?;
                let compressed = item.compressed();
                let data_size = item.data_size();
                let id = item.id.clone();
                query = query
                    .bind(id)
                    .bind(item.data.clone())
                    .bind(metadata_str)
                    .bind(hash)
                    .bind(data_size)
                    .bind(compressed)
                    .bind(now)
                    .bind(now);
            }
            query
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
        let mut tx = self.pool.begin().await.map_err(|e| StorageError::General {
            operation: "delete_batch".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;

        for chunk in ids.chunks(999) {
            let placeholders: Vec<String> = (1..=chunk.len()).map(|i| format!("?{}", i)).collect();
            let sql = format!(
                "DELETE FROM {} WHERE id IN ({})",
                self.table_name,
                placeholders.join(", ")
            );
            let mut query = sqlx::query(&sql);
            for id in chunk {
                query = query.bind(id);
            }
            query
                .execute(&mut *tx)
                .await
                .map_err(|e| StorageError::General {
                    operation: "delete_batch".into(),
                    message: e.to_string(),
                    source: Some(Box::new(e)),
                })?;
        }

        tx.commit().await.map_err(|e| StorageError::General {
            operation: "delete_batch".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;
        Ok(())
    }
}

#[async_trait]
impl Maintainable for SqliteStorage {
    async fn vacuum(&self) -> Result<(), StorageError> {
        sqlx::query("VACUUM")
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "vacuum".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(())
    }

    async fn wal_checkpoint(&self) -> Result<(), StorageError> {
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "wal_checkpoint".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(())
    }

    async fn sync(&self) -> Result<(), StorageError> {
        sqlx::query("PRAGMA wal_checkpoint(PASSIVE)")
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "sync".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::store::StoreExt;

    #[tokio::test]
    async fn test_sqlite_save_load() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        store
            .save(
                "id1",
                b"hello world",
                &serde_json::json!({"entityType": "test"}),
            )
            .await
            .unwrap();
        let (data, meta) = store.load("id1").await.unwrap().unwrap();
        assert_eq!(data, b"hello world");
        assert_eq!(meta["entityType"], "test");
    }

    #[tokio::test]
    async fn test_sqlite_list_filter() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        store
            .save(
                "id1",
                b"data1",
                &serde_json::json!({"entityType": "A", "status": "active"}),
            )
            .await
            .unwrap();
        store
            .save(
                "id2",
                b"data2",
                &serde_json::json!({"entityType": "B", "status": "inactive"}),
            )
            .await
            .unwrap();

        let filter = QueryFilter::new().with_entity_type("A");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "id1");
    }

    #[tokio::test]
    async fn test_sqlite_list_pushdown_ops() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        for i in 0..5 {
            store
                .save(
                    &format!("wf-{}:v{}", i, 1),
                    b"data",
                    &serde_json::json!({"entityType": "workflow", "timestamp": 1000 + i}),
                )
                .await
                .unwrap();
        }

        let filter = QueryFilter::new().with_id_prefix("wf-");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 5);

        let filter = QueryFilter::new()
            .with_field("entityType", "workflow")
            .with_order_by("timestamp", true)
            .with_limit(2);
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].1["timestamp"], 1004);
        assert_eq!(results[1].1["timestamp"], 1003);

        let filter = QueryFilter::new().with_field_lt("timestamp", 1003);
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 3);

        let counts = store.count_by_field("entityType").await.unwrap();
        assert_eq!(*counts.get("workflow").unwrap(), 5);
    }

    #[tokio::test]
    async fn test_sqlite_batch() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        let items: Vec<BatchItem> = (0..100)
            .map(|i| {
                BatchItem::new(
                    format!("id_{}", i),
                    vec![i as u8; 50],
                    serde_json::json!({"index": i}),
                )
            })
            .collect();
        store.save_batch(&items).await.unwrap();
        assert_eq!(store.list(None).await.unwrap().len(), 100);
    }

    #[tokio::test]
    async fn test_sqlite_filter_semantics_aligned() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        store
            .save(
                "n1",
                b"data",
                &serde_json::json!({"entityType": "wf", "timestamp": 1000, "flag": true}),
            )
            .await
            .unwrap();
        store
            .save(
                "s1",
                b"data",
                &serde_json::json!({"entityType": "wf", "timestamp": "abc", "flag": false}),
            )
            .await
            .unwrap();
        store
            .save(
                "s2",
                b"data",
                &serde_json::json!({"entityType": "wf", "timestamp": "500"}),
            )
            .await
            .unwrap();

        // Eq compares text representations: the canonical decimal form
        // matches, but numeric variants like '1e3' do not.
        let filter = QueryFilter::new().with_field("timestamp", "1000");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "n1");

        let filter = QueryFilter::new().with_field("timestamp", "1e3");
        assert!(store.list(Some(&filter)).await.unwrap().is_empty());

        // Booleans match their 'true' / 'false' text form like on the other
        // backends; the integer-looking strings '1' / '0' must not match.
        let filter = QueryFilter::new().with_field("flag", "true");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "n1");

        let filter = QueryFilter::new().with_field("flag", "false");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "s1");

        let filter = QueryFilter::new().with_field("flag", "1");
        assert!(store.list(Some(&filter)).await.unwrap().is_empty());

        // Numeric predicates only match JSON numbers: the numeric-looking
        // string "500" must be excluded just like "abc".
        let filter = QueryFilter::new().with_field_lt("timestamp", 1000);
        assert!(store.list(Some(&filter)).await.unwrap().is_empty());

        // OrderBy puts numeric values first in both directions.
        let filter = QueryFilter::new().with_order_by("timestamp", true);
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results[0].0, "n1");
        assert_eq!(results.len(), 3);

        let filter = QueryFilter::new().with_order_by("timestamp", false);
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results[0].0, "n1");
    }

    #[tokio::test]
    async fn test_sqlite_prefix_like_matches_literal_percent() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        for id in ["wf-1", "wf-2", "other-1", "100%-x"] {
            store
                .save(id, b"data", &serde_json::json!({"entityType": "wf"}))
                .await
                .unwrap();
        }

        let filter = QueryFilter::new().with_id_prefix("wf-");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 2);

        // A '%' inside the prefix stays literal instead of wildcarding.
        let filter = QueryFilter::new().with_id_prefix("100%");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "100%-x");
    }

    #[tokio::test]
    async fn test_sqlite_count_ignores_pagination() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        for i in 0..5 {
            store
                .save(
                    &format!("id-{}", i),
                    b"data",
                    &serde_json::json!({"entityType": "wf", "timestamp": i}),
                )
                .await
                .unwrap();
        }

        let filter = QueryFilter::new()
            .with_field("entityType", "wf")
            .with_order_by("timestamp", true)
            .with_offset(1)
            .with_limit(2);
        assert_eq!(store.list(Some(&filter)).await.unwrap().len(), 2);
        assert_eq!(store.count(Some(&filter)).await.unwrap(), 5);
    }

    #[tokio::test]
    async fn test_sqlite_count_by_field_excludes_version_row() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        store
            .save("id1", b"data", &serde_json::json!({"kind": "a"}))
            .await
            .unwrap();
        let counts = store.count_by_field("kind").await.unwrap();
        assert_eq!(*counts.get("a").unwrap(), 1);
        assert!(counts.keys().all(|k| !k.contains("__schema")));

        let version_counts = store.count_by_field("version").await.unwrap();
        assert!(version_counts.is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_large_and_empty_batches() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        // 250 rows exceed the old single-statement variable budget; chunking
        // must absorb them.
        let items: Vec<BatchItem> = (0..250)
            .map(|i| {
                BatchItem::new(
                    format!("bulk-{}", i),
                    vec![i as u8; 10],
                    serde_json::json!({"index": i}),
                )
            })
            .collect();
        store.save_batch(&items).await.unwrap();
        assert_eq!(store.count(None).await.unwrap(), 250);

        let ids: Vec<String> = (0..250).map(|i| format!("bulk-{}", i)).collect();
        assert_eq!(store.load_batch(&ids).await.unwrap().len(), 250);

        // Empty batches are no-ops without opening a transaction.
        store.save_batch(&[]).await.unwrap();
        store.load_batch(&[]).await.unwrap();
        store.delete_batch(&[]).await.unwrap();
        assert_eq!(store.count(None).await.unwrap(), 250);

        store.delete_batch(&ids).await.unwrap();
        assert_eq!(store.count(None).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_sqlite_update_status() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        store
            .save(
                "exec1",
                b"data",
                &serde_json::json!({"entityType": "execution", "status": "pending"}),
            )
            .await
            .unwrap();
        store.update_status("exec1", "running").await.unwrap();
        let (_, meta) = store.load("exec1").await.unwrap().unwrap();
        assert_eq!(meta["status"], "running");
    }

    #[tokio::test]
    async fn test_sqlite_apply_batch_mixed_ops() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        for i in 0..5 {
            store
                .save(
                    &format!("cp-{}", i),
                    b"data",
                    &serde_json::json!({"entityType": "checkpoint", "index": i}),
                )
                .await
                .unwrap();
        }

        // Atomic batch: delete three checkpoints and write the watermark
        // record in the same transaction.
        let operations = vec![
            StoreOperation::Delete("cp-0".to_string()),
            StoreOperation::Delete("cp-1".to_string()),
            StoreOperation::Delete("cp-2".to_string()),
            StoreOperation::Save(BatchItem::new(
                "__watermark__:exec-1",
                Vec::new(),
                serde_json::json!({"cleanupWatermark": 1000, "cleanupRunCount": 1}),
            )),
        ];
        store.apply_batch(&operations).await.unwrap();

        assert!(!store.exists("cp-0").await.unwrap());
        assert!(!store.exists("cp-2").await.unwrap());
        assert!(store.exists("cp-3").await.unwrap());
        let (_, meta) = store.load("__watermark__:exec-1").await.unwrap().unwrap();
        assert_eq!(meta["cleanupWatermark"], 1000);
        assert_eq!(store.list(None).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_sqlite_apply_batch_empty_is_noop() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        store.apply_batch(&[]).await.unwrap();
        assert!(store.list(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_cross_table_rolls_back_on_failure() {
        use crate::domain::store::CrossTableOperation;

        let pool = SqliteStorage::create_pool(":memory:").await.unwrap();
        let first = SqliteStorage::with_pool(pool.clone(), "rollback_a")
            .await
            .unwrap();
        let _second = SqliteStorage::with_pool(pool.clone(), "rollback_b")
            .await
            .unwrap();

        let save = StoreOperation::Save(BatchItem::new(
            "id1",
            b"data".to_vec(),
            serde_json::json!({"entityType": "test"}),
        ));
        let delete_missing_table = StoreOperation::Delete("id1".into());
        let operations = vec![
            CrossTableOperation {
                table: "rollback_a",
                operation: &save,
            },
            CrossTableOperation {
                table: "no_such_table",
                operation: &delete_missing_table,
            },
        ];
        let result = SqliteStorage::apply_cross_table(&pool, &operations).await;
        assert!(result.is_err());
        // The first write must not survive the failed batch: list hides the
        // internal schema version row, so empty means fully rolled back.
        assert!(first.list(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_save_preserves_created_at() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        store
            .save("id1", b"v1", &serde_json::json!({"entityType": "test"}))
            .await
            .unwrap();
        let sql = format!(
            "SELECT created_at FROM {} WHERE id = ?1",
            store.table_name()
        );
        let first: i64 = sqlx::query_scalar(&sql)
            .bind("id1")
            .fetch_one(store.pool())
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        store
            .save("id1", b"v2", &serde_json::json!({"entityType": "test"}))
            .await
            .unwrap();
        let second: i64 = sqlx::query_scalar(&sql)
            .bind("id1")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(first, second, "overwrite must keep the original created_at");
    }

    #[tokio::test]
    async fn test_sqlite_save_batch_preserves_created_at() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        let items: Vec<BatchItem> = (0..3)
            .map(|i| {
                BatchItem::new(
                    format!("id_{}", i),
                    vec![i as u8; 10],
                    serde_json::json!({"index": i}),
                )
            })
            .collect();
        store.save_batch(&items).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let re_saved: Vec<BatchItem> = (0..3)
            .map(|i| {
                BatchItem::new(
                    format!("id_{}", i),
                    vec![0xAA; 10],
                    serde_json::json!({"index": i}),
                )
            })
            .collect();
        store.save_batch(&re_saved).await.unwrap();

        let sql = format!(
            "SELECT created_at FROM {} WHERE id = ?1",
            store.table_name()
        );
        for i in 0..3 {
            let created: i64 = sqlx::query_scalar(&sql)
                .bind(format!("id_{}", i))
                .fetch_one(store.pool())
                .await
                .unwrap();
            assert!(created > 0);
        }
        let all: Vec<(String, Vec<u8>)> = sqlx::query_as(&format!(
            "SELECT id, data FROM {} WHERE id NOT LIKE '{}'",
            store.table_name(),
            SCHEMA_VERSION_EXCLUDE_PATTERN
        ))
        .fetch_all(store.pool())
        .await
        .unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.iter().all(|(_, data)| data == &vec![0xAA; 10]));
    }
}
