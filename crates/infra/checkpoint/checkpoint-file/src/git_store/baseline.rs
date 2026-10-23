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
use std::process::Command;

use gix_object::Find as _;

use super::{GitStore, GitStoreError, REF_MAIN};

#[cfg(test)]
mod tests;

/// Baseline object mode: borrow reads through an alternates entry,
/// copy duplicates the baseline closure into the local store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone)]
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

/// Health of a previously seeded baseline.
#[derive(Debug, Clone)]
pub enum BaselineStatus {
    NoBaseline,
    Healthy(BaselineAnchor),
    Degraded { anchor: BaselineAnchor, reason: String },
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
        if self.mode == BaselineMode::Copy {
            return Err(GitStoreError::Baseline(
                "baseline copy mode is not implemented yet; use borrow mode".to_string(),
            ));
        }
        Ok(())
    }
}

/// Branch names are passed to `git rev-parse` as argv (never a shell), so
/// the only injection vector is git parsing the value as a flag. Rejecting
/// a leading dash plus whitespace and control characters closes it; anything
/// else git rejects surfaces as a resolution error.
fn check_branch_name(branch: &str) -> Result<(), GitStoreError> {
    let trimmed = branch.trim();
    if trimmed.is_empty() {
        return Err(GitStoreError::InvalidInput(
            "baseline branch must not be empty".to_string(),
        ));
    }
    if trimmed != branch {
        return Err(GitStoreError::InvalidInput(
            "baseline branch must not have surrounding whitespace".to_string(),
        ));
    }
    if branch.starts_with('-')
        || branch
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(GitStoreError::InvalidInput(format!(
            "baseline branch is not a plain ref name: '{branch}'"
        )));
    }
    Ok(())
}

/// Prefixes must be unambiguous hex; git enforces length and uniqueness.
fn check_commit_prefix(prefix: &str) -> Result<(), GitStoreError> {
    let valid = (4..=40).contains(&prefix.len())
        && prefix.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(GitStoreError::InvalidInput(format!(
            "baseline commit_prefix must be 4-40 hex chars, got '{prefix}'"
        )));
    }
    Ok(())
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
                    resolved_at_ms = Some(
                        value
                            .parse::<i64>()
                            .map_err(|_| invalid("bad timestamp"))?,
                    )
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
        let (commit, spec) = resolve_baseline_commit(&workspace_root, req)?;
        self.write_alternates(&objects_dir)?;
        self.read_commit(&commit).map_err(|e| {
            GitStoreError::Baseline(format!(
                "baseline commit {commit} not readable through alternates: {e}"
            ))
        })?;
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
    /// re-discovery when the anchor commit is found there.
    pub fn validate_baseline(&self) -> Result<BaselineStatus, GitStoreError> {
        let Some(mut anchor) = self.read_anchor()? else {
            return Ok(BaselineStatus::NoBaseline);
        };
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
}

/// Whether `dir` looks like a git directory: carries a HEAD (plain, bare
/// and worktree layouts all do) or a commondir pointer.
fn is_git_dir(dir: &Path) -> bool {
    dir.join("HEAD").is_file() || dir.join("commondir").is_file()
}

/// Resolve the true object-database directory for a git directory, following
/// the commondir pointer that linked worktrees use.
fn common_objects_dir(git_dir: &Path) -> Result<PathBuf, GitStoreError> {
    let commondir_file = git_dir.join("commondir");
    let base = if commondir_file.is_file() {
        let target = fs::read_to_string(&commondir_file)?
            .trim()
            .to_string();
        if target.is_empty() {
            return Err(GitStoreError::Baseline(format!(
                "empty commondir pointer in {}",
                git_dir.display()
            )));
        }
        let target_path = PathBuf::from(target);
        if target_path.is_absolute() {
            target_path
        } else {
            git_dir.join(target_path)
        }
    } else {
        git_dir.to_path_buf()
    };
    let objects = base.join("objects");
    if !objects.is_dir() {
        return Err(GitStoreError::Baseline(format!(
            "no object database under {}",
            base.display()
        )));
    }
    fs::canonicalize(&objects).map_err(GitStoreError::from)
}

/// Locate the user repository's object-database directory starting from a
/// workspace root: `.git` directory, `.git` worktree pointer file, or a bare
/// layout at the start directory itself. Never reads refs or objects, only
/// directory structure.
fn discover_user_objects_dir(start: &Path) -> Result<PathBuf, GitStoreError> {
    let start = fs::canonicalize(start).map_err(|e| {
        GitStoreError::Baseline(format!("cannot resolve workspace root: {e}"))
    })?;
    if is_git_dir(&start) {
        return common_objects_dir(&start);
    }
    let mut dir = start.as_path();
    loop {
        let dotgit = dir.join(".git");
        if dotgit.is_dir() {
            return common_objects_dir(&dotgit);
        }
        if dotgit.is_file() {
            if let Ok(content) = fs::read_to_string(&dotgit) {
                if let Some(target) = content.strip_prefix("gitdir:").map(str::trim) {
                    let target_path = PathBuf::from(target);
                    let git_dir = if target_path.is_absolute() {
                        target_path
                    } else {
                        dir.join(target_path)
                    };
                    if is_git_dir(&git_dir) {
                        return common_objects_dir(&git_dir);
                    }
                }
            }
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }
    Err(GitStoreError::Baseline(
        "workspace is not inside a git repository".to_string(),
    ))
}

/// Resolve the configured selector to a full commit id through the git
/// executable (seed time only). Argv-based, never a shell. A non-sha1
/// result (e.g. a sha256 repository) is rejected: the store cannot borrow
/// across hash algorithms.
fn resolve_baseline_commit(
    workspace_root: &Path,
    req: &BaselineRequest,
) -> Result<(String, String), GitStoreError> {
    let spec = match (&req.branch, &req.commit_prefix, req.behind) {
        (Some(branch), _, Some(n)) => format!("{branch}~{n}"),
        (Some(_), Some(prefix), None) => prefix.clone(),
        (Some(branch), None, None) => branch.clone(),
        (None, Some(prefix), None) => prefix.clone(),
        _ => {
            return Err(GitStoreError::InvalidInput(
                "baseline needs a branch or a commit_prefix selector".to_string(),
            ));
        }
    };
    let description = match (&req.branch, &req.commit_prefix, req.behind) {
        (Some(branch), Some(prefix), None) => format!("branch {branch} pinned to {prefix}"),
        (Some(branch), _, Some(n)) => format!("branch {branch}~{n}"),
        (Some(branch), None, None) => format!("branch {branch} tip"),
        (None, Some(prefix), None) => format!("pinned {prefix}"),
        _ => spec.clone(),
    };
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace_root)
        .arg("rev-parse")
        .arg("--verify")
        .arg(format!("{spec}^{{commit}}"))
        .output()
        .map_err(|e| {
            GitStoreError::Baseline(format!("git executable not available for baseline resolution: {e}"))
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitStoreError::Baseline(format!(
            "cannot resolve baseline '{spec}' in user repository: {detail}"
        )));
    }
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if id.len() != 40 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(GitStoreError::Baseline(format!(
            "baseline commit id is not sha1 ({id}); cross-algorithm borrowing is not supported"
        )));
    }
    Ok((id, description))
}

/// Check an object id resolves inside an external object directory without
/// opening the checkpoint store.
fn probe_commit_in_dir(objects_dir: &Path, id: &str) -> bool {
    let Ok(oid) = gix_hash::ObjectId::from_hex(id.as_bytes()) else {
        return false;
    };
    let Ok(handle) = gix_odb::at(objects_dir, gix_hash::Kind::Sha1) else {
        return false;
    };
    let mut buf = Vec::new();
    handle
        .try_find(&oid, &mut buf)
        .map(|found| found.is_some())
        .unwrap_or(false)
}
