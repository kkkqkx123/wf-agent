use crate::git::config::{command_timeout_ms, validate_repo_path};
use crate::git::parse::parse_diff_unified;
use crate::git::runner::run_git;
use crate::git::types::{GitError, GitResult};
use crate::infra::context::ApiContext;

use super::{echo_of, finish, param, require_confirm, ActionOutcome, ActionRequest};

pub fn handle(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> Option<GitResult<ActionOutcome>> {
    match action {
        "stage.toggle" => Some(stage_toggle(ctx, action, request, root)),
        "stage.all" => Some(stage_all(ctx, action, root)),
        "unstage.all" => Some(unstage_all(ctx, action, root)),
        "discard" => Some(discard(ctx, action, request, root)),
        "stage.hunk" => Some(stage_hunk(ctx, action, request, root)),
        "unstage.hunk" => Some(unstage_hunk(ctx, action, request, root)),
        "stage.lines" => Some(stage_lines(ctx, action, request, root)),
        "unstage.lines" => Some(unstage_lines(ctx, action, request, root)),
        "discard.hunk" => Some(discard_hunk(ctx, action, request, root)),
        "discard.lines" => Some(discard_lines(ctx, action, request, root)),
        _ => None,
    }
}

fn stage_toggle(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    validate_repo_path(root, &path)?;
    let staged_check = run_git(
        root,
        &["diff", "--cached", "--name-only", "--", &path],
        command_timeout_ms(ctx, false),
    )?;
    let argv = if staged_check.stdout.lines().any(|l| l.trim() == path) {
        vec![
            "restore".to_string(),
            "--staged".to_string(),
            "--".to_string(),
            path.clone(),
        ]
    } else {
        vec!["add".to_string(), "--".to_string(), path.clone()]
    };
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("toggled {path}"), echo))
}

fn stage_all(ctx: &ApiContext, action: &str, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let argv = vec!["add".to_string(), "-A".to_string()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, "staged all".to_string(), echo))
}

fn unstage_all(ctx: &ApiContext, action: &str, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let argv = vec![
        "restore".to_string(),
        "--staged".to_string(),
        ".".to_string(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, "unstaged all".to_string(), echo))
}

fn discard(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    require_confirm(request, &path, action)?;
    validate_repo_path(root, &path)?;
    let argv = vec![
        "restore".to_string(),
        "--source=HEAD".to_string(),
        "--staged".to_string(),
        "--worktree".to_string(),
        "--".to_string(),
        path.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("discarded {path}"), echo))
}

fn split_hunks(diff_text: &str) -> (String, Vec<String>) {
    let mut header = String::new();
    let mut hunks: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in diff_text.lines() {
        if line.starts_with("@@") {
            if let Some(block) = current.take() {
                hunks.push(block);
            }
            current = Some(format!("{line}\n"));
        } else if let Some(block) = current.as_mut() {
            block.push_str(line);
            block.push('\n');
        } else {
            header.push_str(line);
            header.push('\n');
        }
    }
    if let Some(block) = current.take() {
        hunks.push(block);
    }
    (header, hunks)
}

fn parse_hunk_index(raw: &str, total: usize) -> GitResult<usize> {
    let idx: usize = raw
        .trim()
        .parse()
        .map_err(|_| GitError::Precondition("hunkIndex must be a number".to_string()))?;
    if idx >= total {
        return Err(GitError::Precondition(format!(
            "hunk index {idx} out of range (0..{total})"
        )));
    }
    Ok(idx)
}

fn parse_line_selection(raw: &str, content_len: usize) -> GitResult<Vec<usize>> {
    let mut out: Vec<usize> = Vec::new();
    for token in raw.split([',', ' ', '\t', '\n']) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some((start_raw, end_raw)) = token.split_once('-') {
            let start: usize = start_raw
                .trim()
                .parse()
                .map_err(|_| GitError::Precondition("lines must be numbers".to_string()))?;
            let end: usize = end_raw
                .trim()
                .parse()
                .map_err(|_| GitError::Precondition("lines must be numbers".to_string()))?;
            let (lo, hi) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            for idx in lo..=hi {
                if idx >= content_len {
                    return Err(GitError::Precondition(format!(
                        "line index {idx} out of range (0..{content_len})"
                    )));
                }
                out.push(idx);
            }
        } else {
            let idx: usize = token
                .parse()
                .map_err(|_| GitError::Precondition("lines must be numbers".to_string()))?;
            if idx >= content_len {
                return Err(GitError::Precondition(format!(
                    "line index {idx} out of range (0..{content_len})"
                )));
            }
            out.push(idx);
        }
    }
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        return Err(GitError::Precondition("empty line selection".to_string()));
    }
    Ok(out)
}

fn raw_diff(
    ctx: &ApiContext,
    root: &std::path::Path,
    path: &str,
    staged: bool,
) -> GitResult<String> {
    let mut args: Vec<&str> = if staged {
        vec![
            "diff",
            "--cached",
            "--no-color",
            "--unified=3",
            "--no-ext-diff",
            "--",
        ]
    } else {
        vec!["diff", "--no-color", "--unified=3", "--no-ext-diff", "--"]
    };
    let owned = path.to_string();
    args.push(&owned);
    Ok(run_git(root, &args, command_timeout_ms(ctx, false))?.stdout)
}

fn file_flags(diff_text: &str, path: &str) -> (bool, bool) {
    let views = parse_diff_unified(diff_text);
    for view in &views {
        if view.path == path {
            return (view.binary, view.truncated);
        }
    }
    if let Some(first) = views.first() {
        return (first.binary, first.truncated);
    }
    (false, false)
}

fn check_header(header: &str, path: &str) -> GitResult<()> {
    if !header.contains("diff --git") {
        return Err(GitError::Precondition(
            "unexpected diff header for this file".to_string(),
        ));
    }
    if !header.contains(&format!("a/{path}")) && !header.contains(&format!("b/{path}")) {
        return Err(GitError::Precondition(
            "unexpected diff header for this file".to_string(),
        ));
    }
    Ok(())
}

fn parse_hunk_range(header_line: &str) -> GitResult<(i64, usize, i64, usize, String)> {
    let trimmed = header_line.trim();
    let inner = trimmed
        .strip_prefix("@@")
        .ok_or_else(|| GitError::Precondition("invalid hunk header".to_string()))?;
    let (ranges, suffix) = inner
        .split_once("@@")
        .ok_or_else(|| GitError::Precondition("invalid hunk header".to_string()))?;
    let mut parts = ranges.split_whitespace();
    let old_raw = parts
        .next()
        .ok_or_else(|| GitError::Precondition("invalid hunk header".to_string()))?;
    let new_raw = parts
        .next()
        .ok_or_else(|| GitError::Precondition("invalid hunk header".to_string()))?;
    let (old_start, old_count) = parse_range_part(old_raw, '-')?;
    let (new_start, new_count) = parse_range_part(new_raw, '+')?;
    Ok((
        old_start,
        old_count,
        new_start,
        new_count,
        suffix.to_string(),
    ))
}

fn parse_range_part(raw: &str, sign: char) -> GitResult<(i64, usize)> {
    let body = raw
        .strip_prefix(sign)
        .ok_or_else(|| GitError::Precondition("invalid hunk header".to_string()))?;
    if let Some((start_raw, count_raw)) = body.split_once(',') {
        let start: i64 = start_raw
            .parse()
            .map_err(|_| GitError::Precondition("invalid hunk header".to_string()))?;
        let count: usize = count_raw
            .parse()
            .map_err(|_| GitError::Precondition("invalid hunk header".to_string()))?;
        Ok((start, count))
    } else {
        let start: i64 = body
            .parse()
            .map_err(|_| GitError::Precondition("invalid hunk header".to_string()))?;
        Ok((start, 1))
    }
}

fn build_line_patch(header: &str, hunk_block: &str, selected: &[usize]) -> GitResult<String> {
    let mut lines: Vec<&str> = hunk_block.lines().collect();
    if lines.is_empty() || !lines[0].starts_with("@@") {
        return Err(GitError::Precondition("invalid hunk block".to_string()));
    }
    let header_line = lines.remove(0);
    let (old_start, _, new_start, _, suffix) = parse_hunk_range(header_line)?;
    let selected_set: std::collections::HashSet<usize> = selected.iter().copied().collect();
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    let mut kept_add = 0usize;
    let mut kept_del = 0usize;
    let mut kept_ctx = 0usize;
    let mut last_kept = false;
    for (idx, line) in lines.iter().enumerate() {
        if line.starts_with('\\') {
            if last_kept {
                kept.push(line);
            }
            continue;
        }
        if let Some(body) = line.strip_prefix('+') {
            let _ = body;
            if selected_set.contains(&idx) {
                kept.push(line);
                kept_add += 1;
                last_kept = true;
            } else {
                last_kept = false;
            }
        } else if let Some(body) = line.strip_prefix('-') {
            let _ = body;
            if selected_set.contains(&idx) {
                kept.push(line);
                kept_del += 1;
                last_kept = true;
            } else {
                last_kept = false;
            }
        } else {
            if selected_set.contains(&idx) {
                return Err(GitError::Precondition(
                    "line selection must only contain added or removed lines".to_string(),
                ));
            }
            kept.push(line);
            kept_ctx += 1;
            last_kept = true;
        }
    }
    if kept_add + kept_del == 0 {
        return Err(GitError::Precondition(
            "line selection keeps no changes".to_string(),
        ));
    }
    let old_count = kept_ctx + kept_del;
    let new_count = kept_ctx + kept_add;
    let mut patch = String::new();
    patch.push_str(header);
    patch.push_str(&format!(
        "@@ -{old_start},{old_count} +{new_start},{new_count} @@{suffix}\n"
    ));
    for line in kept {
        patch.push_str(line);
        patch.push('\n');
    }
    if !patch.ends_with('\n') {
        patch.push('\n');
    }
    Ok(patch)
}

fn temp_patch_path(action: &str, idx: usize) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let safe: String = action
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    std::env::temp_dir().join(format!(
        "wf-{safe}-{}-{nanos}-{idx}.patch",
        std::process::id()
    ))
}

fn apply_patch(
    root: &std::path::Path,
    timeout: u64,
    patch: &str,
    extra: &[&str],
    echo: String,
    action: &str,
    message: String,
) -> GitResult<ActionOutcome> {
    let mut owned = patch.to_string();
    if !owned.ends_with('\n') {
        owned.push('\n');
    }
    let patch_path = temp_patch_path(action, owned.len() % 100003);
    std::fs::write(&patch_path, &owned)
        .map_err(|e| GitError::Command(format!("failed to write patch: {e}")))?;
    let patch_str = patch_path.to_string_lossy().to_string();
    let mut check_argv: Vec<String> = vec!["apply".to_string(), "--check".to_string()];
    check_argv.extend(extra.iter().map(|s| s.to_string()));
    check_argv.push(patch_str.clone());
    let check_refs: Vec<&str> = check_argv.iter().map(String::as_str).collect();
    let check_result = run_git(root, &check_refs, timeout);
    if let Err(err) = check_result {
        let _ = std::fs::remove_file(&patch_path);
        let detail = err.to_string();
        return Err(GitError::Precondition(format!(
            "patch check failed, refresh and retry: {detail}"
        )));
    }
    let mut apply_argv: Vec<String> = vec!["apply".to_string()];
    apply_argv.extend(extra.iter().map(|s| s.to_string()));
    apply_argv.push(patch_str);
    let apply_refs: Vec<&str> = apply_argv.iter().map(String::as_str).collect();
    let apply_result = run_git(root, &apply_refs, timeout);
    let _ = std::fs::remove_file(&patch_path);
    apply_result?;
    Ok(finish(action, message, echo))
}

fn stage_hunk(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    let hunk_index_str = param(request, "hunkIndex")?;
    validate_repo_path(root, &path)?;
    let diff_text = raw_diff(ctx, root, &path, false)?;
    let (binary, _) = file_flags(&diff_text, &path);
    if binary {
        return Err(GitError::Precondition(
            "binary files support file-level actions only".to_string(),
        ));
    }
    let (header, hunks) = split_hunks(&diff_text);
    if hunks.is_empty() {
        return Err(GitError::Precondition(
            "no hunks available for this file".to_string(),
        ));
    }
    let idx = parse_hunk_index(&hunk_index_str, hunks.len())?;
    let selected = hunks[idx].clone();
    check_header(&header, &path)?;
    let mut patch = String::new();
    patch.push_str(&header);
    patch.push_str(&selected);
    apply_patch(
        root,
        command_timeout_ms(ctx, false),
        &patch,
        &["--cached"],
        "git apply --cached <patch>".to_string(),
        action,
        format!("staged hunk {idx} of {path}"),
    )
}

fn unstage_hunk(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    let hunk_index_str = param(request, "hunkIndex")?;
    validate_repo_path(root, &path)?;
    let diff_text = raw_diff(ctx, root, &path, true)?;
    let (binary, _) = file_flags(&diff_text, &path);
    if binary {
        return Err(GitError::Precondition(
            "binary files support file-level actions only".to_string(),
        ));
    }
    let (header, hunks) = split_hunks(&diff_text);
    if hunks.is_empty() {
        return Err(GitError::Precondition(
            "no hunks available for this file".to_string(),
        ));
    }
    let idx = parse_hunk_index(&hunk_index_str, hunks.len())?;
    check_header(&header, &path)?;
    let mut patch = String::new();
    patch.push_str(&header);
    patch.push_str(&hunks[idx]);
    apply_patch(
        root,
        command_timeout_ms(ctx, false),
        &patch,
        &["--cached", "--reverse"],
        "git apply --cached --reverse <patch>".to_string(),
        action,
        format!("unstaged hunk {idx} of {path}"),
    )
}

fn line_patch_for(
    ctx: &ApiContext,
    root: &std::path::Path,
    path: &str,
    staged: bool,
    hunk_index_str: &str,
    lines_raw: &str,
) -> GitResult<(String, usize)> {
    let diff_text = raw_diff(ctx, root, path, staged)?;
    let (binary, truncated) = file_flags(&diff_text, path);
    if binary {
        return Err(GitError::Precondition(
            "binary files support file-level actions only".to_string(),
        ));
    }
    if truncated {
        return Err(GitError::Precondition(
            "truncated diff rejects line-level actions".to_string(),
        ));
    }
    let (header, hunks) = split_hunks(&diff_text);
    if hunks.is_empty() {
        return Err(GitError::Precondition(
            "no hunks available for this file".to_string(),
        ));
    }
    let idx = parse_hunk_index(hunk_index_str, hunks.len())?;
    check_header(&header, path)?;
    let content_len = hunks[idx].lines().count().saturating_sub(1);
    let selected = parse_line_selection(lines_raw, content_len)?;
    for line_idx in &selected {
        let line = hunks[idx].lines().nth(line_idx + 1).unwrap_or("");
        if !(line.starts_with('+') || line.starts_with('-')) {
            return Err(GitError::Precondition(
                "line selection must only contain added or removed lines".to_string(),
            ));
        }
    }
    let patch = build_line_patch(&header, &hunks[idx], &selected)?;
    Ok((patch, idx))
}

fn stage_lines(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    let hunk_index_str = param(request, "hunkIndex")?;
    let lines_raw = param(request, "lines")?;
    validate_repo_path(root, &path)?;
    let (patch, idx) = line_patch_for(ctx, root, &path, false, &hunk_index_str, &lines_raw)?;
    apply_patch(
        root,
        command_timeout_ms(ctx, false),
        &patch,
        &["--cached"],
        "git apply --cached <patch>".to_string(),
        action,
        format!("staged lines of hunk {idx} of {path}"),
    )
}

fn unstage_lines(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    let hunk_index_str = param(request, "hunkIndex")?;
    let lines_raw = param(request, "lines")?;
    validate_repo_path(root, &path)?;
    let (patch, idx) = line_patch_for(ctx, root, &path, true, &hunk_index_str, &lines_raw)?;
    apply_patch(
        root,
        command_timeout_ms(ctx, false),
        &patch,
        &["--cached", "--reverse"],
        "git apply --cached --reverse <patch>".to_string(),
        action,
        format!("unstaged lines of hunk {idx} of {path}"),
    )
}

fn discard_hunk(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    let hunk_index_str = param(request, "hunkIndex")?;
    require_confirm(request, &path, action)?;
    validate_repo_path(root, &path)?;
    let diff_text = raw_diff(ctx, root, &path, false)?;
    let (binary, _) = file_flags(&diff_text, &path);
    if binary {
        return Err(GitError::Precondition(
            "binary files support file-level actions only".to_string(),
        ));
    }
    let (header, hunks) = split_hunks(&diff_text);
    if hunks.is_empty() {
        return Err(GitError::Precondition(
            "no hunks available for this file".to_string(),
        ));
    }
    let idx = parse_hunk_index(&hunk_index_str, hunks.len())?;
    check_header(&header, &path)?;
    let mut patch = String::new();
    patch.push_str(&header);
    patch.push_str(&hunks[idx]);
    apply_patch(
        root,
        command_timeout_ms(ctx, false),
        &patch,
        &["--reverse"],
        "git apply --reverse <patch>".to_string(),
        action,
        format!("discarded hunk {idx} of {path}"),
    )
}

fn discard_lines(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    let hunk_index_str = param(request, "hunkIndex")?;
    let lines_raw = param(request, "lines")?;
    require_confirm(request, &path, action)?;
    validate_repo_path(root, &path)?;
    let (patch, idx) = line_patch_for(ctx, root, &path, false, &hunk_index_str, &lines_raw)?;
    apply_patch(
        root,
        command_timeout_ms(ctx, false),
        &patch,
        &["--reverse"],
        "git apply --reverse <patch>".to_string(),
        action,
        format!("discarded lines of hunk {idx} of {path}"),
    )
}

#[cfg(test)]
mod hunk_tests {
    use super::{build_line_patch, parse_hunk_range, parse_line_selection, split_hunks};

    #[test]
    fn split_hunks_keeps_header_and_blocks() {
        let diff = "diff --git a/a.rs b/a.rs\nindex 111..222 100644\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n ctx\n@@ -10,2 +10,2 @@\n-a\n+b\n";
        let (header, hunks) = split_hunks(diff);
        assert!(header.contains("diff --git"));
        assert_eq!(hunks.len(), 2);
        assert!(hunks[0].starts_with("@@ -1,2"));
        assert!(hunks[1].starts_with("@@ -10,2"));
    }

    #[test]
    fn split_hunks_empty_without_hunks() {
        let (_, hunks) = split_hunks("");
        assert!(hunks.is_empty());
    }

    #[test]
    fn line_selection_parses_comma_and_ranges() {
        let selected = parse_line_selection("0,2,3-4", 10).expect("selection parses");
        assert_eq!(selected, vec![0, 2, 3, 4]);
    }

    #[test]
    fn line_selection_rejects_empty_and_oob() {
        assert!(parse_line_selection("", 5).is_err());
        assert!(parse_line_selection("9", 5).is_err());
    }

    #[test]
    fn hunk_range_parses_counts_and_suffix() {
        let (old_start, old_count, new_start, new_count, suffix) =
            parse_hunk_range("@@ -1,3 +1,4 @@ fn").expect("range parses");
        assert_eq!((old_start, old_count, new_start, new_count), (1, 3, 1, 4));
        assert_eq!(suffix.trim(), "fn");
        let (_, _, _, _, _) = parse_hunk_range("@@ -1 +1,2 @@").expect("range parses");
    }

    #[test]
    fn line_patch_keeps_context_and_recounts() {
        let header = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n";
        let hunk = "@@ -1,3 +1,3 @@\n ctx\n-old-a\n-old-b\n+new-a\n+new-b\n";
        let patch = build_line_patch(header, hunk, &[1, 3]).expect("patch builds");
        assert!(patch.contains("@@ -1,2 +1,2 @@"));
        assert!(patch.contains("-old-a"));
        assert!(patch.contains("+new-a"));
        assert!(!patch.contains("-old-b"));
        assert!(!patch.contains("+new-b"));
        assert!(patch.contains(" ctx"));
    }

    #[test]
    fn line_patch_rejects_context_only() {
        let header = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n";
        let hunk = "@@ -1,2 +1,2 @@\n ctx\n-old\n+new\n";
        assert!(build_line_patch(header, hunk, &[0]).is_err());
        assert!(build_line_patch(header, hunk, &[]).is_err());
    }
}
