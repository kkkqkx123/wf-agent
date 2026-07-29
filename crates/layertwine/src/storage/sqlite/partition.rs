use crate::core::partition::Partition;
use crate::core::types::{ContentId, PartitionId, PartitionType, SnapshotId};
use crate::storage::repository::PartitionStore;
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageResult;
use rusqlite::{params, Connection};
use std::collections::HashMap;

type PartitionRow = (Vec<u8>, String, Vec<u8>, Option<String>);

impl PartitionStore for SqliteStorage {
    fn create_partition(&self, partition: &Partition) -> StorageResult<()> {
        let conn = self.conn.lock();
        let partition_type_str = format!("{:?}", partition.partition_type);
        let now = chrono::Utc::now().timestamp_millis();

        conn.execute(
            "INSERT INTO partitions (id, name, current_snapshot, partition_type, partition_data, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                &partition.id.as_bytes().to_vec(),
                partition.name,
                &partition.current_snapshot.0.to_vec(),
                partition_type_str,
                serde_json::to_string(&partition.partition_type)?,
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
            "SELECT id, name, current_snapshot, partition_type, partition_data, created_at, updated_at
             FROM partitions WHERE id = ?1"
        )?;

        let (name, snap_arr, partition_type) = stmt.query_row(params![&id_bytes], |row| {
            let _: Vec<u8> = row.get(0)?;
            let name: String = row.get(1)?;
            let snap_bytes: Vec<u8> = row.get(2)?;
            let mut snap_arr = [0u8; 32];
            snap_arr.copy_from_slice(&snap_bytes);
            let partition_data: Option<String> = row.get(4)?;
            let partition_type: PartitionType = partition_data
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(PartitionType::Manual);
            Ok((name, snap_arr, partition_type))
        })?;

        let history = self.load_history(&conn, &id_bytes)?;

        Ok(Partition {
            id: *id,
            name,
            current_snapshot: ContentId(snap_arr),
            history,
            partition_type,
        })
    }

    fn get_partition_by_name(&self, name: &str) -> StorageResult<Partition> {
        let conn = self.conn.lock();

        let mut stmt = conn.prepare(
            "SELECT id, name, current_snapshot, partition_type, partition_data, created_at, updated_at
             FROM partitions WHERE name = ?1"
        )?;

        let (id_bytes, name_ret, snap_bytes, partition_data) =
            stmt.query_row(params![name], |row| {
                let id_bytes: Vec<u8> = row.get(0)?;
                let name: String = row.get(1)?;
                let snap_bytes: Vec<u8> = row.get(2)?;
                let partition_data: Option<String> = row.get(4)?;
                Ok((id_bytes, name, snap_bytes, partition_data))
            })?;

        let id = uuid::Uuid::from_slice(&id_bytes)
            .map_err(|e| crate::StorageError::Serialization(e.to_string()))?;

        let mut snap_arr = [0u8; 32];
        snap_arr.copy_from_slice(&snap_bytes);

        let partition_type: PartitionType = partition_data
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(PartitionType::Manual);

        let history = self.load_history(&conn, &id_bytes)?;

        Ok(Partition {
            id,
            name: name_ret,
            current_snapshot: ContentId(snap_arr),
            history,
            partition_type,
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

    fn list_partitions(&self) -> StorageResult<Vec<Partition>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, current_snapshot, partition_type, partition_data, created_at, updated_at
             FROM partitions ORDER BY name"
        )?;

        let rows: Vec<PartitionRow> = stmt
            .query_map([], |row| {
                let id_bytes: Vec<u8> = row.get(0)?;
                let name: String = row.get(1)?;
                let snap_bytes: Vec<u8> = row.get(2)?;
                let partition_data: Option<String> = row.get(4)?;
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
            history_map.entry(pid).or_default().push(ContentId(snap_arr));
        }

        let mut result = Vec::with_capacity(rows.len());
        for (id_bytes, name, snap_bytes, partition_data) in rows {
            let id = uuid::Uuid::from_slice(&id_bytes)
                .map_err(|e| crate::StorageError::Serialization(e.to_string()))?;

            let mut snap_arr = [0u8; 32];
            snap_arr.copy_from_slice(&snap_bytes);

            let partition_type: PartitionType = partition_data
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(PartitionType::Manual);

            let history = history_map.remove(&id_bytes).unwrap_or_default();

            result.push(Partition {
                id,
                name,
                current_snapshot: ContentId(snap_arr),
                history,
                partition_type,
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
    fn load_history(&self, conn: &Connection, partition_id: &[u8]) -> StorageResult<Vec<SnapshotId>> {
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

        conn.execute(
            "UPDATE partitions SET current_snapshot = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                &snapshot_id.0.to_vec(),
                now,
                &partition_id.as_bytes().to_vec()
            ],
        )?;

        let max_seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), -1) FROM partition_history WHERE partition_id = ?1",
            params![&partition_id.as_bytes().to_vec()],
            |row| row.get(0),
        )?;

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
