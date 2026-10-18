//! User repository Git domain: read-only queries over the user-visible
//! repository plus the single converging write entry.
//!
//! The checkpoint bare repository and its object-store helpers are never
//! reused here. Every call shells out to the `git` binary with an argument
//! array and parses only machine-stable formats.

pub mod actions;
pub mod config;
pub mod parse;
pub mod queries;
pub mod registry;
pub mod runner;
pub mod types;

pub use actions::{execute_action, ActionRequest};
pub use config::{git_config, repo_root};
pub use parse::{
    parse_branches, parse_diff_unified, parse_log_entries, parse_porcelain_v2_branch_info,
    parse_porcelain_v2_files, parse_stash_list, parse_tags,
};
pub use queries::{
    branches, commit_files, commits, diff_refs, diff_unstaged, file_entries, fingerprint, meta,
    reflog, remotes, stash_list, status, submodule_status, tags, worktrees,
};
pub use registry::{action_registry, registry_snapshot};
pub use types::{
    BranchEntry, CommitEntry, CommitFileEntry, DangerLevel, DiffFileView, DiffHunk, FileEntry,
    FileKind, Fingerprint, GitError, GitResult, ReflogEntry, RegistryEntry, RemoteEntry, RepoMeta,
    StashEntry, StatusView, SubmoduleEntry, TagEntry, WorktreeEntry,
};
