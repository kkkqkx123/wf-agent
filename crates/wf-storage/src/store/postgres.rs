use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;

use crate::domain::store::{BatchItem, BatchStore, Maintainable, QueryFilter, Store};
use crate::error::StorageError;
use crate::util::pool::create_pg_pool;

pub struct PostgresStorage {
    pool: PgPool,
    table_name: String,
}

impl PostgresStorage {
    pub async fn new(
        connection_string: &str,
        table_name: &str,
    ) -> Result<Self, StorageError> {
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
            "CREATE INDEX IF NOT EXISTS idx_{}_metadata ON {} USING GIN (metadata)",
            table_name, table_name
        );
        sqlx::query(&idx1).execute(&pool).await.ok();

        let idx2 = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_entity_type ON {}((metadata->>'entityType'))",
            table_name, table_name
        );
        sqlx::query(&idx2).execute(&pool).await.ok();

        let idx3 = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_status ON {}((metadata->>'status'))",
            table_name, table_name
        );
        sqlx::query(&idx3).execute(&pool).await.ok();

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

    pub async fn update_status(
        &self,
        id: &str,
        status: &str,
    ) -> Result<(), StorageError> {
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
}

#[async_trait]
impl Store for PostgresStorage {
    async fn save(
        &self,
        id: &str,
        data: &[u8],
        metadata: &Value,
    ) -> Result<(), StorageError> {
        let now = chrono::Utc::now().timestamp_millis();
        let hash = crate::util::hash::compute_hash(data);
        let data_size = data.len() as i32;
        let compressed = metadata
            .get("compressed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

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

    async fn load(
        &self,
        id: &str,
    ) -> Result<Option<(Vec<u8>, Value)>, StorageError> {
        let sql = format!(
            "SELECT data, metadata FROM {} WHERE id = $1",
            self.table_name
        );
        let result: Option<(Vec<u8>, Value)> =
            sqlx::query_as(&sql)
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| StorageError::General {
                    operation: "load".into(),
                    message: e.to_string(),
                    source: Some(Box::new(e)),
                })?;

        Ok(result)
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
        let mut sql = format!("SELECT id, metadata FROM {}", self.table_name);
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<&str> = Vec::new();

        if let Some(f) = filter {
            let mut idx = 1;
            if let Some(ref entity_type) = f.entity_type {
                conditions.push(format!("(metadata->>'entityType') = ${}", idx));
                idx += 1;
                params.push(entity_type.as_str());
            }
            if let Some(ref status) = f.status {
                conditions.push(format!("(metadata->>'status') = ${}", idx));
                idx += 1;
                params.push(status.as_str());
            }
            for (key, value) in &f.fields {
                conditions.push(format!("(metadata->>'{}') = ${}", key, idx));
                idx += 1;
                params.push(value.as_str());
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

        let mut query = sqlx::query_as::<_, (String, Value)>(&sql);
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

        Ok(rows)
    }

    async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        let sql = format!(
            "SELECT 1 FROM {} WHERE id = $1 LIMIT 1",
            self.table_name
        );
        let result: Option<(i32,)> =
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

#[async_trait]
impl BatchStore for PostgresStorage {
    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        let now = chrono::Utc::now().timestamp_millis();

        let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        let datas: Vec<&[u8]> = items.iter().map(|i| i.data.as_slice()).collect();
        let metadatas: Vec<&Value> = items.iter().map(|i| &i.metadata).collect();
        let hashes: Vec<String> =
            items.iter().map(|i| crate::util::hash::compute_hash(&i.data)).collect();
        let hashes_ref: Vec<&str> = hashes.iter().map(|h| h.as_str()).collect();
        let sizes: Vec<i32> = items.iter().map(|i| i.data.len() as i32).collect();
        let compresseds: Vec<bool> = items
            .iter()
            .map(|i| {
                i.metadata
                    .get("compressed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
            .collect();
        let nows: Vec<i64> = (0..items.len()).map(|_| now).collect();

        let sql = format!(
            "INSERT INTO {} (id, data, metadata, hash, data_size, compressed, created_at, updated_at)
             SELECT * FROM UNNEST($1::text[], $2::bytea[], $3::jsonb[], $4::text[], $5::int4[], $6::boolean[], $7::bigint[], $8::bigint[])
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
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "save_batch".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
        Ok(())
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        let sql = format!(
            "DELETE FROM {} WHERE id = ANY($1)",
            self.table_name
        );
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
    async fn vacuum(&self) -> Result<(), StorageError> {
        sqlx::query("VACUUM ANALYZE")
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::General {
                operation: "vacuum".into(),
                message: e.to_string(),
                source: Some(Box::new(e)),
            })?;
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
}
