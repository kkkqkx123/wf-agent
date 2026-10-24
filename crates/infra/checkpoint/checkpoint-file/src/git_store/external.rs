// Read-only access to the user's own repository, shared by baseline seeding
// and sync import/export. Only directory structure and object-database reads
// are used here; user refs are resolved through the git executable at seed
// time and pinned afterwards, never tracked live.
use std::path::{Path, PathBuf};
use std::process::Command;

use super::baseline::BaselineLocator;
use super::{GitCommit, GitStoreError};
use gix_object::Find as _;

fn invalid_ref_name(name: &str) -> Result<(), GitStoreError> {
    if name.is_empty() || name.len() > 255 || name.trim() != name {
        return Err(GitStoreError::InvalidInput(
            "external ref must be 1-255 chars without padding".to_string(),
        ));
    }
    if name.starts_with('-')
        || name.starts_with('/')
        || name.starts_with('.')
        || name.ends_with('/')
        || name.ends_with(".lock")
    {
        return Err(GitStoreError::InvalidInput(format!(
            "external ref is not a plain ref name: '{name}'"
        )));
    }
    if name.contains("..") || name.contains("@{") || name.contains("//") {
        return Err(GitStoreError::InvalidInput(format!(
            "external ref is not a plain ref name: '{name}'"
        )));
    }
    // Revision operators must not leak into the selector: the caller builds
    // the final revision itself, so any operator inside a ref would change
    // the resolution semantics.
    if name.bytes().any(|b| {
        matches!(b, b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\' | b'@')
            || (b as char).is_whitespace()
            || (b as char).is_control()
    }) {
        return Err(GitStoreError::InvalidInput(format!(
            "external ref is not a plain ref name: '{name}'"
        )));
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
    {
        return Err(GitStoreError::InvalidInput(format!(
            "external ref is not a plain ref name: '{name}'"
        )));
    }
    Ok(())
}

fn invalid_prefix(prefix: &str) -> Result<(), GitStoreError> {
    let valid = (4..=40).contains(&prefix.len()) && prefix.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(GitStoreError::InvalidInput(format!(
            "external commit_prefix must be 4-40 hex chars, got '{prefix}'"
        )));
    }
    Ok(())
}

pub(crate) fn check_ref_name(name: &str) -> Result<(), GitStoreError> {
    invalid_ref_name(name)
}

pub(crate) fn check_commit_prefix(prefix: &str) -> Result<(), GitStoreError> {
    invalid_prefix(prefix)
}

/// Fully qualified ref for a tag selector. Plain names resolve through
/// `refs/tags/`; an explicit `refs/tags/` path passes through. Any other
/// ref namespace is rejected so a `tag` selector never silently picks a
/// branch or remote-tracking ref with the same short name.
pub(crate) fn tag_ref_name(tag: &str) -> Result<String, GitStoreError> {
    match tag.strip_prefix("refs/") {
        Some(rest) if rest.starts_with("tags/") && rest.len() > "tags/".len() => {
            Ok(tag.to_string())
        }
        Some(_) => Err(GitStoreError::InvalidInput(format!(
            "baseline tag must be a tag name or a refs/tags/ path: '{tag}'"
        ))),
        None => Ok(format!("refs/tags/{tag}")),
    }
}

/// Whether a process id is still alive. Used to tell a live lock holder
/// from a stale lock file left by a crashed process.
pub(crate) fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// Parse the `pid=<n>` payload written by lock files.
pub(crate) fn parse_lock_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok().and_then(|text| {
        text.split_whitespace()
            .find_map(|token| token.strip_prefix("pid=")?.trim().parse::<u32>().ok())
    })
}

/// Whether a lock file is stale: a live holder inside the grace window keeps
/// the lock, anything else is reclaimable. Unreadable metadata leans stale
/// so a broken lock never blocks progress forever.
pub(crate) fn lock_file_is_stale(path: &Path, grace: std::time::Duration) -> bool {
    let age = std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .map(|modified| {
            modified
                .elapsed()
                .unwrap_or(grace + std::time::Duration::from_secs(1))
        })
        .unwrap_or(grace + std::time::Duration::from_secs(1));
    match parse_lock_pid(path) {
        Some(pid) if pid_alive(pid) => age > grace,
        Some(_) => true,
        None => age > grace,
    }
}

/// Explicit object-format check for the user repository. Baseline borrowing
/// only supports sha1; anything else fails with a clear error instead of a
/// late length mismatch.
pub fn user_objects_use_sha1(workspace_root: &Path) -> Result<bool, GitStoreError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace_root)
        .arg("rev-parse")
        .arg("--show-object-format")
        .output();
    match output {
        Ok(out) if out.status.success() => {
            Ok(String::from_utf8_lossy(&out.stdout).trim() == "sha1")
        }
        _ => {
            let config = Command::new("git")
                .arg("-C")
                .arg(workspace_root)
                .arg("config")
                .arg("--get")
                .arg("extensions.objectFormat")
                .output()
                .map_err(|e| GitStoreError::Baseline(format!("cannot query object format: {e}")))?;
            if !config.status.success() {
                return Ok(true);
            }
            Ok(String::from_utf8_lossy(&config.stdout).trim().is_empty()
                || String::from_utf8_lossy(&config.stdout).trim() == "sha1")
        }
    }
}

/// Whether the user repository is shallow. Used to hint at `commit_prefix`
/// when a `behind` walk crosses the shallow boundary.
pub fn is_shallow_repo(workspace_root: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(workspace_root)
        .arg("rev-parse")
        .arg("--is-shallow-repository")
        .output()
        .map(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true")
        .unwrap_or(false)
}

/// Advisory reachability probe for a `branch plus prefix` selector: warns
/// when the pinned commit is not reachable from the branch tip. Never fails.
pub fn warn_unless_reachable(workspace_root: &Path, branch: &str, commit: &str) {
    let reachable = Command::new("git")
        .arg("-C")
        .arg(workspace_root)
        .arg("merge-base")
        .arg("--is-ancestor")
        .arg(commit)
        .arg(branch)
        .status()
        .map(|status| status.success())
        .unwrap_or(true);
    if !reachable {
        tracing::warn!(
            branch = %branch,
            commit = %commit,
            "baseline commit is not reachable from the branch tip; anchoring anyway"
        );
    }
}

fn is_git_dir(dir: &Path) -> bool {
    dir.join("HEAD").is_file() || dir.join("commondir").is_file()
}

fn common_objects_dir(git_dir: &Path) -> Result<PathBuf, GitStoreError> {
    let commondir_file = git_dir.join("commondir");
    let base = if commondir_file.is_file() {
        let target = std::fs::read_to_string(&commondir_file)?.trim().to_string();
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
    std::fs::canonicalize(&objects).map_err(GitStoreError::from)
}

/// Locate the user repository object database starting from a workspace
/// root: `.git` directory, `.git` worktree pointer file, or a bare layout
/// at the start directory itself. Never reads refs or objects, only the
/// directory structure.
pub fn discover_user_objects_dir(start: &Path) -> Result<PathBuf, GitStoreError> {
    let start = std::fs::canonicalize(start)
        .map_err(|e| GitStoreError::Baseline(format!("cannot resolve workspace root: {e}")))?;
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
            if let Ok(content) = std::fs::read_to_string(&dotgit) {
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

/// Configured `remote.origin.url` of the user repository, or an empty
/// string when no origin is configured. Used as the remote identity of a
/// baseline anchor; it never triggers network access.
pub fn remote_origin_identity(workspace_root: &Path) -> String {
    Command::new("git")
        .arg("-C")
        .arg(workspace_root)
        .arg("config")
        .arg("--get")
        .arg("remote.origin.url")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Fingerprint of one external object database: canonical path, object
/// format, and shallow state. Path changes are expected after a local
/// repository move, so validation treats a fingerprint change as advisory
/// as long as the anchor commit still resolves there.
pub fn object_db_fingerprint(workspace_root: &Path, objects_dir: &Path) -> String {
    let object_format = if user_objects_use_sha1(workspace_root).unwrap_or(true) {
        "sha1"
    } else {
        "other"
    };
    let shallow = if is_shallow_repo(workspace_root) {
        "shallow"
    } else {
        "full"
    };
    format!(
        "path:{}|format:{object_format}|{shallow}",
        fingerprint_for_objects_dir(objects_dir)
    )
}

/// Stable identity for an external repository, independent of its on-disk
/// path when a remote origin is configured. Used to tell a moved repository
/// from a replaced one. Falls back to the object-database path when no
/// remote is configured.
pub fn repository_stable_id(workspace_root: &Path, objects_dir: &Path) -> String {
    let remote = remote_origin_identity(workspace_root);
    let object_format = if user_objects_use_sha1(workspace_root).unwrap_or(true) {
        "sha1"
    } else {
        "other"
    };
    let shallow = if is_shallow_repo(workspace_root) {
        "shallow"
    } else {
        "full"
    };
    if remote.is_empty() {
        let common_dir = objects_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| objects_dir.to_path_buf());
        format!(
            "path:{}|{}|{}",
            common_dir.display(),
            object_format,
            shallow
        )
    } else {
        format!("remote:{}|{}|{}", remote, object_format, shallow)
    }
}

/// Canonical fingerprint identifying one external object database. Stored
/// inside reconcile points so a moved repository is detected instead of
/// silently mixing histories.
pub fn fingerprint_for_objects_dir(objects_dir: &Path) -> String {
    std::fs::canonicalize(objects_dir)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| objects_dir.to_string_lossy().into_owned())
}

/// Current `HEAD` commit of the user repository. Uses the git executable;
/// argv based, never a shell.
pub fn external_head_id(workspace_root: &Path) -> Result<String, GitStoreError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace_root)
        .arg("rev-parse")
        .arg("--verify")
        .arg("HEAD^{commit}")
        .output()
        .map_err(|e| {
            GitStoreError::Sync(format!(
                "git executable not available for external read: {e}"
            ))
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitStoreError::Sync(format!(
            "cannot resolve external HEAD: {detail}"
        )));
    }
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if id.len() != 40 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(GitStoreError::Sync(format!(
            "external HEAD is not sha1 ({id}); cross-algorithm borrowing is not supported"
        )));
    }
    Ok(id)
}

/// Resolve a baseline locator to a full commit id through the git
/// executable. Branch selectors follow git's first-parent shorthand
/// (`branch~n`); a pinned commit is verified as a commit; a tag resolves
/// through its fully qualified tag ref. The returned description is the
/// audit string stored in the anchor.
pub(crate) fn rev_parse_commit(
    workspace_root: &Path,
    locator: &BaselineLocator,
) -> Result<(String, String), GitStoreError> {
    if !user_objects_use_sha1(workspace_root)? {
        return Err(GitStoreError::Baseline(
            "user repository does not use sha1; cross-algorithm borrowing is not supported"
                .to_string(),
        ));
    }
    let spec = match locator {
        BaselineLocator::BranchTip(branch) => branch.clone(),
        BaselineLocator::BranchPinned { commit, .. } => commit.clone(),
        BaselineLocator::BranchAncestor { branch, behind } => format!("{branch}~{behind}"),
        BaselineLocator::Commit(prefix) => prefix.clone(),
        BaselineLocator::Tag { reference, .. } => reference.clone(),
    };
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace_root)
        .arg("rev-parse")
        .arg("--verify")
        .arg("--end-of-options")
        .arg(format!("{spec}^{{commit}}"))
        .output()
        .map_err(|e| {
            GitStoreError::Baseline(format!(
                "git executable not available for baseline resolution: {e}"
            ))
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let hint = match locator {
            BaselineLocator::BranchAncestor { .. } if is_shallow_repo(workspace_root) => {
                "; the shallow repository may not cover that first-parent ancestor, \
                 use commit_prefix or a tag instead"
            }
            BaselineLocator::BranchPinned { .. } => {
                "; pin the commit with commit_prefix alone, use a tag, or re-select \
                 the branch tip or ancestor offset"
            }
            BaselineLocator::Tag { .. } => {
                "; verify the tag exists in the user repository, or pin the commit \
                 with commit_prefix"
            }
            _ => "",
        };
        return Err(GitStoreError::Baseline(format!(
            "cannot resolve baseline '{spec}' in user repository: {detail}{hint}"
        )));
    }
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if id.len() != 40 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(GitStoreError::Baseline(format!(
            "baseline commit id is not sha1 ({id}); cross-algorithm borrowing is not supported"
        )));
    }
    if let BaselineLocator::BranchPinned { branch, .. } = locator {
        warn_unless_reachable(workspace_root, branch, &id);
    }
    Ok((id.clone(), locator.describe(&id)))
}

fn external_odb(objects_dir: &Path) -> Result<gix_odb::Handle, GitStoreError> {
    gix_odb::at(objects_dir, gix_hash::Kind::Sha1)
        .map_err(|e| GitStoreError::Sync(format!("cannot open external object database: {e}")))
}

fn find_external(
    objects_dir: &Path,
    id: &str,
) -> Result<(gix_object::Kind, Vec<u8>), GitStoreError> {
    if id.len() != 40 {
        return Err(GitStoreError::InvalidInput(format!("bad object id '{id}'")));
    }
    let oid = gix_hash::ObjectId::from_hex(id.as_bytes())
        .map_err(|e| GitStoreError::InvalidInput(format!("bad object id '{id}': {e}")))?;
    let handle = external_odb(objects_dir)?;
    let mut buf = Vec::new();
    let data = handle
        .try_find(&oid, &mut buf)
        .map_err(|e| GitStoreError::Corrupt {
            id: id.to_string(),
            reason: e.to_string(),
        })?
        .ok_or_else(|| GitStoreError::ObjectNotFound(id.to_string()))?;
    Ok((data.kind, data.data.to_vec()))
}

/// Read an external commit object without touching the checkpoint store.
pub fn read_external_commit(objects_dir: &Path, id: &str) -> Result<GitCommit, GitStoreError> {
    let (kind, body) = find_external(objects_dir, id)?;
    if kind != gix_object::Kind::Commit {
        return Err(GitStoreError::Corrupt {
            id: id.to_string(),
            reason: format!("expected commit, found {kind}"),
        });
    }
    parse_commit(id, &body)
}

/// Read an external tree object into entries.
pub fn read_external_tree(
    objects_dir: &Path,
    id: &str,
) -> Result<Vec<super::objects::TreeEntry>, GitStoreError> {
    let (kind, body) = find_external(objects_dir, id)?;
    if kind != gix_object::Kind::Tree {
        return Err(GitStoreError::Corrupt {
            id: id.to_string(),
            reason: format!("expected tree, found {kind}"),
        });
    }
    let tree = gix_object::TreeRef::from_bytes(&body, gix_hash::Kind::Sha1).map_err(|e| {
        GitStoreError::Corrupt {
            id: id.to_string(),
            reason: e.to_string(),
        }
    })?;
    Ok(tree
        .entries
        .iter()
        .map(|e| super::objects::TreeEntry {
            mode: format!("{:o}", e.mode),
            name: e.filename.to_string(),
            id: e.oid.to_hex().to_string(),
        })
        .collect())
}

/// Expand an external tree into `path -> (mode, blob id)`.
pub fn external_tree_to_files(
    objects_dir: &Path,
    tree_id: &str,
) -> Result<std::collections::HashMap<String, (String, String)>, GitStoreError> {
    let mut out = std::collections::HashMap::new();
    external_tree_to_files_into(objects_dir, tree_id, String::new(), &mut out)?;
    Ok(out)
}

fn external_tree_to_files_into(
    objects_dir: &Path,
    tree_id: &str,
    prefix: String,
    out: &mut std::collections::HashMap<String, (String, String)>,
) -> Result<(), GitStoreError> {
    for entry in read_external_tree(objects_dir, tree_id)? {
        let path = if prefix.is_empty() {
            entry.name.clone()
        } else {
            format!("{}/{}", prefix, entry.name)
        };
        if entry.mode == "40000" {
            external_tree_to_files_into(objects_dir, &entry.id, path, out)?;
        } else {
            out.insert(path, (entry.mode, entry.id));
        }
    }
    Ok(())
}

/// Load an external tree into `path -> bytes`.
pub fn external_tree_to_bytes(
    objects_dir: &Path,
    tree_id: &str,
) -> Result<std::collections::HashMap<String, Vec<u8>>, GitStoreError> {
    let files = external_tree_to_files(objects_dir, tree_id)?;
    let mut out = std::collections::HashMap::with_capacity(files.len());
    for (path, (_, blob)) in files {
        out.insert(path.clone(), read_external_blob(objects_dir, &blob)?);
    }
    Ok(out)
}

/// Read an external blob's bytes.
pub fn read_external_blob(objects_dir: &Path, id: &str) -> Result<Vec<u8>, GitStoreError> {
    let (kind, body) = find_external(objects_dir, id)?;
    if kind != gix_object::Kind::Blob {
        return Err(GitStoreError::Corrupt {
            id: id.to_string(),
            reason: format!("expected blob, found {kind}"),
        });
    }
    Ok(body)
}

fn parse_commit(id: &str, body: &[u8]) -> Result<GitCommit, GitStoreError> {
    let commit = gix_object::CommitRef::from_bytes(body, gix_hash::Kind::Sha1).map_err(|e| {
        GitStoreError::Corrupt {
            id: id.to_string(),
            reason: e.to_string(),
        }
    })?;
    let author_sig = commit.author().map_err(|e| GitStoreError::Corrupt {
        id: id.to_string(),
        reason: e.to_string(),
    })?;
    let committer_sig = commit.committer().map_err(|e| GitStoreError::Corrupt {
        id: id.to_string(),
        reason: e.to_string(),
    })?;
    Ok(GitCommit {
        id: id.to_string(),
        tree: commit.tree().to_hex().to_string(),
        parents: commit.parents().map(|o| o.to_hex().to_string()).collect(),
        author: author_sig.name.to_string(),
        committer: committer_sig.name.to_string(),
        author_ts: author_sig.seconds().saturating_mul(1000),
        committer_ts: committer_sig.seconds().saturating_mul(1000),
        message: commit
            .message
            .to_string()
            .trim_end_matches('\n')
            .to_string(),
    })
}

/// Whether an id resolves inside an external object directory without
/// opening the checkpoint store.
pub fn probe_commit_in_dir(objects_dir: &Path, id: &str) -> bool {
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

/// Collect the full closure of a baseline commit inside an external object
/// directory: the commit itself, every reachable tree, and every blob.
/// Parents beyond the anchor are out of scope and not collected.
pub(crate) fn collect_baseline_closure(
    objects_dir: &Path,
    commit_id: &str,
) -> Result<Vec<String>, GitStoreError> {
    let commit = read_external_commit(objects_dir, commit_id)?;
    let mut ids = vec![commit_id.to_string()];
    let mut stack = vec![commit.tree];
    let mut seen_trees = std::collections::HashSet::new();
    while let Some(tree_id) = stack.pop() {
        if !seen_trees.insert(tree_id.clone()) {
            continue;
        }
        ids.push(tree_id.clone());
        for entry in read_external_tree(objects_dir, &tree_id)? {
            if entry.mode == "40000" {
                stack.push(entry.id);
            } else {
                ids.push(entry.id);
            }
        }
    }
    Ok(ids)
}
