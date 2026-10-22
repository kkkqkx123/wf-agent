use async_trait::async_trait;
use serde_json::Value;

use crate::domain::keys::SCHEMA_VERSION_EXCLUDE_PATTERN;
use crate::domain::store::{BatchItem, Maintainable, StoreExt, StoreOperation};
use crate::error::StorageError;

use super::init::PostgresStorage;
use super::init;
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

