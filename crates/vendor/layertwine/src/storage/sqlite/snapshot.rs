use crate::core::file_node::FileNode;
use crate::core::snapshot::{Snapshot, SnapshotCompression, SnapshotContent};
use crate::core::types::{ContentId, DeltaId, SnapshotId};
use crate::storage::repository::{AtomicOps, SnapshotStore};
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageResult;
use rusqlite::{params, Row};

fn bytes_to_array<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut arr = [0u8; N];
    arr.copy_from_slice(bytes);
    arr
}

fn row_to_snapshot(row: &Row) -> Result<Snapshot, rusqlite::Error> {
    let id_bytes: Vec<u8> = row.get(0)?;
    let id = ContentId(bytes_to_array(&id_bytes));

    let file_path: String = row.get(1)?;
    let file_hash_bytes: Vec<u8> = row.get(2)?;
    let file_hash = bytes_to_array(&file_hash_bytes);

    let deltas_json: Vec<u8> = row.get(3)?;
    let parents_json: Vec<u8> = row.get(4)?;
    let partition_type: String = row.get(5)?;
    let created_at: i64 = row.get(6)?;
    let has_conflicts: bool = row.get::<_, i32>(7)? != 0;

    let deltas: Vec<crate::core::types::DeltaId> = serde_json::from_slice(&deltas_json)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let parents: Vec<SnapshotId> = serde_json::from_slice(&parents_json)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

    // Try to read new columns (may not exist in old databases)
    let source: String = row.get(8).unwrap_or_default();
    let content_type: String = row.get(9).unwrap_or_else(|_| "file".to_string());
    let content_blob: Option<Vec<u8>> = row.get(10).ok();
    let compression_str: String = row.get(11).unwrap_or_else(|_| "none".to_string());
    let content_hash_blob: Option<Vec<u8>> = row.get(12).ok();

    let content = content_blob.map(|bytes| match content_type.as_str() {
        "json" => SnapshotContent::JsonMetadata(
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        ),
        "structured" => SnapshotContent::Structured(bytes),
        "deleted" => SnapshotContent::Deleted,
        _ => SnapshotContent::FileContent(bytes),
    });

    let compression = match compression_str.as_str() {
        "zstd" => SnapshotCompression::Zstd,
        // "gzip" was a no-op placeholder; treat as uncompressed
        _ => SnapshotCompression::None,
    };

    let content_hash = content_hash_blob.and_then(|bytes| {
        if bytes.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Some(ContentId(arr))
        } else {
            None
        }
    });

    // `message` was added after the initial schema; old databases may lack
    // the column (row index out of range) — treat as absent.
    let message: Option<String> = row.get(13).ok().flatten();

    Ok(Snapshot {
        id,
        file: FileNode {
            file_path: std::path::PathBuf::from(file_path),
            base_hash: file_hash,
        },
        deltas,
        parents,
        partition_type,
        created_at,
        has_conflicts,
        content,
        source,
        compression,
        content_hash,
        message,
    })
}

fn snapshot_select_columns() -> &'static str {
    "id, file_path, file_hash, deltas, parents, partition_type, created_at, has_conflicts, source, content_type, content, compression, content_hash, message"
}

/// Whether a storage error indicates the legacy schema without the
/// `snapshots.message` column (pre-upgrade database file).
fn is_missing_message_column(e: &crate::StorageError) -> bool {
    match e {
        crate::StorageError::Database(rusqlite::Error::SqliteFailure(err, msg)) => {
            use rusqlite::ErrorCode;
            // `no such column: message` surfaces as UnknownFailure; match the
            // message text instead of the extended code for robustness.
            err.code == ErrorCode::Unknown && msg.as_deref().unwrap_or_default().contains("message")
        }
        crate::StorageError::Database(e) => e.to_string().contains("message"),
        _ => false,
    }
}

fn insert_snapshot_row(conn: &rusqlite::Connection, snapshot: &Snapshot) -> StorageResult<()> {
    let deltas_json = serde_json::to_vec(&snapshot.deltas)?;
    let parents_json = serde_json::to_vec(&snapshot.parents)?;

    let (content_type, content_blob) = match &snapshot.content {
        Some(sc) => (sc.content_type().to_string(), Some(sc.to_bytes())),
        None => ("file".to_string(), None),
    };

    let compression_str = match snapshot.compression {
        SnapshotCompression::None => "none",
        SnapshotCompression::Zstd => "zstd",
    };

    let content_hash_bytes = snapshot.content_hash.map(|h| h.0.to_vec());

    let result = conn.execute(
        "INSERT INTO snapshots (id, file_path, file_hash, deltas, parents, partition_type, created_at, has_conflicts, source, content_type, content, compression, content_hash, message)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            &snapshot.id.0.to_vec(),
            snapshot.file.path_str(),
            &snapshot.file.base_hash.to_vec(),
            deltas_json,
            parents_json,
            snapshot.partition_type,
            snapshot.created_at,
            snapshot.has_conflicts as i32,
            snapshot.source,
            content_type,
            content_blob,
            compression_str,
            content_hash_bytes,
            snapshot.message,
        ],
    );
    match result {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            // Record-ID collision: the same ID already exists. An identical
            // record (idempotent retry) is fine; a different record under the
            // same ID is an explicit error with context instead of a silent
            // `INSERT OR IGNORE` drop.
            let mut stmt = conn.prepare(&format!(
                "SELECT {} FROM snapshots WHERE id = ?1",
                snapshot_select_columns()
            ))?;
            match stmt.query_row(params![&snapshot.id.0.to_vec()], row_to_snapshot) {
                Ok(existing) => {
                    if existing.file.path_str() == snapshot.file.path_str()
                        && existing.deltas == snapshot.deltas
                        && existing.content == snapshot.content
                    {
                        Ok(())
                    } else {
                        Err(crate::StorageError::Serialization(format!(
                            "snapshot id collision: {} already exists with different content (path '{}' vs '{}')",
                            snapshot.id.to_hex(),
                            existing.file.path_str(),
                            snapshot.file.path_str(),
                        )))
                    }
                }
                Err(_) => Err(crate::StorageError::Serialization(format!(
                    "snapshot id collision: {} already exists",
                    snapshot.id.to_hex()
                ))),
            }
        }
        Err(e) => Err(e.into()),
    }
}

impl SnapshotStore for SqliteStorage {
    fn store_snapshot(&self, snapshot: &Snapshot, _content: &[u8]) -> StorageResult<()> {
        let conn = self.conn.lock();
        match insert_snapshot_row(&conn, snapshot) {
            Ok(()) => Ok(()),
            // Old databases without the `message` column: retry without it.
            Err(e) if is_missing_message_column(&e) => {
                let existing = conn.query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('snapshots') WHERE name = 'message'",
                    [],
                    |row| row.get::<_, i64>(0),
                )?;
                if existing == 0 {
                    let deltas_json = serde_json::to_vec(&snapshot.deltas)?;
                    let parents_json = serde_json::to_vec(&snapshot.parents)?;
                    let (content_type, content_blob) = match &snapshot.content {
                        Some(sc) => (sc.content_type().to_string(), Some(sc.to_bytes())),
                        None => ("file".to_string(), None),
                    };
                    let compression_str = match snapshot.compression {
                        SnapshotCompression::None => "none",
                        SnapshotCompression::Zstd => "zstd",
                    };
                    let content_hash_bytes = snapshot.content_hash.map(|h| h.0.to_vec());
                    conn.execute(
                        "INSERT OR IGNORE INTO snapshots (id, file_path, file_hash, deltas, parents, partition_type, created_at, has_conflicts, source, content_type, content, compression, content_hash)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![
                            &snapshot.id.0.to_vec(),
                            snapshot.file.path_str(),
                            &snapshot.file.base_hash.to_vec(),
                            deltas_json,
                            parents_json,
                            snapshot.partition_type,
                            snapshot.created_at,
                            snapshot.has_conflicts as i32,
                            snapshot.source,
                            content_type,
                            content_blob,
                            compression_str,
                            content_hash_bytes,
                        ],
                    )?;
                    Ok(())
                } else {
                    insert_snapshot_row(&conn, snapshot)
                }
            }
            Err(e) => Err(e),
        }
    }

    fn get_snapshot(&self, id: &SnapshotId) -> StorageResult<Snapshot> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM snapshots WHERE id = ?1",
            snapshot_select_columns()
        ))?;

        let result = stmt.query_row(params![&id.0.to_vec()], row_to_snapshot)?;
        Ok(result)
    }

    fn find_snapshots_by_file(&self, file_path: &str) -> StorageResult<Vec<Snapshot>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM snapshots WHERE file_path = ?1 ORDER BY created_at DESC",
            snapshot_select_columns()
        ))?;

        let snapshots = stmt.query_map(params![file_path], row_to_snapshot)?;

        let mut result = Vec::new();
        for s in snapshots {
            result.push(s?);
        }
        Ok(result)
    }

    fn find_snapshots_by_partition(
        &self,
        partition_type: &crate::core::types::PartitionType,
    ) -> StorageResult<Vec<Snapshot>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM snapshots WHERE partition_type = ?1 ORDER BY created_at DESC",
            snapshot_select_columns()
        ))?;

        let snapshots = stmt.query_map(params![partition_type.name()], row_to_snapshot)?;

        let mut result = Vec::new();
        for s in snapshots {
            result.push(s?);
        }
        Ok(result)
    }

    fn snapshot_exists(&self, id: &SnapshotId) -> StorageResult<bool> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM snapshots WHERE id = ?1")?;
        let count: i64 = stmt.query_row(params![&id.0.to_vec()], |row| row.get(0))?;
        Ok(count > 0)
    }

    fn find_snapshots_by_file_and_time(
        &self,
        file_path: &str,
        time_range: Option<(i64, i64)>,
    ) -> StorageResult<Vec<Snapshot>> {
        let conn = self.conn.lock();
        let sql = match time_range {
            Some(_) => {
                format!(
                    "SELECT {} FROM snapshots WHERE file_path = ?1 AND created_at >= ?2 AND created_at <= ?3 ORDER BY created_at DESC",
                    snapshot_select_columns()
                )
            }
            None => {
                format!(
                    "SELECT {} FROM snapshots WHERE file_path = ?1 ORDER BY created_at DESC",
                    snapshot_select_columns()
                )
            }
        };
        let mut stmt = conn.prepare(sql.as_str())?;
        let snapshots = match time_range {
            Some((start, end)) => {
                stmt.query_map(params![file_path, start, end], row_to_snapshot)?
            }
            None => stmt.query_map(params![file_path], row_to_snapshot)?,
        };
        let mut result = Vec::new();
        for s in snapshots {
            result.push(s?);
        }
        Ok(result)
    }

    fn snapshot_chain_heads(
        &self,
        ids: &[SnapshotId],
    ) -> StorageResult<Vec<(SnapshotId, Option<DeltaId>)>> {
        let mut out = Vec::with_capacity(ids.len());
        if ids.is_empty() {
            return Ok(out);
        }
        let conn = self.conn.lock();
        // Only (id, deltas) columns: no content blobs cross the wire.
        for chunk in ids.chunks(400) {
            let placeholders: Vec<String> = (0..chunk.len()).map(|_| "?".to_string()).collect();
            let sql = format!(
                "SELECT id, deltas FROM snapshots WHERE id IN ({})",
                placeholders.join(", ")
            );
            let mut stmt = conn.prepare(&sql)?;
            let blob_params: Vec<Vec<u8>> = chunk.iter().map(|id| id.0.to_vec()).collect();
            let param_refs: Vec<&[u8]> = blob_params.iter().map(|v| v.as_slice()).collect();
            let rows = stmt.query_map(rusqlite::params_from_iter(&param_refs), |row| {
                let id_bytes: Vec<u8> = row.get(0)?;
                let deltas_json: Vec<u8> = row.get(1)?;
                Ok((id_bytes, deltas_json))
            })?;
            for row in rows {
                let (id_bytes, deltas_json) = row?;
                let deltas: Vec<DeltaId> = serde_json::from_slice(&deltas_json)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                out.push((ContentId(bytes_to_array(&id_bytes)), deltas.last().copied()));
            }
        }
        Ok(out)
    }

    fn store_snapshots_batch(&self, snapshots: &[(&Snapshot, &[u8])]) -> StorageResult<()> {
        self.with_atomic(|storage| {
            for (snapshot, content) in snapshots {
                let conn = storage.conn.lock();
                insert_snapshot_row(&conn, snapshot)?;
                let _ = content;
            }

            Ok(())
        })
    }
}

impl SqliteStorage {
    /// Batch-load snapshots by id with a single SQL query.
    ///
    /// Returns the found snapshots keyed by id; missing ids are skipped.
    /// Callers iterate their own ordered id list against the map so history
    /// order is preserved without N+1 round trips.
    pub fn get_snapshots_map(
        &self,
        ids: &[SnapshotId],
    ) -> StorageResult<std::collections::HashMap<SnapshotId, Snapshot>> {
        let mut map = std::collections::HashMap::with_capacity(ids.len());
        if ids.is_empty() {
            return Ok(map);
        }
        let conn = self.conn.lock();
        // Chunk the IN list to stay under SQLite's variable limit.
        for chunk in ids.chunks(400) {
            let placeholders: Vec<String> = (0..chunk.len()).map(|_| "?".to_string()).collect();
            let sql = format!(
                "SELECT {} FROM snapshots WHERE id IN ({})",
                snapshot_select_columns(),
                placeholders.join(", ")
            );
            let mut stmt = conn.prepare(&sql)?;
            let blob_params: Vec<Vec<u8>> = chunk.iter().map(|id| id.0.to_vec()).collect();
            let param_refs: Vec<&[u8]> = blob_params.iter().map(|v| v.as_slice()).collect();
            let rows = stmt.query_map(rusqlite::params_from_iter(&param_refs), row_to_snapshot)?;
            for snapshot in rows {
                let snapshot = snapshot?;
                map.insert(snapshot.id, snapshot);
            }
        }
        Ok(map)
    }
}
