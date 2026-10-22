use crate::git::config::command_timeout_ms;
use crate::git::runner::{run_git, run_git_with_env};
use crate::git::types::{GitError, GitResult};
use crate::infra::context::ApiContext;

use super::{ActionOutcome, ActionRequest, echo_of, finish, param, require_confirm, validate_branch_name};

pub fn handle(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> Option<GitResult<ActionOutcome>> {
    match action {
        "commit" | "commit.direct" => Some(commit(ctx, action, request, root)),
        "commit.amend" => Some(commit_amend(ctx, action, request, root)),
        "commit.revert" => Some(commit_revert(ctx, action, request, root)),
        "commit.cherry-pick" => Some(commit_cherry_pick(ctx, action, request, root)),
        "commit.to-branch" => Some(commit_to_branch(ctx, action, request, root)),
        "commit.to-tag" => Some(commit_to_tag(ctx, action, request, root)),
        "commit.reword" => Some(commit_reword(ctx, action, request, root)),
        "commit.squash" => Some(commit_squash(ctx, action, request, root)),
        "commit.fixup" => Some(commit_fixup(ctx, action, request, root)),
        "commit.drop" => Some(commit_drop(ctx, action, request, root)),
        _ => None,
    }
}

fn commit(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let message = param(request, "message")?;
    if message.len() > 4096 {
        return Err(GitError::Precondition("message too long".to_string()));
    }
    let argv = vec!["commit".to_string(), "-m".to_string(), message.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("committed: {message}"), echo))
}

fn commit_amend(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    require_confirm(request, "amend", action)?;
    let argv = vec![
        "commit".to_string(),
        "--amend".to_string(),
        "--no-edit".to_string(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, "amended".to_string(), echo))
}

fn commit_revert(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let commit = param(request, "commit")?;
    let argv = vec![
        "revert".to_string(),
        "--no-edit".to_string(),
        commit.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("reverted {commit}"), echo))
}

fn commit_cherry_pick(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let commit = param(request, "commit")?;
    let argv = vec!["cherry-pick".to_string(), commit.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("cherry-picked {commit}"), echo))
}

fn commit_to_branch(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let commit = param(request, "commit")?;
    let branch = param(request, "branch")?;
    validate_branch_name(&branch)?;
    let argv = vec!["branch".to_string(), branch.clone(), commit.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("branch {branch} at {commit}"), echo))
}

fn commit_to_tag(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let commit = param(request, "commit")?;
    let tag = param(request, "tag")?;
    if tag.trim().is_empty() || tag.contains('\0') || tag.contains(' ') {
        return Err(GitError::Precondition("invalid tag".to_string()));
    }
    let argv = vec!["tag".to_string(), tag.clone(), commit.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("tagged {tag} at {commit}"), echo))
}

fn commit_reword(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let commit = param(request, "commit")?;
    let message = param(request, "message")?;
    require_confirm(request, &commit, action)?;
    let seq_script = format!(
        "#!/bin/sh\\nsed -i 's/^pick {commit}/reword {commit}/' \"$1\"\\n",
        commit = commit
    );
    let editor_script = format!(
        "#!/bin/sh\\nprintf '%s\\n' '{}' > \"$1\"\\n",
        message.replace('\'', "'\\''")
    );
    let seq_path = std::env::temp_dir().join(format!("seq-editor-{}.sh", std::process::id()));
    let editor_path = std::env::temp_dir().join(format!("msg-editor-{}.sh", std::process::id()));
    std::fs::write(&seq_path, &seq_script)
        .map_err(|e| GitError::Command(format!("failed to write seq editor: {e}")))?;
    std::fs::write(&editor_path, &editor_script)
        .map_err(|e| GitError::Command(format!("failed to write msg editor: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in [&seq_path, &editor_path] {
            if let Ok(m) = std::fs::metadata(p) {
                let mut perms = m.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(p, perms);
            }
        }
    }
    let base = format!("{commit}^");
    let envs = [
        ("GIT_SEQUENCE_EDITOR", seq_path.to_str().unwrap_or("true")),
        ("GIT_EDITOR", editor_path.to_str().unwrap_or("true")),
    ];
    let result = run_git_with_env(
        root,
        &["rebase", "-i", &base],
        command_timeout_ms(ctx, false),
        &envs,
    );
    let _ = std::fs::remove_file(&seq_path);
    let _ = std::fs::remove_file(&editor_path);
    result?;
    Ok(finish(
        action,
        format!("reworded {commit}"),
        format!("git rebase -i {base}"),
    ))
}

fn commit_squash(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let base = param(request, "base")?;
    require_confirm(request, &base, action)?;
    let head_msg = run_git(
        root,
        &["log", "-1", "--format=%s", "HEAD"],
        command_timeout_ms(ctx, false),
    )?
    .stdout
    .trim()
    .to_string();
    let argv = ["reset".to_string(), "--soft".to_string(), base.clone()];
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    let argv = ["commit".to_string(), "-m".to_string(), head_msg];
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(
        action,
        format!("squashed onto {base}"),
        format!("git reset --soft {base} && git commit"),
    ))
}

fn commit_fixup(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let commit = param(request, "commit")?;
    require_confirm(request, &commit, action)?;
    let seq_script = format!(
        "#!/bin/sh\\nsed -i 's/^pick {commit}/fixup {commit}/' \"$1\"\\n",
        commit = commit
    );
    let seq_path = std::env::temp_dir().join(format!("seq-editor-{}.sh", std::process::id()));
    std::fs::write(&seq_path, &seq_script)
        .map_err(|e| GitError::Command(format!("failed to write seq editor: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = std::fs::metadata(&seq_path) {
            let mut perms = m.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&seq_path, perms);
        }
    }
    let base = format!("{commit}^");
    let envs = [("GIT_SEQUENCE_EDITOR", seq_path.to_str().unwrap_or("true"))];
    let result = run_git_with_env(
        root,
        &["rebase", "-i", &base],
        command_timeout_ms(ctx, false),
        &envs,
    );
    let _ = std::fs::remove_file(&seq_path);
    result?;
    Ok(finish(
        action,
        format!("fixup {commit}"),
        format!("git rebase -i {base}"),
    ))
}

fn commit_drop(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let commit = param(request, "commit")?;
    require_confirm(request, &commit, action)?;
    let parent = format!("{commit}^");
    let argv = vec![
        "rebase".to_string(),
        "--onto".to_string(),
        parent.clone(),
        commit.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("dropped {commit}"), echo))
}