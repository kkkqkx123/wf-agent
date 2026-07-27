use async_trait::async_trait;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use crate::domain::store::{BatchItem, BatchStore, Maintainable, QueryFilter, Store};
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

pub struct SqliteStorage {
    pool: SqlitePool,
    table_name: String,
}

impl SqliteStorage {
    pub async fn new(path: &str, table_name: &str) -> Result<Self, StorageError> {
        let url = to_sqlite_url(path);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect(&url)
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
            "CREATE INDEX IF NOT EXISTS idx_{}_entity_type ON {}(json_extract(metadata, '$.entityType'))",
            table_name, table_name
        );
        sqlx::query(&idx1).execute(&pool).await.ok();

        let idx2 = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_status ON {}(json_extract(metadata, '$.status'))",
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

    pub async fn update_status(
        &self,
        id: &str,
        status: &str,
    ) -> Result<(), StorageError> {
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
    async fn save(
        &self,
        id: &str,
        data: &[u8],
        metadata: &Value,
    ) -> Result<(), StorageError> {
        let now = chrono::Utc::now().timestamp_millis();
        let hash = crate::util::hash::compute_hash(data);
        let data_size = data.len() as i64;
        let metadata_str = serde_json::to_string(metadata)?;
        let compressed = metadata
            .get("compressed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let sql = format!(
            "INSERT OR REPLACE INTO {} (id, data, metadata, hash, data_size, compressed, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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

    async fn load(
        &self,
        id: &str,
    ) -> Result<Option<(Vec<u8>, Value)>, StorageError> {
        let sql = format!(
            "SELECT data, metadata FROM {} WHERE id = ?1",
            self.table_name
        );
        let result: Option<(Vec<u8>, String)> =
            sqlx::query_as(&sql)
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| StorageError::General {
                    operation: "load".into(),
                    message: e.to_string(),
                    source: Some(Box::new(e)),
                })?;

        match result {
            Some((data, metadata_str)) => {
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
        let mut sql = format!("SELECT id, metadata FROM {}", self.table_name);
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(f) = filter {
            if let Some(ref entity_type) = f.entity_type {
                conditions.push("json_extract(metadata, '$.entityType') = ?".into());
                params.push(entity_type.clone());
            }
            if let Some(ref status) = f.status {
                conditions.push("json_extract(metadata, '$.status') = ?".into());
                params.push(status.clone());
            }
            for (key, value) in &f.fields {
                conditions.push(format!("json_extract(metadata, '$.{}') = ?", key));
                params.push(value.clone());
            }
            if let Some((start, end)) = f.timestamp_range {
                conditions.push("(json_extract(metadata, '$.timestamp') >= ? AND json_extract(metadata, '$.timestamp') <= ?)".into());
                params.push(start.to_string());
                params.push(end.to_string());
            }
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        if let Some(f) = filter {
            if let Some(limit) = f.limit {
                sql.push_str(&format!(" LIMIT {}", limit));
            }
            if let Some(offset) = f.offset {
                sql.push_str(&format!(" OFFSET {}", offset));
            }
        }

        let mut query = sqlx::query_as::<_, (String, String)>(&sql);
        for param in &params {
            query = query.bind(param);
        }

        let rows = query.fetch_all(&self.pool).await.map_err(|e| {
            StorageError::General {
                operation: "list".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            }
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
        let mut sql = format!("SELECT data, metadata FROM {}", self.table_name);
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(f) = filter {
            if let Some(ref entity_type) = f.entity_type {
                conditions.push("json_extract(metadata, '$.entityType') = ?".into());
                params.push(entity_type.clone());
            }
            if let Some(ref status) = f.status {
                conditions.push("json_extract(metadata, '$.status') = ?".into());
                params.push(status.clone());
            }
            for (key, value) in &f.fields {
                conditions.push(format!("json_extract(metadata, '$.{}') = ?", key));
                params.push(value.clone());
            }
            if let Some((start, end)) = f.timestamp_range {
                conditions.push("(json_extract(metadata, '$.timestamp') >= ? AND json_extract(metadata, '$.timestamp') <= ?)".into());
                params.push(start.to_string());
                params.push(end.to_string());
            }
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        if let Some(f) = filter {
            if let Some(limit) = f.limit {
                sql.push_str(&format!(" LIMIT {}", limit));
            }
            if let Some(offset) = f.offset {
                sql.push_str(&format!(" OFFSET {}", offset));
            }
        }

        let mut query = sqlx::query_as::<_, (Vec<u8>, String)>(&sql);
        for param in &params {
            query = query.bind(param);
        }

        let rows = query.fetch_all(&self.pool).await.map_err(|e| {
            StorageError::General {
                operation: "list_data".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            }
        })?;

        rows.into_iter()
            .map(|(data, metadata_str)| {
                let metadata: Value = serde_json::from_str(&metadata_str)?;
                Ok((data, metadata))
            })
            .collect()
    }

    async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        let sql = format!(
            "SELECT 1 FROM {} WHERE id = ?1 LIMIT 1",
            self.table_name
        );
        let result: Option<(i64,)> =
            sqlx::query_as(&sql)
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

#[async_trait]
impl BatchStore for SqliteStorage {
    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<u8>, Value)>, StorageError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders: Vec<String> =
            (0..ids.len()).map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT id, data, metadata FROM {} WHERE id IN ({})",
            self.table_name,
            placeholders.join(", ")
        );
        let mut query = sqlx::query_as::<_, (String, Vec<u8>, String)>(&sql);
        for id in ids {
            query = query.bind(id);
        }
        let rows = query.fetch_all(&self.pool).await.map_err(|e| {
            StorageError::General {
                operation: "load_batch".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            }
        })?;
        rows.into_iter()
            .map(|(id, data, metadata_str)| {
                let metadata: Value = serde_json::from_str(&metadata_str)?;
                Ok((id, data, metadata))
            })
            .collect()
    }

    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            StorageError::General {
                operation: "save_batch".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            }
        })?;

        for chunk in items.chunks(500) {
            let mut sql = format!(
                "INSERT OR REPLACE INTO {} (id, data, metadata, hash, data_size, compressed, created_at, updated_at) VALUES ",
                self.table_name
            );
            let placeholders: Vec<String> = (0..chunk.len())
                .map(|_| "(?, ?, ?, ?, ?, ?, ?, ?)".to_string())
                .collect();
            sql.push_str(&placeholders.join(", "));

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
            query.execute(&mut *tx).await.map_err(|e| {
                StorageError::General {
                    operation: "save_batch".into(),
                    message: e.to_string(),
                    source: Some(Box::new(e)),
                }
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
        let mut tx = self.pool.begin().await.map_err(|e| {
            StorageError::General {
                operation: "delete_batch".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            }
        })?;

        for chunk in ids.chunks(999) {
            let placeholders: Vec<String> =
                (1..=chunk.len()).map(|i| format!("?{}", i)).collect();
            let sql = format!(
                "DELETE FROM {} WHERE id IN ({})",
                self.table_name,
                placeholders.join(", ")
            );
            let mut query = sqlx::query(&sql);
            for id in chunk {
                query = query.bind(id);
            }
            query.execute(&mut *tx).await.map_err(|e| {
                StorageError::General {
                    operation: "delete_batch".into(),
                    message: e.to_string(),
                    source: Some(Box::new(e)),
                }
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
            .save("id1", b"hello world", &serde_json::json!({"entityType": "test"}))
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
            .save("id1", b"data1", &serde_json::json!({"entityType": "A", "status": "active"}))
            .await
            .unwrap();
        store
            .save("id2", b"data2", &serde_json::json!({"entityType": "B", "status": "inactive"}))
            .await
            .unwrap();

        let filter = QueryFilter::new().with_entity_type("A");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "id1");
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
    async fn test_sqlite_update_status() {
        let store = SqliteStorage::new(":memory:", "test").await.unwrap();
        store
            .save("exec1", b"data", &serde_json::json!({"entityType": "execution", "status": "pending"}))
            .await
            .unwrap();
        store.update_status("exec1", "running").await.unwrap();
        let (_, meta) = store.load("exec1").await.unwrap().unwrap();
        assert_eq!(meta["status"], "running");
    }
}
