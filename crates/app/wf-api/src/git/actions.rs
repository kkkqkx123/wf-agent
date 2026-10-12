use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::config::{command_timeout_ms, repo_root, validate_repo_path};
use super::registry::{acquire_write_lock, action_spec};
use super::runner::run_git;
use super::types::{DangerLevel, GitError, GitResult};
use crate::infra::context::ApiContext;

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

fn param(request: &ActionRequest, key: &str) -> GitResult<String> {
    request
        .params
        .get(key)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| GitError::Precondition(format!("missing param '{key}'")))
}

fn optional_param(request: &ActionRequest, key: &str) -> Option<String> {
    request
        .params
        .get(key)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn require_confirm(request: &ActionRequest, expected: &str, action: &str) -> GitResult<()> {
    match request.confirm.as_deref().map(str::trim) {
        Some(confirm) if confirm == expected && !expected.is_empty() => Ok(()),
        _ => Err(GitError::ConfirmRequired(action.to_string())),
    }
}

fn validate_branch_name(name: &str) -> GitResult<()> {
    if name.is_empty() || name.len() > 256 {
        return Err(GitError::Precondition("branch must be 1..=256 chars".to_string()));
    }
    if name.contains('\0') || name.contains("..") || name.contains(' ') || name.starts_with('-') {
        return Err(GitError::Precondition(format!("invalid branch '{name}'")));
    }
    Ok(())
}

fn echo_of(action: &str, argv: &[String]) -> String {
    let mut parts = vec!["git".to_string()];
    parts.extend(argv.iter().cloned());
    let _ = action;
    parts.join(" ")
}

pub fn execute_action(ctx: &ApiContext, request: &ActionRequest) -> GitResult<ActionOutcome> {
    let action = request.action.trim();
    if action.is_empty() {
        return Err(GitError::UnknownAction(String::new()));
    }
    let spec = action_spec(action)?;
    if !spec.available {
        return Err(GitError::Unavailable(
            spec.unavailable_reason.unwrap_or("action unavailable").to_string(),
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
    let spec = action_spec(action)?;
    let finish = |message: String, echo: String| ActionOutcome {
        action: action.to_string(),
        ok: true,
        message,
        echo,
    };
    match action {
        "stage.toggle" => {
            let path = param(request, "path")?;
            validate_repo_path(&root, &path)?;
            let staged_check = run_git(
                &root,
                &["diff", "--cached", "--name-only", "--", &path],
                command_timeout_ms(ctx, false),
            )?;
            let argv = if staged_check.stdout.lines().any(|l| l.trim() == path) {
                vec!["restore".to_string(), "--staged".to_string(), "--".to_string(), path.clone()]
            } else {
                vec!["add".to_string(), "--".to_string(), path.clone()]
            };
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("toggled {path}"), echo))
        }
        "stage.all" => {
            let argv = vec!["add".to_string(), "-A".to_string()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish("staged all".to_string(), echo))
        }
        "unstage.all" => {
            let argv = vec!["restore".to_string(), "--staged".to_string(), ".".to_string()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish("unstaged all".to_string(), echo))
        }
        "commit" | "commit.direct" => {
            let message = param(request, "message")?;
            if message.len() > 4096 {
                return Err(GitError::Precondition("message too long".to_string()));
            }
            let argv = vec!["commit".to_string(), "-m".to_string(), message.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("committed: {message}"), echo))
        }
        "commit.amend" => {
            require_confirm(request, "amend", action)?;
            let argv = vec!["commit".to_string(), "--amend".to_string(), "--no-edit".to_string()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish("amended".to_string(), echo))
        }
        "discard" => {
            let path = param(request, "path")?;
            require_confirm(request, &path, action)?;
            validate_repo_path(&root, &path)?;
            let argv = vec![
                "restore".to_string(),
                "--source=HEAD".to_string(),
                "--staged".to_string(),
                "--worktree".to_string(),
                "--".to_string(),
                path.clone(),
            ];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("discarded {path}"), echo))
        }
        "stash.push" => {
            let message = optional_param(request, "message").unwrap_or_default();
            let mut argv = vec!["stash".to_string(), "push".to_string()];
            if !message.is_empty() {
                argv.push("-m".to_string());
                argv.push(message.clone());
            }
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish("stashed".to_string(), echo))
        }
        "stash.pop" | "stash.apply" => {
            let index = optional_param(request, "index").unwrap_or_else(|| "0".to_string());
            let target = format!("stash@{{{}}}", index.trim().parse::<usize>().unwrap_or(0));
            let sub = if action == "stash.pop" { "pop" } else { "apply" };
            let argv = vec!["stash".to_string(), sub.to_string(), target.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("{sub} {target}"), echo))
        }
        "stash.drop" => {
            let index = param(request, "index")?;
            let target = format!("stash@{{{}}}", index.trim().parse::<usize>().unwrap_or(0));
            require_confirm(request, &target, action)?;
            let argv = vec!["stash".to_string(), "drop".to_string(), target.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("dropped {target}"), echo))
        }
        "stash.to-branch" => {
            let branch = param(request, "branch")?;
            validate_branch_name(&branch)?;
            let index = optional_param(request, "index").unwrap_or_else(|| "0".to_string());
            let target = format!("stash@{{{}}}", index.trim().parse::<usize>().unwrap_or(0));
            let argv = vec!["stash".to_string(), "branch".to_string(), branch.clone(), target.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("stash {target} to {branch}"), echo))
        }
        "ignore" => {
            let path = param(request, "path")?;
            validate_repo_path(&root, &path)?;
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
                format!("ignored {path}"),
                format!("echo {path} >> .gitignore"),
            ))
        }
        "branch.checkout" => {
            let branch = param(request, "branch")?;
            validate_branch_name(&branch)?;
            let argv = vec!["checkout".to_string(), branch.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("checked out {branch}"), echo))
        }
        "branch.create" => {
            let branch = param(request, "branch")?;
            validate_branch_name(&branch)?;
            let argv = vec!["branch".to_string(), branch.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("created {branch}"), echo))
        }
        "branch.delete" => {
            let branch = param(request, "branch")?;
            require_confirm(request, &branch, action)?;
            validate_branch_name(&branch)?;
            let argv = vec!["branch".to_string(), "-d".to_string(), branch.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("deleted {branch}"), echo))
        }
        "branch.rename" => {
            let from = param(request, "from")?;
            let to = param(request, "to")?;
            validate_branch_name(&from)?;
            validate_branch_name(&to)?;
            let argv = vec!["branch".to_string(), "-m".to_string(), from.clone(), to.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("renamed {from} to {to}"), echo))
        }
        "branch.merge" => {
            let branch = param(request, "branch")?;
            validate_branch_name(&branch)?;
            let argv = vec!["merge".to_string(), "--no-edit".to_string(), branch.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("merged {branch}"), echo))
        }
        "branch.rebase" => {
            let onto = param(request, "onto")?;
            require_confirm(request, &onto, action)?;
            validate_branch_name(&onto)?;
            let argv = vec!["rebase".to_string(), onto.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("rebased onto {onto}"), echo))
        }
        "branch.fast-forward" => {
            let branch = param(request, "branch")?;
            validate_branch_name(&branch)?;
            let argv = vec!["merge".to_string(), "--ff-only".to_string(), branch.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("fast-forwarded {branch}"), echo))
        }
        "commit.revert" => {
            let commit = param(request, "commit")?;
            let argv = vec!["revert".to_string(), "--no-edit".to_string(), commit.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("reverted {commit}"), echo))
        }
        "commit.cherry-pick" => {
            let commit = param(request, "commit")?;
            let argv = vec!["cherry-pick".to_string(), commit.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("cherry-picked {commit}"), echo))
        }
        "commit.to-branch" => {
            let commit = param(request, "commit")?;
            let branch = param(request, "branch")?;
            validate_branch_name(&branch)?;
            let argv = vec!["branch".to_string(), branch.clone(), commit.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("branch {branch} at {commit}"), echo))
        }
        "commit.to-tag" => {
            let commit = param(request, "commit")?;
            let tag = param(request, "tag")?;
            if tag.trim().is_empty() || tag.contains('\0') || tag.contains(' ') {
                return Err(GitError::Precondition("invalid tag".to_string()));
            }
            let argv = vec!["tag".to_string(), tag.clone(), commit.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("tagged {tag} at {commit}"), echo))
        }
        "push" => {
            let remote = optional_param(request, "remote").unwrap_or_else(|| "origin".to_string());
            let branch = optional_param(request, "branch").unwrap_or_default();
            let target = if branch.is_empty() { remote.clone() } else { format!("{remote} {branch}") };
            require_confirm(request, &target, action)?;
            let mut argv = vec!["push".to_string(), remote.clone()];
            if !branch.is_empty() {
                argv.push(branch.clone());
            }
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, true))?;
            Ok(finish(format!("pushed {target}"), echo))
        }
        "pull" => {
            let remote = optional_param(request, "remote").unwrap_or_else(|| "origin".to_string());
            let branch = optional_param(request, "branch").unwrap_or_default();
            let mut argv = vec!["pull".to_string(), "--ff-only".to_string(), remote.clone()];
            if !branch.is_empty() {
                argv.push(branch.clone());
            }
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, true))?;
            Ok(finish("pulled".to_string(), echo))
        }
        "undo" => {
            let argv = vec!["stash".to_string(), "push".to_string(), "-m".to_string(), "undo".to_string()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish("stashed for undo".to_string(), echo))
        }
        _ => {
            let _ = spec;
            Err(GitError::UnknownAction(action.to_string()))
        }
    }
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
        assert!(matches!(err, GitError::NotEnabled | GitError::Precondition(_) | GitError::ConfirmRequired(_)));
    }
}
