use crate::core::file_node::FileNode;
use crate::storage::repository::FileNodeStore;
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageResult;
use rusqlite::params;

impl FileNodeStore for SqliteStorage {
    fn store_file_node(&self, file_node: &FileNode, content: &[u8]) -> StorageResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO file_nodes (file_path, base_hash, content, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                file_node.path_str(),
                &file_node.base_hash.to_vec(),
                content,
                chrono::Utc::now().timestamp_millis(),
            ],
        )?;
        Ok(())
    }

    fn get_file_content(&self, file_path: &str, base_hash: &[u8; 32]) -> StorageResult<Vec<u8>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT content FROM file_nodes WHERE file_path = ?1 AND base_hash = ?2")?;
        let content: Vec<u8> =
            stmt.query_row(params![file_path, &base_hash.to_vec()], |row| row.get(0))?;
        Ok(content)
    }

    fn file_node_exists(&self, file_path: &str, base_hash: &[u8; 32]) -> StorageResult<bool> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM file_nodes WHERE file_path = ?1 AND base_hash = ?2")?;
        let count: i64 =
            stmt.query_row(params![file_path, &base_hash.to_vec()], |row| row.get(0))?;
        Ok(count > 0)
    }
}
