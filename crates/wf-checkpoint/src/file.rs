use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;

use crate::error::CheckpointError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileState {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub last_modified: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileCheckpoint {
    pub id: String,
    pub timestamp: i64,
    pub full_hash: String,
    pub files: Vec<FileState>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileCheckpointDelta {
    pub added: Vec<FileState>,
    pub modified: Vec<FileState>,
    pub deleted: Vec<String>,
}

/// Metadata for indexing and querying file checkpoints, aligned with the TS
/// `FileCheckpointMetadata`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileCheckpointMetadata {
    pub id: String,
    pub entity_id: String,
    pub timestamp: i64,
    pub checkpoint_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_checkpoint_id: Option<String>,
    pub file_count: u64,
    pub full_hash: String,
    pub total_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<HashMap<String, serde_json::Value>>,
}

/// Storage adapter for file checkpoints, aligned with the TS
/// `FileCheckpointStorageAdapter`.
pub trait FileCheckpointStorageAdapter: Send + Sync {
    fn save(&self, entity_id: &str, checkpoint: &FileCheckpoint) -> Result<(), CheckpointError>;
    fn load(&self, checkpoint_id: &str) -> Result<Option<FileCheckpoint>, CheckpointError>;
    fn delete(&self, checkpoint_id: &str) -> Result<bool, CheckpointError>;
    fn list_by_entity(
        &self,
        entity_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<FileCheckpointMetadata>, CheckpointError>;
    fn get_latest_by_entity(
        &self,
        entity_id: &str,
    ) -> Result<Option<FileCheckpointMetadata>, CheckpointError>;
    fn delete_by_entity(
        &self,
        entity_id: &str,
        keep_latest: Option<usize>,
    ) -> Result<usize, CheckpointError>;
}

pub struct InMemoryFileCheckpointStorage {
    checkpoints: DashMap<String, FileCheckpoint>,
    entity_index: DashMap<String, Vec<String>>,
}

impl InMemoryFileCheckpointStorage {
    pub fn new() -> Self {
        Self {
            checkpoints: DashMap::new(),
            entity_index: DashMap::new(),
        }
    }
}

impl Default for InMemoryFileCheckpointStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl FileCheckpointStorageAdapter for InMemoryFileCheckpointStorage {
    fn save(&self, entity_id: &str, checkpoint: &FileCheckpoint) -> Result<(), CheckpointError> {
        self.checkpoints
            .insert(checkpoint.id.clone(), checkpoint.clone());
        self.entity_index
            .entry(entity_id.to_string())
            .or_default()
            .push(checkpoint.id.clone());
        Ok(())
    }

    fn load(&self, checkpoint_id: &str) -> Result<Option<FileCheckpoint>, CheckpointError> {
        Ok(self.checkpoints.get(checkpoint_id).map(|c| c.clone()))
    }

    fn delete(&self, checkpoint_id: &str) -> Result<bool, CheckpointError> {
        if self.checkpoints.remove(checkpoint_id).is_none() {
            return Ok(false);
        }
        for mut ids in self.entity_index.iter_mut() {
            ids.retain(|id| id != checkpoint_id);
        }
        Ok(true)
    }

    fn list_by_entity(
        &self,
        entity_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<FileCheckpointMetadata>, CheckpointError> {
        let ids: Vec<String> = self
            .entity_index
            .get(entity_id)
            .map(|v| v.clone())
            .unwrap_or_default();
        let mut with_order: Vec<(String, i64, usize)> = ids
            .iter()
            .enumerate()
            .filter_map(|(order, id)| {
                self.checkpoints
                    .get(id)
                    .map(|c| (id.clone(), c.timestamp, order))
            })
            .collect();
        with_order.sort_by_key(|(_, timestamp, order)| (*timestamp, *order));
        if let Some(limit) = limit {
            let start = with_order.len().saturating_sub(limit);
            with_order.drain(..start);
        }
        let ids: Vec<String> = with_order.into_iter().map(|(id, _, _)| id).collect();
        Ok(ids
            .iter()
            .filter_map(|id| {
                self.checkpoints.get(id).map(|c| FileCheckpointMetadata {
                    id: c.id.clone(),
                    entity_id: entity_id.to_string(),
                    timestamp: c.timestamp,
                    checkpoint_type: "full".to_string(),
                    base_checkpoint_id: None,
                    file_count: c.files.len() as u64,
                    full_hash: c.full_hash.clone(),
                    total_size: c.files.iter().map(|f| f.size).sum(),
                    tags: None,
                    custom_fields: None,
                })
            })
            .collect())
    }

    fn get_latest_by_entity(
        &self,
        entity_id: &str,
    ) -> Result<Option<FileCheckpointMetadata>, CheckpointError> {
        let list = self.list_by_entity(entity_id, Some(1))?;
        Ok(list.into_iter().next())
    }

    fn delete_by_entity(
        &self,
        entity_id: &str,
        keep_latest: Option<usize>,
    ) -> Result<usize, CheckpointError> {
        let list = self.list_by_entity(entity_id, None)?;
        let keep = keep_latest.unwrap_or(0);
        let mut deleted = 0usize;
        let delete_count = list.len().saturating_sub(keep);
        for meta in list.into_iter().take(delete_count) {
            if self.delete(&meta.id)? {
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}

#[derive(Clone)]
pub struct FileCheckpointManager {
    storage: Option<Arc<dyn FileCheckpointStorageAdapter>>,
}

impl FileCheckpointManager {
    pub fn new() -> Self {
        Self { storage: None }
    }

    pub fn with_storage(adapter: Arc<dyn FileCheckpointStorageAdapter>) -> Self {
        Self {
            storage: Some(adapter),
        }
    }

    pub fn storage(&self) -> Option<&Arc<dyn FileCheckpointStorageAdapter>> {
        self.storage.as_ref()
    }

    pub fn compute_file_hash(data: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    pub fn compute_diff(previous: &[FileState], current: &[FileState]) -> FileCheckpointDelta {
        let prev_map: HashMap<&str, &FileState> =
            previous.iter().map(|f| (f.path.as_str(), f)).collect();
        let curr_map: HashMap<&str, &FileState> =
            current.iter().map(|f| (f.path.as_str(), f)).collect();

        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut deleted = Vec::new();

        for (path, state) in &curr_map {
            match prev_map.get(path) {
                None => added.push((*state).clone()),
                Some(prev) if prev.hash != state.hash => modified.push((*state).clone()),
                _ => {}
            }
        }

        for path in prev_map.keys() {
            if !curr_map.contains_key(path) {
                deleted.push(path.to_string());
            }
        }

        FileCheckpointDelta {
            added,
            modified,
            deleted,
        }
    }

    pub fn apply_diff(files: &[FileState], delta: &FileCheckpointDelta) -> Vec<FileState> {
        let mut file_map: HashMap<String, FileState> =
            files.iter().map(|f| (f.path.clone(), f.clone())).collect();

        for path in &delta.deleted {
            file_map.remove(path);
        }

        for state in &delta.added {
            file_map.insert(state.path.clone(), state.clone());
        }

        for state in &delta.modified {
            file_map.insert(state.path.clone(), state.clone());
        }

        file_map.into_values().collect()
    }

    pub fn unified_diff(
        previous_content: &str,
        current_content: &str,
        context_lines: usize,
    ) -> String {
        let prev_lines: Vec<&str> = previous_content.lines().collect();
        let curr_lines: Vec<&str> = current_content.lines().collect();

        let mut output = String::new();
        let mut diff_found = false;

        for (i, (p, c)) in prev_lines.iter().zip(curr_lines.iter()).enumerate() {
            if p != c {
                if !diff_found {
                    let start = i.saturating_sub(context_lines);
                    let end = (i + context_lines + 1).min(prev_lines.len());
                    for line in &prev_lines[start..end] {
                        output.push_str(&format!(" {}\n", line));
                    }
                    diff_found = true;
                }
                output.push_str(&format!("-{}\n", p));
                output.push_str(&format!("+{}\n", c));
            } else {
                diff_found = false;
            }
        }

        output
    }

    /// Create a full file checkpoint for an entity and persist it through the
    /// configured storage adapter. Returns the persisted checkpoint.
    pub fn create_checkpoint(
        &self,
        entity_id: &str,
        files: &[FileState],
    ) -> Result<FileCheckpoint, CheckpointError> {        let storage = self.storage.as_ref().ok_or_else(|| {
            CheckpointError::Coordinator("no file checkpoint storage configured".to_string())
        })?;
        let full_hash = {
            let mut parts: Vec<&FileState> = files.iter().collect();
            parts.sort_by(|a, b| a.path.cmp(&b.path));
            let mut digest = String::new();
            for f in parts {
                digest.push_str(&f.path);
                digest.push('=');
                digest.push_str(&f.hash);
                digest.push(';');
            }
            digest
        };
        let checkpoint = FileCheckpoint {
            id: wf_common::generate_id(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            full_hash,
            files: files.to_vec(),
        };
        storage.save(entity_id, &checkpoint)?;
        Ok(checkpoint)
    }

    /// Create a file checkpoint for an entity from the file states recorded
    /// in the entity's latest file checkpoint (the deferred snapshot path
    /// used by async persistence). Returns `None` when the entity has no
    /// previous file checkpoint yet.
    pub fn create_latest_file_checkpoint(
        &self,
        entity_id: &str,
    ) -> Result<Option<FileCheckpoint>, CheckpointError> {
        let storage = self.storage.as_ref().ok_or_else(|| {
            CheckpointError::Coordinator("no file checkpoint storage configured".to_string())
        })?;
        match storage.get_latest_by_entity(entity_id)? {
            Some(meta) => {
                let files = storage
                    .load(&meta.id)?
                    .map(|c| c.files)
                    .unwrap_or_default();
                Ok(Some(self.create_checkpoint(entity_id, &files)?))
            }
            None => Ok(None),
        }
    }

    /// Restore the file checkpoint with `checkpoint_id` for `entity_id`,
    /// returning the restored file states. Best-effort semantics: missing
    /// checkpoints yield an error, mirroring the TS `restoreCheckpoint`.
    pub fn restore_checkpoint(
        &self,
        _entity_id: &str,
        checkpoint_id: &str,
    ) -> Result<Vec<FileState>, CheckpointError> {
        let storage = self.storage.as_ref().ok_or_else(|| {
            CheckpointError::Coordinator("no file checkpoint storage configured".to_string())
        })?;
        storage
            .load(checkpoint_id)?
            .map(|c| c.files)
            .ok_or_else(|| CheckpointError::NotFound {
                id: checkpoint_id.to_string(),
            })
    }

    /// Restore the latest file checkpoint for an entity, if any.
    pub fn restore_latest(
        &self,
        entity_id: &str,
    ) -> Result<Option<Vec<FileState>>, CheckpointError> {
        let storage = self.storage.as_ref().ok_or_else(|| {
            CheckpointError::Coordinator("no file checkpoint storage configured".to_string())
        })?;
        match storage.get_latest_by_entity(entity_id)? {
            Some(meta) => Ok(Some(self.restore_checkpoint(entity_id, &meta.id)?)),
            None => Ok(None),
        }
    }
}

impl Default for FileCheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file(path: &str, hash: &str) -> FileState {
        FileState {
            path: path.to_string(),
            hash: hash.to_string(),
            size: 100,
            last_modified: 1000,
        }
    }

    #[test]
    fn compute_file_hash_produces_consistent_output() {
        let hash1 = FileCheckpointManager::compute_file_hash(b"hello world");
        let hash2 = FileCheckpointManager::compute_file_hash(b"hello world");
        assert_eq!(hash1, hash2);
        assert_ne!(
            hash1,
            FileCheckpointManager::compute_file_hash(b"different")
        );
    }

    #[test]
    fn compute_diff_detects_all_changes() {
        let previous = vec![
            make_file("a.txt", "hash_a"),
            make_file("b.txt", "hash_b"),
            make_file("c.txt", "hash_c"),
        ];
        let current = vec![
            make_file("a.txt", "hash_a"),
            make_file("b.txt", "hash_b_modified"),
            make_file("d.txt", "hash_d"),
        ];

        let diff = FileCheckpointManager::compute_diff(&previous, &current);

        assert_eq!(diff.deleted.len(), 1);
        assert_eq!(diff.deleted[0], "c.txt");
        assert_eq!(diff.modified.len(), 1);
        assert_eq!(diff.modified[0].path, "b.txt");
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].path, "d.txt");
    }

    #[test]
    fn apply_diff_correctly_modifies_files() {
        let files = vec![make_file("a.txt", "hash_a"), make_file("b.txt", "hash_b")];

        let delta = FileCheckpointDelta {
            added: vec![make_file("c.txt", "hash_c")],
            modified: vec![make_file("a.txt", "hash_a_new")],
            deleted: vec!["b.txt".to_string()],
        };

        let result = FileCheckpointManager::apply_diff(&files, &delta);
        let result_map: HashMap<&str, &FileState> =
            result.iter().map(|f| (f.path.as_str(), f)).collect();

        assert_eq!(result.len(), 2);
        assert!(result_map.contains_key("a.txt"));
        assert!(!result_map.contains_key("b.txt"));
        assert!(result_map.contains_key("c.txt"));
        assert_eq!(result_map["a.txt"].hash, "hash_a_new");
    }

    #[test]
    fn compute_diff_empty_previous() {
        let current = vec![make_file("a.txt", "hash_a"), make_file("b.txt", "hash_b")];

        let diff = FileCheckpointManager::compute_diff(&[], &current);
        assert_eq!(diff.added.len(), 2);
        assert!(diff.modified.is_empty());
        assert!(diff.deleted.is_empty());
    }

    #[test]
    fn unified_diff_shows_changes() {
        let prev = "line1\nline2\nline3\n";
        let curr = "line1\nline2_modified\nline3\n";

        let diff = FileCheckpointManager::unified_diff(prev, curr, 1);
        assert!(diff.contains("line2"));
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+line2_modified"));
    }

    #[test]
    fn create_and_restore_checkpoint_roundtrip() {
        let storage = Arc::new(InMemoryFileCheckpointStorage::new());
        let manager = FileCheckpointManager::with_storage(storage);

        let files = vec![make_file("a.txt", "hash_a"), make_file("b.txt", "hash_b")];
        let cp = manager.create_checkpoint("exec-1", &files).unwrap();

        let restored = manager.restore_checkpoint("exec-1", &cp.id).unwrap();
        assert_eq!(restored.len(), 2);
        assert!(restored.contains(&files[0]));
    }

    #[test]
    fn restore_latest_returns_most_recent() {
        let storage = Arc::new(InMemoryFileCheckpointStorage::new());
        let manager = FileCheckpointManager::with_storage(storage);

        manager
            .create_checkpoint("exec-1", &[make_file("a.txt", "v1")])
            .unwrap();
        let cp2 = manager
            .create_checkpoint("exec-1", &[make_file("a.txt", "v2")])
            .unwrap();

        let latest = manager.restore_latest("exec-1").unwrap().unwrap();
        assert_eq!(latest[0].hash, "v2");
        assert_eq!(
            manager
                .storage()
                .unwrap()
                .load(&cp2.id)
                .unwrap()
                .unwrap()
                .files[0]
                .hash,
            "v2"
        );
    }

    #[test]
    fn restore_latest_none_without_checkpoints() {
        let storage = Arc::new(InMemoryFileCheckpointStorage::new());
        let manager = FileCheckpointManager::with_storage(storage);

        assert!(manager.restore_latest("exec-1").unwrap().is_none());
    }

    #[test]
    fn list_and_delete_by_entity() {
        let storage = Arc::new(InMemoryFileCheckpointStorage::new());
        let manager = FileCheckpointManager::with_storage(storage.clone());

        manager
            .create_checkpoint("exec-1", &[make_file("a.txt", "v1")])
            .unwrap();
        manager
            .create_checkpoint("exec-1", &[make_file("a.txt", "v2")])
            .unwrap();
        manager
            .create_checkpoint("exec-2", &[make_file("c.txt", "v1")])
            .unwrap();

        let list = storage.list_by_entity("exec-1", None).unwrap();
        assert_eq!(list.len(), 2);

        let deleted = storage.delete_by_entity("exec-1", Some(1)).unwrap();
        assert_eq!(deleted, 1);
        let remaining = storage.list_by_entity("exec-1", None).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(
            remaining[0].full_hash, "a.txt=v2;",
            "latest checkpoint kept"
        );
    }

    #[test]
    fn delete_missing_returns_false() {
        let storage = Arc::new(InMemoryFileCheckpointStorage::new());
        assert!(!storage.delete("nope").unwrap());
    }
}
