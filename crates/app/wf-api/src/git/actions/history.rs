use crate::git::config::command_timeout_ms;
use crate::git::runner::run_git;
use crate::git::types::GitResult;
use crate::infra::context::ApiContext;

use crate::git::config::validate_repo_path;
use super::{ActionOutcome, ActionRequest, echo_of, finish, param, require_confirm};

pub fn handle(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> Option<GitResult<ActionOutcome>> {
    match action {
        "reset.soft" | "reset.mixed" | "reset.hard" => Some(reset(ctx, action, request, root)),
        "conflict.accept-ours" | "conflict.accept-theirs" => Some(conflict_accept(ctx, action, request, root)),
        _ => None,
    }
}

fn reset(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let commit = param(request, "commit")?;
    if action == "reset.hard" {
        require_confirm(request, &commit, action)?;
    }
    let flag = if action == "reset.soft" {
        "--soft"
    } else if action == "reset.mixed" {
        "--mixed"
    } else {
        "--hard"
    };
    let argv = vec!["reset".to_string(), flag.to_string(), commit.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("reset {flag} to {commit}"), echo))
}

fn conflict_accept(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    validate_repo_path(root, &path)?;
    let side = if action == "conflict.accept-ours" {
        "--ours"
    } else {
        "--theirs"
    };
    let argv = vec![
        "checkout".to_string(),
        side.to_string(),
        "--".to_string(),
        path.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    let argv = ["add".to_string(), "--".to_string(), path.clone()];
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("accepted {side} for {path}"), echo))
}