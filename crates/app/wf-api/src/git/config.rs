use std::path::{Path, PathBuf};

use wf_types::config::user_git::UserGitConfig;

use super::types::{GitError, GitResult};
use crate::infra::context::ApiContext;

pub fn git_config(ctx: &ApiContext) -> GitResult<UserGitConfig> {
    ctx.user_git_config()
        .cloned()
        .filter(|c| c.enabled)
        .ok_or(GitError::NotEnabled)
}

pub fn repo_root(ctx: &ApiContext) -> GitResult<PathBuf> {
    let config = git_config(ctx)?;
    let root = config.repo_root.clone().ok_or_else(|| {
        GitError::NotInitialized("user_git.repo_root is not configured".to_string())
    })?;
    let path = PathBuf::from(&root);
    if !path.join(".git").exists() && !path.join("HEAD").exists() {
        return Err(GitError::NotInitialized(format!(
            "no git repository at {root}"
        )));
    }
    Ok(path)
}

pub fn command_timeout_ms(ctx: &ApiContext, remote: bool) -> u64 {
    match ctx.user_git_config() {
        Some(c) if remote => c.remote_timeout_ms,
        Some(c) => c.command_timeout_ms,
        None => {
            if remote {
                120_000
            } else {
                30_000
            }
        }
    }
}

pub fn validate_repo_path(root: &Path, path: &str) -> GitResult<PathBuf> {
    if path.is_empty() || path.len() > 1024 {
        return Err(GitError::PathEscape(
            "path must be 1..=1024 chars".to_string(),
        ));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(GitError::PathEscape(
            "absolute paths are rejected".to_string(),
        ));
    }
    for component in path.split('/') {
        if component == ".." {
            tracing::warn!(path = %path, "user git path rejected: escape");
            return Err(GitError::PathEscape(format!(
                "path escapes the repository: {path}"
            )));
        }
    }
    if path.contains('\\') || path.contains('\0') {
        return Err(GitError::PathEscape(
            "path contains illegal characters".to_string(),
        ));
    }
    let joined = root.join(path);
    let normalized = normalize_join(root, &joined);
    if !normalized.starts_with(root) {
        tracing::warn!(path = %path, "user git path rejected: outside root");
        return Err(GitError::PathEscape(format!(
            "path escapes the repository: {path}"
        )));
    }
    Ok(normalized)
}

fn normalize_join(root: &Path, joined: &Path) -> PathBuf {
    let mut out = PathBuf::from(root);
    if let Ok(rel) = joined.strip_prefix(root) {
        for component in rel.components() {
            match component {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                std::path::Component::CurDir => {}
                other => out.push(other.as_os_str()),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_path_rejects_escape_and_absolute() {
        let root = Path::new("/repo");
        assert!(validate_repo_path(root, "src/main.rs").is_ok());
        assert!(validate_repo_path(root, "").is_err());
        assert!(validate_repo_path(root, "/etc/passwd").is_err());
        assert!(validate_repo_path(root, "../secret").is_err());
        assert!(validate_repo_path(root, "a/../../b").is_err());
        assert!(validate_repo_path(root, "a\\b").is_err());
    }
}
