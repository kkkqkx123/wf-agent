use serde::{Deserialize, Serialize};

use crate::git::config::command_timeout_ms;
use crate::git::runner::run_git;
use crate::git::types::{GitError, GitResult};
use crate::infra::context::ApiContext;

use super::{echo_of, finish, ActionOutcome, ActionRequest};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct RedoEntry {
    sha: String,
    branch: String,
    checkout: bool,
}

fn redo_stack_path(git_dir: &std::path::Path) -> std::path::PathBuf {
    git_dir.join("wf-git-redo-stack")
}

fn resolve_git_dir(ctx: &ApiContext, root: &std::path::Path) -> GitResult<std::path::PathBuf> {
    let out = run_git(
        root,
        &["rev-parse", "--absolute-git-dir"],
        command_timeout_ms(ctx, false),
    )?;
    let raw = out.stdout.trim().to_string();
    if raw.is_empty() {
        return Err(GitError::Command("cannot resolve git dir".to_string()));
    }
    let path = std::path::PathBuf::from(&raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(root.join(path))
    }
}

fn current_head(root: &std::path::Path, timeout: u64) -> GitResult<String> {
    match run_git(root, &["rev-parse", "HEAD"], timeout) {
        Ok(out) => {
            let sha = out.stdout.trim().to_string();
            if sha.is_empty() {
                Err(GitError::Precondition("nothing to undo".to_string()))
            } else {
                Ok(sha)
            }
        }
        Err(_) => Err(GitError::Precondition("nothing to undo".to_string())),
    }
}

fn current_branch(root: &std::path::Path, timeout: u64) -> String {
    run_git(root, &["branch", "--show-current"], timeout)
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_default()
}

fn guard_mid_operation(git_dir: &std::path::Path) -> GitResult<()> {
    if git_dir.join("MERGE_HEAD").exists() {
        return Err(GitError::Precondition(
            "merge in progress; resolve it before undo/redo".to_string(),
        ));
    }
    if git_dir.join("CHERRY_PICK_HEAD").exists() {
        return Err(GitError::Precondition(
            "cherry-pick in progress; resolve it before undo/redo".to_string(),
        ));
    }
    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        return Err(GitError::Precondition(
            "rebase in progress; resolve it before undo/redo".to_string(),
        ));
    }
    Ok(())
}

fn load_redo_stack(git_dir: &std::path::Path) -> Vec<RedoEntry> {
    let path = redo_stack_path(git_dir);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if text.trim().is_empty() {
        return Vec::new();
    }
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_redo_stack(git_dir: &std::path::Path, stack: &[RedoEntry]) -> GitResult<()> {
    let path = redo_stack_path(git_dir);
    let text = serde_json::to_string(stack)
        .map_err(|e| GitError::Command(format!("failed to encode redo stack: {e}")))?;
    std::fs::write(&path, text)
        .map_err(|e| GitError::Command(format!("failed to write redo stack: {e}")))?;
    Ok(())
}

pub(crate) fn clear_redo_best_effort(ctx: &ApiContext, root: &std::path::Path) {
    let timeout = command_timeout_ms(ctx, false);
    let git_dir = match resolve_git_dir(ctx, root) {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let _ = std::fs::remove_file(redo_stack_path(&git_dir));
    let _ = timeout;
}

pub(crate) fn clear_redo_after_write(ctx: &ApiContext) {
    let root = match super::super::config::repo_root(ctx) {
        Ok(root) => root,
        Err(_) => return,
    };
    clear_redo_best_effort(ctx, &root);
}

fn parse_checkout_source(message: &str) -> Option<String> {
    let rest = message.strip_prefix("checkout: moving from ")?;
    let (from, _) = rest.split_once(" to ")?;
    let from = from.trim().to_string();
    if from.is_empty() {
        None
    } else {
        Some(from)
    }
}

fn short_of(sha: &str) -> String {
    sha.chars().take(7).collect()
}

fn ensure_object(root: &std::path::Path, timeout: u64, sha: &str) -> GitResult<()> {
    run_git(root, &["cat-file", "-e", sha], timeout).map_err(|_| {
        GitError::Precondition(format!(
            "reflog target {} is no longer available",
            short_of(sha)
        ))
    })?;
    Ok(())
}

pub fn handle(
    ctx: &ApiContext,
    action: &str,
    _request: &ActionRequest,
    root: &std::path::Path,
) -> Option<GitResult<ActionOutcome>> {
    match action {
        "undo" => Some(undo(ctx, root)),
        "redo" => Some(redo(ctx, root)),
        _ => None,
    }
}

fn undo(ctx: &ApiContext, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let timeout = command_timeout_ms(ctx, false);
    let git_dir = resolve_git_dir(ctx, root)?;
    guard_mid_operation(&git_dir)?;
    let cur_sha = current_head(root, timeout)?;
    let cur_branch = current_branch(root, timeout);
    let logs = super::super::queries::reflog(ctx, 20)?;
    if logs.is_empty() {
        return Err(GitError::Precondition("nothing to undo".to_string()));
    }
    let prev = logs
        .iter()
        .find(|entry| entry.id != cur_sha)
        .ok_or_else(|| GitError::Precondition("nothing to undo".to_string()))?;
    ensure_object(root, timeout, &prev.id)?;
    let top_message = logs
        .first()
        .map(|entry| entry.message.as_str())
        .unwrap_or("");
    let is_checkout = top_message.starts_with("checkout: moving from ");
    let mut stack = load_redo_stack(&git_dir);
    stack.push(RedoEntry {
        sha: cur_sha.clone(),
        branch: cur_branch,
        checkout: is_checkout,
    });
    if is_checkout {
        let target_branch = parse_checkout_source(top_message);
        let target = target_branch.clone().unwrap_or_else(|| prev.id.clone());
        let argv = vec!["checkout".to_string(), target.clone()];
        let echo = echo_of("undo", &argv);
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        match run_git(root, &refs, timeout) {
            Ok(_) => {}
            Err(err) => {
                if target_branch.is_some() {
                    let fallback = vec!["checkout".to_string(), prev.id.clone()];
                    let echo_fallback = echo_of("undo", &fallback);
                    let refs: Vec<&str> = fallback.iter().map(String::as_str).collect();
                    run_git(root, &refs, timeout).map_err(|_| err)?;
                    save_redo_stack(&git_dir, &stack)?;
                    return Ok(finish(
                        "undo",
                        format!("undone to {} ({})", short_of(&prev.id), prev.selector),
                        echo_fallback,
                    ));
                }
                return Err(err);
            }
        }
        save_redo_stack(&git_dir, &stack)?;
        Ok(finish(
            "undo",
            format!("undone to {} ({})", short_of(&prev.id), prev.selector),
            echo,
        ))
    } else {
        let argv = vec!["reset".to_string(), "--soft".to_string(), prev.id.clone()];
        let echo = echo_of("undo", &argv);
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        run_git(root, &refs, timeout)?;
        save_redo_stack(&git_dir, &stack)?;
        Ok(finish(
            "undo",
            format!("undone to {} ({})", short_of(&prev.id), prev.selector),
            echo,
        ))
    }
}

fn redo(ctx: &ApiContext, root: &std::path::Path) -> GitResult<ActionOutcome> {
    let timeout = command_timeout_ms(ctx, false);
    let git_dir = resolve_git_dir(ctx, root)?;
    guard_mid_operation(&git_dir)?;
    let mut stack = load_redo_stack(&git_dir);
    let entry = stack
        .pop()
        .ok_or_else(|| GitError::Precondition("nothing to redo".to_string()))?;
    ensure_object(root, timeout, &entry.sha)?;
    if entry.checkout {
        let target = if entry.branch.trim().is_empty() {
            entry.sha.clone()
        } else {
            entry.branch.clone()
        };
        let argv = vec!["checkout".to_string(), target.clone()];
        let echo = echo_of("redo", &argv);
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        match run_git(root, &refs, timeout) {
            Ok(_) => {
                save_redo_stack(&git_dir, &stack)?;
                Ok(finish(
                    "redo",
                    format!("redone to {}", short_of(&entry.sha)),
                    echo,
                ))
            }
            Err(err) => {
                if target != entry.sha {
                    let fallback = vec!["checkout".to_string(), entry.sha.clone()];
                    let echo_fallback = echo_of("redo", &fallback);
                    let refs: Vec<&str> = fallback.iter().map(String::as_str).collect();
                    run_git(root, &refs, timeout).map_err(|_| err)?;
                    save_redo_stack(&git_dir, &stack)?;
                    return Ok(finish(
                        "redo",
                        format!("redone to {}", short_of(&entry.sha)),
                        echo_fallback,
                    ));
                }
                stack.push(entry);
                let _ = save_redo_stack(&git_dir, &stack);
                Err(err)
            }
        }
    } else {
        let argv = vec!["reset".to_string(), "--soft".to_string(), entry.sha.clone()];
        let echo = echo_of("redo", &argv);
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        match run_git(root, &refs, timeout) {
            Ok(_) => {
                save_redo_stack(&git_dir, &stack)?;
                Ok(finish(
                    "redo",
                    format!("redone to {}", short_of(&entry.sha)),
                    echo,
                ))
            }
            Err(err) => {
                stack.push(entry);
                let _ = save_redo_stack(&git_dir, &stack);
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_source_parses_branch_move() {
        assert_eq!(
            parse_checkout_source("checkout: moving from main to feature"),
            Some("main".to_string())
        );
        assert_eq!(parse_checkout_source("commit: hello"), None);
        assert_eq!(parse_checkout_source("checkout: moving from  to x"), None);
    }

    #[test]
    fn short_of_truncates_to_seven() {
        assert_eq!(short_of("abcdef123456"), "abcdef1");
    }
}
