use crate::git::config::validate_repo_path;
use crate::git::types::{GitError, GitResult};
use crate::infra::context::ApiContext;

use super::{ActionOutcome, ActionRequest, finish, param};

pub fn handle(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> Option<GitResult<ActionOutcome>> {
    if action != "ignore" {
        return None;
    }
    Some(ignore(ctx, action, request, root))
}

fn ignore(
    _ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    validate_repo_path(root, &path)?;
    let ignore_path = root.join(".gitignore");
    let mut existing = std::fs::read_to_string(&ignore_path).unwrap_or_default();
    if !existing.ends_with('\n') && !existing.is_empty() {
        existing.push('\n');
    }
    existing.push_str(&path);
    existing.push('\n');
    std::fs::write(&ignore_path, existing)
        .map_err(|e| GitError::Command(format!("failed to update .gitignore: {e}")))?;
    Ok(finish(
        action,
        format!("ignored {path}"),
        format!("echo {path} >> .gitignore"),
    ))
}