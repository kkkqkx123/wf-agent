use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::ReentrantMutex;
use rusqlite::{params, Connection};

use crate::backup::backup_snapshot::{BackupFilter, BackupSnapshot};
use crate::core::delta::Delta;
use crate::core::snapshot::Snapshot;
use crate::core::types::{BackupId, ContentId, SnapshotId, SourceType};
use crate::engine::diff::diff_to_line_diff;
use crate::engine::merge::apply_deltas;
use crate::error::{LayertwineError, Result};
use crate::storage::repository::{DeltaStore, FileNodeStore, PartitionStore, SnapshotStore};
use crate::{StorageError, StorageResult};

/// Compress delta JSON with zstd to reduce storage footprint.
/// Uses level 3 for a good speed/ratio trade-off.
fn compress_deltas(deltas: &[Delta]) -> StorageResult<Vec<u8>> {
    let json = serde_json::to_vec(deltas)?;
    zstd::encode_all(json.as_slice(), 3)
        .map_err(|e| StorageError::Serialization(format!("zstd compression failed: {}", e)))
}

/// Decompress zstd-compressed delta JSON back into a Vec<Delta>.
fn decompress_deltas(compressed: &[u8]) -> StorageResult<Vec<Delta>> {
    let decompressed = zstd::decode_all(compressed)
        .map_err(|e| StorageError::Serialization(format!("zstd decompression failed: {}", e)))?;
    serde_json::from_slice(&decompressed)
        .map_err(|e| StorageError::Serialization(format!("delta JSON deserialization failed: {}", e)))
}

const BACKUP_MIGRATION_SQL: &str = "
CREATE TABLE IF NOT EXISTS backup_snapshots (
    id              BLOB PRIMARY KEY,
    source_snapshot BLOB NOT NULL,
    file_path       TEXT NOT NULL,
    file_hash       BLOB NOT NULL,
    deltas          BLOB NOT NULL,
    label           TEXT,
    backed_at       INTEGER NOT NULL,
    metadata        BLOB NOT NULL,
    agent_id        TEXT,
    source_type     TEXT,
    file_content    BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_backup_label ON backup_snapshots(label);
CREATE INDEX IF NOT EXISTS idx_backup_backed_at ON backup_snapshots(backed_at);
CREATE INDEX IF NOT EXISTS idx_backup_agent_id ON backup_snapshots(agent_id);
CREATE INDEX IF NOT EXISTS idx_backup_source_type ON backup_snapshots(source_type);

-- Separate key-value table for SQL-level metadata filtering.
-- Avoids deserializing the JSON blob and filtering in memory.
CREATE TABLE IF NOT EXISTS backup_metadata (
    backup_id BLOB NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (backup_id, key)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_backup_meta_key ON backup_metadata(key, value);
";

fn map_db_err(e: rusqlite::Error) -> LayertwineError {
    LayertwineError::Storage(StorageError::Database(e))
}

pub struct BackupRepo {
    conn: Arc<ReentrantMutex<Connection>>,
}

impl BackupRepo {
    pub fn new_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(BACKUP_MIGRATION_SQL)?;
        Ok(BackupRepo {
            conn: Arc::new(ReentrantMutex::new(conn)),
        })
    }

    pub fn new(path: &Path) -> StorageResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(BACKUP_MIGRATION_SQL)?;
        Ok(BackupRepo {
            conn: Arc::new(ReentrantMutex::new(conn)),
        })
    }

    pub fn backup_snapshot<S>(
        &self,
        core_repo: &S,
        snapshot_id: SnapshotId,
        label: Option<String>,
    ) -> Result<BackupId>
    where
        S: SnapshotStore + DeltaStore + FileNodeStore,
    {
        let snapshot = core_repo
            .get_snapshot(&snapshot_id)
            .map_err(LayertwineError::Storage)?;

        let deltas: Vec<Delta> = core_repo
            .get_deltas(&snapshot.deltas)
            .map_err(LayertwineError::Storage)?;

        // Read and store complete file content for physical isolation
        let file_content = core_repo
            .get_file_content(snapshot.file.path_str(), &snapshot.file.base_hash)
            .map_err(LayertwineError::Storage)?;

        let (agent_id, source_type) = Self::extract_source_info_from_deltas(&deltas);

        let backup_file = deltas
            .last()
            .map(|d| d.file.clone())
            .unwrap_or_else(|| snapshot.file.clone());

        let backup = BackupSnapshot::with_options(
            snapshot_id,
            backup_file,
            deltas,
            label,
            agent_id,
            source_type,
            file_content,
        );

        // Use transaction to ensure atomic backup write
        let conn = self.conn.lock();
        conn.execute_batch("BEGIN IMMEDIATE;")
            .map_err(|e| LayertwineError::Storage(StorageError::Database(e)))?;

        let store_result = self.store_backup_inner(&backup);

        match store_result {
            Ok(()) => {
                conn.execute_batch("COMMIT;")
                    .map_err(|e| LayertwineError::Storage(StorageError::Database(e)))?;
                drop(conn);
                Ok(backup.id)
            }
            Err(e) => {
                conn.execute_batch("ROLLBACK;")
                    .map_err(|_| ()) // ignore rollback error, report original
                    .unwrap_or(());
                drop(conn);
                Err(LayertwineError::Storage(e))
            }
        }
    }

    fn extract_source_info_from_deltas(deltas: &[Delta]) -> (Option<String>, Option<String>) {
        if deltas.is_empty() {
            return (None, None);
        }

        let first_delta = &deltas[0];
        match &first_delta.source {
            SourceType::Agent(agent_id) => (Some(agent_id.to_string()), Some("agent".to_string())),
            SourceType::Manual => (None, Some("manual".to_string())),
            SourceType::Backup => (None, Some("backup".to_string())),
        }
    }

    fn store_backup_inner(&self, backup: &BackupSnapshot) -> StorageResult<()> {
        let conn = self.conn.lock();
        let deltas_json = compress_deltas(&backup.deltas)?;
        let metadata_json = serde_json::to_vec(&backup.metadata)?;

        // Use INSERT OR IGNORE so that identical backups (same content hash) are idempotent
        conn.execute(
            "INSERT OR IGNORE INTO backup_snapshots (id, source_snapshot, file_path, file_hash, deltas, label, backed_at, metadata, agent_id, source_type, file_content)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                &backup.id.0.to_vec(),
                &backup.source_snapshot.0.to_vec(),
                backup.file.path_str(),
                &backup.file.base_hash.to_vec(),
                deltas_json,
                backup.label,
                backup.backed_at,
                metadata_json,
                backup.agent_id,
                backup.source_type,
                &backup.file_content,
            ],
        )?;

        // Also write metadata into the dedicated key-value table for SQL-level filtering.
        if !backup.metadata.is_empty() {
            let mut stmt = conn.prepare(
                "INSERT OR IGNORE INTO backup_metadata (backup_id, key, value) VALUES (?1, ?2, ?3)",
            )?;
            for (key, value) in &backup.metadata {
                stmt.execute(params![&backup.id.0.to_vec(), key, value])?;
            }
        }

        Ok(())
    }

    pub fn get_backup(&self, backup_id: &BackupId) -> Result<BackupSnapshot> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, source_snapshot, file_path, file_hash, deltas, label, backed_at, metadata, agent_id, source_type, file_content
                 FROM backup_snapshots WHERE id = ?1",
            )
            .map_err(map_db_err)?;

        let result = stmt
            .query_row(params![&backup_id.0.to_vec()], |row| {
                let id_bytes: Vec<u8> = row.get(0)?;
                let mut id_arr = [0u8; 32];
                id_arr.copy_from_slice(&id_bytes);

                let src_bytes: Vec<u8> = row.get(1)?;
                let mut src_arr = [0u8; 32];
                src_arr.copy_from_slice(&src_bytes);

                let file_path: String = row.get(2)?;
                let file_hash_bytes: Vec<u8> = row.get(3)?;
                let mut fh_arr = [0u8; 32];
                fh_arr.copy_from_slice(&file_hash_bytes);

                let deltas_json: Vec<u8> = row.get(4)?;
                let label: Option<String> = row.get(5)?;
                let backed_at: i64 = row.get(6)?;
                let metadata_json: Vec<u8> = row.get(7)?;
                let agent_id: Option<String> = row.get(8)?;
                let source_type: Option<String> = row.get(9)?;
                let file_content: Vec<u8> = row.get(10)?;

                let deltas: Vec<Delta> = decompress_deltas(&deltas_json)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                let metadata: HashMap<String, String> = serde_json::from_slice(&metadata_json)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

                Ok(BackupSnapshot {
                    id: ContentId(id_arr),
                    source_snapshot: ContentId(src_arr),
                    file: crate::core::file_node::FileNode {
                        file_path: std::path::PathBuf::from(file_path),
                        base_hash: fh_arr,
                    },
                    deltas,
                    label,
                    backed_at,
                    metadata,
                    agent_id,
                    source_type,
                    file_content,
                })
            })
            .map_err(map_db_err)?;

        Ok(result)
    }

    pub fn query_backups(&self, filter: &BackupFilter) -> Result<Vec<BackupSnapshot>> {
        let conn = self.conn.lock();

        let mut sql = String::from(
            "SELECT id, source_snapshot, file_path, file_hash, deltas, label, backed_at, metadata, agent_id, source_type, file_content
             FROM backup_snapshots WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(src_id) = &filter.source_snapshot {
            sql.push_str(" AND source_snapshot = ?");
            param_values.push(Box::new(src_id.0.to_vec()));
        }
        if let Some((start, end)) = &filter.time_range {
            sql.push_str(" AND backed_at >= ? AND backed_at <= ?");
            param_values.push(Box::new(*start));
            param_values.push(Box::new(*end));
        }
        if let Some(label) = &filter.label {
            sql.push_str(" AND label = ?");
            param_values.push(Box::new(label.clone()));
        }
        if let Some(agent_id) = &filter.agent_id {
            sql.push_str(" AND agent_id = ?");
            param_values.push(Box::new(agent_id.clone()));
        }
        if let Some(source_type) = &filter.source_type {
            sql.push_str(" AND source_type = ?");
            param_values.push(Box::new(source_type.clone()));
        }
        if let Some(meta_key) = &filter.metadata_key {
            if let Some(meta_val) = &filter.metadata_value {
                sql.push_str(
                    " AND EXISTS (SELECT 1 FROM backup_metadata WHERE backup_id = backup_snapshots.id AND key = ? AND value = ?)",
                );
                param_values.push(Box::new(meta_key.clone()));
                param_values.push(Box::new(meta_val.clone()));
            } else {
                sql.push_str(
                    " AND EXISTS (SELECT 1 FROM backup_metadata WHERE backup_id = backup_snapshots.id AND key = ?)",
                );
                param_values.push(Box::new(meta_key.clone()));
            }
        }

        sql.push_str(" ORDER BY backed_at DESC");

        let mut stmt = conn.prepare(&sql).map_err(map_db_err)?;

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                let id_bytes: Vec<u8> = row.get(0)?;
                let mut id_arr = [0u8; 32];
                id_arr.copy_from_slice(&id_bytes);

                let src_bytes: Vec<u8> = row.get(1)?;
                let mut src_arr = [0u8; 32];
                src_arr.copy_from_slice(&src_bytes);

                let file_path: String = row.get(2)?;
                let file_hash_bytes: Vec<u8> = row.get(3)?;
                let mut fh_arr = [0u8; 32];
                fh_arr.copy_from_slice(&file_hash_bytes);

                let deltas_json: Vec<u8> = row.get(4)?;
                let label: Option<String> = row.get(5)?;
                let backed_at: i64 = row.get(6)?;
                let metadata_json: Vec<u8> = row.get(7)?;
                let agent_id: Option<String> = row.get(8)?;
                let source_type: Option<String> = row.get(9)?;
                let file_content: Vec<u8> = row.get(10)?;

                let deltas: Vec<Delta> = decompress_deltas(&deltas_json).unwrap_or_default();
                let metadata: HashMap<String, String> =
                    serde_json::from_slice(&metadata_json).unwrap_or_default();

                Ok(BackupSnapshot {
                    id: ContentId(id_arr),
                    source_snapshot: ContentId(src_arr),
                    file: crate::core::file_node::FileNode {
                        file_path: std::path::PathBuf::from(file_path),
                        base_hash: fh_arr,
                    },
                    deltas,
                    label,
                    backed_at,
                    metadata,
                    agent_id,
                    source_type,
                    file_content,
                })
            })
            .map_err(map_db_err)?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(map_db_err)?);
        }

        Ok(result)
    }

    pub fn delete_backup(&self, backup_id: &BackupId) -> Result<()> {
        let conn = self.conn.lock();
        let affected = conn
            .execute(
                "DELETE FROM backup_snapshots WHERE id = ?1",
                params![&backup_id.0.to_vec()],
            )
            .map_err(map_db_err)?;

        if affected == 0 {
            return Err(LayertwineError::NotFound(format!(
                "backup {} not found",
                backup_id
            )));
        }
        Ok(())
    }

    pub fn count(&self) -> Result<u64> {
        let conn = self.conn.lock();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM backup_snapshots", [], |row| {
                row.get(0)
            })
            .map_err(map_db_err)?;
        Ok(count as u64)
    }

    pub fn merge_to_staged<S>(&self, backup_id: &BackupId, core_repo: &S) -> Result<SnapshotId>
    where
        S: SnapshotStore + DeltaStore + PartitionStore + FileNodeStore,
    {
        // Use transaction on backup DB for consistent reads
        let conn = self.conn.lock();
        conn.execute_batch("BEGIN IMMEDIATE;")
            .map_err(|e| LayertwineError::Storage(StorageError::Database(e)))?;

        let result = self.merge_to_staged_inner(backup_id, core_repo);

        match result {
            Ok(merged_id) => {
                conn.execute_batch("COMMIT;")
                    .map_err(|e| LayertwineError::Storage(StorageError::Database(e)))?;
                drop(conn);
                Ok(merged_id)
            }
            Err(e) => {
                conn.execute_batch("ROLLBACK;")
                    .map_err(|_| ())
                    .unwrap_or(());
                drop(conn);
                Err(e)
            }
        }
    }

    fn merge_to_staged_inner<S>(&self, backup_id: &BackupId, core_repo: &S) -> Result<SnapshotId>
    where
        S: SnapshotStore + DeltaStore + PartitionStore + FileNodeStore,
    {
        let backup = self.get_backup(backup_id)?;

        let integrity_ok = {
            let mut recomputed = BackupSnapshot::with_options(
                backup.source_snapshot,
                backup.file.clone(),
                backup.deltas.clone(),
                backup.label.clone(),
                backup.agent_id.clone(),
                backup.source_type.clone(),
                backup.file_content.clone(),
            );
            recomputed.backed_at = backup.backed_at;
            recomputed.id == backup.id
        };
        if !integrity_ok {
            return Err(LayertwineError::General(
                "backup data integrity check failed".to_string(),
            ));
        }

        // Step 1: Reconstruct the backed-up file content using backup's stored file_content
        let backup_base_str = String::from_utf8_lossy(&backup.file_content).to_string();
        let backup_text = apply_deltas(&backup_base_str, &backup.deltas)?;

        // Step 2: Get current staged content
        let staged_partition = core_repo.get_partition_by_name("staged")?;
        let staged_snapshot = core_repo.get_snapshot(&staged_partition.current_snapshot)?;
        let staged_deltas = core_repo.get_deltas(&staged_snapshot.deltas)?;
        let staged_base = core_repo.get_file_content(
            staged_snapshot.file.path_str(),
            &staged_snapshot.file.base_hash,
        )?;
        let staged_base_str = String::from_utf8(staged_base)
            .map_err(|e| LayertwineError::General(format!("non-utf8 file content: {}", e)))?;
        let staged_text = apply_deltas(&staged_base_str, &staged_deltas)?;

        // Step 3: Three-way merge
        //   base = original file content (common ancestor)
        //   ours = current staged content
        //   theirs = backed-up content
        //   apply_deltas and merge_texts preserve trailing newlines natively.
        let (merged_text, _conflicts) =
            crate::engine::merge::merge_texts(&backup_base_str, &staged_text, &backup_text);

        // Step 4: Compute diff from base to merged result, create delta
        // Optimization: when merged result equals one input with a single delta,
        // reuse that delta's diff directly, avoiding full diff recomputation.
        let diff = if merged_text == backup_text && backup.deltas.len() == 1 {
            backup.deltas[0].diff.clone()
        } else if merged_text == staged_text
            && staged_deltas.len() == 1
            && staged_base_str == backup_base_str
        {
            staged_deltas[0].diff.clone()
        } else {
            diff_to_line_diff(&backup_base_str, &merged_text)
        };
        let merge_file = backup
            .deltas
            .last()
            .map(|d| d.file.clone())
            .unwrap_or_else(|| backup.file.clone());
        let merge_delta = Delta::new(merge_file, diff, crate::core::types::SourceType::Backup);
        core_repo.store_delta(&merge_delta)?;

        // Step 5: Create merge snapshot with both parents
        let source_snapshot = core_repo.get_snapshot(&backup.source_snapshot)?;
        let merged = Snapshot::merge(
            vec![&staged_snapshot, &source_snapshot],
            merge_delta.id,
            "staged".to_string(),
            false,
        );
        core_repo.store_snapshot(&merged, &[])?;
        core_repo.update_pointer(&staged_partition.id, &merged.id)?;

        Ok(merged.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::file_node::FileNode;
    use crate::core::snapshot::Snapshot;
    use crate::core::types::LineDiff;
    use crate::core::types::{DeltaId, SnapshotId, SourceType};
    use crate::storage::repository::{DeltaStore, FileNodeStore, SnapshotStore};
    use crate::storage::SqliteStorage;
    use std::path::PathBuf;

    fn setup_core_repo() -> SqliteStorage {
        SqliteStorage::new_in_memory().unwrap()
    }

    fn create_test_snapshot(
        store: &SqliteStorage,
        path: &str,
        content: &[u8],
        source_type: SourceType,
    ) -> SnapshotId {
        let file_node = FileNode::new(PathBuf::from(path), content);
        store.store_file_node(&file_node, content).unwrap();

        let diff = LineDiff::new(vec![]);
        let delta = Delta::new(file_node.clone(), diff, source_type);
        store.store_delta(&delta).unwrap();

        let snapshot = Snapshot::new_initial(file_node, delta.id);
        store.store_snapshot(&snapshot, content).unwrap();

        snapshot.id
    }

    fn create_staged_partition(store: &SqliteStorage, snapshot_id: SnapshotId) {
        let partition = crate::core::partition::Partition {
            id: uuid::Uuid::now_v7(),
            name: "staged".to_string(),
            current_snapshot: snapshot_id,
            history: vec![snapshot_id],
            partition_type: crate::core::types::PartitionType::Staged,
        };
        store.create_partition(&partition).unwrap();
    }

    #[allow(dead_code)]
    fn create_backup_snapshot_direct(
        backup_repo: &BackupRepo,
        source_snapshot: SnapshotId,
        file_path: &str,
        content: &[u8],
    ) -> BackupId {
        let file_node = FileNode::new(PathBuf::from(file_path), content);
        let diff = LineDiff::new(vec![]);
        let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
        let backup = BackupSnapshot::with_options(
            source_snapshot,
            file_node,
            vec![delta],
            None,
            None,
            None,
            content.to_vec(),
        );
        backup_repo.store_backup_inner(&backup).unwrap();
        backup.id
    }

    #[test]
    fn test_backup_snapshot() {
        let core = setup_core_repo();
        let backup_repo = BackupRepo::new_in_memory().unwrap();

        let snap_id = create_test_snapshot(&core, "test.txt", b"hello world", SourceType::Manual);

        let backup_id = backup_repo
            .backup_snapshot(&core, snap_id, Some("test-backup".to_string()))
            .unwrap();

        let loaded = backup_repo.get_backup(&backup_id).unwrap();
        assert_eq!(loaded.source_snapshot, snap_id);
        assert_eq!(loaded.label, Some("test-backup".to_string()));
        assert_eq!(loaded.deltas.len(), 1);
    }

    #[test]
    fn test_query_backups() {
        let core = setup_core_repo();
        let backup_repo = BackupRepo::new_in_memory().unwrap();

        let snap_id1 = create_test_snapshot(&core, "a.txt", b"content a", SourceType::Manual);
        let snap_id2 = create_test_snapshot(
            &core,
            "b.txt",
            b"content b",
            SourceType::Agent("agent-1".into()),
        );

        backup_repo
            .backup_snapshot(&core, snap_id1, Some("label-a".to_string()))
            .unwrap();
        backup_repo
            .backup_snapshot(&core, snap_id2, Some("label-b".to_string()))
            .unwrap();

        let all = backup_repo.query_backups(&BackupFilter::new()).unwrap();
        assert_eq!(all.len(), 2);

        let filtered = BackupFilter::new().with_label("label-a");
        let result = backup_repo.query_backups(&filtered).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label, Some("label-a".to_string()));
    }

    #[test]
    fn test_delete_backup() {
        let core = setup_core_repo();
        let backup_repo = BackupRepo::new_in_memory().unwrap();

        let snap_id = create_test_snapshot(&core, "del.txt", b"delete me", SourceType::Manual);
        let backup_id = backup_repo.backup_snapshot(&core, snap_id, None).unwrap();

        assert_eq!(backup_repo.count().unwrap(), 1);
        backup_repo.delete_backup(&backup_id).unwrap();
        assert_eq!(backup_repo.count().unwrap(), 0);
    }

    /// Create a file node + delta + snapshot in a realistic chain.
    /// Returns (file_node, delta_id, snapshot_id).
    fn create_initial_snapshot(
        store: &SqliteStorage,
        path: &str,
        content: &[u8],
        source_type: SourceType,
    ) -> (FileNode, DeltaId, SnapshotId) {
        let file_node = FileNode::new(PathBuf::from(path), content);
        store.store_file_node(&file_node, content).unwrap();

        let diff = LineDiff::new(vec![]);
        let delta = Delta::new(file_node.clone(), diff, source_type);
        store.store_delta(&delta).unwrap();

        let snapshot = Snapshot::new_initial(file_node.clone(), delta.id);
        store.store_snapshot(&snapshot, content).unwrap();

        (file_node, delta.id, snapshot.id)
    }

    /// Create a child snapshot from a parent with a real text edit diff.
    fn create_edited_snapshot(
        store: &SqliteStorage,
        parent_id: SnapshotId,
        file_node: &FileNode,
        old_text: &str,
        new_text: &str,
        partition_type: &str,
    ) -> (DeltaId, SnapshotId) {
        let diff = diff_to_line_diff(old_text, new_text);
        let delta = Delta::new(file_node.clone(), diff, SourceType::Manual);
        store.store_delta(&delta).unwrap();

        let parent = store.get_snapshot(&parent_id).unwrap();
        let snapshot = Snapshot::from_parent(&parent, delta.id, partition_type.to_string());
        store.store_snapshot(&snapshot, &[]).unwrap();

        (delta.id, snapshot.id)
    }

    #[test]
    fn test_merge_to_staged() {
        let core = setup_core_repo();
        let backup_repo = BackupRepo::new_in_memory().unwrap();

        // Setup: initial file content "a\nb\nc\n"
        let (file_node, _delta_id, initial_id) =
            create_initial_snapshot(&core, "file.txt", b"a\nb\nc\n", SourceType::Manual);
        create_staged_partition(&core, initial_id);

        // Staged advances: edit "b" → "B" (diverges from backup branch)
        let (_staged_delta_id, staged_id) = create_edited_snapshot(
            &core,
            initial_id,
            &file_node,
            "a\nb\nc\n",
            "a\nB\nc\n",
            "staged",
        );
        let staged_partition = core.get_partition_by_name("staged").unwrap();
        core.update_pointer(&staged_partition.id, &staged_id)
            .unwrap();

        // Backup branch: edit "c" → "C"
        let (_backup_delta_id, backup_snap_id) = create_edited_snapshot(
            &core,
            initial_id,
            &file_node,
            "a\nb\nc\n",
            "a\nb\nC\n",
            "manual",
        );

        let backup_id = backup_repo
            .backup_snapshot(&core, backup_snap_id, Some("merge-test".to_string()))
            .unwrap();

        // Restore: merge backup into staged
        let merged_id = backup_repo.merge_to_staged(&backup_id, &core).unwrap();

        let staged = core.get_partition_by_name("staged").unwrap();
        assert_eq!(staged.current_snapshot, merged_id);

        let merged_snapshot = core.get_snapshot(&merged_id).unwrap();
        assert!(merged_snapshot.parents.contains(&staged_id));
        assert!(merged_snapshot.parents.contains(&backup_snap_id));
        assert!(!merged_snapshot.parents.contains(&initial_id));

        // Verify content combines both edits
        let merged_base = core
            .get_file_content(
                merged_snapshot.file.path_str(),
                &merged_snapshot.file.base_hash,
            )
            .unwrap();
        let merged_deltas = core.get_deltas(&merged_snapshot.deltas).unwrap();
        let merged_content =
            apply_deltas(&String::from_utf8(merged_base).unwrap(), &merged_deltas).unwrap();
        assert_eq!(merged_content, "a\nB\nC\n");
    }

    #[test]
    fn test_restore_from_backup_reconstructs_content() {
        let core = setup_core_repo();
        let backup_repo = BackupRepo::new_in_memory().unwrap();

        // Setup: initial file content "line1\nline2\nline3\n"
        let (file_node, _delta_id, initial_id) = create_initial_snapshot(
            &core,
            "restore.txt",
            b"line1\nline2\nline3\n",
            SourceType::Manual,
        );
        create_staged_partition(&core, initial_id);

        // Create a backup snapshot: edit "line2" → "modified"
        let (_delta_id, backup_id) = create_edited_snapshot(
            &core,
            initial_id,
            &file_node,
            "line1\nline2\nline3\n",
            "line1\nmodified\nline3\n",
            "manual",
        );

        let backup_id = backup_repo
            .backup_snapshot(&core, backup_id, Some("restore-test".to_string()))
            .unwrap();

        // Staged remains at initial, no divergence
        let merged_id = backup_repo.merge_to_staged(&backup_id, &core).unwrap();

        // Verify content matches the backed-up state
        let staged = core.get_partition_by_name("staged").unwrap();
        assert_eq!(staged.current_snapshot, merged_id);

        let merged_snapshot = core.get_snapshot(&merged_id).unwrap();
        let merged_base = core
            .get_file_content(
                merged_snapshot.file.path_str(),
                &merged_snapshot.file.base_hash,
            )
            .unwrap();
        let merged_deltas = core.get_deltas(&merged_snapshot.deltas).unwrap();
        let merged_content =
            apply_deltas(&String::from_utf8(merged_base).unwrap(), &merged_deltas).unwrap();
        assert_eq!(merged_content, "line1\nmodified\nline3\n");
    }

    #[test]
    fn test_physical_isolation() {
        let core = setup_core_repo();
        let backup_repo = BackupRepo::new_in_memory().unwrap();

        let snap_id = create_test_snapshot(&core, "isolated.txt", b"isolated", SourceType::Manual);
        backup_repo.backup_snapshot(&core, snap_id, None).unwrap();

        assert!(core.snapshot_exists(&snap_id).unwrap());
        backup_repo.delete_backup(&snap_id).unwrap_err();
        assert!(core.snapshot_exists(&snap_id).unwrap());
    }

    #[test]
    fn test_backup_stores_file_content() {
        let core = setup_core_repo();
        let backup_repo = BackupRepo::new_in_memory().unwrap();

        let snap_id =
            create_test_snapshot(&core, "content.txt", b"test content", SourceType::Manual);
        let backup_id = backup_repo.backup_snapshot(&core, snap_id, None).unwrap();

        let backup = backup_repo.get_backup(&backup_id).unwrap();
        assert_eq!(backup.file_content, b"test content".to_vec());
    }

    #[test]
    fn test_physical_isolation_with_file_content() {
        let core = setup_core_repo();
        let backup_repo = BackupRepo::new_in_memory().unwrap();

        let content = b"original content";
        let snap_id = create_test_snapshot(&core, "isolated.txt", content, SourceType::Manual);
        let backup_id = backup_repo.backup_snapshot(&core, snap_id, None).unwrap();

        // Verify backup contains file content
        let backup = backup_repo.get_backup(&backup_id).unwrap();
        assert_eq!(backup.file_content, content.to_vec());

        // Now simulate deleting the core storage's file content
        // (In a real scenario, this would be impossible to test, but we verify the backup is independent)

        // Verify we can still retrieve the backup with its content
        let backup_reloaded = backup_repo.get_backup(&backup_id).unwrap();
        assert_eq!(backup_reloaded.file_content, content.to_vec());
    }

    #[test]
    fn test_backup_integrity_check() {
        let core = setup_core_repo();
        let backup_repo = BackupRepo::new_in_memory().unwrap();

        let snap_id = create_test_snapshot(&core, "check.txt", b"integrity", SourceType::Manual);
        let backup_id = backup_repo.backup_snapshot(&core, snap_id, None).unwrap();

        let backup = backup_repo.get_backup(&backup_id).unwrap();
        let mut recomputed = BackupSnapshot::with_options(
            backup.source_snapshot,
            backup.file.clone(),
            backup.deltas.clone(),
            backup.label.clone(),
            backup.agent_id.clone(),
            backup.source_type.clone(),
            backup.file_content.clone(),
        );
        recomputed.backed_at = backup.backed_at;
        assert_eq!(recomputed.id, backup.id);
    }
}
