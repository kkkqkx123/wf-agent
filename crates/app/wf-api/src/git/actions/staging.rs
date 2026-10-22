use crate::git::config::{command_timeout_ms, validate_repo_path};
use crate::git::runner::run_git;
use crate::git::types::{GitError, GitResult};
use crate::infra::context::ApiContext;

use super::{ActionOutcome, ActionRequest, echo_of, finish, param, require_confirm};

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

fn stage_hunk(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    let hunk_index_str = param(request, "hunkIndex")?;
    validate_repo_path(root, &path)?;
    let out = run_git(
        root,
        &["diff", "--no-color", "--unified=3", "--", &path],
        command_timeout_ms(ctx, false),
    )?;
    let diff_text = out.stdout;
    let hunks: Vec<&str> = diff_text.split("@@").skip(1).collect();
    let idx: usize = hunk_index_str.parse().unwrap_or(0);
    if idx >= hunks.len() {
        return Err(GitError::Precondition(
            "hunk index out of range".to_string(),
        ));
    }
    let hunk_text = format!("@@{}", hunks[idx]);
    let patch_path = std::env::temp_dir().join(format!("hunk-{}.patch", std::process::id()));
    let mut patch = String::new();
    patch.push_str(&format!("--- a/{}\n", path));
    patch.push_str(&format!("+++ b/{}\n", path));
    patch.push_str(&hunk_text);
    std::fs::write(&patch_path, &patch)
        .map_err(|e| GitError::Command(format!("failed to write patch: {e}")))?;
    let patch_str = patch_path.to_str().unwrap_or("").to_string();
    let argv = [
        "apply".to_string(),
        "--cached".to_string(),
        patch_str.clone(),
    ];
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let result = run_git(root, &refs, command_timeout_ms(ctx, false));
    let _ = std::fs::remove_file(&patch_path);
    result?;
    Ok(finish(
        action,
        format!("staged hunk {idx} of {path}"),
        "git apply --cached <patch>".to_string(),
    ))
}