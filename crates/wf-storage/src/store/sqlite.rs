use async_trait::async_trait;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::domain::store::{
    BatchItem, BatchStore, FilterOp, Maintainable, QueryFilter, Store, StoreOperation,
};
use crate::error::StorageError;

fn to_sqlite_url(path: &str) -> String {
    if path.starts_with("sqlite:") {
        path.to_string()
    } else if path == ":memory:" || path == "file::memory:" {
        "sqlite::memory:".to_string()
    } else if path.starts_with('/') {
        format!("sqlite://{}", path)
    } else {
        format!("sqlite:{}", path)
    }
}

fn sanitize_url(url: &str) -> String {
    if url.starts_with("sqlite::memory:") {
        url.to_string()
    } else if let Some(pos) = url.find("://") {
        let scheme = &url[..pos];
        format!("{}://<path>", scheme)
    } else {
        url.to_string()
    }
}

enum BindValue {
    S(String),
    I(i64),
}

/// Normalizes a metadata value to its text representation, mirroring
/// PostgreSQL's `metadata->>'key'` operator so that string equality is
/// based on the value text (numbers match their canonical decimal form).
/// SQLite's JSON1 stores booleans as 1/0, so they cannot match 'true' /
/// 'false' the way PostgreSQL or the in-memory backend do.
fn metadata_text_expr(key: &str) -> String {
    format!("CAST(json_extract(metadata, '$.{}') AS TEXT)", key)
}

/// True when the metadata value is a JSON number, mirroring PostgreSQL's
/// `jsonb_typeof(...) = 'number'` guard so that numeric predicates never
/// match non-numeric values.
fn is_numeric_expr(key: &str) -> String {
    format!("json_type(metadata, '$.{}') IN ('integer', 'real')", key)
}

/// Translates a QueryFilter into a complete SELECT statement.
/// Field names come from a fixed metadata schema, so interpolation is safe.
fn build_select_sql(
    filter: Option<&QueryFilter>,
    table_name: &str,
    select_columns: &str,
) -> (String, Vec<BindValue>) {
    let mut sql = format!("SELECT {} FROM {}", select_columns, table_name);
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<BindValue> = Vec::new();
    let mut order_by: Option<(String, bool)> = None;
    let mut offset: Option<u64> = None;
    let mut limit: Option<u64> = None;

    if let Some(f) = filter {
        for op in &f.ops {
            match op {
                FilterOp::Eq(key, value) => {
                    conditions.push(format!("{} = ?", metadata_text_expr(key)));
                    params.push(BindValue::S(value.clone()));
                }
                FilterOp::IdPrefix(prefix) => {
                    conditions.push("substr(id, 1, length(?)) = ?".into());
                    params.push(BindValue::S(prefix.clone()));
                    params.push(BindValue::S(prefix.clone()));
                }
                FilterOp::Prefix(key, prefix) => {
                    conditions.push(format!(
                        "substr({}, 1, length(?)) = ?",
                        metadata_text_expr(key)
                    ));
                    params.push(BindValue::S(prefix.clone()));
                    params.push(BindValue::S(prefix.clone()));
                }
                FilterOp::Lt(key, value) => {
                    conditions.push(format!(
                        "({} AND json_extract(metadata, '$.{}') < ?)",
                        is_numeric_expr(key),
                        key
                    ));
                    params.push(BindValue::I(*value));
                }
                FilterOp::Gt(key, value) => {
                    conditions.push(format!(
                        "({} AND json_extract(metadata, '$.{}') > ?)",
                        is_numeric_expr(key),
                        key
                    ));
                    params.push(BindValue::I(*value));
                }
                FilterOp::Between(key, start, end) => {
                    conditions.push(format!(
                        "({} AND json_extract(metadata, '$.{}') >= ? AND json_extract(metadata, '$.{}') <= ?)",
                        is_numeric_expr(key),
                        key,
                        key
                    ));
                    params.push(BindValue::I(*start));
                    params.push(BindValue::I(*end));
                }
                FilterOp::In(key, values) => {
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
                        params.extend(values.iter().cloned().map(BindValue::S));
                    }
                }
                FilterOp::OrderBy(key, descending) => {
                    order_by = Some((key.clone(), *descending));
                }
                FilterOp::Offset(o) => offset = Some(*o),
                FilterOp::Limit(l) => limit = Some(*l),
            }
        }
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    if let Some((key, descending)) = order_by {
        // Numeric-aware ordering matching PostgreSQL: numeric values sort by
        // their numeric value and always come first, everything else (missing
        // keys and non-numeric values) sorts last in both directions. The
        // leading flag column emulates PostgreSQL's `NULLS LAST`.
        sql.push_str(&format!(
            " ORDER BY (CASE WHEN {} THEN 0 ELSE 1 END) ASC, json_extract(metadata, '$.{}') {}",
            is_numeric_expr(&key),
            key,
            if descending { "DESC" } else { "ASC" }
        ));
    }
    if let Some(limit) = limit {
        sql.push_str(&format!(" LIMIT {}", limit));
    }
    if let Some(offset) = offset {
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
        let url = to_sqlite_url(path);

        // sqlx 0.8 defaults `create_if_missing` to false, which makes
        // connecting to a not-yet-existing database file fail with
        // "unable to open database file". Storage opens always create the
        // file when missing.
        let options = SqliteConnectOptions::from_str(&url)
            .map_err(|e| StorageError::Initialization {
                backend: "sqlite".into(),
                message: format!("Failed to parse URL: {}", sanitize_url(&url)),
                source: Some(Box::new(e)),
            })?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .map_err(|e| StorageError::Initialization {
                backend: "sqlite".into(),
                message: format!("Failed to connect: {}", sanitize_url(&url)),
                source: Some(Box::new(e)),
            })?;

        sqlx::query("PRAGMA journal_mode = WAL;")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("PRAGMA synchronous = NORMAL;")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("PRAGMA busy_timeout = 5000;")
            .execute(&pool)
            .await
            .ok();

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

        let idx1 = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_entity_type ON {}(CAST(json_extract(metadata, '$.entityType') AS TEXT))",
            table_name, table_name
        );
        sqlx::query(&idx1).execute(&pool).await.ok();

        let idx2 = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_status ON {}(CAST(json_extract(metadata, '$.status') AS TEXT))",
            table_name, table_name
        );
        sqlx::query(&idx2).execute(&pool).await.ok();

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
}

#[async_trait]
impl Store for SqliteStorage {
    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        SqliteStorage::update_status(self, id, status).await
    }

    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError> {
        let now = chrono::Utc::now().timestamp_millis();
        let hash = crate::util::hash::compute_hash(data);
        let data_size = data.len() as i64;
        let metadata_str = serde_json::to_string(metadata)?;
        let compressed = metadata
            .get("compressed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

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
                BindValue::S(s) => query = query.bind(s),
                BindValue::I(i) => query = query.bind(*i),
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
                BindValue::S(s) => query = query.bind(s),
                BindValue::I(i) => query = query.bind(*i),
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

    async fn count_by_metadata_field(
        &self,
        field: &str,
    ) -> Result<std::collections::HashMap<String, u64>, StorageError> {
        let sql = format!(
            "SELECT CAST(json_extract(metadata, '$.{}') AS TEXT) AS k, COUNT(*) AS c FROM {} GROUP BY k",
            field, self.table_name
        );
        let rows: Vec<(Option<String>, i64)> = sqlx::query_as(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "count_by_metadata_field".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(rows
            .into_iter()
            .filter_map(|(key, count)| key.map(|k| (k, count as u64)))
            .collect())
    }

    async fn count(&self, filter: Option<&QueryFilter>) -> Result<u64, StorageError> {
        let (sql, params) = build_select_sql(filter, &self.table_name, "1");
        let sql = format!("SELECT COUNT(*) FROM ({}) AS filtered", sql);
        let mut query = sqlx::query_scalar::<_, i64>(&sql);
        for param in &params {
            match param {
                BindValue::S(s) => query = query.bind(s),
                BindValue::I(i) => query = query.bind(*i),
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
                    let hash = crate::util::hash::compute_hash(&item.data);
                    let data_size = item.data.len() as i64;
                    let metadata_str = serde_json::to_string(&item.metadata)?;
                    let compressed = item
                        .metadata
                        .get("compressed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
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
                        .bind(&item.id)
                        .bind(&item.data)
                        .bind(&metadata_str)
                        .bind(&hash)
                        .bind(data_size)
                        .bind(compressed)
                        .bind(now)
                        .bind(now)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| StorageError::General {
                            operation: "apply_batch.save".into(),
                            message: e.to_string(),
                            source: Some(Box::new(e)),
                        })?;
                }
                StoreOperation::Delete(id) => {
                    let sql = format!("DELETE FROM {} WHERE id = ?1", self.table_name);
                    sqlx::query(&sql)
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| StorageError::General {
                            operation: "apply_batch.delete".into(),
                            message: e.to_string(),
                            source: Some(Box::new(e)),
                        })?;
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
}

#[async_trait]
impl BatchStore for SqliteStorage {
    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<u8>, Value)>, StorageError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders: Vec<String> = (0..ids.len()).map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT id, data, metadata, hash FROM {} WHERE id IN ({})",
            self.table_name,
            placeholders.join(", ")
        );
        let mut query = sqlx::query_as::<_, (String, Vec<u8>, String, String)>(&sql);
        for id in ids {
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
        rows.into_iter()
            .map(|(id, data, metadata_str, hash)| {
                crate::util::hash::verify_integrity(&id, &data, &hash)?;
                let metadata: Value = serde_json::from_str(&metadata_str)?;
                Ok((id, data, metadata))
            })
            .collect()
    }

    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await.map_err(|e| StorageError::General {
            operation: "save_batch".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;

        for chunk in items.chunks(500) {
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
                let metadata_str = serde_json::to_string(&item.metadata)?;
                let compressed = item
                    .metadata
                    .get("compressed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let data_size = item.data.len() as i64;
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

    async fn checkpoint(&self) -> Result<(), StorageError> {
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "checkpoint".into(),
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
    use crate::domain::store::BatchStore;

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

        let counts = store.count_by_metadata_field("entityType").await.unwrap();
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
                &serde_json::json!({"entityType": "wf", "timestamp": 1000}),
            )
            .await
            .unwrap();
        store
            .save(
                "s1",
                b"data",
                &serde_json::json!({"entityType": "wf", "timestamp": "abc"}),
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
        let all: Vec<(String, Vec<u8>)> =
            sqlx::query_as(&format!("SELECT id, data FROM {}", store.table_name()))
                .fetch_all(store.pool())
                .await
                .unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.iter().all(|(_, data)| data == &vec![0xAA; 10]));
    }
}
