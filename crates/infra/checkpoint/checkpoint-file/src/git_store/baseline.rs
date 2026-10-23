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
    check_branch_name, check_commit_prefix, discover_user_objects_dir, probe_commit_in_dir,
    rev_parse_commit,
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
}

/// Pinned baseline anchor persisted beside the store.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BaselineAnchor {
    pub id: String,
    pub mode: BaselineMode,
    pub objects_path: String,
    pub source_repo: String,
    pub source_spec: String,
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

/// Alternates declaration path relative to the store root.
const ALTERNATES_REL: &str = "objects/info/alternates";

impl BaselineRequest {
    fn validate(&self) -> Result<(), GitStoreError> {
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
            check_branch_name(branch)?;
        }
        if let Some(prefix) = &self.commit_prefix {
            check_commit_prefix(prefix)?;
        }
        if self.branch.is_none() && self.commit_prefix.is_none() {
            return Err(GitStoreError::InvalidInput(
                "baseline needs a branch or a commit_prefix selector".to_string(),
            ));
        }
        Ok(())
    }
}

impl BaselineAnchor {
    fn serialize(&self) -> String {
        let mode = match self.mode {
            BaselineMode::Borrow => "borrow",
            BaselineMode::Copy => "copy",
        };
        format!(
            "baseline-anchor {ANCHOR_VERSION}\nid {}\nmode {mode}\nobjects {}\nrepo {}\nspec {}\nresolved_at_ms {}\n",
            self.id, self.objects_path, self.source_repo, self.source_spec, self.resolved_at_ms
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
                "resolved_at_ms" => {
                    resolved_at_ms =
                        Some(value.parse::<i64>().map_err(|_| invalid("bad timestamp"))?)
                }
                _ => return Err(invalid("unknown field")),
            }
        }
        Ok(BaselineAnchor {
            id: id.ok_or_else(|| invalid("missing id"))?,
            mode: mode.ok_or_else(|| invalid("missing mode"))?,
            objects_path: objects_path.ok_or_else(|| invalid("missing objects path"))?,
            source_repo: source_repo.ok_or_else(|| invalid("missing repo"))?,
            source_spec: source_spec.ok_or_else(|| invalid("missing spec"))?,
            resolved_at_ms: resolved_at_ms.ok_or_else(|| invalid("missing timestamp"))?,
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
    /// shape stays predictable; half-installed packs are removed.
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
        std::fs::rename(staging.path().join(&pack_name), &dest_pack)?;
        std::fs::rename(staging.path().join(&idx_name), &dest_idx)?;
        if let Err(e) = self.read_commit(commit_id) {
            let _ = std::fs::remove_file(&dest_pack);
            let _ = std::fs::remove_file(&dest_idx);
            return Err(GitStoreError::Baseline(format!(
                "baseline commit {commit_id} not readable after pack copy: {e}"
            )));
        }
        Ok(())
    }

    /// Pinned anchor id for history traversal boundaries. Total: missing or
    /// corrupt anchors behave as no baseline so reads never fail on the hot
    /// path; staleness is reported by [`GitStore::validate_baseline`].
    pub(super) fn baseline_anchor_oid(&self) -> Option<gix_hash::ObjectId> {
        let anchor = self.read_anchor().ok()??;
        if anchor.id.len() != 40 {
            return None;
        }
        gix_hash::ObjectId::from_hex(anchor.id.as_bytes()).ok()
    }

    /// Seed the baseline once for a fresh store. Idempotent across sessions:
    /// an existing anchor converges to [`BaselineSeedOutcome::AlreadySeeded`],
    /// a pre-existing main ref leaves the store untouched.
    pub fn ensure_baseline(
        &self,
        req: &BaselineRequest,
    ) -> Result<BaselineSeedOutcome, GitStoreError> {
        req.validate()?;
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
        let (commit, spec) = rev_parse_commit(
            &workspace_root,
            req.branch.as_deref(),
            req.commit_prefix.as_deref(),
            req.behind,
        )?;
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
                // between seeds). Keep the winner; drop ours so the anchor
                // file never disagrees with the main ref.
                let _ = fs::remove_file(self.anchor_path());
                Ok(BaselineSeedOutcome::Skipped(
                    "another session seeded a different baseline; keeping it".to_string(),
                ))
            }
        }
    }

    /// Check a previously seeded baseline: alternates path still valid, anchor
    /// still resolvable. Repairs a stale alternates entry through repository
    /// re-discovery when the anchor commit is found there. Copy-mode stores
    /// carry their own objects and never need an alternates entry.
    pub fn validate_baseline(&self) -> Result<BaselineStatus, GitStoreError> {
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
        let alternates_ok = self
            .read_alternates()
            .is_some_and(|entry| entry == anchor.objects_path)
            && Path::new(&anchor.objects_path).is_dir();
        if !alternates_ok {
            match discover_user_objects_dir(self.workspace_root()) {
                Ok(found) if probe_commit_in_dir(&found, &anchor.id) => {
                    anchor.objects_path = found.to_string_lossy().into_owned();
                    self.write_alternates(&found)?;
                    self.write_anchor(&anchor)?;
                }
                _ => {
                    return Ok(BaselineStatus::Degraded {
                        reason: "alternates target missing and anchor commit not \
                                 found via repository discovery"
                            .to_string(),
                        anchor,
                    });
                }
            }
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
        self.read_anchor().ok()??.id.into()
    }

    /// Move the baseline to a newly resolved commit. The old main tree is
    /// carried onto the new baseline in one transplant commit so history
    /// stays continuous; the previous chain is left as unreachable orphans
    /// under the normal grace window for rollback.
    pub fn rebase_to(&self, req: &BaselineRequest) -> Result<BaselineRebaseOutcome, GitStoreError> {
        req.validate()?;
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
        let (commit, spec) = rev_parse_commit(
            &workspace_root,
            req.branch.as_deref(),
            req.commit_prefix.as_deref(),
            req.behind,
        )?;
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
        self.compare_and_swap(REF_MAIN, Some(&old_main), &transplant_id)?;
        let new_anchor = BaselineAnchor {
            id: commit,
            mode: req.mode,
            objects_path: objects_dir.to_string_lossy().into_owned(),
            source_repo: source_repo.to_string_lossy().into_owned(),
            source_spec: spec,
            resolved_at_ms: super::commit::now_millis(),
        };
        self.write_anchor(&new_anchor)?;
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
