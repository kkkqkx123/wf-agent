use crate::git::config::command_timeout_ms;
use crate::git::runner::run_git;
use crate::git::types::GitResult;
use crate::infra::context::ApiContext;

use super::{ActionOutcome, ActionRequest, echo_of, finish, param, require_confirm, validate_branch_name};

pub fn handle(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> Option<GitResult<ActionOutcome>> {
    match action {
        "branch.checkout" => Some(branch_checkout(ctx, action, request, root)),
        "branch.create" => Some(branch_create(ctx, action, request, root)),
        "branch.delete" => Some(branch_delete(ctx, action, request, root)),
        "branch.rename" => Some(branch_rename(ctx, action, request, root)),
        "branch.merge" => Some(branch_merge(ctx, action, request, root)),
        "branch.rebase" => Some(branch_rebase(ctx, action, request, root)),
        "branch.fast-forward" => Some(branch_fast_forward(ctx, action, request, root)),
        "branch.set-upstream" => Some(branch_set_upstream(ctx, action, request, root)),
        "branch.unset-upstream" => Some(branch_unset_upstream(ctx, action, request, root)),
        _ => None,
    }
}

fn branch_checkout(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let branch = param(request, "branch")?;
    validate_branch_name(&branch)?;
    let argv = vec!["checkout".to_string(), branch.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("checked out {branch}"), echo))
}

fn branch_create(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let branch = param(request, "branch")?;
    validate_branch_name(&branch)?;
    let argv = vec!["branch".to_string(), branch.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("created {branch}"), echo))
}

fn branch_delete(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let branch = param(request, "branch")?;
    require_confirm(request, &branch, action)?;
    validate_branch_name(&branch)?;
    let argv = vec!["branch".to_string(), "-d".to_string(), branch.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("deleted {branch}"), echo))
}

fn branch_rename(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let from = param(request, "from")?;
    let to = param(request, "to")?;
    validate_branch_name(&from)?;
    validate_branch_name(&to)?;
    let argv = vec![
        "branch".to_string(),
        "-m".to_string(),
        from.clone(),
        to.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("renamed {from} to {to}"), echo))
}

fn branch_merge(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let branch = param(request, "branch")?;
    validate_branch_name(&branch)?;
    let argv = vec!["merge".to_string(), "--no-edit".to_string(), branch.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("merged {branch}"), echo))
}

fn branch_rebase(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let onto = param(request, "onto")?;
    require_confirm(request, &onto, action)?;
    validate_branch_name(&onto)?;
    let argv = vec!["rebase".to_string(), onto.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("rebased onto {onto}"), echo))
}

fn branch_fast_forward(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let branch = param(request, "branch")?;
    validate_branch_name(&branch)?;
    let argv = vec!["merge".to_string(), "--ff-only".to_string(), branch.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("fast-forwarded {branch}"), echo))
}

fn branch_set_upstream(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let branch = param(request, "branch")?;
    let remote = param(request, "remote")?;
    validate_branch_name(&branch)?;
    let upstream = format!("{remote}/{branch}");
    let argv = vec![
        "branch".to_string(),
        format!("--set-upstream-to={upstream}"),
        branch.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("set upstream for {branch} to {upstream}"), echo))
}

fn branch_unset_upstream(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let branch = param(request, "branch")?;
    validate_branch_name(&branch)?;
    let argv = vec![
        "branch".to_string(),
        "--unset-upstream".to_string(),
        branch.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("unset upstream for {branch}"), echo))
}