use crate::storage::repository::{GraphBlob, GraphBlobStore};
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageResult;

impl GraphBlobStore for SqliteStorage {
    fn store_graph_blob(
        &self,
        id: &str,
        data: &[u8],
        parent_id: Option<&str>,
        branch_id: Option<&str>,
    ) -> StorageResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO graph_blobs (id, data, parent_id, branch_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data,
                parent_id = excluded.parent_id, branch_id = excluded.branch_id,
                updated_at = excluded.updated_at",
            rusqlite::params![id, data, parent_id, branch_id, now],
        )?;
        Ok(())
    }

    fn load_graph_blob(&self, id: &str) -> StorageResult<Option<GraphBlob>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, data, parent_id, branch_id, created_at, updated_at
                 FROM graph_blobs WHERE id = ?1",
            )?;
            let result = stmt.query_row(rusqlite::params![id], |row| {
                Ok(GraphBlob {
                    id: row.get(0)?,
                    data: row.get(1)?,
                    parent_id: row.get(2)?,
                    branch_id: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            });
            match result {
                Ok(blob) => Ok(Some(blob)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(crate::StorageError::Database(e)),
            }
        })
    }

    fn delete_graph_blob(&self, id: &str) -> StorageResult<bool> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM graph_blobs WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(deleted > 0)
    }

    fn list_graph_blob_ids(&self, parent_id: Option<&str>) -> StorageResult<Vec<String>> {
        self.with_conn(|conn| {
            if let Some(parent) = parent_id {
                let mut stmt = conn.prepare(
                    "SELECT id FROM graph_blobs WHERE parent_id = ?1 ORDER BY updated_at, id",
                )?;
                let ids = stmt
                    .query_map(rusqlite::params![parent], |row| row.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                Ok(ids)
            } else {
                let mut stmt =
                    conn.prepare("SELECT id FROM graph_blobs ORDER BY updated_at, id")?;
                let ids = stmt
                    .query_map([], |row| row.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                Ok(ids)
            }
        })
    }

    fn list_graph_blob_ids_by_branch(&self, branch_id: &str) -> StorageResult<Vec<String>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM graph_blobs WHERE branch_id = ?1 ORDER BY updated_at, id",
            )?;
            let ids = stmt
                .query_map(rusqlite::params![branch_id], |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            Ok(ids)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::connection::SqliteStorage;

    #[test]
    fn blob_roundtrip_with_indexes() {
        let storage = SqliteStorage::new_full_in_memory().unwrap();
        storage
            .store_graph_blob("cp-1", b"data-1", Some("exec-1"), Some("main"))
            .unwrap();
        storage
            .store_graph_blob("cp-2", b"data-2", Some("exec-1"), Some("main"))
            .unwrap();
        storage
            .store_graph_blob("cp-3", b"data-3", Some("exec-2"), None)
            .unwrap();

        let blob = storage.load_graph_blob("cp-1").unwrap().unwrap();
        assert_eq!(blob.data, b"data-1");
        assert_eq!(blob.parent_id.as_deref(), Some("exec-1"));

        let mut by_parent = storage.list_graph_blob_ids(Some("exec-1")).unwrap();
        by_parent.sort();
        assert_eq!(by_parent, vec!["cp-1".to_string(), "cp-2".to_string()]);

        let mut by_branch = storage.list_graph_blob_ids_by_branch("main").unwrap();
        by_branch.sort();
        assert_eq!(by_branch, vec!["cp-1".to_string(), "cp-2".to_string()]);

        assert!(storage.delete_graph_blob("cp-3").unwrap());
        assert!(!storage.delete_graph_blob("cp-3").unwrap());
        assert!(storage.load_graph_blob("cp-3").unwrap().is_none());
    }
}
