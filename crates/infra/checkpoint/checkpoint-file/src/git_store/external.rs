// Read-only access to the user's own repository, shared by baseline seeding
// and sync import/export. Only directory structure and object-database reads
// are used here; user refs are resolved through the git executable at seed
// time and pinned afterwards, never tracked live.
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{GitCommit, GitStoreError};
use gix_object::Find as _;

fn invalid_branch(branch: &str) -> Result<(), GitStoreError> {
    let trimmed = branch.trim();
    if trimmed.is_empty() || trimmed != branch {
        return Err(GitStoreError::InvalidInput(
            "external branch must not be empty or padded".to_string(),
        ));
    }
    if branch.starts_with('-') || branch.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(GitStoreError::InvalidInput(format!(
            "external branch is not a plain ref name: '{branch}'"
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

pub(crate) fn check_branch_name(branch: &str) -> Result<(), GitStoreError> {
    invalid_branch(branch)
}

pub(crate) fn check_commit_prefix(prefix: &str) -> Result<(), GitStoreError> {
    invalid_prefix(prefix)
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

/// Resolve a baseline-style selector to a full commit id through the git
/// executable. `branch` plus optional `behind` follows the first-parent
/// chain (`branch~n`); a lone `commit_prefix` is verified as a commit.
pub fn rev_parse_commit(
    workspace_root: &Path,
    branch: Option<&str>,
    commit_prefix: Option<&str>,
    behind: Option<u64>,
) -> Result<(String, String), GitStoreError> {
    if let Some(branch) = branch {
        check_branch_name(branch)?;
    }
    if let Some(prefix) = commit_prefix {
        check_commit_prefix(prefix)?;
    }
    let spec = match (branch, commit_prefix, behind) {
        (Some(branch), _, Some(n)) => format!("{branch}~{n}"),
        (Some(_), Some(prefix), None) => prefix.to_string(),
        (Some(branch), None, None) => branch.to_string(),
        (None, Some(prefix), None) => prefix.to_string(),
        _ => {
            return Err(GitStoreError::InvalidInput(
                "external selector needs a branch or a commit_prefix".to_string(),
            ));
        }
    };
    let description = match (branch, commit_prefix, behind) {
        (Some(branch), Some(_), None) => format!("branch {branch} pinned to {spec}"),
        (Some(branch), _, Some(n)) => format!("branch {branch}~{n}"),
        (Some(branch), None, None) => format!("branch {branch} tip"),
        (None, Some(_), None) => format!("pinned {spec}"),
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
            GitStoreError::Baseline(format!(
                "git executable not available for baseline resolution: {e}"
            ))
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

/// Copy one object by id from an external object directory into a local
/// objects directory, preserving the id exactly. Reads handle packed and
/// loose sources transparently; writes are loose only. Existing local
/// objects are left untouched, so the copy is idempotent.
pub(crate) fn copy_object_raw(
    objects_dir: &Path,
    local_objects_dir: &Path,
    id: &str,
) -> Result<(), GitStoreError> {
    if id.len() != 40 {
        return Err(GitStoreError::InvalidInput(format!("bad object id '{id}'")));
    }
    let oid = gix_hash::ObjectId::from_hex(id.as_bytes())
        .map_err(|e| GitStoreError::InvalidInput(format!("bad object id '{id}': {e}")))?;
    let handle = external_odb(objects_dir)?;
    let mut buf = Vec::new();
    let found = handle
        .try_find(&oid, &mut buf)
        .map_err(|e| GitStoreError::Corrupt {
            id: id.to_string(),
            reason: e.to_string(),
        })?
        .ok_or_else(|| GitStoreError::ObjectNotFound(id.to_string()))?;
    let kind_str = match found.kind {
        gix_object::Kind::Blob => "blob",
        gix_object::Kind::Tree => "tree",
        gix_object::Kind::Commit => "commit",
        gix_object::Kind::Tag => "tag",
    };
    let mut raw = Vec::with_capacity(found.data.len() + 32);
    raw.extend_from_slice(kind_str.as_bytes());
    raw.push(b' ');
    raw.extend_from_slice(found.data.len().to_string().as_bytes());
    raw.push(0);
    raw.extend_from_slice(found.data);
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    use std::io::Write as _;
    encoder
        .write_all(&raw)
        .map_err(|e| GitStoreError::Io(format!("cannot compress object {id}: {e}")))?;
    let compressed = encoder
        .finish()
        .map_err(|e| GitStoreError::Io(format!("cannot compress object {id}: {e}")))?;
    let dir = local_objects_dir.join(&id[..2]);
    let dest = dir.join(&id[2..]);
    if dest.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(GitStoreError::from)?;
    let tmp = dest.with_extension("tmp");
    std::fs::write(&tmp, &compressed).map_err(GitStoreError::from)?;
    std::fs::rename(&tmp, &dest).map_err(GitStoreError::from)?;
    Ok(())
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
