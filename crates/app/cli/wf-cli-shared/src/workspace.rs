//! Workspace resolution: the single place a CLI frontend decides which
//! business directory its requests belong to.
//!
//! Precedence is explicit flag, then the positional directory, then the
//! `WF_DIRECTORY` environment entry, then the process working directory.
//! Every form (headless, mini, full TUI) resolves through here so local
//! affordances (prompt completion, mention scanning, keymap project layer)
//! and the transmitted `x-wf-directory` header cannot disagree.
//!
//! Resolution also normalizes the process: the canonical directory becomes
//! the process working directory, matching the reference behavior where the
//! invocation directory anchors both local file access and the request.

use std::path::PathBuf;

use crate::args::Cli;

/// Environment entry pinning the business directory. Wins over the process
/// working directory; loses to explicit CLI flags.
pub const DIRECTORY_ENV_VAR: &str = "WF_DIRECTORY";

/// Resolve the canonical business directory for this invocation.
/// Rejects ambiguous input (both flag and positional set) and anything that
/// is not an existing directory, so typos fail here instead of surfacing as
/// a later server rejection.
pub fn resolve_workspace_dir(cli: &Cli) -> Result<PathBuf, String> {
    if cli.directory.is_some() && cli.directory_arg.is_some() {
        return Err("--directory and the positional DIRECTORY are mutually exclusive".to_string());
    }
    if cli.command.is_some() && cli.directory_arg.is_some() {
        return Err("the positional DIRECTORY applies to interactive forms; use --directory with subcommands".to_string());
    }
    let raw = cli
        .directory
        .clone()
        .or_else(|| cli.directory_arg.clone())
        .or_else(|| {
            std::env::var(DIRECTORY_ENV_VAR)
                .ok()
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
        });
    let raw = match raw {
        Some(dir) => dir,
        None => std::env::current_dir()
            .map_err(|e| format!("cannot read the working directory: {e}"))?,
    };
    canonical_workspace_dir(&raw)
}

/// Validate and canonicalize one candidate directory. Symlinks resolve so
/// equivalent spellings share one server cache key.
pub fn canonical_workspace_dir(raw: &PathBuf) -> Result<PathBuf, String> {
    if raw.as_os_str().is_empty() {
        return Err("directory must not be empty".to_string());
    }
    if raw.is_relative() {
        let cwd = std::env::current_dir()
            .map_err(|e| format!("cannot read the working directory: {e}"))?;
        return canonical_workspace_dir(&cwd.join(raw));
    }
    match std::fs::symlink_metadata(raw) {
        Ok(meta) => {
            if !meta.file_type().is_dir() && !meta.is_dir() {
                return Err(format!("directory is not a directory: {}", raw.display()));
            }
        }
        Err(_) => {
            return Err(format!("directory does not exist: {}", raw.display()));
        }
    }
    std::fs::canonicalize(raw).map_err(|e| format!("cannot resolve directory {}: {e}", raw.display()))
}

/// Adopt the workspace as the process working directory so local file
/// access (completion, mentions, keymaps) observes the same root that
/// requests announce. Idempotent for the already-current directory.
pub fn adopt_workspace_dir(dir: &PathBuf) -> Result<(), String> {
    std::env::set_current_dir(dir).map_err(|e| format!("cannot enter directory {}: {e}", dir.display()))
}

/// Find the owning project root for worktree placement: nearest ancestor
/// (inclusive) carrying a version-control marker, else the directory
/// itself.
pub fn discover_project_root(dir: &std::path::Path) -> PathBuf {
    let mut current = dir.to_path_buf();
    loop {
        for marker in [".git", ".hg"] {
            if current.join(marker).exists() {
                return current;
            }
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return dir.to_path_buf(),
        }
    }
}

/// Create a new git worktree of the owning project, keeping the caller's
/// subfolder when the worktree has it. A named worktree lands in a
/// sibling `worktrees` directory; an unnamed one gets a timestamped
/// directory. Returns the directory the session should run in.
pub fn ensure_worktree(dir: &std::path::Path, name: Option<&str>) -> Result<PathBuf, String> {
    let root = discover_project_root(dir);
    if !root.join(".git").exists() {
        return Err(format!(
            "--worktree needs a git project: {} has no .git marker",
            root.display()
        ));
    }
    let sibling = match root.parent() {
        Some(parent) => parent.to_path_buf(),
        None => {
            return Err(format!(
                "--worktree cannot place a sibling of the filesystem root: {}",
                root.display()
            ));
        }
    };
    let stem = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("run-{}", wf_common::generate_id()));
    if stem.contains('/') || stem.contains('\\') || stem == "." || stem == ".." {
        return Err(format!("invalid --worktree name '{stem}': use a plain directory name"));
    }
    let target = sibling.join(format!(
        "{}-worktrees",
        root.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".to_string())
    ));
    let target = target.join(&stem);
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .arg("worktree")
        .arg("add")
        .arg(&target)
        .output()
        .map_err(|e| format!("cannot run git worktree add: {e}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("git worktree add failed for {}", target.display())
        } else {
            format!("git worktree add failed: {detail}")
        });
    }
    let subfolder = dir.strip_prefix(&root).ok().filter(|rel| {
        !rel.as_os_str().is_empty()
            && !rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
    });
    let candidate = match subfolder {
        Some(rel) => target.join(rel),
        None => target.clone(),
    };
    if candidate.is_dir() {
        Ok(candidate)
    } else {
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli_with(direct: Option<PathBuf>, positional: Option<PathBuf>) -> Cli {
        use clap::Parser;
        let mut argv = vec!["wf".to_string()];
        if let Some(dir) = &direct {
            argv.push("--directory".to_string());
            argv.push(dir.to_string_lossy().into_owned());
        }
        if let Some(dir) = &positional {
            argv.push(dir.to_string_lossy().into_owned());
        }
        Cli::try_parse_from(argv).expect("test CLI parses")
    }

    #[test]
    fn flag_and_positional_are_exclusive() {
        let cli = cli_with(Some(PathBuf::from("/a")), Some(PathBuf::from("/b")));
        assert!(resolve_workspace_dir(&cli).is_err());
    }

    #[test]
    fn positional_rejected_with_subcommand() {
        let mut cli = cli_with(None, Some(PathBuf::from("/a")));
        cli.command = Some(crate::args::Command::Health);
        assert!(resolve_workspace_dir(&cli).is_err());
    }

    #[test]
    fn canonical_rejects_missing_and_files() {
        let err = canonical_workspace_dir(&PathBuf::from("/definitely/not/here-wf-test")).unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
        let file = std::env::temp_dir().join("wf-workspace-file-probe");
        std::fs::write(&file, "x").unwrap();
        let err = canonical_workspace_dir(&file).unwrap_err();
        assert!(err.contains("not a directory"), "{err}");
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn canonical_accepts_real_directory() {
        let dir = std::env::temp_dir();
        let canonical = canonical_workspace_dir(&dir).unwrap();
        assert!(canonical.is_absolute());
    }

    #[test]
    fn env_entry_used_when_no_flag() {
        let dir = std::env::temp_dir().canonicalize().unwrap();
        std::env::set_var(DIRECTORY_ENV_VAR, &dir);
        let cli = cli_with(None, None);
        let resolved = resolve_workspace_dir(&cli).unwrap();
        std::env::remove_var(DIRECTORY_ENV_VAR);
        assert_eq!(resolved, dir);
    }

    #[test]
    fn invalid_worktree_name_rejected_before_git() {
        let dir = std::env::temp_dir();
        assert!(ensure_worktree(&dir, Some("../evil")).is_err());
    }

    #[test]
    fn non_git_directory_rejects_worktree() {
        let dir = tempfile_like_dir();
        assert!(ensure_worktree(&dir, Some("x")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tempfile_like_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wf-workspace-{}", wf_common::generate_id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
