//! Baseline support: borrow a user-repository commit as the initial
//! history of a checkpoint object store.
//!
//! A fresh store inside a git workspace can point its main ref at a commit
//! from the user's own repository instead of copying every file object.
//! The commit and its history stay in the user repository; the store
//! declares that repository read-only through an alternates entry, so
//! lookups fall through transparently while all writes land locally.
//!
//! The resolved commit is pinned in an anchor file next to the store. Later
//! opens reuse the pinned id and never re-resolve the configured branch,
//! so branch movement in the user repository cannot shift history under
//! an existing store.

use std::fs;
use std::path::{Path, PathBuf};

use super::external::{
    check_commit_prefix, check_ref_name, discover_user_objects_dir, object_db_fingerprint,
    probe_commit_in_dir, remote_origin_identity, rev_parse_commit, tag_ref_name,
};
use super::{GitStore, GitStoreError, REF_MAIN};

#[cfg(test)]
mod tests;

/// Baseline object mode: borrow reads through an alternates entry,
/// copy duplicates the baseline closure into the local store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineMode {
    Borrow,
    Copy,
}

/// Validated baseline request for [`GitStore::ensure_baseline`].
#[derive(Debug, Clone)]
pub struct BaselineRequest {
    pub mode: BaselineMode,
    pub branch: Option<String>,
    pub commit_prefix: Option<String>,
    pub behind: Option<u64>,
    pub tag: Option<String>,
}

/// Resolved selector intent. Exactly one variant is built from a
/// [`BaselineRequest`] after mutual-exclusion and character-set checks, so
/// resolution only has to apply the chosen rule.
#[derive(Debug, Clone)]
pub(crate) enum BaselineLocator {
    /// Current tip of a branch.
    BranchTip(String),
    /// Commit pinned by prefix; the branch is advisory for audit and a
    /// reachability warning, never part of the resolution.
    BranchPinned { branch: String, commit: String },
    /// Nth ancestor back from the branch tip along the first-parent chain.
    BranchAncestor { branch: String, behind: u64 },
    /// Commit pinned by prefix without any branch context.
    Commit(String),
    /// Tag resolved through its fully qualified tag ref.
    Tag { name: String, reference: String },
}

impl BaselineLocator {
    /// Audit description stored in the anchor. Uses the resolved commit id
    /// so the anchor records exactly what was pinned, independent of how
    /// the selector was written.
    pub(crate) fn describe(&self, id: &str) -> String {
        match self {
            BaselineLocator::BranchTip(branch) => format!("branch {branch} tip"),
            BaselineLocator::BranchPinned { branch, .. } => {
                format!("branch {branch} pinned to commit {id}")
            }
            BaselineLocator::BranchAncestor { branch, behind } => {
                format!("branch {branch} first-parent ancestor {behind}")
            }
            BaselineLocator::Commit(_) => format!("pinned commit {id}"),
            BaselineLocator::Tag { name, .. } => format!("tag {name} -> {id}"),
        }
    }
}

/// Pinned baseline anchor persisted beside the store.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BaselineAnchor {
    pub id: String,
    pub mode: BaselineMode,
    pub objects_path: String,
    pub source_repo: String,
    pub source_spec: String,
    /// Configured `remote.origin.url` at seed time, empty when none.
    pub remote_identity: String,
    /// Canonical objects path, object format, and shallow state.
    pub objects_fingerprint: String,
    pub resolved_at_ms: i64,
}

/// Outcome of [`GitStore::ensure_baseline`].
#[derive(Debug, Clone)]
pub enum BaselineSeedOutcome {
    Seeded(BaselineAnchor),
    AlreadySeeded(BaselineAnchor),
    Skipped(String),
}

/// Outcome of an explicit baseline move. The transplant commit carries
/// the old main tree on top of the new baseline so history stays
/// continuous; the previous chain is left as unreachable orphans under the
/// normal grace window for rollback.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BaselineRebaseOutcome {
    pub old_anchor: BaselineAnchor,
    pub new_anchor: BaselineAnchor,
    pub old_main: String,
    pub transplant_id: String,
}

/// Health of a previously seeded baseline.
#[derive(Debug, Clone, serde::Serialize)]
pub enum BaselineStatus {
    NoBaseline,
    Healthy(BaselineAnchor),
    Degraded {
        anchor: BaselineAnchor,
        reason: String,
    },
}

/// Anchor file version. Bump when the on-disk layout changes.
const ANCHOR_VERSION: &str = "v1";

/// File holding the pinned anchor, next to the store's `HEAD` and `config`.
const ANCHOR_FILE_NAME: &str = "baseline-anchor";

/// Pending anchor staged before a rebase moves the main ref. Renamed to the
/// anchor file only after the ref update lands, so a crash always resolves
/// to either the old or the new anchor.
const ANCHOR_PENDING_NAME: &str = "baseline-anchor.pending";

/// Seed lock serializing concurrent baseline seeds and rebases on one store.
const SEED_LOCK_NAME: &str = "baseline-seed.lock";

/// How long a live seed holder may keep the lock before it is considered
/// stuck. Seeds run a few git commands; five minutes is generous.
const SEED_LOCK_GRACE: std::time::Duration = std::time::Duration::from_secs(300);

struct SeedLock {
    path: PathBuf,
}

impl SeedLock {
    fn acquire(git_dir: &Path) -> Result<Self, GitStoreError> {
        let path = git_dir.join(SEED_LOCK_NAME);
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
                if super::external::lock_file_is_stale(&path, SEED_LOCK_GRACE) {
                    let _ = std::fs::remove_file(&path);
                    return Self::acquire(git_dir);
                }
                Err(GitStoreError::Baseline(
                    "baseline seed busy: another session is seeding this workspace".to_string(),
                ))
            }
            Err(e) => Err(GitStoreError::from(e)),
        }
    }
}

impl Drop for SeedLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Alternates declaration path relative to the store root.
const ALTERNATES_REL: &str = "objects/info/alternates";

impl BaselineRequest {
    /// Validate selector exclusivity and character sets, then build the
    /// single locator that resolution applies.
    fn locator(&self) -> Result<BaselineLocator, GitStoreError> {
        if let Some(tag) = &self.tag {
            if self.branch.is_some() || self.commit_prefix.is_some() || self.behind.is_some() {
                return Err(GitStoreError::InvalidInput(
                    "baseline tag is mutually exclusive with branch, commit_prefix, and behind"
                        .to_string(),
                ));
            }
            check_ref_name(tag)?;
            return Ok(BaselineLocator::Tag {
                name: tag.clone(),
                reference: tag_ref_name(tag)?,
            });
        }
        if self.commit_prefix.is_some() && self.behind.is_some() {
            return Err(GitStoreError::InvalidInput(
                "baseline commit_prefix and behind are mutually exclusive".to_string(),
            ));
        }
        if self.behind.is_some() && self.branch.is_none() {
            return Err(GitStoreError::InvalidInput(
                "baseline behind requires a branch".to_string(),
            ));
        }
        if let Some(branch) = &self.branch {
            check_ref_name(branch)?;
        }
        if let Some(prefix) = &self.commit_prefix {
            check_commit_prefix(prefix)?;
        }
        match (&self.branch, &self.commit_prefix, self.behind) {
            (Some(branch), Some(commit), None) => Ok(BaselineLocator::BranchPinned {
                branch: branch.clone(),
                commit: commit.clone(),
            }),
            (Some(branch), None, Some(behind)) => Ok(BaselineLocator::BranchAncestor {
                branch: branch.clone(),
                behind,
            }),
            (Some(branch), None, None) => Ok(BaselineLocator::BranchTip(branch.clone())),
            (None, Some(commit), None) => Ok(BaselineLocator::Commit(commit.clone())),
            _ => Err(GitStoreError::InvalidInput(
                "baseline needs a branch, commit_prefix, or tag selector".to_string(),
            )),
        }
    }
}

impl BaselineAnchor {
    fn serialize(&self) -> String {
        let mode = match self.mode {
            BaselineMode::Borrow => "borrow",
            BaselineMode::Copy => "copy",
        };
        format!(
            "baseline-anchor {ANCHOR_VERSION}\nid {}\nmode {mode}\nobjects {}\nrepo {}\nspec {}\nremote_identity {}\nobjects_fingerprint {}\nresolved_at_ms {}\n",
            self.id,
            self.objects_path,
            self.source_repo,
            self.source_spec,
            self.remote_identity,
            self.objects_fingerprint,
            self.resolved_at_ms
        )
    }

    fn parse(text: &str) -> Result<Self, GitStoreError> {
        let invalid = |why: &str| GitStoreError::Baseline(format!("corrupt anchor file: {why}"));
        let mut lines = text.lines();
        match lines.next() {
            Some(first) if first == format!("baseline-anchor {ANCHOR_VERSION}") => {}
            _ => return Err(invalid("bad version header")),
        }
        let mut id = None;
        let mut mode = None;
        let mut objects_path = None;
        let mut source_repo = None;
        let mut source_spec = None;
        let mut remote_identity = None;
        let mut objects_fingerprint = None;
        let mut resolved_at_ms = None;
        for line in lines {
            let (key, value) = line
                .split_once(' ')
                .ok_or_else(|| invalid("malformed line"))?;
            match key {
                "id" => id = Some(value.to_string()),
                "mode" => {
                    mode = Some(match value {
                        "borrow" => BaselineMode::Borrow,
                        "copy" => BaselineMode::Copy,
                        _ => return Err(invalid("bad mode")),
                    })
                }
                "objects" => objects_path = Some(value.to_string()),
                "repo" => source_repo = Some(value.to_string()),
                "spec" => source_spec = Some(value.to_string()),
                "remote_identity" => remote_identity = Some(value.to_string()),
                "objects_fingerprint" => objects_fingerprint = Some(value.to_string()),
                "resolved_at_ms" => {
                    resolved_at_ms =
                        Some(value.parse::<i64>().map_err(|_| invalid("bad timestamp"))?)
                }
                _ => return Err(invalid("unknown field")),
            }
        }
        let id = id.ok_or_else(|| invalid("missing id"))?;
        if id.len() != 40 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid("id must be 40 hex chars"));
        }
        let non_empty = |value: Option<String>, field: &str| {
            value
                .filter(|text| !text.trim().is_empty())
                .ok_or_else(|| invalid(&format!("missing or empty {field}")))
        };
        let resolved_at_ms = resolved_at_ms.ok_or_else(|| invalid("missing timestamp"))?;
        if resolved_at_ms < 0 {
            return Err(invalid("negative timestamp"));
        }
        Ok(BaselineAnchor {
            id,
            mode: mode.ok_or_else(|| invalid("missing mode"))?,
            objects_path: non_empty(objects_path, "objects path")?,
            source_repo: non_empty(source_repo, "repo")?,
            source_spec: non_empty(source_spec, "spec")?,
            remote_identity: remote_identity.ok_or_else(|| invalid("missing remote identity"))?,
            objects_fingerprint: non_empty(objects_fingerprint, "objects fingerprint")?,
            resolved_at_ms,
        })
    }
}

impl GitStore {
    fn anchor_path(&self) -> PathBuf {
        self.git_dir().join(ANCHOR_FILE_NAME)
    }

    fn read_anchor(&self) -> Result<Option<BaselineAnchor>, GitStoreError> {
        match fs::read_to_string(self.anchor_path()) {
            Ok(text) => Ok(Some(BaselineAnchor::parse(&text)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(GitStoreError::from(e)),
        }
    }

    fn write_anchor(&self, anchor: &BaselineAnchor) -> Result<(), GitStoreError> {
        let path = self.anchor_path();
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, anchor.serialize())?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn pending_anchor_path(&self) -> PathBuf {
        self.git_dir().join(ANCHOR_PENDING_NAME)
    }

    fn read_pending_anchor(&self) -> Result<Option<BaselineAnchor>, GitStoreError> {
        match fs::read_to_string(self.pending_anchor_path()) {
            Ok(text) => Ok(Some(BaselineAnchor::parse(&text)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(GitStoreError::from(e)),
        }
    }

    fn write_pending_anchor(&self, anchor: &BaselineAnchor) -> Result<(), GitStoreError> {
        let path = self.pending_anchor_path();
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, anchor.serialize())?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Resolve a pending rebase left by a crash: when the main head is a
    /// transplant on top of the pending anchor, complete the rename; when
    /// the main head never moved, drop the pending file and restore the
    /// alternates entry for the settled anchor.
    fn complete_pending_rebase(&self) -> Result<(), GitStoreError> {
        let Some(pending) = self.read_pending_anchor()? else {
            return Ok(());
        };
        let main_head = self.read_ref(REF_MAIN)?;
        let completed = match &main_head {
            Some(head) => self
                .read_commit(head)
                .map(|commit| commit.parents.first().is_some_and(|p| p == &pending.id))
                .unwrap_or(false),
            None => false,
        };
        if completed {
            let tmp = self.anchor_path().with_extension("tmp");
            fs::write(&tmp, pending.serialize())?;
            fs::rename(&tmp, self.anchor_path())?;
            let _ = fs::remove_file(self.pending_anchor_path());
            return Ok(());
        }
        let still_old = matches!((&main_head, self.read_anchor()?), (Some(_), Some(_)));
        if still_old {
            if let Ok(Some(settled)) = self.read_anchor() {
                if settled.mode == BaselineMode::Borrow {
                    let _ = self.write_alternates(Path::new(&settled.objects_path));
                }
            }
        }
        let _ = fs::remove_file(self.pending_anchor_path());
        Ok(())
    }

    fn read_alternates(&self) -> Option<String> {
        fs::read_to_string(self.git_dir().join(ALTERNATES_REL))
            .ok()
            .map(|s| s.trim().to_string())
    }

    fn write_alternates(&self, objects_dir: &Path) -> Result<(), GitStoreError> {
        let info = self.git_dir().join("objects/info");
        fs::create_dir_all(&info)?;
        let path = info.join("alternates");
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, format!("{}\n", objects_dir.display()))?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Copy a baseline commit closure into the local object directory as
    /// one packfile so the store stays self-contained without loose-object
    /// fan-out. The closure stays exact (commit plus reachable trees and
    /// blobs, no parent history). Pack failures fail directly so the store
    /// shape stays predictable; half-installed packs are removed. Post-copy
    /// validation reads the anchor commit and its main tree so a corrupt
    /// pack cannot seed a store. Seeds are serialized by the seed lock;
    /// the ref update still uses compare-and-swap as a second guard.
    fn copy_closure_from_external(
        &self,
        objects_dir: &Path,
        commit_id: &str,
    ) -> Result<(), GitStoreError> {
        use super::external::collect_baseline_closure;
        use std::io::Write as _;
        let ids = collect_baseline_closure(objects_dir, commit_id)?;
        let local_objects = self.git_dir().join("objects");
        std::fs::create_dir_all(&local_objects)?;
        let workspace_root = self.workspace_root().to_path_buf();
        let staging = tempfile::tempdir()?;
        let pack_base = staging.path().join("pack");
        let mut child = std::process::Command::new("git")
            .arg("-C")
            .arg(&workspace_root)
            .arg("pack-objects")
            .arg("-q")
            .arg(&pack_base)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| GitStoreError::Baseline(format!("git pack-objects unavailable: {e}")))?;
        {
            let stdin = child.stdin.as_mut().ok_or_else(|| {
                GitStoreError::Baseline("pack-objects stdin unavailable".to_string())
            })?;
            for id in &ids {
                writeln!(stdin, "{id}").map_err(|e| {
                    GitStoreError::Baseline(format!("pack-objects stdin failed: {e}"))
                })?;
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|e| GitStoreError::Baseline(format!("pack-objects wait failed: {e}")))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(GitStoreError::Baseline(format!(
                "git pack-objects failed: {detail}"
            )));
        }
        let pack_name = std::fs::read_dir(staging.path())?
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| name.ends_with(".pack"))
            .ok_or_else(|| {
                GitStoreError::Baseline("pack-objects produced no packfile".to_string())
            })?;
        let idx_name = pack_name.replace(".pack", ".idx");
        if !staging.path().join(&idx_name).is_file() {
            return Err(GitStoreError::Baseline(
                "pack-objects produced no index".to_string(),
            ));
        }
        let pack_dir = self.git_dir().join("objects/pack");
        std::fs::create_dir_all(&pack_dir)?;
        let dest_pack = pack_dir.join(&pack_name);
        let dest_idx = pack_dir.join(&idx_name);
        std::fs::rename(staging.path().join(&pack_name), &dest_pack)
            .map_err(|e| GitStoreError::Baseline(format!("pack install failed: {e}")))?;
        if let Err(e) = std::fs::rename(staging.path().join(&idx_name), &dest_idx) {
            let _ = std::fs::remove_file(&dest_pack);
            return Err(GitStoreError::Baseline(format!(
                "pack index install failed: {e}"
            )));
        }
        let commit = match self.read_commit(commit_id) {
            Ok(commit) => commit,
            Err(e) => {
                let _ = std::fs::remove_file(&dest_pack);
                let _ = std::fs::remove_file(&dest_idx);
                return Err(GitStoreError::Baseline(format!(
                    "baseline commit {commit_id} not readable after pack copy: {e}"
                )));
            }
        };
        if let Err(e) = self.tree_to_files(&commit.tree) {
            let _ = std::fs::remove_file(&dest_pack);
            let _ = std::fs::remove_file(&dest_idx);
            return Err(GitStoreError::Baseline(format!(
                "baseline tree {} not readable after pack copy: {e}",
                commit.tree
            )));
        }
        Ok(())
    }

    /// Anchor for read-only hot paths. A missing or corrupt anchor behaves
    /// as no baseline so reads never fail there; corruption is logged so a
    /// silently disabled traversal boundary is visible in diagnostics.
    fn anchor_lossy(&self) -> Option<BaselineAnchor> {
        match self.read_anchor() {
            Ok(anchor) => anchor,
            Err(e) => {
                tracing::warn!(error = %e, "ignoring unreadable baseline anchor");
                None
            }
        }
    }

    /// Pinned anchor id for history traversal boundaries. Total: missing or
    /// corrupt anchors behave as no baseline so reads never fail on the hot
    /// path; staleness is reported by [`GitStore::validate_baseline`].
    pub(super) fn baseline_anchor_oid(&self) -> Option<gix_hash::ObjectId> {
        let anchor = self.anchor_lossy()?;
        gix_hash::ObjectId::from_hex(anchor.id.as_bytes()).ok()
    }

    /// Seed the baseline once for a fresh store. Idempotent across sessions:
    /// an existing anchor converges to [`BaselineSeedOutcome::AlreadySeeded`],
    /// a pre-existing main ref leaves the store untouched.
    pub fn ensure_baseline(
        &self,
        req: &BaselineRequest,
    ) -> Result<BaselineSeedOutcome, GitStoreError> {
        let locator = req.locator()?;
        let _seed_lock = SeedLock::acquire(self.git_dir())?;
        self.complete_pending_rebase()?;
        if let Some(existing) = self.read_anchor()? {
            return self.finish_seed_from_anchor(existing);
        }
        if self.read_ref(REF_MAIN)?.is_some() {
            return Ok(BaselineSeedOutcome::Skipped(
                "main ref already initialized; baseline seeding only applies to fresh stores"
                    .to_string(),
            ));
        }
        let workspace_root = self.workspace_root().to_path_buf();
        let objects_dir = discover_user_objects_dir(&workspace_root)?;
        let source_repo = objects_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| objects_dir.clone());
        let (commit, spec) = rev_parse_commit(&workspace_root, &locator)?;
        if req.mode == BaselineMode::Copy {
            self.copy_closure_from_external(&objects_dir, &commit)?;
            self.read_commit(&commit).map_err(|e| {
                GitStoreError::Baseline(format!(
                    "baseline commit {commit} not readable after copy: {e}"
                ))
            })?;
            let _ = fs::remove_file(self.git_dir().join(ALTERNATES_REL));
        } else {
            self.write_alternates(&objects_dir)?;
            self.read_commit(&commit).map_err(|e| {
                GitStoreError::Baseline(format!(
                    "baseline commit {commit} not readable through alternates: {e}"
                ))
            })?;
        }
        let anchor = BaselineAnchor {
            id: commit,
            mode: req.mode,
            objects_path: objects_dir.to_string_lossy().into_owned(),
            source_repo: source_repo.to_string_lossy().into_owned(),
            source_spec: spec,
            remote_identity: remote_origin_identity(&workspace_root),
            objects_fingerprint: object_db_fingerprint(&workspace_root, &objects_dir),
            resolved_at_ms: super::commit::now_millis(),
        };
        // Anchor before ref: a crash here retries into the anchor-exists
        // path, which finishes the ref update. The reverse order could leave
        // a main ref with no recorded anchor.
        self.write_anchor(&anchor)?;
        match self.compare_and_swap(REF_MAIN, None, &anchor.id) {
            Ok(()) => Ok(BaselineSeedOutcome::Seeded(anchor)),
            Err(GitStoreError::RefConflict(_)) => self.finish_seed_from_anchor(anchor),
            Err(other) => Err(other),
        }
    }

    /// Complete a seed when the anchor exists but the main ref may not
    /// (crashed first seed, or a lost seeding race).
    fn finish_seed_from_anchor(
        &self,
        anchor: BaselineAnchor,
    ) -> Result<BaselineSeedOutcome, GitStoreError> {
        match self.read_ref(REF_MAIN)? {
            None => {
                self.compare_and_swap(REF_MAIN, None, &anchor.id)
                    .map_err(|e| match e {
                        GitStoreError::RefConflict(_) => {
                            GitStoreError::Baseline("lost baseline seeding race".to_string())
                        }
                        other => other,
                    })?;
                Ok(BaselineSeedOutcome::AlreadySeeded(anchor))
            }
            Some(head) if head == anchor.id => Ok(BaselineSeedOutcome::AlreadySeeded(anchor)),
            Some(_) => {
                // Another session seeded a different anchor (the branch moved
                // between seeds). Keep the winner; only remove the anchor
                // file when it still holds our own id, so a concurrent
                // winner is never deleted.
                let owned = self
                    .read_anchor()
                    .map(|current| current.is_some_and(|c| c.id == anchor.id))
                    .unwrap_or(false);
                if owned {
                    let _ = fs::remove_file(self.anchor_path());
                }
                Ok(BaselineSeedOutcome::Skipped(
                    "another session seeded a different baseline; keeping it".to_string(),
                ))
            }
        }
    }

    /// Check a previously seeded baseline: identity hints changed, alternates
    /// path still valid, anchor still resolvable. The hard gate is anchor
    /// commit resolvability; remote and fingerprint changes are advisory and
    /// refreshed when the commit is still reachable. Copy-mode stores carry
    /// their own objects and never need an alternates entry.
    pub fn validate_baseline(&self) -> Result<BaselineStatus, GitStoreError> {
        self.complete_pending_rebase()?;
        let Some(mut anchor) = self.read_anchor()? else {
            return Ok(BaselineStatus::NoBaseline);
        };
        if anchor.mode == BaselineMode::Copy {
            let _ = fs::remove_file(self.git_dir().join(ALTERNATES_REL));
            return match self.read_commit(&anchor.id) {
                Ok(_) => Ok(BaselineStatus::Healthy(anchor)),
                Err(e) => Ok(BaselineStatus::Degraded {
                    reason: format!("copied anchor commit {} not resolvable: {e}", anchor.id),
                    anchor,
                }),
            };
        }
        let workspace_root = self.workspace_root();
        if !super::external::user_objects_use_sha1(workspace_root)? {
            return Ok(BaselineStatus::Degraded {
                reason:
                    "user repository object format is no longer sha1; borrowed history is unusable"
                        .to_string(),
                anchor,
            });
        }
        let remote = remote_origin_identity(workspace_root);
        if !anchor.remote_identity.is_empty()
            && !remote.is_empty()
            && remote != anchor.remote_identity
        {
            return Ok(BaselineStatus::Degraded {
                reason: format!(
                    "user repository remote changed from '{}' to '{remote}'; it may be a different repository, re-seed the baseline explicitly",
                    anchor.remote_identity
                ),
                anchor,
            });
        }
        let old_objects = PathBuf::from(&anchor.objects_path);
        let objects = match discover_user_objects_dir(workspace_root) {
            Ok(found) if probe_commit_in_dir(&found, &anchor.id) => found,
            _ if probe_commit_in_dir(&old_objects, &anchor.id) => old_objects,
            _ => {
                return Ok(BaselineStatus::Degraded {
                    reason: format!(
                        "anchor commit {} is not resolvable in the current repository or the recorded objects path",
                        anchor.id
                    ),
                    anchor,
                });
            }
        };
        let fingerprint = object_db_fingerprint(workspace_root, &objects);
        let objects_text = objects.to_string_lossy().into_owned();
        let mut changed = false;
        if anchor.objects_path != objects_text {
            anchor.objects_path = objects_text;
            changed = true;
        }
        if anchor.objects_fingerprint != fingerprint {
            tracing::warn!(
                anchor = %anchor.id,
                previous = %anchor.objects_fingerprint,
                current = %fingerprint,
                "baseline object database fingerprint changed; anchor commit is still resolvable"
            );
            anchor.objects_fingerprint = fingerprint;
            changed = true;
        }
        if anchor.remote_identity != remote {
            tracing::warn!(
                anchor = %anchor.id,
                previous = %anchor.remote_identity,
                current = %remote,
                "baseline remote identity changed; anchor commit is still resolvable"
            );
            anchor.remote_identity = remote;
            changed = true;
        }
        let alternates_ok = self
            .read_alternates()
            .is_some_and(|entry| entry == anchor.objects_path)
            && Path::new(&anchor.objects_path).is_dir();
        if !alternates_ok {
            self.write_alternates(&objects)?;
            changed = true;
        }
        if changed {
            self.write_anchor(&anchor)?;
        }
        match self.read_commit(&anchor.id) {
            Ok(_) => Ok(BaselineStatus::Healthy(anchor)),
            Err(e) => Ok(BaselineStatus::Degraded {
                reason: format!("anchor commit {} not resolvable: {e}", anchor.id),
                anchor,
            }),
        }
    }

    /// Read the pinned anchor without validation, if any.
    pub fn baseline_anchor(&self) -> Result<Option<BaselineAnchor>, GitStoreError> {
        self.read_anchor()
    }

    /// Pinned anchor id for GC roots and sync start points.
    pub fn baseline_anchor_id(&self) -> Option<String> {
        self.anchor_lossy().map(|anchor| anchor.id)
    }

    /// Move the baseline to a newly resolved commit. The old main tree is
    /// carried onto the new baseline in one transplant commit so history
    /// stays continuous; the previous chain is left as unreachable orphans
    /// under the normal grace window for rollback.
    pub fn rebase_to(&self, req: &BaselineRequest) -> Result<BaselineRebaseOutcome, GitStoreError> {
        let locator = req.locator()?;
        let _seed_lock = SeedLock::acquire(self.git_dir())?;
        self.complete_pending_rebase()?;
        let Some(old_anchor) = self.read_anchor()? else {
            return Err(GitStoreError::Baseline(
                "no baseline anchor to rebase; seed a baseline first".to_string(),
            ));
        };
        let old_main = self.read_ref(REF_MAIN)?.ok_or_else(|| {
            GitStoreError::Baseline("main ref missing; cannot rebase without a head".to_string())
        })?;
        let workspace_root = self.workspace_root().to_path_buf();
        let objects_dir = discover_user_objects_dir(&workspace_root)?;
        let (commit, spec) = rev_parse_commit(&workspace_root, &locator)?;
        if commit == old_anchor.id {
            let transplant = old_main.clone();
            return Ok(BaselineRebaseOutcome {
                old_anchor: old_anchor.clone(),
                new_anchor: old_anchor,
                old_main,
                transplant_id: transplant,
            });
        }
        let source_repo = objects_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| objects_dir.clone());
        if req.mode == BaselineMode::Copy {
            self.copy_closure_from_external(&objects_dir, &commit)?;
            self.read_commit(&commit).map_err(|e| {
                GitStoreError::Baseline(format!(
                    "new baseline commit {commit} not readable after copy: {e}"
                ))
            })?;
            let _ = fs::remove_file(self.git_dir().join(ALTERNATES_REL));
        } else {
            self.write_alternates(&objects_dir)?;
            self.read_commit(&commit).map_err(|e| {
                GitStoreError::Baseline(format!(
                    "new baseline commit {commit} not readable through alternates: {e}"
                ))
            })?;
        }
        let new_tree = self.read_commit(&commit)?.tree;
        let old_tree = self.read_commit(&old_main)?.tree;
        let transplant_tree = self.transplant_tree(&new_tree, &old_tree)?;
        let message = super::commit_message(
            &format!("rebase baseline to {spec}"),
            None,
            None,
            Some("baseline"),
            &[],
        );
        let transplant_id = self.write_commit(
            &transplant_tree,
            std::slice::from_ref(&commit),
            super::SYSTEM_COMMITTER,
            super::SYSTEM_COMMITTER,
            super::commit::now_millis(),
            &message,
        )?;
        let new_anchor = BaselineAnchor {
            id: commit,
            mode: req.mode,
            objects_path: objects_dir.to_string_lossy().into_owned(),
            source_repo: source_repo.to_string_lossy().into_owned(),
            source_spec: spec,
            remote_identity: remote_origin_identity(&workspace_root),
            objects_fingerprint: object_db_fingerprint(&workspace_root, &objects_dir),
            resolved_at_ms: super::commit::now_millis(),
        };
        self.write_pending_anchor(&new_anchor)?;
        match self.compare_and_swap(REF_MAIN, Some(&old_main), &transplant_id) {
            Ok(()) => {
                let tmp = self.anchor_path().with_extension("tmp");
                fs::write(&tmp, new_anchor.serialize())?;
                fs::rename(&tmp, self.anchor_path())?;
                let _ = fs::remove_file(self.pending_anchor_path());
            }
            Err(e) => {
                let _ = fs::remove_file(self.pending_anchor_path());
                return Err(e);
            }
        }
        Ok(BaselineRebaseOutcome {
            old_anchor,
            new_anchor,
            old_main,
            transplant_id,
        })
    }

    /// Build the transplant tree: content of the old main tree expressed as
    /// a delta on top of the new baseline tree. Identical subtrees are
    /// reused by id; changed files are rewritten locally so the transplant
    /// never depends on objects that only the old baseline provided.
    fn transplant_tree(&self, new_tree: &str, old_tree: &str) -> Result<String, GitStoreError> {
        if new_tree == old_tree {
            return Ok(old_tree.to_string());
        }
        let new_files = self.tree_to_files(new_tree)?;
        let old_files = self.tree_to_files(old_tree)?;
        let mut new_blobs: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for (path, (_, blob)) in &new_files {
            new_blobs.insert(path.clone(), blob.clone());
        }
        let mut changes: std::collections::HashMap<String, Option<(String, Vec<u8>)>> =
            std::collections::HashMap::new();
        let mut paths: std::collections::HashSet<String> = std::collections::HashSet::new();
        paths.extend(new_files.keys().cloned());
        paths.extend(old_files.keys().cloned());
        for path in paths {
            let old_entry = old_files.get(&path);
            let new_blob = new_blobs.get(&path);
            match (old_entry, new_blob) {
                (None, _) => {
                    changes.insert(path, None);
                }
                (Some((mode, blob)), Some(new_blob)) if blob == new_blob => {}
                (Some((mode, _)), _) => {
                    let bytes = self.read_blob(&old_files[&path].1)?;
                    changes.insert(path, Some((mode.clone(), bytes)));
                }
            }
        }
        self.build_tree_from_parent(Some(new_tree), &changes)
    }
}
