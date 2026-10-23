use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use wf_types::config::file_checkpoint::FailureBehavior;

use crate::file::util::normalize_posix_separators;
use crate::file::FileState;
use checkpoint_base::error::CheckpointError;

/// Hardcoded ignore names.
/// Any directory (or file) with these names is excluded from scanning at any
/// depth, and restore never deletes them.
/// Ignore stacking order: these repository-local excludes come first, then
/// the workspace's own `.gitignore` files, then custom patterns.
/// `.wf-checkpoint-git` is the independent bare object store: it is never
/// scanned, committed or deleted by restore.
pub const HARDCODED_IGNORE_DIRS: &[&str] = &[".git", "node_modules", ".wf-checkpoint-git"];

/// Volatile subdirectories of the project `.wf` directory: synced project
/// configuration lives directly under `.wf` and stays tracked, while
/// machine-local metadata and spill output live here and must never enter
/// scans, commits or restores.
pub const VOLATILE_WF_SUBDIRS: &[&str] = &["meta", "tmp"];

/// Whether a workspace-relative posix path falls inside a volatile `.wf`
/// subdirectory (the directory itself or anything beneath it). Tracked
/// `.wf` roots such as `.wf/config.toml` are not volatile.
pub fn is_volatile_wf_path(relative_path: &str) -> bool {
    let mut parts = relative_path.split('/');
    if parts.next() != Some(".wf") {
        return false;
    }
    matches!(parts.next(), Some(dir) if VOLATILE_WF_SUBDIRS.contains(&dir))
}

/// Files larger than this are not read into checkpoint content by default.
/// They stay on disk untouched: never captured, never restored, never
/// deleted. `0` disables the limit.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Configuration for workspace scanning and content capture.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Additional ignore patterns (gitignore syntax).
    pub custom_ignore_patterns: Vec<String>,
    /// Per-file error handling: `Error` propagates, `Warn` logs and skips,
    /// `Ignore` skips silently.
    pub failure_behavior: FailureBehavior,
    /// Per-file content cap. Files above it are recorded as opaque
    /// oversized entries so capture never holds their bytes in memory and
    /// restore never touches them. `0` disables the cap.
    pub max_file_size: u64,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            custom_ignore_patterns: Vec::new(),
            failure_behavior: FailureBehavior::default(),
            max_file_size: DEFAULT_MAX_FILE_SIZE,
        }
    }
}

/// Result of a full workspace scan.
#[derive(Debug, Clone)]
pub struct WorkspaceScan {
    /// Hashed file states with workspace-relative paths.
    pub files: Vec<FileState>,
    /// All visited directories (relative, posix separators), excluding
    /// ignored ones.
    pub dirs: Vec<String>,
    /// Directories that contain no files (recreated on restore).
    pub empty_dirs: Vec<String>,
    /// Nested Git boundaries found during the scan (submodule checkouts
    /// and independent embedded repositories), each with its resolved
    /// HEAD pointer. Their contents are opaque to the outer workspace.
    pub nested_repos: Vec<NestedRepoState>,
}

/// One nested Git boundary found inside the workspace: a directory
/// containing `.git` (a submodule checkout or an independent embedded
/// repository). Real Git treats such a directory as opaque to the outer
/// repository — its working files are never tracked, its `.gitignore`
/// files are never read, and only the pointer (HEAD commit) belongs to
/// the outer history. The checkpoint model mirrors that contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestedRepoState {
    /// Workspace-relative directory that contains `.git`.
    pub path: String,
    /// HEAD commit id when resolvable (loose ref, packed-refs or detached
    /// HEAD); `None` when the nested store cannot be read.
    pub head_commit: Option<String>,
    /// `.git` is a gitfile pointer (submodule / linked worktree) rather
    /// than a repository directory.
    pub is_gitfile: bool,
}

/// Probe one directory for a nested Git boundary and resolve its HEAD.
/// Best effort: any read failure yields `head_commit: None` instead of
/// blocking the scan.
pub(crate) fn probe_nested_repo(root: &Path, dir_relative: &str) -> Option<NestedRepoState> {
    let dir = root.join(dir_relative);
    let git = dir.join(".git");
    let metadata = fs::symlink_metadata(&git).ok()?;
    let is_gitfile = metadata.is_file();
    let git_dir = if metadata.is_dir() {
        git
    } else if is_gitfile {
        let content = fs::read_to_string(&git).ok()?;
        let target = content.strip_prefix("gitdir:")?.trim();
        let target_path = std::path::Path::new(target);
        if target_path.is_absolute() {
            target_path.to_path_buf()
        } else {
            dir.join(target_path)
        }
    } else {
        return None;
    };
    Some(NestedRepoState {
        path: dir_relative.to_string(),
        head_commit: read_nested_head(&git_dir),
        is_gitfile,
    })
}

/// Whether a string is a 40-char hex object id (nested HEAD / ref targets).
fn is_loose_object_id(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Resolve the HEAD commit of a nested repository: detached HEAD is read
/// directly; a symbolic HEAD is resolved through the loose ref file and
/// then `packed-refs`. `None` when nothing resolves.
fn read_nested_head(git_dir: &Path) -> Option<String> {
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    if is_loose_object_id(head) {
        return Some(head.to_string());
    }
    let refname = head.strip_prefix("ref: ")?.trim();
    if let Ok(id) = fs::read_to_string(git_dir.join(refname)) {
        let id = id.trim();
        if is_loose_object_id(id) {
            return Some(id.to_string());
        }
    }
    let packed = fs::read_to_string(git_dir.join("packed-refs")).ok()?;
    for line in packed.lines() {
        if line.starts_with('#') || line.starts_with('^') {
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let (Some(id), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if name.trim() == refname && is_loose_object_id(id) {
            return Some(id.to_string());
        }
    }
    None
}

/// Whether a workspace-relative path is protected by the hardcoded ignore
/// rules: any path component equals a hardcoded name, or the path falls
/// inside a volatile `.wf` subdirectory.
pub fn is_hardcoded_ignored(relative_path: &str) -> bool {
    if is_volatile_wf_path(relative_path) {
        return true;
    }
    relative_path
        .split('/')
        .any(|part| HARDCODED_IGNORE_DIRS.contains(&part))
}

/// Ignore patterns excluding a SQLite database file inside the workspace
/// plus its WAL sidecars. Returns empty when the database lives outside
/// the workspace root.
pub fn storage_exclude_patterns(root: &Path, db_path: Option<&Path>) -> Vec<String> {
    let Some(db) = db_path else {
        return Vec::new();
    };
    let Ok(relative) = db.strip_prefix(root) else {
        return Vec::new();
    };
    let posix = crate::file::util::normalize_posix_separators(&relative.to_string_lossy());
    if posix.is_empty() {
        return Vec::new();
    }
    vec![
        format!("/{posix}"),
        format!("/{posix}-wal"),
        format!("/{posix}-shm"),
        format!("/{posix}-journal"),
    ]
}

fn custom_search(custom: &[String]) -> gix_ignore::Search {
    gix_ignore::Search::from_overrides(custom.iter().cloned(), Default::default())
}

fn search_is_ignored(
    search: &gix_ignore::Search,
    relative_path: &str,
    is_dir: Option<bool>,
) -> bool {
    let path = gix_object::bstr::BStr::new(relative_path.as_bytes());
    match search.pattern_matching_relative_path(
        path,
        is_dir,
        gix_ignore::glob::pattern::Case::Sensitive,
    ) {
        Some(m) => !m.pattern.is_negative(),
        None => false,
    }
}

/// Single ignore predicate shared by full scans and one-off path checks:
/// hardcoded excludes first, then every ancestor directory (so a bare
/// directory name matches at any depth), then the path itself as a
/// directory and as a file.
fn path_is_ignored(search: &gix_ignore::Search, relative_path: &str) -> bool {
    if is_hardcoded_ignored(relative_path) {
        return true;
    }
    for ancestor in ancestors(relative_path) {
        if search_is_ignored(search, &ancestor, Some(true)) {
            return true;
        }
    }
    if search_is_ignored(search, relative_path, Some(true)) {
        return true;
    }
    search_is_ignored(search, relative_path, Some(false))
}

/// Standalone gitignore verdict for an anchor-relative posix path, without
/// the hardcoded excludes. Shares the ancestor walk with full scans so tool
/// discovery and checkpoint capture read the same workspace ignore files.
pub fn gitignore_ignored(search: &gix_ignore::Search, anchor_relative_path: &str) -> bool {
    for ancestor in ancestors(anchor_relative_path) {
        if search_is_ignored(search, &ancestor, Some(true)) {
            return true;
        }
    }
    if search_is_ignored(search, anchor_relative_path, Some(true)) {
        return true;
    }
    search_is_ignored(search, anchor_relative_path, Some(false))
}

/// Collect every `.gitignore` under an anchor directory into one search,
/// each file anchored at its own directory. Hardcoded directories are not
/// traversed. Unreadable branches are skipped best effort: a missing ignore
/// file must not fail the caller, it only narrows what is hidden.
pub fn collect_gitignore_search(anchor: &Path) -> gix_ignore::Search {
    let mut search = custom_search(&[]);
    collect_gitignore_files(anchor, anchor, &mut search);
    search
}

fn collect_gitignore_files(root: &Path, current: &Path, search: &mut gix_ignore::Search) {
    let gitignore_path = current.join(".gitignore");
    if let Ok(bytes) = std::fs::read(&gitignore_path) {
        search.add_patterns_buffer(
            &bytes,
            gitignore_path.clone(),
            Some(root),
            Default::default(),
        );
    }
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        if HARDCODED_IGNORE_DIRS.contains(&name.to_string_lossy().as_ref()) {
            continue;
        }
        // Nested repositories are opaque: their `.gitignore` files are
        // never folded into the outer workspace's ignore stack.
        let relative = path_to_relative(root, &entry.path());
        if relative
            .as_deref()
            .is_some_and(|rel| probe_nested_repo(root, rel).is_some())
        {
            continue;
        }
        if relative.as_deref().is_some_and(is_volatile_wf_path) {
            continue;
        }
        collect_gitignore_files(root, &entry.path(), search);
    }
}

fn ancestors(relative_path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = PathBuf::from(relative_path);
    while let Some(parent) = current.parent() {
        let s = normalize_posix_separators(&parent.to_string_lossy());
        if s.is_empty() || s == "." {
            break;
        }
        out.push(s.clone());
        current = parent.to_path_buf();
    }
    out
}

/// Mutable outputs threaded through the recursive directory scan: hashed
/// files, visited directories and the nested Git boundaries found.
struct ScanOutputs {
    files: Vec<FileState>,
    dirs: Vec<String>,
    nested_repos: Vec<NestedRepoState>,
}

impl ScanOutputs {
    fn new() -> Self {
        Self {
            files: Vec::new(),
            dirs: Vec::new(),
            nested_repos: Vec::new(),
        }
    }
}

/// Recursive workspace scanner: enumerates files, applies ignore rules
/// (hardcoded + per-directory `.gitignore` + custom patterns) and computes
/// content hashes.
pub struct WorkspaceScanner {
    config: ScanConfig,
    matcher: gix_ignore::Search,
}

impl WorkspaceScanner {
    pub fn new(config: ScanConfig) -> Self {
        let matcher = custom_search(&config.custom_ignore_patterns);
        Self { config, matcher }
    }

    pub fn config(&self) -> &ScanConfig {
        &self.config
    }

    /// Whether a workspace-relative path matches any ignore rule.
    /// Same predicate as the full scan, evaluated against the scanner's
    /// custom patterns. Paths covered only by workspace `.gitignore` files
    /// additionally require a scan-built retrieval; use `scan` for those.
    pub fn is_ignored(&self, relative_path: &str) -> bool {
        path_is_ignored(&self.matcher, relative_path)
    }

    /// Scan the workspace root recursively and return hashed file states,
    /// visited directories and empty directories. Per-directory `.gitignore`
    /// files are collected upfront and combined with the hardcoded and custom
    /// patterns.
    pub fn scan(&self, root: &Path) -> Result<WorkspaceScan, CheckpointError> {
        self.scan_with_baseline(root, &HashMap::new())
    }

    /// Incremental scan: entries whose size and modification time match the
    /// baseline reuse the baseline hash without reading bytes. Clock skew or
    /// coarse timestamps fall back to hashing on any mismatch, so the hash
    /// stays authoritative.
    pub fn scan_with_baseline(
        &self,
        root: &Path,
        baseline: &HashMap<String, FileState>,
    ) -> Result<WorkspaceScan, CheckpointError> {
        let mut search = custom_search(&[]);
        self.collect_gitignore(root, root, &mut search)?;
        if !self.config.custom_ignore_patterns.is_empty() {
            let buffer = self.config.custom_ignore_patterns.join("\n");
            search.add_patterns_buffer(
                buffer.as_bytes(),
                root.join(".wf-custom-ignore"),
                Some(root),
                Default::default(),
            );
        }

        let mut out = ScanOutputs::new();
        self.scan_dir(root, root, &search, baseline, &mut out)?;
        let empty_dirs = find_empty_dirs(&out.dirs, &out.files);
        Ok(WorkspaceScan {
            files: out.files,
            dirs: out.dirs,
            empty_dirs,
            nested_repos: out.nested_repos,
        })
    }

    /// Read every `.gitignore` under the workspace into the shared search,
    /// with each file anchored at its own directory.
    /// `.git`/`node_modules` directories are not traversed.
    fn collect_gitignore(
        &self,
        root: &Path,
        current: &Path,
        search: &mut gix_ignore::Search,
    ) -> Result<(), CheckpointError> {
        let gitignore_path = current.join(".gitignore");
        if let Ok(bytes) = std::fs::read(&gitignore_path) {
            search.add_patterns_buffer(
                &bytes,
                gitignore_path.clone(),
                Some(root),
                Default::default(),
            );
        }

        let entries = match fs::read_dir(current) {
            Ok(entries) => entries,
            Err(err) => {
                return self.handle_failure(
                    &format!("failed to read directory '{}'", current.display()),
                    &err,
                );
            }
        };
        let mut entries: Vec<_> = match entries.collect::<Result<Vec<_>, _>>() {
            Ok(entries) => entries,
            Err(err) => {
                return self.handle_failure(
                    &format!("failed to read directory '{}'", current.display()),
                    &err,
                );
            }
        };
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if HARDCODED_IGNORE_DIRS.contains(&name.as_ref()) {
                continue;
            }
            // Nested repositories are opaque: their `.gitignore` files are
            // never folded into the outer workspace's ignore stack.
            let relative = path_to_relative(root, &entry.path());
            if relative
                .as_deref()
                .is_some_and(|rel| probe_nested_repo(root, rel).is_some())
            {
                continue;
            }
            if relative.as_deref().is_some_and(is_volatile_wf_path) {
                continue;
            }
            self.collect_gitignore(root, &entry.path(), search)?;
        }
        Ok(())
    }

    fn scan_dir(
        &self,
        root: &Path,
        current: &Path,
        search: &gix_ignore::Search,
        baseline: &HashMap<String, FileState>,
        out: &mut ScanOutputs,
    ) -> Result<(), CheckpointError> {
        let entries = match fs::read_dir(current) {
            Ok(entries) => entries,
            Err(err) => {
                return self.handle_failure(
                    &format!("failed to read directory '{}'", current.display()),
                    &err,
                );
            }
        };
        let mut entries: Vec<_> = match entries.collect::<Result<Vec<_>, _>>() {
            Ok(entries) => entries,
            Err(err) => {
                return self.handle_failure(
                    &format!("failed to read directory '{}'", current.display()),
                    &err,
                );
            }
        };
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(err) => {
                    return self.handle_failure(
                        &format!("failed to stat '{}'", entry.path().display()),
                        &err,
                    );
                }
            };
            let relative = match path_to_relative(root, &entry.path()) {
                Some(rel) => rel,
                None => continue,
            };
            if is_hardcoded_ignored(&relative) {
                continue;
            }
            if path_is_ignored(search, &relative) {
                continue;
            }
            if file_type.is_dir() {
                // A directory with its own `.git` (dir or gitfile) is a
                // nested repository: opaque to the outer workspace. Record
                // the pointer and do not traverse its contents.
                if let Some(nested) = probe_nested_repo(root, &relative) {
                    out.nested_repos.push(nested);
                    continue;
                }
                out.dirs.push(relative);
                self.scan_dir(root, &entry.path(), search, baseline, out)?;
            } else if file_type.is_file() {
                match self.hash_file_with_baseline(&relative, &entry.path(), baseline) {
                    Ok(state) => out.files.push(state),
                    Err(CheckpointError::Io(err)) => {
                        self.handle_failure(&format!("failed to hash '{}'", relative), &err)?;
                    }
                    Err(err) => return Err(err),
                }
            }
        }
        Ok(())
    }

    fn hash_file_with_baseline(
        &self,
        relative: &str,
        absolute: &Path,
        baseline: &HashMap<String, FileState>,
    ) -> Result<FileState, CheckpointError> {
        use std::io::Read;
        let metadata = fs::metadata(absolute)?;
        let size = metadata.len();
        let last_modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        if let Some(previous) = baseline.get(relative) {
            if previous.size == size
                && previous.last_modified == last_modified
                && !previous.hash.is_empty()
            {
                return Ok(FileState {
                    path: relative.to_string(),
                    hash: previous.hash.clone(),
                    size,
                    last_modified,
                    deleted: false,
                });
            }
        }
        let file = std::fs::File::open(absolute)?;
        let mut reader = std::io::BufReader::with_capacity(65536, file);
        let mut hasher = sha2::Sha256::new();
        use sha2::Digest;
        let mut buf = [0u8; 65536];
        loop {
            let read = reader.read(&mut buf)?;
            if read == 0 {
                break;
            }
            hasher.update(&buf[..read]);
        }
        let hash = hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        Ok(FileState {
            path: relative.to_string(),
            hash,
            size,
            last_modified,
            deleted: false,
        })
    }

    fn handle_failure(&self, ctx: &str, err: &std::io::Error) -> Result<(), CheckpointError> {
        match self.config.failure_behavior {
            FailureBehavior::Error => Err(CheckpointError::Io(std::io::Error::other(format!(
                "{ctx}: {err}"
            )))),
            FailureBehavior::Warn => {
                tracing::warn!("{ctx}: {err}");
                Ok(())
            }
            FailureBehavior::Ignore => Ok(()),
        }
    }
}

/// Compute the workspace-relative posix path of an entry.
fn path_to_relative(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|p| normalize_posix_separators(&p.to_string_lossy()))
}

/// Directories without any file underneath: collect every ancestor directory
/// of a file, then keep the visited directories that are not among them.
fn find_empty_dirs(dirs: &[String], files: &[FileState]) -> Vec<String> {
    let mut non_empty: HashSet<String> = HashSet::new();
    for file in files {
        let mut dir = PathBuf::from(&file.path);
        while let Some(parent) = dir.parent() {
            let s = normalize_posix_separators(&parent.to_string_lossy());
            if s.is_empty() || s == "." {
                break;
            }
            non_empty.insert(s);
            dir = parent.to_path_buf();
        }
    }
    dirs.iter()
        .filter(|dir| !non_empty.contains(*dir))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "scan/tests.rs"]
mod tests;
