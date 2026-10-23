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
        "tag.delete" => Some(tag_delete(ctx, action, request, root)),
        "tag.push" => Some(tag_push(ctx, action, request, root)),
        "tag.checkout" => Some(tag_checkout(ctx, action, request, root)),
        _ => None,
    }
}

fn tag_delete(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let tag = param(request, "tag")?;
    require_confirm(request, &tag, action)?;
    let argv = vec!["tag".to_string(), "-d".to_string(), tag.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("deleted tag {tag}"), echo))
}

fn tag_push(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let tag = param(request, "tag")?;
    let remote = optional_param(request, "remote").unwrap_or_else(|| "origin".to_string());
    require_confirm(request, &tag, action)?;
    let argv = vec!["push".to_string(), remote.clone(), tag.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, true))?;
    Ok(finish(
        action,
        format!("pushed tag {tag} to {remote}"),
        echo,
    ))
}

fn tag_checkout(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let tag = param(request, "tag")?;
    let argv = vec!["checkout".to_string(), tag.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("checked out tag {tag}"), echo))
}
