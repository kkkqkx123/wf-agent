use async_trait::async_trait;
use serde_json::Value;

use crate::domain::keys::SCHEMA_VERSION_EXCLUDE_PATTERN;
use crate::domain::store::{FilterBindValue, QueryFilter, Store};
use crate::error::StorageError;

use super::init::PostgresStorage;
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
            .map_err(|e| StorageError::from_sqlx_with_operation(e, "save"))?;
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
            .map_err(|e| StorageError::from_sqlx_with_operation(e, "load"))?;

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
            .map_err(|e| StorageError::from_sqlx_with_operation(e, "delete"))?;
        Ok(())
    }

    async fn list(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(String, Value)>, StorageError> {
        let (sql, params) =
            super::query::build_select_sql(filter, &self.table_name, "id, metadata");
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
            .map_err(|e| StorageError::from_sqlx_with_operation(e, "list"))?;

        Ok(rows)
    }

    async fn list_data(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(Vec<u8>, Value)>, StorageError> {
        let (sql, params) =
            super::query::build_select_sql(filter, &self.table_name, "id, data, metadata, hash");
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
            .map_err(|e| StorageError::from_sqlx_with_operation(e, "list_data"))?;

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
        let (sql, params) =
            super::query::build_select_sql(stripped.as_ref(), &self.table_name, "1");
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
            .map_err(|e| StorageError::from_sqlx_with_operation(e, "count"))?;
        Ok(count as u64)
    }

    async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        let sql = format!("SELECT 1 FROM {} WHERE id = $1 LIMIT 1", self.table_name);
        let result: Option<(i32,)> = sqlx::query_as(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::from_sqlx_with_operation(e, "exists"))?;
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
            .map_err(|e| StorageError::from_sqlx_with_operation(e, "clear"))?;
        Ok(())
    }
}
