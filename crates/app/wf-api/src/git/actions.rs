use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::config::repo_root;
use super::registry::{acquire_write_lock, action_spec};
use super::types::{DangerLevel, GitError, GitResult};
use crate::infra::context::ApiContext;

mod branch;
mod commit;
mod history;
mod ignore;
mod remote;
mod staging;
mod stash;
mod submodule;
mod tag;
mod worktree;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRequest {
    pub action: String,
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub confirm: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionOutcome {
    pub action: String,
    pub ok: bool,
    pub message: String,
    pub echo: String,
}

pub(crate) fn param(request: &ActionRequest, key: &str) -> GitResult<String> {
    request
        .params
        .get(key)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| GitError::Precondition(format!("missing param '{key}'")))
}

pub(crate) fn optional_param(request: &ActionRequest, key: &str) -> Option<String> {
    request
        .params
        .get(key)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub(crate) fn require_confirm(
    request: &ActionRequest,
    expected: &str,
    action: &str,
) -> GitResult<()> {
    match request.confirm.as_deref().map(str::trim) {
        Some(confirm) if confirm == expected && !expected.is_empty() => Ok(()),
        _ => Err(GitError::ConfirmRequired(action.to_string())),
    }
}

pub(crate) fn validate_branch_name(name: &str) -> GitResult<()> {
    if name.is_empty() || name.len() > 256 {
        return Err(GitError::Precondition(
            "branch must be 1..=256 chars".to_string(),
        ));
    }
    if name.contains('\0') || name.contains("..") || name.contains(' ') || name.starts_with('-') {
        return Err(GitError::Precondition(format!("invalid branch '{name}'")));
    }
    Ok(())
}

pub(crate) fn echo_of(action: &str, argv: &[String]) -> String {
    let mut parts = vec!["git".to_string()];
    parts.extend(argv.iter().cloned());
    let _ = action;
    parts.join(" ")
}

pub(crate) fn finish(action: &str, message: String, echo: String) -> ActionOutcome {
    ActionOutcome {
        action: action.to_string(),
        ok: true,
        message,
        echo,
    }
}

pub fn execute_action(ctx: &ApiContext, request: &ActionRequest) -> GitResult<ActionOutcome> {
    let action = request.action.trim();
    if action.is_empty() {
        return Err(GitError::UnknownAction(String::new()));
    }
    let spec = action_spec(action)?;
    if !spec.available {
        return Err(GitError::Unavailable(
            spec.unavailable_reason
                .unwrap_or("action unavailable")
                .to_string(),
        ));
    }
    if spec.danger == DangerLevel::Readonly && action == "refresh" {
        let fp = super::queries::fingerprint(ctx).map_err(crate::ApiError::from);
        return match fp {
            Ok(_) => Ok(ActionOutcome {
                action: action.to_string(),
                ok: true,
                message: "refreshed".to_string(),
                echo: "git status --porcelain=v2 --branch".to_string(),
            }),
            Err(e) => Err(GitError::Command(e.to_string())),
        };
    }
    if spec.danger == DangerLevel::Readonly {
        return Err(GitError::Precondition(format!(
            "readonly action '{action}' is not dispatched through the write entry"
        )));
    }
    let _guard = acquire_write_lock();
    dispatch_write(ctx, action, request)
}

fn dispatch_write(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
) -> GitResult<ActionOutcome> {
    let root = repo_root(ctx)?;
    if let Some(res) = staging::handle(ctx, action, request, &root) {
        return res;
    }
    if let Some(res) = commit::handle(ctx, action, request, &root) {
        return res;
    }
    if let Some(res) = stash::handle(ctx, action, request, &root) {
        return res;
    }
    if let Some(res) = branch::handle(ctx, action, request, &root) {
        return res;
    }
    if let Some(res) = history::handle(ctx, action, request, &root) {
        return res;
    }
    if let Some(res) = remote::handle(ctx, action, request, &root) {
        return res;
    }
    if let Some(res) = tag::handle(ctx, action, request, &root) {
        return res;
    }
    if let Some(res) = submodule::handle(ctx, action, request, &root) {
        return res;
    }
    if let Some(res) = worktree::handle(ctx, action, request, &root) {
        return res;
    }
    if let Some(res) = ignore::handle(ctx, action, request, &root) {
        return res;
    }
    Err(GitError::UnknownAction(action.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(action: &str) -> ActionRequest {
        ActionRequest {
            action: action.to_string(),
            params: HashMap::new(),
            confirm: None,
        }
    }

    #[tokio::test]
    async fn unknown_action_rejected() {
        let ctx = ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            std::sync::Arc::new(wf_resource::registry::ResourceRegistries::new()),
        );
        let err = execute_action(&ctx, &request("nope.missing")).unwrap_err();
        assert!(matches!(err, GitError::UnknownAction(_)));
    }

    #[tokio::test]
    async fn missing_param_and_confirm_rejected_without_touching_git() {
        let ctx = ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            std::sync::Arc::new(wf_resource::registry::ResourceRegistries::new()),
        );
        let err = execute_action(&ctx, &request("branch.delete")).unwrap_err();
        assert!(matches!(
            err,
            GitError::NotEnabled | GitError::Precondition(_) | GitError::ConfirmRequired(_)
        ));
    }
}
