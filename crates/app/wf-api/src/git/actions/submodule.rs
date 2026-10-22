use crate::git::config::{command_timeout_ms, validate_repo_path};
use crate::git::runner::run_git;
use crate::git::types::GitResult;
use crate::infra::context::ApiContext;

use super::{ActionOutcome, ActionRequest, echo_of, finish, param, require_confirm};

pub fn handle(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> Option<GitResult<ActionOutcome>> {
    match action {
        "submodule.init" => Some(submodule_init(ctx, action, request, root)),
        "submodule.update" => Some(submodule_update(ctx, action, request, root)),
        "submodule.add" => Some(submodule_add(ctx, action, request, root)),
        "submodule.set-url" => Some(submodule_set_url(ctx, action, request, root)),
        "submodule.remove" => Some(submodule_remove(ctx, action, request, root)),
        "submodule.enter" => Some(submodule_enter(action)),
        _ => None,
    }
}

fn submodule_init(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    validate_repo_path(root, &path)?;
    let argv = vec!["submodule".to_string(), "init".to_string(), path.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("initialized submodule {path}"), echo))
}

fn submodule_update(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    validate_repo_path(root, &path)?;
    let argv = vec!["submodule".to_string(), "update".to_string(), path.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("updated submodule {path}"), echo))
}

fn submodule_add(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    let url = param(request, "url")?;
    validate_repo_path(root, &path)?;
    let argv = vec![
        "submodule".to_string(),
        "add".to_string(),
        url.clone(),
        path.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("added submodule {path}"), echo))
}

fn submodule_set_url(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    let url = param(request, "url")?;
    validate_repo_path(root, &path)?;
    let argv = vec![
        "submodule".to_string(),
        "set-url".to_string(),
        path.clone(),
        url.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("set url for submodule {path}"), echo))
}

fn submodule_remove(ctx: &ApiContext, action: &str, request: &ActionRequest, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    require_confirm(request, &path, action)?;
    validate_repo_path(root, &path)?;
    let argv = [
        "submodule".to_string(),
        "deinit".to_string(),
        "-f".to_string(),
        path.clone(),
    ];
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let _ = run_git(root, &refs, command_timeout_ms(ctx, false));
    let argv = vec!["rm".to_string(), "-f".to_string(), path.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("removed submodule {path}"), echo))
}

fn submodule_enter(action: &str) -> GitResult<ActionOutcome> {
    Ok(finish(
        action,
        "enter submodule context".to_string(),
        "cd <submodule>".to_string(),
    ))
}