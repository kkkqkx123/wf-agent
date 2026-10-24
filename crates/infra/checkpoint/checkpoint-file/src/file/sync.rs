// Explicit sync with the user repository: export the main tree into the
// worktree plus an optional external commit, and import external commits
// back as human history. The checkpoint store stays the process record;
// the user repository stays the delivery record. Reconcile points in the
// metadata KV link one export to the next import.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::file::git_write::map_git_error;
use crate::file::{FileCheckpointManager, WorkspaceRestoreResult};
use crate::git_store::{REF_HUMAN, REF_MAIN};
use crate::scan::{is_hardcoded_ignored, ScanConfig, WorkspaceScanner};
use crate::storage::MetadataStore;
use checkpoint_base::error::CheckpointError;

pub use crate::git_store::{BaselineRebaseOutcome, BaselineSeedOutcome, BaselineStatus};

/// Metadata key for the latest reconcile point. The baseline anchor lives
/// in its own file beside the store, so the two namespaces never overlap.
pub const SYNC_RECONCILE_KEY: &str = "file_checkpoint.sync.reconcile";

/// Export lock file inside the checkpoint directory. Non-blocking: a live
/// lock means another session is exporting.
const EXPORT_LOCK_NAME: &str = "export.lock";

/// One export/import link: which internal main commit was delivered as
/// which external commit, in which repository, and when.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReconcilePoint {
    pub internal_main: String,
    pub external_commit: String,
    pub external_repo: String,
    #[serde(default)]
    pub external_repo_id: String,
    pub updated_at_ms: i64,
}

/// Export controls. Overwriting the worktree always needs an explicit
/// confirmation; committing to the user repository is an explicit opt-in.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub confirm_overwrite: bool,
    pub auto_commit: bool,
    pub message: Option<String>,
}

/// Export result: what was materialized, what message was suggested or
/// committed, and where the reconcile point moved.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportOutcome {
    pub restore: WorkspaceRestoreResult,
    pub suggested_message: String,
    pub external_commit: Option<String>,
    pub reconcile: ReconcilePoint,
    pub idempotent: bool,
}

/// Import result: the new human commit when external files changed, and
/// where the reconcile point moved.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportOutcome {
    pub created: Option<String>,
    pub advanced_to: String,
    pub file_count: usize,
    pub had_conflicts: bool,
    pub no_base: bool,
    pub fallback_poll: bool,
    pub idempotent: bool,
}

/// Hint for polling loops: the external head moved past the reconcile
/// point and waits for an explicit import.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExternalUpdateHint {
    pub external_head: String,
    pub reconcile: ReconcilePoint,
    pub file_count: usize,
}

struct ExportLock {
    path: PathBuf,
}

/// How long a live export holder may keep the lock. Materializing a large
/// worktree can take minutes; thirty minutes keeps genuine long exports
/// safe while a dead holder is reclaimed through process liveness.
const EXPORT_LOCK_GRACE: std::time::Duration = std::time::Duration::from_secs(1800);

impl ExportLock {
    fn acquire(git_dir: &Path) -> Result<Self, CheckpointError> {
        let path = git_dir.join(EXPORT_LOCK_NAME);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write as _;
                let _ = writeln!(file, "pid={}", std::process::id());
                Ok(Self { path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if crate::git_store::external::lock_file_is_stale(&path, EXPORT_LOCK_GRACE) {
                    let _ = std::fs::remove_file(&path);
                    return Self::acquire(git_dir);
                }
                Err(CheckpointError::Branch(
                    "export busy: another session is exporting this workspace".to_string(),
                ))
            }
            Err(e) => Err(CheckpointError::Internal(format!(
                "cannot acquire export lock: {e}"
            ))),
        }
    }
}

impl Drop for ExportLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl FileCheckpointManager {
    fn load_reconcile_point(&self) -> Result<Option<ReconcilePoint>, CheckpointError> {
        let storage = self.storage_ref()?;
        let Some(raw) = storage.load_metadata(SYNC_RECONCILE_KEY)? else {
            return Ok(None);
        };
        let point: ReconcilePoint = serde_json::from_str(&raw)?;
        Ok(Some(point))
    }

    fn store_reconcile_point(&self, point: &ReconcilePoint) -> Result<(), CheckpointError> {
        let storage = self.storage_ref()?;
        let raw = serde_json::to_string(point)?;
        storage.store_metadata(SYNC_RECONCILE_KEY, &raw)?;
        Ok(())
    }

    /// Current reconcile point, if an export has recorded one.
    pub fn reconcile_point(&self) -> Result<Option<ReconcilePoint>, CheckpointError> {
        self.load_reconcile_point()
    }

    /// Move the baseline anchor to a newly resolved commit. Old main content
    /// is transplanted on top so history stays continuous; the previous chain
    /// remains under the normal grace window for rollback.
    pub fn rebase_baseline(
        &self,
        baseline: &wf_types::config::file_checkpoint::BaselineConfig,
    ) -> Result<crate::git_store::BaselineRebaseOutcome, CheckpointError> {
        let git = self.git_ref()?;
        let request = crate::git_store::BaselineRequest {
            mode: match baseline.mode {
                wf_types::config::file_checkpoint::BaselineMode::Borrow => {
                    crate::git_store::BaselineMode::Borrow
                }
                wf_types::config::file_checkpoint::BaselineMode::Copy => {
                    crate::git_store::BaselineMode::Copy
                }
            },
            branch: baseline.branch.clone(),
            commit_prefix: baseline.commit_prefix.clone(),
            behind: baseline.behind,
            tag: baseline.tag.clone(),
        };
        git.rebase_to(&request).map_err(map_git_error)
    }

    /// Paths where the worktree differs from the main tree. Mirrors the
    /// materialize ignore contract so a clean report guarantees a no-op
    /// materialize: hardcoded ignores, nested repositories and oversized
    /// files never count as dirty.
    pub fn export_dirty_paths(&self, base_dir: &Path) -> Result<Vec<String>, CheckpointError> {
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let Some(main_head) = git.read_ref(REF_MAIN).map_err(map_git_error)? else {
            return Err(CheckpointError::NotFound {
                id: REF_MAIN.to_string(),
            });
        };
        let commit = git.read_commit(&main_head).map_err(map_git_error)?;
        let target_bytes = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
        let mut ignore_patterns = self.policy.scan_config.custom_ignore_patterns.clone();
        ignore_patterns.extend(crate::scan::storage_exclude_patterns(
            base_dir,
            storage.db_path().as_deref(),
        ));
        let scanner = WorkspaceScanner::new(ScanConfig {
            custom_ignore_patterns: ignore_patterns,
            failure_behavior: self.policy.scan_config.failure_behavior,
            max_file_size: self.policy.scan_config.max_file_size,
        });
        let scan = scanner.scan(base_dir)?;
        let mut target_hash: HashMap<String, String> = HashMap::with_capacity(target_bytes.len());
        for (path, bytes) in &target_bytes {
            target_hash.insert(path.clone(), crate::file::util::sha256_hex(bytes));
        }
        let nested_dirs: Vec<String> = scan.nested_repos.iter().map(|n| n.path.clone()).collect();
        let under_nested = |relative: &str| {
            nested_dirs.iter().any(|dir| {
                relative == dir
                    || relative
                        .strip_prefix(dir)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
        };
        let oversized = storage.load_oversized_files(&main_head)?;
        let is_oversized = |path: &str| oversized.iter().any(|p| p == path);
        let mut dirty: HashSet<String> = HashSet::new();
        let current_map: HashMap<&str, &crate::file::FileState> =
            scan.files.iter().map(|f| (f.path.as_str(), f)).collect();
        for (path, want) in &target_hash {
            if under_nested(path) || is_oversized(path) {
                continue;
            }
            match current_map.get(path.as_str()) {
                Some(current) if &current.hash == want => {}
                _ => {
                    dirty.insert(path.clone());
                }
            }
        }
        for file in &scan.files {
            if target_hash.contains_key(&file.path)
                || is_hardcoded_ignored(&file.path)
                || under_nested(&file.path)
                || is_oversized(&file.path)
            {
                continue;
            }
            dirty.insert(file.path.clone());
        }
        let mut out: Vec<String> = dirty.into_iter().collect();
        out.sort();
        Ok(out)
    }

    fn suggest_message(&self, main_head: &str, since: Option<&str>) -> String {
        let git = match self.git_ref() {
            Ok(git) => git,
            Err(_) => return "checkpoint export".to_string(),
        };
        let log = git.log(main_head, 0).unwrap_or_default();
        let mut actors: HashSet<String> = HashSet::new();
        let mut count = 0usize;
        let mut earliest: Option<i64> = None;
        let mut latest: Option<i64> = None;
        for commit in &log {
            if Some(commit.id.as_str()) == since {
                break;
            }
            count += 1;
            let actor = commit
                .trailer(crate::git_store::TRAILER_ACTOR)
                .unwrap_or_else(|| commit.author.clone());
            actors.insert(actor);
            earliest = Some(earliest.map_or(commit.committer_ts, |v| v.min(commit.committer_ts)));
            latest = Some(latest.map_or(commit.committer_ts, |v| v.max(commit.committer_ts)));
        }
        let mut names: Vec<String> = actors.into_iter().collect();
        names.sort();
        let who = if names.is_empty() {
            "checkpoint".to_string()
        } else {
            names.join(", ")
        };
        match (count, earliest, latest) {
            (0, _, _) => "checkpoint export: no new commits".to_string(),
            (n, Some(first), Some(last)) => format!(
                "checkpoint export: {n} commit(s) by {who} range {first}..{last} head {}",
                &main_head[..main_head.len().min(8)]
            ),
            (n, _, _) => format!(
                "checkpoint export: {n} commit(s) by {who} head {}",
                &main_head[..main_head.len().min(8)]
            ),
        }
    }

    /// Export the main tree into the worktree and record a reconcile point.
    /// With `auto_commit` the worktree state is also committed to the user
    /// repository; otherwise only materialization plus a suggested message
    /// is produced and the user commits externally.
    pub fn export_to_workspace(
        &self,
        base_dir: &Path,
        opts: ExportOptions,
    ) -> Result<ExportOutcome, CheckpointError> {
        let objects_dir =
            crate::git_store::external::discover_user_objects_dir(base_dir).map_err(|e| {
                CheckpointError::Validation {
                    reason: format!("export needs a git workspace: {e}"),
                }
            })?;
        let fingerprint = crate::git_store::external::fingerprint_for_objects_dir(&objects_dir);
        let repo_id = crate::git_store::external::repository_stable_id(base_dir, &objects_dir);
        let git = self.git_ref()?;
        let _lock = ExportLock::acquire(git.git_dir())?;
        let Some(main_head) = git.read_ref(REF_MAIN).map_err(map_git_error)? else {
            return Err(CheckpointError::NotFound {
                id: REF_MAIN.to_string(),
            });
        };
        if !git
            .list_unresolved_conflicts()
            .map_err(map_git_error)?
            .is_empty()
        {
            return Err(CheckpointError::MergeConflict {
                actor: "export".to_string(),
                files: vec!["resolve conflicts before export".to_string()],
            });
        }
        let previous = self.load_reconcile_point()?;
        if let Some(prev) = &previous {
            if prev.external_repo != fingerprint
                || (!prev.external_repo_id.is_empty() && prev.external_repo_id != repo_id)
            {
                // Repository moved: treat as a fresh export from the current
                // main head instead of mixing histories.
            } else if !prev.external_commit.is_empty()
                && crate::git_store::external::read_external_commit(
                    &objects_dir,
                    &prev.external_commit,
                )
                .is_err()
            {
                tracing::warn!(
                    commit = %prev.external_commit,
                    "previous reconcile commit is not resolvable in the user repository; \
                     continuing from a fresh baseline-anchored export"
                );
            }
        }
        let dirty = self.export_dirty_paths(base_dir)?;
        if !dirty.is_empty() && !opts.confirm_overwrite {
            return Err(CheckpointError::Validation {
                reason: format!(
                    "worktree has {} unexported change(s); confirm overwrite to export",
                    dirty.len()
                ),
            });
        }
        let external_before = crate::git_store::external::external_head_id(base_dir).ok();
        let base_for_message = previous
            .as_ref()
            .filter(|p| {
                p.external_repo == fingerprint
                    && (p.external_repo_id.is_empty() || p.external_repo_id == repo_id)
            })
            .map(|p| p.internal_main.clone())
            .or_else(|| git.baseline_anchor_id());
        let suggested = opts
            .message
            .clone()
            .unwrap_or_else(|| self.suggest_message(&main_head, base_for_message.as_deref()));
        let file_opts = crate::file::FileCheckpointOptions {
            failure_behavior: self.policy.scan_config.failure_behavior,
            custom_ignore_patterns: self.policy.scan_config.custom_ignore_patterns.clone(),
        };
        if dirty.is_empty() {
            if let (Some(prev), Some(head)) = (&previous, &external_before) {
                if prev.external_repo == fingerprint
                    && (prev.external_repo_id.is_empty() || prev.external_repo_id == repo_id)
                    && prev.internal_main == main_head
                    && prev.external_commit == *head
                {
                    return Ok(ExportOutcome {
                        restore: WorkspaceRestoreResult::default(),
                        suggested_message: suggested,
                        external_commit: Some(head.clone()),
                        reconcile: prev.clone(),
                        idempotent: true,
                    });
                }
            }
            if external_before.is_none() && previous.is_none() {
                let point = ReconcilePoint {
                    internal_main: main_head.clone(),
                    external_commit: String::new(),
                    external_repo: fingerprint,
                    external_repo_id: repo_id.clone(),
                    updated_at_ms: now_millis(),
                };
                self.store_reconcile_point(&point)?;
                return Ok(ExportOutcome {
                    restore: WorkspaceRestoreResult::default(),
                    suggested_message: suggested,
                    external_commit: None,
                    reconcile: point,
                    idempotent: true,
                });
            }
        }
        let restore = self.materialize_commit(&main_head, base_dir, &file_opts)?;
        let external_commit = if opts.auto_commit {
            Some(self.commit_external(base_dir, &suggested, &dirty)?)
        } else {
            external_before.clone()
        };
        let point = ReconcilePoint {
            internal_main: main_head.clone(),
            external_commit: external_commit.clone().unwrap_or_default(),
            external_repo: fingerprint,
            external_repo_id: repo_id.clone(),
            updated_at_ms: now_millis(),
        };
        self.store_reconcile_point(&point)?;
        Ok(ExportOutcome {
            restore,
            suggested_message: suggested,
            external_commit,
            reconcile: point,
            idempotent: false,
        })
    }

    fn commit_external(
        &self,
        base_dir: &Path,
        message: &str,
        paths: &[String],
    ) -> Result<String, CheckpointError> {
        if !paths.is_empty() {
            let mut command = std::process::Command::new("git");
            command
                .arg("-C")
                .arg(base_dir)
                .arg("add")
                .arg("-A")
                .arg("--");
            for path in paths {
                command.arg(path);
            }
            let status = command
                .status()
                .map_err(|e| CheckpointError::Internal(format!("git add failed: {e}")))?;
            if !status.success() {
                return Err(CheckpointError::Internal("git add failed".to_string()));
            }
        }
        let staged_clean = std::process::Command::new("git")
            .arg("-C")
            .arg(base_dir)
            .arg("diff")
            .arg("--cached")
            .arg("--quiet")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if staged_clean {
            let head = crate::git_store::external::external_head_id(base_dir).map_err(|e| {
                CheckpointError::Internal(format!("cannot read external HEAD: {e}"))
            })?;
            return Ok(head);
        }
        let commit = std::process::Command::new("git")
            .arg("-C")
            .arg(base_dir)
            .arg("-c")
            .arg("user.email=checkpoint@local")
            .arg("-c")
            .arg("user.name=checkpoint")
            .arg("-c")
            .arg("commit.gpgsign=false")
            .arg("commit")
            .arg("-m")
            .arg(message)
            .output()
            .map_err(|e| CheckpointError::Internal(format!("git commit failed to spawn: {e}")))?;
        if !commit.status.success() {
            let detail = String::from_utf8_lossy(&commit.stderr).trim().to_string();
            return Err(CheckpointError::Internal(format!(
                "git commit failed: {detail}"
            )));
        }
        let head = crate::git_store::external::external_head_id(base_dir)
            .map_err(|e| CheckpointError::Internal(format!("cannot read external HEAD: {e}")))?;
        Ok(head)
    }

    /// Polling hint: the external head moved past the reconcile point.
    /// Never captures; the caller decides between an explicit import and
    /// the opt-in automatic path.
    pub fn external_update_hint(
        &self,
        base_dir: &Path,
    ) -> Result<Option<ExternalUpdateHint>, CheckpointError> {
        let Some(reconcile) = self.load_reconcile_point()? else {
            return Ok(None);
        };
        let objects_dir =
            crate::git_store::external::discover_user_objects_dir(base_dir).map_err(|e| {
                CheckpointError::Validation {
                    reason: format!("import needs a git workspace: {e}"),
                }
            })?;
        let fingerprint = crate::git_store::external::fingerprint_for_objects_dir(&objects_dir);
        let repo_id = crate::git_store::external::repository_stable_id(base_dir, &objects_dir);
        if reconcile.external_repo != fingerprint {
            return Ok(None);
        }
        if !reconcile.external_repo_id.is_empty() && reconcile.external_repo_id != repo_id {
            return Ok(None);
        }
        let Ok(head) = crate::git_store::external::external_head_id(base_dir) else {
            tracing::warn!("external HEAD unreadable; hint degraded to null");
            return Ok(None);
        };
        if head == reconcile.external_commit {
            return Ok(None);
        }
        let file_count =
            match self.external_diff_size(&objects_dir, &reconcile.external_commit, &head) {
                Ok(count) => count,
                Err(err) => {
                    tracing::warn!(error = %err, "external diff unreadable; hint degraded to null");
                    return Ok(None);
                }
            };
        Ok(Some(ExternalUpdateHint {
            external_head: head,
            reconcile,
            file_count,
        }))
    }

    fn external_diff_size(
        &self,
        objects_dir: &Path,
        base: &str,
        head: &str,
    ) -> Result<usize, CheckpointError> {
        use crate::git_store::external as ext;
        if base.is_empty() {
            let head_commit = ext::read_external_commit(objects_dir, head).map_err(|e| {
                CheckpointError::Internal(format!("cannot read external HEAD: {e}"))
            })?;
            let files =
                ext::external_tree_to_files(objects_dir, &head_commit.tree).map_err(|e| {
                    CheckpointError::Internal(format!("cannot read external tree: {e}"))
                })?;
            return Ok(files.len());
        }
        let base_tree = ext::read_external_commit(objects_dir, base)
            .map(|c| c.tree)
            .unwrap_or_default();
        let head_commit = ext::read_external_commit(objects_dir, head)
            .map_err(|e| CheckpointError::Internal(format!("cannot read external HEAD: {e}")))?;
        if base_tree.is_empty() {
            let files =
                ext::external_tree_to_files(objects_dir, &head_commit.tree).map_err(|e| {
                    CheckpointError::Internal(format!("cannot read external tree: {e}"))
                })?;
            return Ok(files.len());
        }
        let base_files = ext::external_tree_to_files(objects_dir, &base_tree)
            .map_err(|e| CheckpointError::Internal(format!("cannot read external base: {e}")))?;
        let head_files = ext::external_tree_to_files(objects_dir, &head_commit.tree)
            .map_err(|e| CheckpointError::Internal(format!("cannot read external tree: {e}")))?;
        let mut keys: HashSet<String> = HashSet::new();
        keys.extend(base_files.keys().cloned());
        keys.extend(head_files.keys().cloned());
        let mut count = 0usize;
        for key in keys {
            if base_files.get(&key).map(|(_, b)| b) != head_files.get(&key).map(|(_, b)| b) {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Explicit import: capture the external net change as one human commit
    /// and advance the reconcile point. Overlapping edits reuse the standard
    /// three-way merge; conflicted paths land with markers and an unresolved
    /// marker so the existing conflict listing surfaces them.
    pub fn import_from_external(&self, base_dir: &Path) -> Result<ImportOutcome, CheckpointError> {
        use crate::git_store::external as ext;
        let Some(reconcile) = self.load_reconcile_point()? else {
            return Err(CheckpointError::Validation {
                reason: "no reconcile point; export before import".to_string(),
            });
        };
        let objects_dir =
            ext::discover_user_objects_dir(base_dir).map_err(|e| CheckpointError::Validation {
                reason: format!("import needs a git workspace: {e}"),
            })?;
        let fingerprint = ext::fingerprint_for_objects_dir(&objects_dir);
        let repo_id = ext::repository_stable_id(base_dir, &objects_dir);
        if reconcile.external_repo != fingerprint {
            return Err(CheckpointError::Validation {
                reason: "reconcile points at a different repository; export again".to_string(),
            });
        }
        if !reconcile.external_repo_id.is_empty() && reconcile.external_repo_id != repo_id {
            return Err(CheckpointError::Validation {
                reason: "reconcile points at a different repository; export again".to_string(),
            });
        }
        let head = match ext::external_head_id(base_dir) {
            Ok(head) => head,
            Err(_) => {
                // External history unreadable: fall back to the existing
                // worktree polling capture instead of adding a new channel.
                let created = self.poll_human_edits(base_dir)?;
                let done = created.is_none();
                return Ok(ImportOutcome {
                    created,
                    advanced_to: reconcile.external_commit.clone(),
                    file_count: 0,
                    had_conflicts: false,
                    no_base: false,
                    fallback_poll: true,
                    idempotent: done,
                });
            }
        };
        if head == reconcile.external_commit {
            return Ok(ImportOutcome {
                created: None,
                advanced_to: head,
                file_count: 0,
                had_conflicts: false,
                no_base: false,
                fallback_poll: false,
                idempotent: true,
            });
        }
        let git = self.git_ref()?;
        let Some(main_head) = git.read_ref(REF_MAIN).map_err(map_git_error)? else {
            return Err(CheckpointError::NotFound {
                id: REF_MAIN.to_string(),
            });
        };
        if self.external_tree_matches_main(&objects_dir, &head, &main_head)? {
            let point = ReconcilePoint {
                internal_main: main_head,
                external_commit: head.clone(),
                external_repo: fingerprint,
                external_repo_id: repo_id.clone(),
                updated_at_ms: now_millis(),
            };
            self.store_reconcile_point(&point)?;
            return Ok(ImportOutcome {
                created: None,
                advanced_to: head,
                file_count: 0,
                had_conflicts: false,
                no_base: false,
                fallback_poll: false,
                idempotent: true,
            });
        }
        let (base_files, no_base): (HashMap<String, (String, Vec<u8>)>, bool) = if reconcile
            .external_commit
            .is_empty()
        {
            (HashMap::new(), true)
        } else {
            match ext::read_external_commit(&objects_dir, &reconcile.external_commit) {
                Ok(base_commit) => {
                    let base_modes = ext::external_tree_to_files(&objects_dir, &base_commit.tree)
                        .map_err(|e| {
                        CheckpointError::Internal(format!("cannot read external base: {e}"))
                    })?;
                    let base_bytes = ext::external_tree_to_bytes(&objects_dir, &base_commit.tree)
                        .map_err(|e| {
                        CheckpointError::Internal(format!("cannot read external base: {e}"))
                    })?;
                    let mut joined = HashMap::with_capacity(base_bytes.len());
                    for (path, bytes) in base_bytes {
                        let mode = base_modes
                            .get(&path)
                            .map(|(m, _)| m.clone())
                            .unwrap_or_else(|| crate::git_store::MODE_FILE.to_string());
                        joined.insert(path, (mode, bytes));
                    }
                    (joined, false)
                }
                Err(_) => (HashMap::new(), true),
            }
        };
        let head_commit = ext::read_external_commit(&objects_dir, &head)
            .map_err(|e| CheckpointError::Internal(format!("cannot read external HEAD: {e}")))?;
        let head_files = ext::external_tree_to_bytes(&objects_dir, &head_commit.tree)
            .map_err(|e| CheckpointError::Internal(format!("cannot read external tree: {e}")))?;
        let head_modes = ext::external_tree_to_files(&objects_dir, &head_commit.tree)
            .map_err(|e| CheckpointError::Internal(format!("cannot read external tree: {e}")))?;
        let main_commit = git.read_commit(&main_head).map_err(map_git_error)?;
        let main_files = git
            .tree_to_bytes(&main_commit.tree)
            .map_err(map_git_error)?;
        let mut keys: HashSet<String> = HashSet::new();
        keys.extend(base_files.keys().cloned());
        keys.extend(head_files.keys().cloned());
        let mut changes: HashMap<String, Option<(String, Vec<u8>)>> = HashMap::new();
        let mut conflicts: Vec<String> = Vec::new();
        for key in keys {
            let base = base_files.get(&key).map(|(_, b)| b.as_slice());
            let ours = main_files.get(&key).map(|v| v.as_slice());
            let theirs = head_files.get(&key).cloned();
            if no_base {
                if let Some(bytes) = theirs {
                    // Skip files that already match the internal main tree:
                    // they carry no net change and would only add noise to the
                    // human commit. Deletions cannot be inferred without a
                    // base, so only additions and updates are imported.
                    if main_files.get(&key).map(|existing| existing.as_slice())
                        == Some(bytes.as_slice())
                    {
                        continue;
                    }
                    let mode = head_modes
                        .get(&key)
                        .map(|(m, _)| m.clone())
                        .unwrap_or_else(|| crate::git_store::MODE_FILE.to_string());
                    changes.insert(key, Some((mode, bytes)));
                }
                continue;
            }
            let base_same_as_ours = match (base, ours) {
                (None, None) => true,
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            let base_same_as_theirs = match (base, theirs.as_deref()) {
                (None, None) => true,
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            if base_same_as_theirs {
                continue;
            }
            if base_same_as_ours {
                match theirs {
                    Some(bytes) => {
                        let mode = head_modes
                            .get(&key)
                            .map(|(m, _)| m.clone())
                            .unwrap_or_else(|| crate::git_store::MODE_FILE.to_string());
                        changes.insert(key, Some((mode, bytes)));
                    }
                    None => {
                        changes.insert(key, None);
                    }
                }
                continue;
            }
            // Both sides changed relative to the base: standard three-way
            // merge decides clean versus conflicted. An empty outcome means
            // deletion won (both sides deleted a base file), not a missing
            // value.
            let outcome = crate::git_store::merge_file_contents(base, ours, theirs.as_deref());
            match outcome.bytes {
                Some(bytes) => {
                    let mode = head_modes
                        .get(&key)
                        .map(|(m, _)| m.clone())
                        .unwrap_or_else(|| crate::git_store::MODE_FILE.to_string());
                    changes.insert(key.clone(), Some((mode, bytes)));
                }
                None => {
                    changes.insert(key.clone(), None);
                }
            }
            if outcome.conflicted {
                conflicts.push(key);
            }
        }
        if changes.is_empty() {
            let point = ReconcilePoint {
                internal_main: main_head,
                external_commit: head.clone(),
                external_repo: fingerprint,
                external_repo_id: repo_id.clone(),
                updated_at_ms: now_millis(),
            };
            self.store_reconcile_point(&point)?;
            return Ok(ImportOutcome {
                created: None,
                advanced_to: head,
                file_count: 0,
                had_conflicts: false,
                no_base,
                fallback_poll: false,
                idempotent: true,
            });
        }
        conflicts.sort();
        let file_count = changes.len();
        let mut trailers = Vec::new();
        if !conflicts.is_empty() {
            trailers.push((
                crate::git_store::TRAILER_STATE.to_string(),
                crate::git_store::STATE_CONFLICT_UNRESOLVED.to_string(),
            ));
            for file in &conflicts {
                trailers.push((
                    crate::git_store::TRAILER_CONFLICT_FILE.to_string(),
                    file.clone(),
                ));
            }
        }
        let message = crate::git_store::commit_message(
            &format!("import external {head}"),
            Some("human"),
            None,
            Some("sync-import"),
            &trailers,
        );
        let storage = self.storage_ref()?;
        let outcome = git
            .commit_on_ref(REF_HUMAN, &changes, "human", &message)
            .map_err(map_git_error)?;
        if outcome.created {
            let mut paths: Vec<String> = changes.keys().cloned().collect();
            paths.sort();
            self.index_commit(storage, &outcome.id, "human", "", "sync-import", &paths)?;
            for (path, change) in &changes {
                let bytes = change.as_ref().map(|(_, b)| b.as_slice());
                self.publish_file_event(&outcome.id, path, "human", bytes);
            }
        }
        let point = ReconcilePoint {
            internal_main: main_head,
            external_commit: head.clone(),
            external_repo: fingerprint,
            external_repo_id: repo_id.clone(),
            updated_at_ms: now_millis(),
        };
        self.store_reconcile_point(&point)?;
        Ok(ImportOutcome {
            created: outcome.created.then_some(outcome.id),
            advanced_to: head,
            file_count,
            had_conflicts: !conflicts.is_empty(),
            no_base,
            fallback_poll: false,
            idempotent: false,
        })
    }

    fn external_tree_matches_main(
        &self,
        objects_dir: &Path,
        external_head: &str,
        main_head: &str,
    ) -> Result<bool, CheckpointError> {
        use crate::git_store::external as ext;
        let git = self.git_ref()?;
        let head_commit = ext::read_external_commit(objects_dir, external_head)
            .map_err(|e| CheckpointError::Internal(format!("cannot read external HEAD: {e}")))?;
        let main_commit = git.read_commit(main_head).map_err(map_git_error)?;
        let external_files = ext::external_tree_to_files(objects_dir, &head_commit.tree)
            .map_err(|e| CheckpointError::Internal(format!("cannot read external tree: {e}")))?;
        let internal_files = git
            .tree_to_files(&main_commit.tree)
            .map_err(map_git_error)?;
        if external_files.len() != internal_files.len() {
            return Ok(false);
        }
        for (path, (mode, blob)) in &external_files {
            match internal_files.get(path) {
                Some((internal_mode, internal_blob))
                    if internal_mode == mode && internal_blob == blob => {}
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    /// Configured sync behavior, when the manager was opened from a full
    /// file-checkpoint config. `None` keeps manual export and explicit
    /// import.
    pub fn sync_config(&self) -> Option<wf_types::config::file_checkpoint::SyncConfig> {
        self.policy.sync.clone()
    }

    /// Whether exports should commit to the user repository by default.
    /// Explicit `ExportOptions.auto_commit` always wins; this only feeds
    /// the default constructor below.
    pub fn auto_commit_enabled(&self) -> bool {
        self.policy.sync.as_ref().is_some_and(|s| s.auto_commit)
    }

    /// Whether background polling may capture external commits without an
    /// explicit import call. Defaults to false.
    pub fn auto_import_enabled(&self) -> bool {
        self.policy.sync.as_ref().is_some_and(|s| s.auto_import)
    }

    /// Default export options honoring the configured sync section:
    /// confirmation and message stay caller-provided, `auto_commit`
    /// falls back to the configured default.
    pub fn default_export_options(
        &self,
        confirm_overwrite: bool,
        message: Option<String>,
    ) -> ExportOptions {
        ExportOptions {
            confirm_overwrite,
            auto_commit: self.auto_commit_enabled(),
            message,
        }
    }
}

#[cfg(test)]
mod tests;
