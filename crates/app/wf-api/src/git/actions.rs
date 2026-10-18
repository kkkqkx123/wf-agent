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
        return Err(GitError::Precondition(
            "branch must be 1..=256 chars".to_string(),
        ));
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
                vec![
                    "restore".to_string(),
                    "--staged".to_string(),
                    "--".to_string(),
                    path.clone(),
                ]
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
            let argv = vec![
                "restore".to_string(),
                "--staged".to_string(),
                ".".to_string(),
            ];
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
            let argv = vec![
                "commit".to_string(),
                "--amend".to_string(),
                "--no-edit".to_string(),
            ];
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
            let sub = if action == "stash.pop" {
                "pop"
            } else {
                "apply"
            };
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
            let argv = vec![
                "stash".to_string(),
                "branch".to_string(),
                branch.clone(),
                target.clone(),
            ];
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
            let argv = vec![
                "branch".to_string(),
                "-m".to_string(),
                from.clone(),
                to.clone(),
            ];
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
            let argv = vec![
                "revert".to_string(),
                "--no-edit".to_string(),
                commit.clone(),
            ];
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
            let argv = vec![
                "stash".to_string(),
                "push".to_string(),
                "-m".to_string(),
                "undo".to_string(),
            ];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish("stashed for undo".to_string(), echo))
        }
        "fetch" => {
            let remote = optional_param(request, "remote").unwrap_or_else(|| "origin".to_string());
            let argv = vec!["fetch".to_string(), remote.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, true))?;
            Ok(finish(format!("fetched {remote}"), echo))
        }
        "commit.reword" => {
            let commit = param(request, "commit")?;
            let message = param(request, "message")?;
            require_confirm(request, &commit, action)?;
            // Use git rebase with sequence editor to mark commit as reword,
            // and GIT_EDITOR to set the new message.
            let seq_script = format!(
                "#!/bin/sh\\nsed -i 's/^pick {commit}/reword {commit}/' \"$1\"\\n",
                commit = commit
            );
            let editor_script = format!(
                "#!/bin/sh\\nprintf '%s\\n' '{}' > \"$1\"\\n",
                message.replace('\'', "'\\''")
            );
            let seq_path =
                std::env::temp_dir().join(format!("seq-editor-{}.sh", std::process::id()));
            let editor_path =
                std::env::temp_dir().join(format!("msg-editor-{}.sh", std::process::id()));
            std::fs::write(&seq_path, &seq_script)
                .map_err(|e| GitError::Command(format!("failed to write seq editor: {e}")))?;
            std::fs::write(&editor_path, &editor_script)
                .map_err(|e| GitError::Command(format!("failed to write msg editor: {e}")))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                for p in [&seq_path, &editor_path] {
                    if let Ok(m) = std::fs::metadata(p) {
                        let mut perms = m.permissions();
                        perms.set_mode(0o755);
                        let _ = std::fs::set_permissions(p, perms);
                    }
                }
            }
            let base = format!("{commit}^");
            let envs = [
                ("GIT_SEQUENCE_EDITOR", seq_path.to_str().unwrap_or("true")),
                ("GIT_EDITOR", editor_path.to_str().unwrap_or("true")),
            ];
            let result = super::runner::run_git_with_env(
                &root,
                &["rebase", "-i", &base],
                command_timeout_ms(ctx, false),
                &envs,
            );
            let _ = std::fs::remove_file(&seq_path);
            let _ = std::fs::remove_file(&editor_path);
            result?;
            Ok(finish(
                format!("reworded {commit}"),
                format!("git rebase -i {base}"),
            ))
        }
        "commit.squash" => {
            let base = param(request, "base")?;
            require_confirm(request, &base, action)?;
            // Squash all commits from base..HEAD into base using reset --soft + commit
            let head_msg = run_git(
                &root,
                &["log", "-1", "--format=%s", "HEAD"],
                command_timeout_ms(ctx, false),
            )?
            .stdout
            .trim()
            .to_string();
            let argv = vec!["reset".to_string(), "--soft".to_string(), base.clone()];
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            let argv = vec!["commit".to_string(), "-m".to_string(), head_msg];
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(
                format!("squashed onto {base}"),
                format!("git reset --soft {base} && git commit"),
            ))
        }
        "commit.fixup" => {
            let commit = param(request, "commit")?;
            require_confirm(request, &commit, action)?;
            // Use git rebase with sequence editor to mark commit as fixup
            let seq_script = format!(
                "#!/bin/sh\\nsed -i 's/^pick {commit}/fixup {commit}/' \"$1\"\\n",
                commit = commit
            );
            let seq_path =
                std::env::temp_dir().join(format!("seq-editor-{}.sh", std::process::id()));
            std::fs::write(&seq_path, &seq_script)
                .map_err(|e| GitError::Command(format!("failed to write seq editor: {e}")))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(m) = std::fs::metadata(&seq_path) {
                    let mut perms = m.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&seq_path, perms);
                }
            }
            let base = format!("{commit}^");
            let envs = [("GIT_SEQUENCE_EDITOR", seq_path.to_str().unwrap_or("true"))];
            let result = super::runner::run_git_with_env(
                &root,
                &["rebase", "-i", &base],
                command_timeout_ms(ctx, false),
                &envs,
            );
            let _ = std::fs::remove_file(&seq_path);
            result?;
            Ok(finish(
                format!("fixup {commit}"),
                format!("git rebase -i {base}"),
            ))
        }
        "commit.drop" => {
            let commit = param(request, "commit")?;
            require_confirm(request, &commit, action)?;
            // Drop commit using rebase --onto
            let parent = format!("{commit}^");
            let argv = vec![
                "rebase".to_string(),
                "--onto".to_string(),
                parent.clone(),
                commit.clone(),
            ];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("dropped {commit}"), echo))
        }
        "stage.hunk" => {
            let path = param(request, "path")?;
            let hunk_index_str = param(request, "hunkIndex")?;
            validate_repo_path(&root, &path)?;
            // Get the unstaged diff for this file
            let out = run_git(
                &root,
                &["diff", "--no-color", "--unified=3", "--", &path],
                command_timeout_ms(ctx, false),
            )?;
            // Parse hunks and extract the target hunk
            let diff_text = out.stdout;
            let hunks: Vec<&str> = diff_text.split("@@").skip(1).collect();
            let idx: usize = hunk_index_str.parse().unwrap_or(0);
            if idx >= hunks.len() {
                return Err(GitError::Precondition(
                    "hunk index out of range".to_string(),
                ));
            }
            // Build a partial patch with just this hunk
            let hunk_text = format!("@@{}", hunks[idx]);
            // Write to temp file and apply
            let patch_path =
                std::env::temp_dir().join(format!("hunk-{}.patch", std::process::id()));
            let mut patch = String::new();
            // Add diff header
            patch.push_str(&format!("--- a/{}\\n", path));
            patch.push_str(&format!("+++ b/{}\\n", path));
            patch.push_str(&hunk_text);
            std::fs::write(&patch_path, &patch)
                .map_err(|e| GitError::Command(format!("failed to write patch: {e}")))?;
            let patch_str = patch_path.to_str().unwrap_or("").to_string();
            let argv = vec![
                "apply".to_string(),
                "--cached".to_string(),
                patch_str.clone(),
            ];
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            let result = run_git(&root, &refs, command_timeout_ms(ctx, false));
            let _ = std::fs::remove_file(&patch_path);
            result?;
            Ok(finish(
                format!("staged hunk {idx} of {path}"),
                format!("git apply --cached <patch>"),
            ))
        }
        "remote.add" => {
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
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("added remote {name}"), echo))
        }
        "remote.remove" => {
            let name = param(request, "name")?;
            require_confirm(request, &name, action)?;
            let argv = vec!["remote".to_string(), "remove".to_string(), name.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("removed remote {name}"), echo))
        }
        "remote.set-url" => {
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
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("set url for {name}"), echo))
        }
        "tag.delete" => {
            let tag = param(request, "tag")?;
            require_confirm(request, &tag, action)?;
            let argv = vec!["tag".to_string(), "-d".to_string(), tag.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("deleted tag {tag}"), echo))
        }
        "tag.push" => {
            let tag = param(request, "tag")?;
            let remote = optional_param(request, "remote").unwrap_or_else(|| "origin".to_string());
            require_confirm(request, &tag, action)?;
            let argv = vec!["push".to_string(), remote.clone(), tag.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, true))?;
            Ok(finish(format!("pushed tag {tag} to {remote}"), echo))
        }
        "tag.checkout" => {
            let tag = param(request, "tag")?;
            let argv = vec!["checkout".to_string(), tag.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("checked out tag {tag}"), echo))
        }
        "submodule.init" => {
            let path = param(request, "path")?;
            validate_repo_path(&root, &path)?;
            let argv = vec!["submodule".to_string(), "init".to_string(), path.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("initialized submodule {path}"), echo))
        }
        "submodule.update" => {
            let path = param(request, "path")?;
            validate_repo_path(&root, &path)?;
            let argv = vec!["submodule".to_string(), "update".to_string(), path.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("updated submodule {path}"), echo))
        }
        "submodule.add" => {
            let path = param(request, "path")?;
            let url = param(request, "url")?;
            validate_repo_path(&root, &path)?;
            let argv = vec![
                "submodule".to_string(),
                "add".to_string(),
                url.clone(),
                path.clone(),
            ];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("added submodule {path}"), echo))
        }
        "submodule.set-url" => {
            let path = param(request, "path")?;
            let url = param(request, "url")?;
            validate_repo_path(&root, &path)?;
            let argv = vec![
                "submodule".to_string(),
                "set-url".to_string(),
                path.clone(),
                url.clone(),
            ];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("set url for submodule {path}"), echo))
        }
        "submodule.remove" => {
            let path = param(request, "path")?;
            require_confirm(request, &path, action)?;
            validate_repo_path(&root, &path)?;
            // deinit + rm
            let argv = vec![
                "submodule".to_string(),
                "deinit".to_string(),
                "-f".to_string(),
                path.clone(),
            ];
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            let _ = run_git(&root, &refs, command_timeout_ms(ctx, false));
            let argv = vec!["rm".to_string(), "-f".to_string(), path.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("removed submodule {path}"), echo))
        }
        "submodule.enter" => {
            // Readonly: frontend handles context switch
            Ok(finish(
                "enter submodule context".to_string(),
                "cd <submodule>".to_string(),
            ))
        }
        "worktree.add" => {
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
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("added worktree {path}"), echo))
        }
        "worktree.remove" => {
            let path = param(request, "path")?;
            require_confirm(request, &path, action)?;
            let argv = vec!["worktree".to_string(), "remove".to_string(), path.clone()];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("removed worktree {path}"), echo))
        }
        "worktree.switch" => {
            let path = param(request, "path")?;
            // Frontend handles context switch; backend just validates
            Ok(finish(
                format!("switch to worktree {path}"),
                format!("cd {path}"),
            ))
        }
        "worktree.open" => {
            let path = param(request, "path")?;
            Ok(finish(
                format!("open worktree {path} in terminal"),
                format!("cd {path}"),
            ))
        }
        "reset.soft" | "reset.mixed" | "reset.hard" => {
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
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("reset {flag} to {commit}"), echo))
        }
        "stash.rename" => {
            let index = param(request, "index")?;
            let message = param(request, "message")?;
            let idx: usize = index.trim().parse().unwrap_or(0);
            let target = format!("stash@{{{idx}}}");
            let commit = run_git(
                &root,
                &["rev-parse", &target],
                command_timeout_ms(ctx, false),
            )?
            .stdout
            .trim()
            .to_string();
            // Drop the old stash entry, then re-store it under the new message.
            let _ = run_git(
                &root,
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
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("renamed stash {target}"), echo))
        }
        "branch.set-upstream" => {
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
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(
                format!("set upstream for {branch} to {upstream}"),
                echo,
            ))
        }
        "branch.unset-upstream" => {
            let branch = param(request, "branch")?;
            validate_branch_name(&branch)?;
            let argv = vec![
                "branch".to_string(),
                "--unset-upstream".to_string(),
                branch.clone(),
            ];
            let echo = echo_of(action, &argv);
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("unset upstream for {branch}"), echo))
        }
        "conflict.accept-ours" | "conflict.accept-theirs" => {
            let path = param(request, "path")?;
            validate_repo_path(&root, &path)?;
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
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            let argv = vec!["add".to_string(), "--".to_string(), path.clone()];
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            run_git(&root, &refs, command_timeout_ms(ctx, false))?;
            Ok(finish(format!("accepted {side} for {path}"), echo))
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
        assert!(matches!(
            err,
            GitError::NotEnabled | GitError::Precondition(_) | GitError::ConfirmRequired(_)
        ));
    }
}
