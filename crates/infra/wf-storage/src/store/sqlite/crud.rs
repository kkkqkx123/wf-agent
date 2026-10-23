use async_trait::async_trait;
use serde_json::Value;

use crate::domain::keys::SCHEMA_VERSION_EXCLUDE_PATTERN;
use crate::domain::store::{FilterBindValue, QueryFilter, Store};
use crate::error::StorageError;

use super::init::SqliteStorage;
use super::query;

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
        let (sql, params) = query::build_select_sql(filter, &self.table_name, "id, metadata");
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
        let (sql, params) =
            query::build_select_sql(filter, &self.table_name, "id, data, metadata, hash");
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
        let (sql, params) = query::build_select_sql(stripped.as_ref(), &self.table_name, "1");
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
        let sql = format!(
            "DELETE FROM {} WHERE id NOT LIKE '{}'",
            self.table_name, SCHEMA_VERSION_EXCLUDE_PATTERN
        );
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
