use crate::git::config::command_timeout_ms;
use crate::git::runner::run_git;
use crate::git::types::GitResult;
use crate::infra::context::ApiContext;

use super::{
    echo_of, finish, param, require_confirm, validate_branch_name, ActionOutcome, ActionRequest,
};

pub fn handle(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> Option<GitResult<ActionOutcome>> {
    match action {
        "worktree.add" => Some(worktree_add(ctx, action, request, root)),
        "worktree.remove" => Some(worktree_remove(ctx, action, request, root)),
        "worktree.switch" => Some(worktree_switch(action, request)),
        "worktree.open" => Some(worktree_open(action, request)),
        _ => None,
    }
}

fn worktree_add(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    let branch = param(request, "branch")?;
    validate_branch_name(&branch)?;
    let argv = vec![
        "worktree".to_string(),
        "add".to_string(),
        path.clone(),
        branch.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("added worktree {path}"), echo))
}

fn worktree_remove(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    require_confirm(request, &path, action)?;
    let argv = vec!["worktree".to_string(), "remove".to_string(), path.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("removed worktree {path}"), echo))
}

fn worktree_switch(action: &str, request: &ActionRequest) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    Ok(finish(
        action,
        format!("switch to worktree {path}"),
        format!("cd {path}"),
    ))
}

fn worktree_open(action: &str, request: &ActionRequest) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    Ok(finish(
        action,
        format!("open worktree {path} in terminal"),
        format!("cd {path}"),
    ))
}
