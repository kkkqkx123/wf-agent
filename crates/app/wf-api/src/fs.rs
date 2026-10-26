use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::infra::context::ApiContext;

pub const MAX_READ_BYTES: usize = 2 * 1024 * 1024;
const BINARY_PROBE_BYTES: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsFileContent {
    pub path: String,
    pub content: String,
    pub size: u64,
    pub truncated: bool,
    pub is_binary: bool,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsStat {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub is_binary: Option<bool>,
}

#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("user git is not enabled; set user_git.enabled=true with repo_root")]
    NotEnabled,
    #[error("workspace is not initialized: {0}")]
    NotInitialized(String),
    #[error("path escapes the workspace: {0}")]
    PathEscape(String),
    #[error("filesystem entry not found: {0}")]
    NotFound(String),
    #[error("filesystem entry already exists: {0}")]
    AlreadyExists(String),
    #[error("filesystem conflict: {0}")]
    Conflict(String),
    #[error("path is a directory: {0}")]
    IsDirectory(String),
    #[error("path is not a directory: {0}")]
    NotDirectory(String),
    #[error("file too large: {0}")]
    TooLarge(String),
    #[error("filesystem error: {0}")]
    Io(String),
}

pub type FsResult<T> = Result<T, FsError>;

impl From<FsError> for crate::ApiError {
    fn from(err: FsError) -> Self {
        match &err {
            FsError::Validation(_) | FsError::PathEscape(_) | FsError::TooLarge(_) => {
                crate::ApiError::Validation(err.to_string())
            }
            FsError::NotInitialized(_) | FsError::NotFound(_) => crate::ApiError::NotFound {
                entity_type: "workspace-fs".to_string(),
                id: err.to_string(),
            },
            FsError::AlreadyExists(_) | FsError::Conflict(_) | FsError::NotEnabled => {
                crate::ApiError::Conflict(err.to_string())
            }
            FsError::IsDirectory(_) | FsError::NotDirectory(_) => {
                crate::ApiError::Validation(err.to_string())
            }
            FsError::Io(_) => crate::ApiError::execution(err.to_string()),
        }
    }
}

pub fn workspace_root(ctx: &ApiContext) -> FsResult<PathBuf> {
    let config = ctx
        .user_git_config()
        .cloned()
        .filter(|c| c.enabled)
        .ok_or(FsError::NotEnabled)?;
    let root = config.repo_root.ok_or_else(|| {
        FsError::NotInitialized("user_git.repo_root is not configured".to_string())
    })?;
    let path = PathBuf::from(&root);
    if !path.join(".git").exists() && !path.join("HEAD").exists() && !path.is_dir() {
        return Err(FsError::NotInitialized(format!("no workspace at {root}")));
    }
    Ok(path)
}

pub fn validate_fs_path(root: &Path, path: &str) -> FsResult<PathBuf> {
    if path.len() > 1024 {
        return Err(FsError::PathEscape(
            "path must be 1..=1024 chars".to_string(),
        ));
    }
    if path.is_empty() {
        return Ok(root.to_path_buf());
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(FsError::PathEscape(
            "absolute paths are rejected".to_string(),
        ));
    }
    for component in path.split('/') {
        if component == ".." {
            return Err(FsError::PathEscape(format!(
                "path escapes the workspace: {path}"
            )));
        }
    }
    if path.contains('\\') || path.contains('\0') {
        return Err(FsError::PathEscape(
            "path contains illegal characters".to_string(),
        ));
    }
    let joined = root.join(path);
    let normalized = normalize_join(root, &joined);
    if normalized != *root && !normalized.starts_with(root) {
        return Err(FsError::PathEscape(format!(
            "path escapes the workspace: {path}"
        )));
    }
    enforce_symlink_containment(root, &normalized, path)?;
    Ok(normalized)
}

fn normalize_join(root: &Path, joined: &Path) -> PathBuf {
    let mut out = PathBuf::from(root);
    if let Ok(rel) = joined.strip_prefix(root) {
        for component in rel.components() {
            match component {
                Component::ParentDir => {
                    out.pop();
                }
                Component::CurDir => {}
                other => out.push(other.as_os_str()),
            }
        }
    }
    out
}

fn canonical_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

fn enforce_symlink_containment(root: &Path, normalized: &Path, display: &str) -> FsResult<()> {
    let canonical = canonical_root(root);
    let check_target = if normalized.exists() {
        normalized
            .canonicalize()
            .map_err(|e| FsError::Io(format!("failed to resolve {display}: {e}")))?
    } else if let Some(parent) = normalized.parent() {
        if parent.exists() {
            let canonical_parent = parent
                .canonicalize()
                .map_err(|e| FsError::Io(format!("failed to resolve parent of {display}: {e}")))?;
            if !canonical_parent.starts_with(&canonical) {
                return Err(FsError::PathEscape(format!(
                    "path escapes the workspace: {display}"
                )));
            }
            return Ok(());
        } else {
            return Ok(());
        }
    } else {
        return Ok(());
    };
    if !check_target.starts_with(&canonical) {
        return Err(FsError::PathEscape(format!(
            "path escapes the workspace: {display}"
        )));
    }
    Ok(())
}

fn relative_path(root: &Path, absolute: &Path) -> String {
    if absolute == root {
        return String::new();
    }
    absolute
        .strip_prefix(root)
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

fn file_hash(bytes: &[u8]) -> String {
    wf_checkpoint::file::util::sha256_hex(bytes)
}

fn modified_ms(metadata: &std::fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| (d.as_millis().min(i64::MAX as u128)) as i64)
}

fn is_binary_bytes(bytes: &[u8]) -> bool {
    let probe = &bytes[..bytes.len().min(BINARY_PROBE_BYTES)];
    probe.contains(&0)
}

fn entry_of(root: &Path, absolute: &Path) -> FsResult<FsEntry> {
    let metadata = std::fs::symlink_metadata(absolute)
        .map_err(|_| FsError::NotFound(relative_path(root, absolute)))?;
    let file_type = metadata.file_type();
    let is_dir = file_type.is_dir();
    let name = absolute
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let size = if is_dir { 0 } else { metadata.len() };
    Ok(FsEntry {
        name,
        path: relative_path(root, absolute),
        is_dir,
        size,
        modified_ms: modified_ms(&metadata),
    })
}

pub fn list_dir(ctx: &ApiContext, path: &str) -> FsResult<Vec<FsEntry>> {
    let root = workspace_root(ctx)?;
    let absolute = validate_fs_path(&root, path)?;
    let metadata =
        std::fs::symlink_metadata(&absolute).map_err(|_| FsError::NotFound(path.to_string()))?;
    if !metadata.file_type().is_dir() {
        return Err(FsError::NotDirectory(path.to_string()));
    }
    let mut entries = Vec::new();
    let read_dir =
        std::fs::read_dir(&absolute).map_err(|e| FsError::Io(format!("list {path}: {e}")))?;
    for child in read_dir {
        let child = child.map_err(|e| FsError::Io(format!("list {path}: {e}")))?;
        let child_path = child.path();
        let canonical = canonical_root(&root);
        if let Ok(resolved) = child_path.canonicalize() {
            if !resolved.starts_with(&canonical) {
                continue;
            }
        }
        let Ok(entry) = entry_of(&root, &child_path) else {
            continue;
        };
        entries.push(entry);
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsTreeEntry {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsTreeResult {
    pub entries: Vec<FsTreeEntry>,
    pub truncated: bool,
}

pub const FS_TREE_DEFAULT_DEPTH: u32 = 6;
pub const FS_TREE_MAX_DEPTH: u32 = 12;
pub const FS_TREE_DEFAULT_LIMIT: usize = 2000;
pub const FS_TREE_MAX_LIMIT: usize = 5000;

const FS_TREE_SKIPPED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    ".svelte-kit",
    "__pycache__",
];

fn tree_skip_dir(name: &str) -> bool {
    FS_TREE_SKIPPED_DIRS.contains(&name)
}

/// Read-only recursive listing for the command palette file index. Shares
/// path validation with the single-level listing and never reads contents.
pub fn list_tree(
    ctx: &ApiContext,
    base: &str,
    max_depth: Option<u32>,
    max_entries: Option<usize>,
) -> FsResult<FsTreeResult> {
    let depth = max_depth
        .unwrap_or(FS_TREE_DEFAULT_DEPTH)
        .clamp(1, FS_TREE_MAX_DEPTH);
    let limit = max_entries
        .unwrap_or(FS_TREE_DEFAULT_LIMIT)
        .clamp(1, FS_TREE_MAX_LIMIT);
    let root = workspace_root(ctx)?;
    let base_absolute = validate_fs_path(&root, base)?;
    let metadata =
        std::fs::symlink_metadata(&base_absolute).map_err(|_| FsError::NotFound(base.to_string()))?;
    if !metadata.file_type().is_dir() {
        return Err(FsError::NotDirectory(base.to_string()));
    }
    let canonical = canonical_root(&root);
    let mut entries = Vec::new();
    let mut truncated = false;
    let mut stack: Vec<(PathBuf, u32)> = vec![(base_absolute, 0)];
    while let Some((dir, level)) = stack.pop() {
        if level >= depth {
            continue;
        }
        let read_dir =
            std::fs::read_dir(&dir).map_err(|e| FsError::Io(format!("list tree: {e}")))?;
        let mut children: Vec<PathBuf> = Vec::new();
        for child in read_dir {
            let child = child.map_err(|e| FsError::Io(format!("list tree: {e}")))?;
            children.push(child.path());
        }
        children.sort_by(|a, b| {
            let an = a.file_name().map(|n| n.to_string_lossy().to_lowercase());
            let bn = b.file_name().map(|n| n.to_string_lossy().to_lowercase());
            an.cmp(&bn).then_with(|| a.cmp(b))
        });
        for child_path in children {
            if entries.len() >= limit {
                truncated = true;
                break;
            }
            if let Ok(resolved) = child_path.canonicalize() {
                if !resolved.starts_with(&canonical) {
                    continue;
                }
            }
            let Ok(entry) = entry_of(&root, &child_path) else {
                continue;
            };
            if entry.is_dir && tree_skip_dir(&entry.name) {
                continue;
            }
            entries.push(FsTreeEntry {
                path: entry.path.clone(),
                is_dir: entry.is_dir,
                size: entry.size,
            });
            if entries.len() >= limit {
                truncated = true;
                break;
            }
            if entry.is_dir {
                stack.push((child_path, level + 1));
            }
        }
        if truncated {
            break;
        }
    }
    Ok(FsTreeResult { entries, truncated })
}

pub fn stat_path(ctx: &ApiContext, path: &str) -> FsResult<FsStat> {
    let root = workspace_root(ctx)?;
    let absolute = validate_fs_path(&root, path)?;
    let metadata =
        std::fs::symlink_metadata(&absolute).map_err(|_| FsError::NotFound(path.to_string()))?;
    let is_dir = metadata.file_type().is_dir();
    let size = if is_dir { 0 } else { metadata.len() };
    let is_binary = if is_dir {
        None
    } else {
        let probe = read_probe(&absolute)?;
        Some(is_binary_bytes(&probe))
    };
    Ok(FsStat {
        path: path.to_string(),
        is_dir,
        size,
        modified_ms: modified_ms(&metadata),
        is_binary,
    })
}

fn read_probe(absolute: &Path) -> FsResult<Vec<u8>> {
    use std::io::Read;
    let mut file =
        std::fs::File::open(absolute).map_err(|e| FsError::Io(format!("open file: {e}")))?;
    let mut buf = vec![0u8; BINARY_PROBE_BYTES];
    let n = file
        .read(&mut buf)
        .map_err(|e| FsError::Io(format!("read file: {e}")))?;
    buf.truncate(n);
    Ok(buf)
}

pub fn read_file(ctx: &ApiContext, path: &str) -> FsResult<FsFileContent> {
    if path.is_empty() {
        return Err(FsError::IsDirectory("(workspace root)".to_string()));
    }
    let root = workspace_root(ctx)?;
    let absolute = validate_fs_path(&root, path)?;
    let metadata =
        std::fs::symlink_metadata(&absolute).map_err(|_| FsError::NotFound(path.to_string()))?;
    if metadata.file_type().is_dir() {
        return Err(FsError::IsDirectory(path.to_string()));
    }
    if metadata.len() > MAX_READ_BYTES as u64 {
        return Err(FsError::TooLarge(format!(
            "{path} is {} bytes, limit is {MAX_READ_BYTES}",
            metadata.len()
        )));
    }
    let bytes = std::fs::read(&absolute).map_err(|e| FsError::Io(format!("read {path}: {e}")))?;
    if bytes.len() > MAX_READ_BYTES {
        return Err(FsError::TooLarge(format!(
            "{path} exceeds {MAX_READ_BYTES} bytes"
        )));
    }
    let hash = file_hash(&bytes);
    if is_binary_bytes(&bytes) {
        return Ok(FsFileContent {
            path: path.to_string(),
            content: String::new(),
            size: bytes.len() as u64,
            truncated: false,
            is_binary: true,
            hash,
        });
    }
    let content = String::from_utf8(bytes.clone())
        .map_err(|_| FsError::Io(format!("{path} is not valid UTF-8")))?;
    let truncated = content.len() > MAX_READ_BYTES;
    Ok(FsFileContent {
        path: path.to_string(),
        content,
        size: bytes.len() as u64,
        truncated,
        is_binary: false,
        hash,
    })
}

pub fn write_file(
    ctx: &ApiContext,
    path: &str,
    content: &str,
    expected_hash: Option<&str>,
) -> FsResult<FsFileContent> {
    if path.is_empty() {
        return Err(FsError::IsDirectory("(workspace root)".to_string()));
    }
    if content.len() > MAX_READ_BYTES {
        return Err(FsError::TooLarge(format!(
            "content exceeds {MAX_READ_BYTES} bytes"
        )));
    }
    let root = workspace_root(ctx)?;
    let absolute = validate_fs_path(&root, path)?;
    if let Ok(metadata) = std::fs::symlink_metadata(&absolute) {
        if metadata.file_type().is_dir() {
            return Err(FsError::IsDirectory(path.to_string()));
        }
        if let Some(expected) = expected_hash {
            let current =
                std::fs::read(&absolute).map_err(|e| FsError::Io(format!("read {path}: {e}")))?;
            let actual = file_hash(&current);
            if actual != expected {
                return Err(FsError::Conflict(format!(
                    "{path} changed on disk; expected {expected}, found {actual}"
                )));
            }
        }
    } else if expected_hash.is_some() {
        return Err(FsError::NotFound(path.to_string()));
    }
    if let Some(parent) = absolute.parent() {
        std::fs::create_dir_all(parent).map_err(|e| FsError::Io(format!("create parent: {e}")))?;
        let canonical = canonical_root(&root);
        let resolved_parent = parent
            .canonicalize()
            .map_err(|e| FsError::Io(format!("resolve parent: {e}")))?;
        if !resolved_parent.starts_with(&canonical) {
            return Err(FsError::PathEscape(format!(
                "path escapes the workspace: {path}"
            )));
        }
    }
    let bytes = content.as_bytes();
    write_atomic(&absolute, bytes)?;
    Ok(FsFileContent {
        path: path.to_string(),
        content: content.to_string(),
        size: bytes.len() as u64,
        truncated: false,
        is_binary: false,
        hash: file_hash(bytes),
    })
}

fn write_atomic(absolute: &Path, bytes: &[u8]) -> FsResult<()> {
    let parent = absolute
        .parent()
        .ok_or_else(|| FsError::Io("cannot resolve parent directory".to_string()))?;
    let temp = parent.join(format!(
        ".wf-write-{}-{}.tmp",
        std::process::id(),
        wf_common::now()
    ));
    std::fs::write(&temp, bytes).map_err(|e| FsError::Io(format!("write temp file: {e}")))?;
    std::fs::rename(&temp, absolute).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        FsError::Io(format!("replace file: {e}"))
    })?;
    Ok(())
}

pub fn create_entry(ctx: &ApiContext, path: &str, is_dir: bool) -> FsResult<FsEntry> {
    if path.is_empty() {
        return Err(FsError::AlreadyExists("(workspace root)".to_string()));
    }
    let root = workspace_root(ctx)?;
    let absolute = validate_fs_path(&root, path)?;
    if absolute.exists() {
        return Err(FsError::AlreadyExists(path.to_string()));
    }
    if let Some(parent) = absolute.parent() {
        if !parent.exists() {
            return Err(FsError::NotFound(
                parent
                    .strip_prefix(&root)
                    .map(|r| r.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ));
        }
        if !parent.is_dir() {
            return Err(FsError::NotDirectory(path.to_string()));
        }
    }
    if is_dir {
        std::fs::create_dir(&absolute).map_err(|e| FsError::Io(format!("create dir: {e}")))?;
    } else {
        std::fs::write(&absolute, b"").map_err(|e| FsError::Io(format!("create file: {e}")))?;
    }
    entry_of(&root, &absolute)
}

pub fn delete_entry(ctx: &ApiContext, path: &str) -> FsResult<()> {
    if path.is_empty() {
        return Err(FsError::Validation(
            "workspace root cannot be deleted".to_string(),
        ));
    }
    let root = workspace_root(ctx)?;
    let absolute = validate_fs_path(&root, path)?;
    let metadata =
        std::fs::symlink_metadata(&absolute).map_err(|_| FsError::NotFound(path.to_string()))?;
    if metadata.file_type().is_dir() {
        std::fs::remove_dir_all(&absolute).map_err(|e| FsError::Io(format!("delete dir: {e}")))?;
    } else {
        std::fs::remove_file(&absolute).map_err(|e| FsError::Io(format!("delete file: {e}")))?;
    }
    Ok(())
}

pub fn rename_entry(ctx: &ApiContext, from: &str, to: &str) -> FsResult<FsEntry> {
    if from.is_empty() || to.is_empty() {
        return Err(FsError::Validation(
            "source and destination must be non-empty".to_string(),
        ));
    }
    let root = workspace_root(ctx)?;
    let absolute_from = validate_fs_path(&root, from)?;
    let absolute_to = validate_fs_path(&root, to)?;
    if !absolute_from.exists() {
        return Err(FsError::NotFound(from.to_string()));
    }
    if absolute_to.exists() {
        return Err(FsError::AlreadyExists(to.to_string()));
    }
    if let Some(parent) = absolute_to.parent() {
        if !parent.exists() {
            return Err(FsError::NotFound(to.to_string()));
        }
    }
    std::fs::rename(&absolute_from, &absolute_to)
        .map_err(|e| FsError::Io(format!("rename {from} to {to}: {e}")))?;
    entry_of(&root, &absolute_to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn fs_path_validation_matches_git_rules() {
        let root = Path::new("/repo");
        assert!(validate_fs_path(root, "").is_ok());
        assert!(validate_fs_path(root, "src/main.rs").is_ok());
        assert!(validate_fs_path(root, "/etc/passwd").is_err());
        assert!(validate_fs_path(root, "../secret").is_err());
        assert!(validate_fs_path(root, "a/../../b").is_err());
        assert!(validate_fs_path(root, "a\\b").is_err());
        assert!(validate_fs_path(root, "a\0b").is_err());
    }

    #[test]
    fn binary_detection_uses_nul_probe() {
        assert!(is_binary_bytes(b"hello\0world"));
        assert!(!is_binary_bytes(b"hello world\n"));
        assert!(!is_binary_bytes(b""));
    }

    #[test]
    fn file_hash_is_stable_hex() {
        let first = file_hash(b"hello");
        let second = file_hash(b"hello");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert_ne!(first, file_hash(b"world"));
    }
}
