use crate::git::config::command_timeout_ms;
use crate::git::runner::run_git;
use crate::git::types::GitResult;
use crate::infra::context::ApiContext;

use super::{
    echo_of, finish, optional_param, param, require_confirm, ActionOutcome, ActionRequest,
};

pub fn handle(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> Option<GitResult<ActionOutcome>> {
    match action {
        "stash.push" => Some(stash_push(ctx, action, request, root)),
        "stash.pop" | "stash.apply" => Some(stash_pop_apply(ctx, action, request, root)),
        "stash.drop" => Some(stash_drop(ctx, action, request, root)),
        "stash.to-branch" => Some(stash_to_branch(ctx, action, request, root)),
        "stash.rename" => Some(stash_rename(ctx, action, request, root)),
        "undo" => Some(undo(ctx, action, root)),
        _ => None,
    }
}

fn stash_push(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let message = optional_param(request, "message").unwrap_or_default();
    let mut argv = vec!["stash".to_string(), "push".to_string()];
    if !message.is_empty() {
        argv.push("-m".to_string());
        argv.push(message.clone());
    }
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, "stashed".to_string(), echo))
}

fn stash_pop_apply(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let index = optional_param(request, "index").unwrap_or_else(|| "0".to_string());
    let target = format!("stash@{{{}}}", index.trim().parse::<usize>().unwrap_or(0));
    let sub = if action == "stash.pop" {
        "pop"
    } else {
        "apply"
    };
    let argv = vec!["stash".to_string(), sub.to_string(), target.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("{sub} {target}"), echo))
}

fn stash_drop(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let index = param(request, "index")?;
    let target = format!("stash@{{{}}}", index.trim().parse::<usize>().unwrap_or(0));
    require_confirm(request, &target, action)?;
    let argv = vec!["stash".to_string(), "drop".to_string(), target.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("dropped {target}"), echo))
}

fn stash_to_branch(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let branch = param(request, "branch")?;
    super::validate_branch_name(&branch)?;
    let index = optional_param(request, "index").unwrap_or_else(|| "0".to_string());
    let target = format!("stash@{{{}}}", index.trim().parse::<usize>().unwrap_or(0));
    let argv = vec![
        "stash".to_string(),
        "branch".to_string(),
        branch.clone(),
        target.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("stash {target} to {branch}"), echo))
}

fn stash_rename(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let index = param(request, "index")?;
    let message = param(request, "message")?;
    let idx: usize = index.trim().parse().unwrap_or(0);
    let target = format!("stash@{{{idx}}}");
    let commit = run_git(
        root,
        &["rev-parse", &target],
        command_timeout_ms(ctx, false),
    )?
    .stdout
    .trim()
    .to_string();
    let _ = run_git(
        root,
        &["stash", "drop", &target],
        command_timeout_ms(ctx, false),
    );
    let argv = vec![
        "stash".to_string(),
        "store".to_string(),
        "-m".to_string(),
        message.clone(),
        commit,
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("renamed stash {target}"), echo))
}

fn undo(ctx: &ApiContext, action: &str, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let argv = vec![
        "stash".to_string(),
        "push".to_string(),
        "-m".to_string(),
        "undo".to_string(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, "stashed for undo".to_string(), echo))
}
