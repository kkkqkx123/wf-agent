use crate::core::delta::Delta;
use crate::core::file_node::FileNode;
use crate::core::types::{ContentId, DeltaId, LineDiff};
use crate::storage::repository::DeltaStore;
use crate::storage::sqlite::connection::SqliteStorage;
use crate::StorageResult;
use rusqlite::{params, Row};
use std::collections::HashMap;

fn bytes_to_array<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut arr = [0u8; N];
    arr.copy_from_slice(bytes);
    arr
}

fn row_to_delta(row: &Row) -> Result<Delta, rusqlite::Error> {
    let id_bytes: Vec<u8> = row.get(0)?;
    let id = ContentId(bytes_to_array(&id_bytes));

    let file_path: String = row.get(1)?;
    let file_hash_bytes: Vec<u8> = row.get(2)?;
    let file_hash = bytes_to_array(&file_hash_bytes);

    let diff_json: Vec<u8> = row.get(3)?;
    let source: String = row.get(4)?;
    let source_data: Option<String> = row.get(5)?;
    let timestamp: i64 = row.get(6)?;

    let diff: LineDiff = serde_json::from_slice(&diff_json)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let source_type = match source.as_str() {
        "manual" => crate::core::types::SourceType::Manual,
        "agent" => crate::core::types::SourceType::Agent(crate::core::types::AgentInstanceId(
            source_data.unwrap_or_default(),
        )),
        "backup" => crate::core::types::SourceType::Backup,
        _ => crate::core::types::SourceType::Manual,
    };

    Ok(Delta {
        id,
        file: FileNode {
            file_path: std::path::PathBuf::from(file_path),
            base_hash: file_hash,
        },
        diff,
        source: source_type,
        timestamp,
    })
}

impl DeltaStore for SqliteStorage {
    fn store_delta(&self, delta: &Delta) -> StorageResult<()> {
        let conn = self.conn.lock();
        let diff_json = serde_json::to_vec(&delta.diff)?;
        let source_data = match &delta.source {
            crate::core::types::SourceType::Agent(id) => Some(id.to_string()),
            _ => None,
        };
        let source_str = match &delta.source {
            crate::core::types::SourceType::Manual => "manual",
            crate::core::types::SourceType::Agent(_) => "agent",
            crate::core::types::SourceType::Backup => "backup",
        };

        conn.execute(
            "INSERT OR IGNORE INTO deltas (id, file_path, file_hash, diff, source, source_data, timestamp, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &delta.id.0.to_vec(),
                delta.file.path_str(),
                &delta.file.base_hash.to_vec(),
                diff_json,
                source_str,
                source_data,
                delta.timestamp,
                chrono::Utc::now().timestamp_millis(),
            ],
        )?;
        Ok(())
    }

    fn get_delta(&self, id: &DeltaId) -> StorageResult<Delta> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, file_hash, diff, source, source_data, timestamp FROM deltas WHERE id = ?1"
        )?;

        let result = stmt.query_row(params![&id.0.to_vec()], row_to_delta)?;
        Ok(result)
    }

    fn get_deltas(&self, ids: &[DeltaId]) -> StorageResult<Vec<Delta>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock();
        let placeholders: Vec<String> = (0..ids.len()).map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT id, file_path, file_hash, diff, source, source_data, timestamp \
             FROM deltas WHERE id IN ({})",
            placeholders.join(", ")
        );
        let mut stmt = conn.prepare(&sql)?;

        let blob_params: Vec<Vec<u8>> = ids.iter().map(|id| id.0.to_vec()).collect();
        let param_refs: Vec<&[u8]> = blob_params.iter().map(|v| v.as_slice()).collect();

        let deltas = stmt.query_map(
            rusqlite::params_from_iter(&param_refs),
            row_to_delta,
        )?;

        // Collect into a HashMap so we can return results in the original ID order
        let mut delta_map: HashMap<Vec<u8>, Delta> = HashMap::new();
        for d in deltas {
            let delta = d?;
            delta_map.insert(delta.id.0.to_vec(), delta);
        }

        let mut result = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(delta) = delta_map.remove(&id.0.to_vec()) {
                result.push(delta);
            }
        }
        Ok(result)
    }

    fn delta_exists(&self, id: &DeltaId) -> StorageResult<bool> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM deltas WHERE id = ?1")?;
        let count: i64 = stmt.query_row(params![&id.0.to_vec()], |row| row.get(0))?;
        Ok(count > 0)
    }
}
