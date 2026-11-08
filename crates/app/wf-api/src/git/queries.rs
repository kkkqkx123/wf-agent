use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::config::{command_timeout_ms, repo_root_for, validate_repo_path};
use super::parse::{
    log_format_args, parse_branches, parse_diff_unified, parse_log_entries, parse_name_status,
    parse_porcelain_v2_branch_info, parse_porcelain_v2_files, parse_stash_list, parse_tags,
    short_commit,
};
use super::runner::{run_git, run_git_bytes};
use super::types::{
    BranchEntry, CommitEntry, CommitFileEntry, DiffFileView, FileEntry, Fingerprint, GitError,
    GitFileView, GitResult, ReflogEntry, RemoteEntry, RepoMeta, StashEntry, StatusView,
    SubmoduleEntry, TagEntry, WorktreeEntry,
};
use crate::infra::context::ApiContext;

fn hash_hex(input: &str) -> String {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn status_output(ctx: &ApiContext, dir: Option<&std::path::Path>) -> GitResult<(std::path::PathBuf, String)> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    let out = run_git(
        &root,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ],
        timeout,
    )?;
    Ok((root, out.stdout))
}

fn head_commit(root: &std::path::Path, timeout: u64) -> String {
    run_git(root, &["rev-parse", "HEAD"], timeout)
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_default()
}

fn branch_summary(root: &std::path::Path, timeout: u64) -> String {
    run_git(
        root,
        &[
            "for-each-ref",
            "--format=%(refname:short)%00%(upstream:short)%00%(ahead)%00%(behind)%00%(objectname:short)%00%(subject)",
            "refs/heads",
        ],
        timeout,
    )
    .map(|o| o.stdout.trim().to_string())
    .unwrap_or_default()
}

pub fn fingerprint(ctx: &ApiContext, dir: Option<&std::path::Path>) -> GitResult<Fingerprint> {
    let (root, status) = status_output(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    let head = head_commit(&root, timeout);
    let summary = branch_summary(&root, timeout);
    Ok(Fingerprint {
        status_hash: hash_hex(&status),
        head,
        branch_summary: hash_hex(&summary),
    })
}

pub fn meta(ctx: &ApiContext, dir: Option<&std::path::Path>) -> GitResult<RepoMeta> {
    let (root, status) = status_output(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    let (branch, upstream, ahead, behind) = parse_porcelain_v2_branch_info(&status);
    let head = head_commit(&root, timeout);
    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".to_string());
    let bare = run_git(&root, &["rev-parse", "--is-bare-repository"], timeout)
        .map(|o| o.stdout.trim() == "true")
        .unwrap_or(false);
    let git_dir = run_git(&root, &["rev-parse", "--absolute-git-dir"], timeout)
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_default();
    let merge_head =
        !git_dir.is_empty() && std::path::Path::new(&format!("{git_dir}/MERGE_HEAD")).exists();
    let rebase_head = !git_dir.is_empty()
        && (std::path::Path::new(&format!("{git_dir}/rebase-merge")).exists()
            || std::path::Path::new(&format!("{git_dir}/rebase-apply")).exists());
    let cherry_pick_head = !git_dir.is_empty()
        && std::path::Path::new(&format!("{git_dir}/CHERRY_PICK_HEAD")).exists();
    let branch = if branch.is_empty() {
        run_git(&root, &["branch", "--show-current"], timeout)
            .map(|o| o.stdout.trim().to_string())
            .unwrap_or_default()
    } else {
        branch
    };
    Ok(RepoMeta {
        name,
        root: root.to_string_lossy().to_string(),
        branch,
        upstream,
        ahead,
        behind,
        head,
        bare,
        merge_head,
        rebase_head,
        cherry_pick_head,
    })
}

pub fn status(ctx: &ApiContext, dir: Option<&std::path::Path>) -> GitResult<StatusView> {
    let (_, output) = status_output(ctx, dir)?;
    let (branch, upstream, ahead, behind) = parse_porcelain_v2_branch_info(&output);
    let files = parse_porcelain_v2_files(&output);
    let mut staged = 0usize;
    let mut unstaged = 0usize;
    let mut untracked = 0usize;
    let mut conflicted = 0usize;
    for f in &files {
        match f.kind {
            super::types::FileKind::Untracked => untracked += 1,
            super::types::FileKind::Conflicted => conflicted += 1,
            super::types::FileKind::Staged | super::types::FileKind::Renamed => staged += 1,
            super::types::FileKind::Unstaged => unstaged += 1,
        }
    }
    Ok(StatusView {
        branch,
        upstream,
        ahead,
        behind,
        staged,
        unstaged,
        untracked,
        conflicted,
        merge_head: false,
        rebase_head: false,
        cherry_pick_head: false,
    })
}

pub fn file_entries(ctx: &ApiContext, dir: Option<&std::path::Path>) -> GitResult<Vec<FileEntry>> {
    let (_, output) = status_output(ctx, dir)?;
    Ok(parse_porcelain_v2_files(&output))
}

pub fn diff_unstaged(ctx: &ApiContext, dir: Option<&std::path::Path>, path: Option<&str>) -> GitResult<Vec<DiffFileView>> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    if let Some(path) = path {
        validate_repo_path(&root, path)?;
    }
    let mut args: Vec<&str> = vec!["diff", "--no-color", "--unified=3", "--no-ext-diff", "--"];
    let owned;
    if let Some(path) = path {
        owned = path.to_string();
        args.push(&owned);
    }
    let out = run_git(&root, &args, timeout)?;
    Ok(parse_diff_unified(&out.stdout))
}

pub fn diff_staged(ctx: &ApiContext, dir: Option<&std::path::Path>, path: Option<&str>) -> GitResult<Vec<DiffFileView>> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    if let Some(path) = path {
        validate_repo_path(&root, path)?;
    }
    let mut args: Vec<&str> = vec![
        "diff",
        "--cached",
        "--no-color",
        "--unified=3",
        "--no-ext-diff",
        "--",
    ];
    let owned;
    if let Some(path) = path {
        owned = path.to_string();
        args.push(&owned);
    }
    let out = run_git(&root, &args, timeout)?;
    Ok(parse_diff_unified(&out.stdout))
}

pub fn diff_refs(
    ctx: &ApiContext,
    dir: Option<&std::path::Path>,
    from: &str,
    to: &str,
    path: Option<&str>,
) -> GitResult<Vec<DiffFileView>> {
    if from.trim().is_empty() || to.trim().is_empty() {
        return Err(GitError::Precondition(
            "diff refs must not be empty".to_string(),
        ));
    }
    if from.contains('\0') || to.contains('\0') || from.contains("....") {
        return Err(GitError::Precondition("invalid diff ref".to_string()));
    }
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    if let Some(path) = path {
        validate_repo_path(&root, path)?;
    }
    let range = format!("{from}...{to}");
    let mut args: Vec<&str> = vec!["diff", "--no-color", "--unified=3", "--no-ext-diff"];
    let owned_range = range;
    args.push(&owned_range);
    args.push("--");
    let owned_path;
    if let Some(path) = path {
        owned_path = path.to_string();
        args.push(&owned_path);
    }
    let out = run_git(&root, &args, timeout)?;
    Ok(parse_diff_unified(&out.stdout))
}

/// Normalize the `rev` parameter for blob reads. Empty means `HEAD`;
/// `:staged` addresses the index. Anything that could escape into option
/// injection or range syntax is rejected; the value travels as one argv
/// element so no shell quoting is involved.
fn normalize_blob_rev(raw: Option<&str>) -> GitResult<String> {
    let rev = raw.unwrap_or("").trim();
    if rev.is_empty() {
        return Ok("HEAD".to_string());
    }
    if rev == ":staged" || rev == ":0" {
        return Ok(":staged".to_string());
    }
    if rev.len() > 256
        || rev.contains('\0')
        || rev.contains('\n')
        || rev.starts_with('-')
        || rev.contains("..")
        || rev.contains(':')
        || rev.contains(' ')
    {
        return Err(GitError::Precondition(format!(
            "invalid git revision '{rev}'"
        )));
    }
    Ok(rev.to_string())
}

/// Full file content at one revision. Backs the diff-tab left side:
/// unstaged tabs read `HEAD`, staged tabs read `:staged` and `HEAD`,
/// commit tabs read the parent commit. Missing blobs report
/// `exists: false` instead of failing so added files degrade to hunk view.
pub fn git_file(ctx: &ApiContext, dir: Option<&std::path::Path>, path: &str, rev: Option<&str>) -> GitResult<GitFileView> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    validate_repo_path(&root, path)?;
    let rev = normalize_blob_rev(rev)?;
    let spec = if rev == ":staged" {
        format!(":{path}")
    } else {
        format!("{rev}:{path}")
    };
    let bytes = match run_git_bytes(&root, &["show", &spec], timeout) {
        Ok(out) => out.stdout,
        Err(GitError::Command(detail))
            if detail.contains("does not exist") || detail.contains("but not in") =>
        {
            return Ok(GitFileView {
                path: path.to_string(),
                rev,
                content: String::new(),
                size: 0,
                truncated: false,
                is_binary: false,
                exists: false,
            });
        }
        Err(other) => return Err(other),
    };
    let size = bytes.len() as u64;
    let capped = bytes.len().min(crate::fs::MAX_READ_BYTES);
    let head = &bytes[..capped];
    if crate::fs::is_binary_bytes(head) {
        return Ok(GitFileView {
            path: path.to_string(),
            rev,
            content: String::new(),
            size,
            truncated: false,
            is_binary: true,
            exists: true,
        });
    }
    Ok(GitFileView {
        path: path.to_string(),
        rev,
        content: String::from_utf8_lossy(head).into_owned(),
        size,
        truncated: bytes.len() > capped,
        is_binary: false,
        exists: true,
    })
}

pub fn commits(
    ctx: &ApiContext,
    dir: Option<&std::path::Path>,
    limit: u32,
    offset: u32,
    branch: Option<&str>,
) -> GitResult<Vec<CommitEntry>> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    let limit = limit.clamp(1, 200).to_string();
    let offset = offset.to_string();
    let mut args: Vec<String> = vec![
        "log".to_string(),
        format!("--max-count={limit}"),
        format!("--skip={offset}"),
        "--no-color".to_string(),
    ];
    args.extend(log_format_args());
    if let Some(branch) = branch {
        if branch.trim().is_empty() || branch.contains('\0') || branch.contains("..") {
            return Err(GitError::Precondition("invalid branch ref".to_string()));
        }
        args.push(branch.to_string());
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_git(&root, &arg_refs, timeout)?;
    Ok(parse_log_entries(&out.stdout))
}

pub fn commit_files(ctx: &ApiContext, dir: Option<&std::path::Path>, commit: &str) -> GitResult<Vec<CommitFileEntry>> {
    if commit.trim().is_empty() || commit.contains('\0') {
        return Err(GitError::Precondition(
            "commit must not be empty".to_string(),
        ));
    }
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    let out = run_git(
        &root,
        &["show", "--no-color", "--name-status", "--format=", commit],
        timeout,
    )?;
    Ok(parse_name_status(&out.stdout))
}

pub fn branches(ctx: &ApiContext, dir: Option<&std::path::Path>) -> GitResult<Vec<BranchEntry>> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    let format =
        "%(refname:short)%1f%(HEAD)%1f%(upstream:short)%1f%(objectname:short)%1f%(subject)%1e";
    let out = run_git(
        &root,
        &["for-each-ref", &format!("--format={format}"), "refs/heads"],
        timeout,
    )?;
    let current = run_git(&root, &["branch", "--show-current"], timeout)
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_default();
    let mut entries = parse_branches(&out.stdout);
    for entry in &mut entries {
        if entry.name == current {
            entry.current = true;
        }
        if let Some(upstream) = entry.upstream.clone() {
            if let Ok(counts) = run_git(
                &root,
                &[
                    "rev-list",
                    "--left-right",
                    "--count",
                    &format!("HEAD...{upstream}"),
                ],
                timeout,
            ) {
                let mut parts = counts.stdout.split_whitespace();
                // rev-list prints "<ahead> <behind>" only when the range is
                // valid; detached or missing upstreams keep the zero default.
                if let (Some(a), Some(b)) = (parts.next(), parts.next()) {
                    entry.ahead = a.parse().unwrap_or(0);
                    entry.behind = b.parse().unwrap_or(0);
                }
            }
        }
    }
    Ok(entries)
}

pub fn remotes(ctx: &ApiContext, dir: Option<&std::path::Path>) -> GitResult<Vec<RemoteEntry>> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, true);
    let out = run_git(&root, &["remote", "-v"], timeout)?;
    let mut map: std::collections::BTreeMap<String, String> = Default::default();
    for line in out.stdout.lines() {
        let mut parts = line.split_whitespace();
        let name = parts.next().unwrap_or("").to_string();
        let url = parts.next().unwrap_or("").to_string();
        let kind = parts.next().unwrap_or("");
        if name.is_empty() || kind != "(fetch)" {
            continue;
        }
        map.entry(name).or_insert(url);
    }
    let branch_out = run_git(
        &root,
        &["for-each-ref", "--format=%(refname:short)", "refs/remotes"],
        timeout,
    )
    .map(|o| o.stdout)
    .unwrap_or_default();
    let mut branches_by_remote: std::collections::BTreeMap<String, Vec<String>> =
        Default::default();
    for line in branch_out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((remote, rest)) = line.split_once('/') {
            branches_by_remote
                .entry(remote.to_string())
                .or_default()
                .push(rest.to_string());
        }
    }
    let mut out_entries: Vec<RemoteEntry> = map
        .into_iter()
        .map(|(name, url)| RemoteEntry {
            branches: branches_by_remote.remove(&name).unwrap_or_default(),
            name,
            url,
        })
        .collect();
    out_entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out_entries)
}

pub fn tags(ctx: &ApiContext, dir: Option<&std::path::Path>) -> GitResult<Vec<TagEntry>> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    let format = "%(refname:short)%1f%(objectname:short)%1f%(*objectname:short)%1f%(subject)%1e";
    let out = run_git(
        &root,
        &["for-each-ref", &format!("--format={format}"), "refs/tags"],
        timeout,
    )?;
    Ok(parse_tags(&out.stdout))
}

pub fn stash_list(ctx: &ApiContext, dir: Option<&std::path::Path>) -> GitResult<Vec<StashEntry>> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    let out = run_git(&root, &["stash", "list"], timeout)?;
    Ok(parse_stash_list(&out.stdout))
}

pub fn submodule_status(ctx: &ApiContext, dir: Option<&std::path::Path>) -> GitResult<Vec<SubmoduleEntry>> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    let out = run_git(&root, &["submodule", "status"], timeout);
    let output = match out {
        Ok(o) => o.stdout,
        Err(_) => return Ok(Vec::new()),
    };
    let mut entries = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let commit = parts
            .next()
            .unwrap_or("")
            .trim_start_matches([' ', '-', '+', 'U'])
            .to_string();
        let path = parts.next().unwrap_or("").to_string();
        if path.is_empty() {
            continue;
        }
        entries.push(SubmoduleEntry {
            path,
            commit: short_commit(&commit),
            status: "tracked".to_string(),
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

pub fn worktrees(ctx: &ApiContext, dir: Option<&std::path::Path>) -> GitResult<Vec<WorktreeEntry>> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    let out = run_git(&root, &["worktree", "list", "--porcelain"], timeout);
    let output = match out {
        Ok(o) => o.stdout,
        Err(_) => return Ok(Vec::new()),
    };
    let mut entries = Vec::new();
    let mut path = String::new();
    let mut branch = String::new();
    let mut commit = String::new();
    let mut bare = false;
    let flush = |path: &mut String,
                 branch: &mut String,
                 commit: &mut String,
                 bare: &mut bool,
                 entries: &mut Vec<WorktreeEntry>| {
        if path.is_empty() {
            return;
        }
        entries.push(WorktreeEntry {
            path: std::mem::take(path),
            branch: std::mem::take(branch),
            commit: short_commit(&std::mem::take(commit)),
            bare: std::mem::replace(bare, false),
        });
    };
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            flush(&mut path, &mut branch, &mut commit, &mut bare, &mut entries);
            path = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            commit = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("branch ") {
            branch = rest.trim().trim_start_matches("refs/heads/").to_string();
        } else if line.trim() == "bare" {
            bare = true;
        } else if line.trim() == "detached" {
            branch = "(detached)".to_string();
        }
    }
    flush(&mut path, &mut branch, &mut commit, &mut bare, &mut entries);
    Ok(entries)
}

pub fn reflog(ctx: &ApiContext, dir: Option<&std::path::Path>, limit: u32) -> GitResult<Vec<ReflogEntry>> {
    let root = repo_root_for(ctx, dir)?;
    let timeout = command_timeout_ms(ctx, false);
    let limit = limit.clamp(1, 200).to_string();
    let out = run_git(
        &root,
        &[
            "reflog",
            &format!("-n{limit}"),
            "--format=%H%x1f%h%x1f%gD%x1f%gs%x1f%ct",
        ],
        timeout,
    )?;
    let mut entries = Vec::new();
    for line in out.stdout.lines() {
        let fields: Vec<&str> = line.split('\u{001f}').collect();
        if fields.len() < 5 {
            continue;
        }
        let id = fields[0].trim().to_string();
        if id.is_empty() {
            continue;
        }
        entries.push(ReflogEntry {
            id: id.clone(),
            short_id: fields[1].trim().to_string(),
            selector: fields[2].trim().to_string(),
            message: fields[3].trim().to_string(),
            timestamp: fields[4].trim().parse().unwrap_or(0),
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::normalize_blob_rev;

    #[test]
    fn blob_rev_defaults_to_head() {
        assert_eq!(normalize_blob_rev(None).unwrap(), "HEAD");
        assert_eq!(normalize_blob_rev(Some("")).unwrap(), "HEAD");
        assert_eq!(normalize_blob_rev(Some("  ")).unwrap(), "HEAD");
    }

    #[test]
    fn blob_rev_accepts_index_alias() {
        assert_eq!(normalize_blob_rev(Some(":staged")).unwrap(), ":staged");
        assert_eq!(normalize_blob_rev(Some(":0")).unwrap(), ":staged");
    }

    #[test]
    fn blob_rev_accepts_commits_and_ancestry() {
        assert_eq!(normalize_blob_rev(Some("abc1234")).unwrap(), "abc1234");
        assert_eq!(normalize_blob_rev(Some("HEAD~1")).unwrap(), "HEAD~1");
        assert_eq!(normalize_blob_rev(Some("main")).unwrap(), "main");
    }

    #[test]
    fn blob_rev_rejects_injection_shapes() {
        for raw in [
            "--upload-pack=t",
            "a..b",
            "a....b",
            "HEAD:other",
            "a b",
            "a\0b",
            "a\nb",
        ] {
            assert!(normalize_blob_rev(Some(raw)).is_err(), "{raw}");
        }
    }
}
