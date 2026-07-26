use std::collections::HashMap;

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

pub struct FileCheckpointManager;

impl FileCheckpointManager {
    pub fn new() -> Self {
        Self
    }

    pub fn compute_file_hash(data: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    pub fn compute_diff(
        previous: &[FileState],
        current: &[FileState],
    ) -> FileCheckpointDelta {
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
        let mut file_map: HashMap<String, FileState> = files
            .iter()
            .map(|f| (f.path.clone(), f.clone()))
            .collect();

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
        assert_ne!(hash1, FileCheckpointManager::compute_file_hash(b"different"));
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
}
