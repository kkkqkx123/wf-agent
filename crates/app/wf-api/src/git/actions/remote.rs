use crate::git::config::command_timeout_ms;
use crate::git::runner::run_git;
use crate::git::types::{GitError, GitResult};
use crate::infra::context::ApiContext;

use super::{ActionOutcome, ActionRequest, echo_of, finish, optional_param, param, require_confirm};

pub fn handle(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> Option<GitResult<ActionOutcome>> {
    match action {
        "push" => Some(push(ctx, action, request, root)),
        "pull" => Some(pull(ctx, action, request, root)),
        "fetch" => Some(fetch(ctx, action, request, root)),
        "remote.add" => Some(remote_add(ctx, action, request, root)),
        "remote.remove" => Some(remote_remove(ctx, action, request, root)),
        "remote.set-url" => Some(remote_set_url(ctx, action, request, root)),
        _ => None,
    }
}

fn push(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let remote = optional_param(request, "remote").unwrap_or_else(|| "origin".to_string());
    let branch = optional_param(request, "branch").unwrap_or_default();
    let target = if branch.is_empty() {
        remote.clone()
    } else {
        format!("{remote} {branch}")
    };
    require_confirm(request, &target, action)?;
    let mut argv = vec!["push".to_string(), remote.clone()];
    if !branch.is_empty() {
        argv.push(branch.clone());
    }
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, true))?;
    Ok(finish(action, format!("pushed {target}"), echo))
}

fn pull(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let remote = optional_param(request, "remote").unwrap_or_else(|| "origin".to_string());
    let branch = optional_param(request, "branch").unwrap_or_default();
    let mut argv = vec!["pull".to_string(), "--ff-only".to_string(), remote.clone()];
    if !branch.is_empty() {
        argv.push(branch.clone());
    }
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, true))?;
    Ok(finish(action, "pulled".to_string(), echo))
}

fn fetch(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let remote = optional_param(request, "remote").unwrap_or_else(|| "origin".to_string());
    let argv = vec!["fetch".to_string(), remote.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, true))?;
    Ok(finish(action, format!("fetched {remote}"), echo))
}

fn remote_add(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let name = param(request, "name")?;
    let url = param(request, "url")?;
    if name.is_empty() || name.contains(' ') || name.contains('\0') {
        return Err(GitError::Precondition("invalid remote name".to_string()));
    }
    let argv = vec![
        "remote".to_string(),
        "add".to_string(),
        name.clone(),
        url.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("added remote {name}"), echo))
}

fn remote_remove(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let name = param(request, "name")?;
    require_confirm(request, &name, action)?;
    let argv = vec!["remote".to_string(), "remove".to_string(), name.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("removed remote {name}"), echo))
}

fn remote_set_url(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let name = param(request, "name")?;
    let url = param(request, "url")?;
    let argv = vec![
        "remote".to_string(),
        "set-url".to_string(),
        name.clone(),
        url.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("set url for {name}"), echo))
}