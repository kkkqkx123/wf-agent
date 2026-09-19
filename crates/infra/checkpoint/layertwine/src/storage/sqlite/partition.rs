use crate::core::partition::Partition;
use crate::core::types::{ContentId, PartitionId, PartitionType, SnapshotId};
use crate::storage::repository::PartitionStore;
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageResult;
use rusqlite::{params, Connection};
use std::collections::HashMap;

type PartitionRow = (Vec<u8>, String, Vec<u8>, Option<String>);

/// Serialized partition metadata stored in the `partition_data` column.
///
/// Contains both the partition type and the redo stack. Older databases
/// may contain a bare JSON string (just the partition type); `deserialize`
/// handles both formats.
#[derive(Debug, Serialize, Deserialize)]
struct PartitionData {
    partition_type: PartitionType,
    #[serde(default)]
    redo_stack: Vec<SnapshotId>,
}

use serde::{Deserialize, Serialize};

impl PartitionStore for SqliteStorage {
    fn create_partition(&self, partition: &Partition) -> StorageResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp_millis();

        let data = PartitionData {
            partition_type: partition.partition_type.clone(),
            redo_stack: partition.redo_stack.clone(),
        };
        let partition_data = serde_json::to_string(&data)?;

        conn.execute(
            "INSERT INTO partitions (id, name, current_snapshot, partition_data, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &partition.id.as_bytes().to_vec(),
                partition.name,
                &partition.current_snapshot.0.to_vec(),
                partition_data,
                now,
                now,
            ],
        )?;

        for (seq, snap_id) in partition.history.iter().enumerate() {
            conn.execute(
                "INSERT INTO partition_history (partition_id, snapshot_id, seq, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    &partition.id.as_bytes().to_vec(),
                    &snap_id.0.to_vec(),
                    seq as i64,
                    now,
                ],
            )?;
        }

        Ok(())
    }

    fn update_pointer(
        &self,
        partition_id: &PartitionId,
        snapshot_id: &SnapshotId,
    ) -> StorageResult<()> {
        let conn = self.conn.lock();
        Self::update_pointer_internal(&conn, partition_id, snapshot_id)
    }

    fn get_partition(&self, id: &PartitionId) -> StorageResult<Partition> {
        let conn = self.conn.lock();
        let id_bytes = id.as_bytes().to_vec();
        let mut stmt = conn.prepare(
            "SELECT id, name, current_snapshot, partition_data, created_at, updated_at
             FROM partitions WHERE id = ?1",
        )?;

        let (name, snap_arr, partition_type, redo_stack) =
            stmt.query_row(params![&id_bytes], |row| {
                let _: Vec<u8> = row.get(0)?;
                let name: String = row.get(1)?;
                let snap_bytes: Vec<u8> = row.get(2)?;
                let mut snap_arr = [0u8; 32];
                snap_arr.copy_from_slice(&snap_bytes);
                let partition_data: Option<String> = row.get(3)?;
                let (partition_type, redo_stack) = parse_partition_data(&partition_data);
                Ok((name, snap_arr, partition_type, redo_stack))
            })?;

        let history = self.load_history(&conn, &id_bytes)?;

        Ok(Partition {
            id: *id,
            name,
            current_snapshot: ContentId(snap_arr),
            history,
            partition_type,
            redo_stack,
        })
    }

    fn get_partition_by_name(&self, name: &str) -> StorageResult<Partition> {
        let conn = self.conn.lock();

        let mut stmt = conn.prepare(
            "SELECT id, name, current_snapshot, partition_data, created_at, updated_at
             FROM partitions WHERE name = ?1",
        )?;

        let (id_bytes, name_ret, snap_bytes, partition_data) =
            stmt.query_row(params![name], |row| {
                let id_bytes: Vec<u8> = row.get(0)?;
                let name: String = row.get(1)?;
                let snap_bytes: Vec<u8> = row.get(2)?;
                let partition_data: Option<String> = row.get(3)?;
                Ok((id_bytes, name, snap_bytes, partition_data))
            })?;

        let id = uuid::Uuid::from_slice(&id_bytes)
            .map_err(|e| crate::StorageError::Serialization(e.to_string()))?;

        let mut snap_arr = [0u8; 32];
        snap_arr.copy_from_slice(&snap_bytes);

        let (partition_type, redo_stack) = parse_partition_data(&partition_data);

        let history = self.load_history(&conn, &id_bytes)?;

        Ok(Partition {
            id,
            name: name_ret,
            current_snapshot: ContentId(snap_arr),
            history,
            partition_type,
            redo_stack,
        })
    }

    fn reset_partition_to_baseline(&self, partition_id: &PartitionId) -> StorageResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp_millis();

        // Get the first history entry seq
        let first_seq: i64 = conn.query_row(
            "SELECT COALESCE(MIN(seq), 0) FROM partition_history WHERE partition_id = ?1",
            params![&partition_id.as_bytes().to_vec()],
            |row| row.get(0),
        )?;
        let first_snapshot: Vec<u8> = conn.query_row(
            "SELECT snapshot_id FROM partition_history WHERE partition_id = ?1 AND seq = ?2",
            params![&partition_id.as_bytes().to_vec(), first_seq],
            |row| row.get(0),
        )?;
        let mut snap_arr = [0u8; 32];
        snap_arr.copy_from_slice(&first_snapshot);

        // Reset current_snapshot to baseline
        conn.execute(
            "UPDATE partitions SET current_snapshot = ?1, updated_at = ?2 WHERE id = ?3",
            params![&first_snapshot, now, &partition_id.as_bytes().to_vec()],
        )?;

        // Delete all history except the first entry
        conn.execute(
            "DELETE FROM partition_history WHERE partition_id = ?1 AND seq > ?2",
            params![&partition_id.as_bytes().to_vec(), first_seq],
        )?;

        Ok(())
    }

    fn update_redo_stack(
        &self,
        partition_id: &PartitionId,
        redo_stack: &[SnapshotId],
    ) -> StorageResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp_millis();

        // Read current partition_data to preserve partition_type
        let current_data: Option<String> = conn.query_row(
            "SELECT partition_data FROM partitions WHERE id = ?1",
            params![&partition_id.as_bytes().to_vec()],
            |row| row.get(0),
        )?;

        let (partition_type, _) = parse_partition_data(&current_data);

        let data = PartitionData {
            partition_type,
            redo_stack: redo_stack.to_vec(),
        };
        let new_data = serde_json::to_string(&data)?;

        conn.execute(
            "UPDATE partitions SET partition_data = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_data, now, &partition_id.as_bytes().to_vec()],
        )?;
        Ok(())
    }

    fn reset_partition_to(
        &self,
        partition_id: &PartitionId,
        snapshot_id: &SnapshotId,
    ) -> StorageResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp_millis();

        // Truncate history to a single baseline entry. The history insert is the
        // single write path: trg_partition_current_from_history keeps
        // partitions.current_snapshot in lockstep with the new sole history entry.
        conn.execute(
            "DELETE FROM partition_history WHERE partition_id = ?1",
            params![&partition_id.as_bytes().to_vec()],
        )?;
        conn.execute(
            "INSERT INTO partition_history (partition_id, snapshot_id, seq, created_at)
             VALUES (?1, ?2, 0, ?3)",
            params![
                &partition_id.as_bytes().to_vec(),
                &snapshot_id.0.to_vec(),
                now,
            ],
        )?;
        Ok(())
    }

    fn list_partitions(&self) -> StorageResult<Vec<Partition>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, current_snapshot, partition_data, created_at, updated_at
             FROM partitions ORDER BY name",
        )?;

        let rows: Vec<PartitionRow> = stmt
            .query_map([], |row| {
                let id_bytes: Vec<u8> = row.get(0)?;
                let name: String = row.get(1)?;
                let snap_bytes: Vec<u8> = row.get(2)?;
                let partition_data: Option<String> = row.get(3)?;
                Ok((id_bytes, name, snap_bytes, partition_data))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Batch load ALL history in one query instead of N separate queries
        let mut hist_stmt = conn.prepare(
            "SELECT partition_id, snapshot_id, seq FROM partition_history ORDER BY partition_id, seq",
        )?;
        let mut history_map: HashMap<Vec<u8>, Vec<SnapshotId>> = HashMap::new();
        let history_rows = hist_stmt.query_map([], |row| {
            let pid: Vec<u8> = row.get(0)?;
            let sid: Vec<u8> = row.get(1)?;
            Ok((pid, sid))
        })?;
        for row in history_rows {
            let (pid, sid) = row?;
            let mut snap_arr = [0u8; 32];
            snap_arr.copy_from_slice(&sid);
            history_map
                .entry(pid)
                .or_default()
                .push(ContentId(snap_arr));
        }

        let mut result = Vec::with_capacity(rows.len());
        for (id_bytes, name, snap_bytes, partition_data) in rows {
            let id = uuid::Uuid::from_slice(&id_bytes)
                .map_err(|e| crate::StorageError::Serialization(e.to_string()))?;

            let mut snap_arr = [0u8; 32];
            snap_arr.copy_from_slice(&snap_bytes);

            let (partition_type, redo_stack) = parse_partition_data(&partition_data);

            let history = history_map.remove(&id_bytes).unwrap_or_default();

            result.push(Partition {
                id,
                name,
                current_snapshot: ContentId(snap_arr),
                history,
                partition_type,
                redo_stack,
            });
        }
        Ok(result)
    }
}

impl SqliteStorage {
    /// Delete a partition and its history.
    pub fn delete_partition(&self, id: &PartitionId) -> StorageResult<()> {
        let conn = self.conn.lock();
        // Delete history first (FK constraint)
        conn.execute(
            "DELETE FROM partition_history WHERE partition_id = ?1",
            params![&id.as_bytes().to_vec()],
        )?;
        conn.execute(
            "DELETE FROM partitions WHERE id = ?1",
            params![&id.as_bytes().to_vec()],
        )?;
        Ok(())
    }

    /// Load partition history for a given partition ID.
    fn load_history(
        &self,
        conn: &Connection,
        partition_id: &[u8],
    ) -> StorageResult<Vec<SnapshotId>> {
        let mut hist_stmt = conn.prepare(
            "SELECT snapshot_id, seq FROM partition_history WHERE partition_id = ?1 ORDER BY seq",
        )?;
        let history: Vec<SnapshotId> = hist_stmt
            .query_map(params![partition_id], |row| {
                let snap_bytes: Vec<u8> = row.get(0)?;
                let mut snap_arr = [0u8; 32];
                snap_arr.copy_from_slice(&snap_bytes);
                Ok(ContentId(snap_arr))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(history)
    }

    fn update_pointer_internal(
        conn: &Connection,
        partition_id: &PartitionId,
        snapshot_id: &SnapshotId,
    ) -> StorageResult<()> {
        let now = chrono::Utc::now().timestamp_millis();

        let max_seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), -1) FROM partition_history WHERE partition_id = ?1",
            params![&partition_id.as_bytes().to_vec()],
            |row| row.get(0),
        )?;

        // Single write path: appending to partition_history is the only way the
        // current_snapshot moves. The trg_partition_current_from_history trigger
        // keeps partitions.current_snapshot in lockstep with the new history tail.
        conn.execute(
            "INSERT INTO partition_history (partition_id, snapshot_id, seq, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                &partition_id.as_bytes().to_vec(),
                &snapshot_id.0.to_vec(),
                max_seq + 1,
                now,
            ],
        )?;

        Ok(())
    }
}

/// Parse `partition_data` column, supporting both legacy bare JSON strings
/// (just the partition type) and the new `PartitionData` format.
fn parse_partition_data(data: &Option<String>) -> (PartitionType, Vec<SnapshotId>) {
    match data {
        None => (PartitionType::Manual, Vec::new()),
        Some(s) => {
            // Try the new format first
            if let Ok(pd) = serde_json::from_str::<PartitionData>(s) {
                return (pd.partition_type, pd.redo_stack);
            }
            // Fallback: bare PartitionType JSON string
            let pt = serde_json::from_str::<PartitionType>(s).unwrap_or(PartitionType::Manual);
            (pt, Vec::new())
        }
    }
}
