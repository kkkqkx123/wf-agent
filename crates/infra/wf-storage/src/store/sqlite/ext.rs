use async_trait::async_trait;
use serde_json::Value;

use crate::domain::keys::SCHEMA_VERSION_EXCLUDE_PATTERN;
use crate::domain::store::{BatchItem, Maintainable, StoreExt, StoreOperation};
use crate::error::StorageError;

use super::init;
use super::init::SqliteStorage;
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
            super::query::metadata_text_expr(field),
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
                    init::exec_save(&mut *tx, &self.table_name, item, now).await?;
                }
                StoreOperation::Delete(id) => {
                    init::exec_delete(&mut *tx, &self.table_name, id).await?;
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
