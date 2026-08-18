use crate::checkpoint::branch::Branch;
use crate::checkpoint::types::Checkpoint;
use crate::core::types::{CheckpointId, ContentId, SnapshotId};
use crate::storage::repository::CheckpointPersist;
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageResult;

impl CheckpointPersist for SqliteStorage {
    fn store_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()> {
        let conn = self.conn.lock();
        let parents_json = serde_json::to_vec(&checkpoint.parents)
            .map_err(|e| crate::StorageError::Serialization(e.to_string()))?;
        let snapshot_ids_json = serde_json::to_vec(&checkpoint.baseline_snapshots)
            .map_err(|e| crate::StorageError::Serialization(e.to_string()))?;
        let snapshot_sources_json = if checkpoint.snapshot_sources.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&checkpoint.snapshot_sources)
                    .map_err(|e| crate::StorageError::Serialization(e.to_string()))?,
            )
        };

        conn.execute(
            "INSERT OR IGNORE INTO checkpoints (id, parents, snapshot_ids, author, message, git_anchor, created_at, snapshot_sources)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                &checkpoint.id.0.to_vec(),
                parents_json,
                snapshot_ids_json,
                checkpoint.metadata.author,
                checkpoint.metadata.message,
                checkpoint.metadata.git_anchor,
                checkpoint.created_at,
                snapshot_sources_json,
            ],
        )?;
        Ok(())
    }

    fn get_checkpoint(&self, id: &CheckpointId) -> StorageResult<Checkpoint> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, parents, snapshot_ids, author, message, git_anchor, created_at, snapshot_sources FROM checkpoints WHERE id = ?1"
        )?;

        let result = stmt.query_row(rusqlite::params![&id.0.to_vec()], |row| {
            let id_bytes: Vec<u8> = row.get(0)?;
            let mut id_arr = [0u8; 32];
            id_arr.copy_from_slice(&id_bytes);

            let parents_json: Vec<u8> = row.get(1)?;
            let parents: Vec<CheckpointId> = serde_json::from_slice(&parents_json)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let snap_ids_json: Vec<u8> = row.get(2)?;
            let baseline_snapshots: Vec<SnapshotId> = serde_json::from_slice(&snap_ids_json)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let author: String = row.get(3)?;
            let message: String = row.get(4)?;
            let git_anchor: Option<String> = row.get(5)?;
            let created_at: i64 = row.get(6)?;
            let snapshot_sources_json: Option<String> = row.get(7)?;
            let snapshot_sources: std::collections::HashMap<SnapshotId, String> =
                snapshot_sources_json
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();

            Ok(Checkpoint {
                id: ContentId(id_arr),
                parents,
                baseline_snapshots,
                metadata: crate::checkpoint::types::CheckpointMetadata {
                    author,
                    message,
                    git_anchor,
                },
                created_at,
                snapshot_sources,
            })
        })?;
        Ok(result)
    }

    fn checkpoint_exists(&self, id: &CheckpointId) -> StorageResult<bool> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM checkpoints WHERE id = ?1")?;
        let count: i64 = stmt.query_row(rusqlite::params![&id.0.to_vec()], |row| row.get(0))?;
        Ok(count > 0)
    }

    fn list_checkpoints(&self) -> StorageResult<Vec<Checkpoint>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, parents, snapshot_ids, author, message, git_anchor, created_at, snapshot_sources \
             FROM checkpoints ORDER BY created_at DESC",
        )?;

        let checkpoints = stmt
            .query_map([], |row| {
                let id_bytes: Vec<u8> = row.get(0)?;
                let mut id_arr = [0u8; 32];
                id_arr.copy_from_slice(&id_bytes);

                let parents_json: Vec<u8> = row.get(1)?;
                let parents: Vec<CheckpointId> = serde_json::from_slice(&parents_json)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

                let snap_ids_json: Vec<u8> = row.get(2)?;
                let baseline_snapshots: Vec<SnapshotId> = serde_json::from_slice(&snap_ids_json)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

                let author: String = row.get(3)?;
                let message: String = row.get(4)?;
                let git_anchor: Option<String> = row.get(5)?;
                let created_at: i64 = row.get(6)?;
                let snapshot_sources_json: Option<String> = row.get(7)?;
                let snapshot_sources: std::collections::HashMap<SnapshotId, String> =
                    snapshot_sources_json
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default();

                Ok(Checkpoint {
                    id: ContentId(id_arr),
                    parents,
                    baseline_snapshots,
                    metadata: crate::checkpoint::types::CheckpointMetadata {
                        author,
                        message,
                        git_anchor,
                    },
                    created_at,
                    snapshot_sources,
                })
            })
            .map_err(crate::StorageError::Database)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(checkpoints)
    }

    fn delete_checkpoint(&self, id: &CheckpointId) -> StorageResult<()> {
        let conn = self.conn.lock();
        let affected = conn.execute(
            "DELETE FROM checkpoints WHERE id = ?1",
            rusqlite::params![&id.0.to_vec()],
        )?;
        if affected == 0 {
            return Err(crate::StorageError::NotFound(format!(
                "checkpoint {} not found",
                id
            )));
        }
        Ok(())
    }

    fn store_branch(&self, branch: &Branch) -> StorageResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO branches (name, head, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                branch.name,
                &branch.head.0.to_vec(),
                branch.created_at,
                branch.updated_at,
            ],
        )?;
        Ok(())
    }

    fn get_branch(&self, name: &str) -> StorageResult<Branch> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT name, head, created_at, updated_at FROM branches WHERE name = ?1")?;

        let result = stmt.query_row(rusqlite::params![name], |row| {
            let name: String = row.get(0)?;
            let head_bytes: Vec<u8> = row.get(1)?;
            let mut head_arr = [0u8; 32];
            head_arr.copy_from_slice(&head_bytes);
            let created_at: i64 = row.get(2)?;
            let updated_at: i64 = row.get(3)?;

            Ok(Branch {
                name,
                head: ContentId(head_arr),
                created_at,
                updated_at,
            })
        })?;
        Ok(result)
    }

    fn update_branch_head(&self, name: &str, head: &CheckpointId) -> StorageResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE branches SET head = ?1, updated_at = ?2 WHERE name = ?3",
            rusqlite::params![&head.0.to_vec(), now, name],
        )?;
        Ok(())
    }

    fn list_branches(&self) -> StorageResult<Vec<Branch>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT name, head, created_at, updated_at FROM branches ORDER BY name")?;

        let branches = stmt
            .query_map([], |row| {
                let name: String = row.get(0)?;
                let head_bytes: Vec<u8> = row.get(1)?;
                let mut head_arr = [0u8; 32];
                head_arr.copy_from_slice(&head_bytes);
                let created_at: i64 = row.get(2)?;
                let updated_at: i64 = row.get(3)?;

                Ok(Branch {
                    name,
                    head: ContentId(head_arr),
                    created_at,
                    updated_at,
                })
            })
            .map_err(crate::StorageError::Database)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(branches)
    }

    fn delete_branch(&self, name: &str) -> StorageResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM branches WHERE name = ?1",
            rusqlite::params![name],
        )?;
        Ok(())
    }
}
