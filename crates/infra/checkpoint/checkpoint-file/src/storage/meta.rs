use super::connection::SqliteStorage;
use super::repository::{MetadataStore, StorageResult};
use checkpoint_base::error::CheckpointError;

fn db_err(e: rusqlite::Error) -> CheckpointError {
    CheckpointError::Internal(format!("sqlite error: {e}"))
}

impl MetadataStore for SqliteStorage {
    fn store_metadata(&self, key: &str, value: &str) -> StorageResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR REPLACE INTO meta_kv (key, value, updated_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![key, value.as_bytes(), now],
        )
        .map_err(db_err)?;
        Ok(())
    }

    fn load_metadata(&self, key: &str) -> StorageResult<Option<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT value FROM meta_kv WHERE key = ?1")
            .map_err(db_err)?;
        let result = stmt.query_row(rusqlite::params![key], |row| {
            let value: Vec<u8> = row.get(0)?;
            String::from_utf8(value)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        });
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(db_err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_roundtrip_replace_and_isolation() {
        let storage = SqliteStorage::new_full_in_memory().unwrap();
        assert_eq!(storage.load_metadata("k").unwrap(), None);

        storage.store_metadata("k", "v1").unwrap();
        assert_eq!(storage.load_metadata("k").unwrap(), Some("v1".to_string()));

        // Re-storing replaces in place.
        storage.store_metadata("k", "v2").unwrap();
        assert_eq!(storage.load_metadata("k").unwrap(), Some("v2".to_string()));

        // Other keys are untouched.
        storage.store_metadata("other", "o").unwrap();
        assert_eq!(storage.load_metadata("other").unwrap(), Some("o".to_string()));
        assert_eq!(storage.load_metadata("k").unwrap(), Some("v2".to_string()));
    }

    #[test]
    fn metadata_empty_value_roundtrips_distinct_from_absent() {
        let storage = SqliteStorage::new_full_in_memory().unwrap();
        storage.store_metadata("empty", "").unwrap();
        assert_eq!(storage.load_metadata("empty").unwrap(), Some(String::new()));
        assert_eq!(storage.load_metadata("absent").unwrap(), None);
    }
}
