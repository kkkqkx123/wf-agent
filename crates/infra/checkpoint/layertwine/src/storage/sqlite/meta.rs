use crate::storage::repository::MetadataStore;
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageResult;

impl MetadataStore for SqliteStorage {
    fn store_metadata(&self, key: &str, value: &str) -> StorageResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR REPLACE INTO meta_kv (key, value, updated_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![key, value.as_bytes(), now],
        )?;
        Ok(())
    }

    fn load_metadata(&self, key: &str) -> StorageResult<Option<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT value FROM meta_kv WHERE key = ?1")?;
        let result = stmt.query_row(rusqlite::params![key], |row| {
            let value: Vec<u8> = row.get(0)?;
            String::from_utf8(value)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        });
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(crate::StorageError::Database(e)),
        }
    }

    fn delete_metadata(&self, key: &str) -> StorageResult<bool> {
        let conn = self.conn.lock();
        let deleted = conn.execute("DELETE FROM meta_kv WHERE key = ?1", rusqlite::params![key])?;
        Ok(deleted > 0)
    }
}
