//! Independent bare Git object store for file-content history.
//!
//! Frozen rules (no behavior beyond this module may redefine them):
//! - one bare repository per workspace, stored at
//!   `<workspace>/.wf-checkpoint-git`, fully separated from the user's own
//!   repository (the user's `.git` is never read or written);
//! - refs live only under `refs/wf/`: `refs/wf/main` is the mainline
//!   truth, `refs/wf/edit/<actor>` is one actor's edit line,
//!   `refs/wf/review/<id>` is one submission awaiting review,
//!   `refs/wf/feat/<name>` is one collaboration target,
//!   `refs/wf/human` carries external human edits and is never auto-merged;
//! - commit trailers carry attribution: `Wf-Actor`, `Wf-Session`, `Wf-Tool`;
//!   `Wf-State: conflict-unresolved` / `conflict-resolved` marks merge state;
//! - ignore stacking order: repository-local excludes (`.git`,
//!   `.wf-checkpoint-git`, checkpoint database file), then the workspace's
//!   own `.gitignore` files, then custom patterns;
//! - conflicts land on disk with standard `<<<<<<<` / `=======` / `>>>>>>>`
//!   markers and block the corresponding merge until resolved.
//!

/// Directory name of the bare repository inside the workspace root.
pub const CHECKPOINT_GIT_DIR_NAME: &str = ".wf-checkpoint-git";

/// Integration truth: the only ref whose tree may be materialized.
pub const REF_MAIN: &str = "refs/wf/main";
/// One actor's edit line: `refs/wf/edit/<actor>`.
pub const REF_EDIT_PREFIX: &str = "refs/wf/edit/";
/// One submission awaiting review: `refs/wf/review/<id>`.
pub const REF_REVIEW_PREFIX: &str = "refs/wf/review/";
/// One collaboration target: `refs/wf/feat/<name>`.
pub const REF_FEAT_PREFIX: &str = "refs/wf/feat/";
/// External human edits. Never auto-merged into any other ref.
pub const REF_HUMAN: &str = "refs/wf/human";

/// Commit trailer carrying the acting executor id.
pub const TRAILER_ACTOR: &str = "Wf-Actor";
/// Commit trailer carrying the session id shared by one operation group.
pub const TRAILER_SESSION: &str = "Wf-Session";
/// Commit trailer carrying the tool source of the operation.
pub const TRAILER_TOOL: &str = "Wf-Tool";
/// Commit trailer carrying merge state (`conflict-unresolved/resolved`).
pub const TRAILER_STATE: &str = "Wf-State";
/// Commit trailer value marking an unresolved merge.
pub const STATE_CONFLICT_UNRESOLVED: &str = "conflict-unresolved";
/// Commit trailer value marking a resolved merge.
pub const STATE_CONFLICT_RESOLVED: &str = "conflict-resolved";
/// Commit trailer repeating once per conflicted path.
pub const TRAILER_CONFLICT_FILE: &str = "Wf-Conflict-File";

/// Author/committer identity used for system-created commits.
pub const SYSTEM_COMMITTER: &str = "wf-checkpoint <system@local>";

/// Bounded compare-and-swap retries for `commit_on_ref`: a conflicting
/// concurrent writer forces a rebuild on the new head, and persistent
/// contention surfaces as a ref conflict instead of spinning forever.
pub const MAX_COMMIT_CAS_RETRIES: u32 = 8;

/// Regular file mode used for every tracked blob.
pub const MODE_FILE: &str = "100644";
/// Executable file mode preserved from the worktree when present.
pub const MODE_EXEC: &str = "100755";

use std::fs;
use std::path::{Path, PathBuf};

/// Independent bare repository handle for one workspace.
pub struct GitStore {
    git_dir: PathBuf,
    workspace_root: PathBuf,
    /// Owned temp dir for in-memory managers; `None` for real workspaces.
    /// The path is removed best-effort on drop.
    owned_temp: Option<PathBuf>,
}

impl Drop for GitStore {
    fn drop(&mut self) {
        if let Some(dir) = self.owned_temp.take() {
            if dir.starts_with(std::env::temp_dir()) {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
    }
}

impl GitStore {
    /// Default bare repository location for a workspace root.
    pub fn default_git_dir(workspace_root: &Path) -> PathBuf {
        workspace_root.join(CHECKPOINT_GIT_DIR_NAME)
    }

    /// Open (creating if needed) the bare repository at `git_dir`.
    pub fn init(workspace_root: &Path, git_dir: &Path) -> Result<Self, GitStoreError> {
        fs::create_dir_all(git_dir.join("objects"))?;
        fs::create_dir_all(git_dir.join("refs/wf"))?;
        let head = git_dir.join("HEAD");
        if !head.exists() {
            fs::write(&head, format!("ref: {REF_MAIN}\n"))?;
        }
        let config = git_dir.join("config");
        if !config.exists() {
            fs::write(
                &config,
                "[core]\n\trepositoryformatversion = 0\n\tbare = true\n",
            )?;
        }
        Ok(Self {
            git_dir: git_dir.to_path_buf(),
            workspace_root: workspace_root.to_path_buf(),
            owned_temp: None,
        })
    }

    /// Open the default bare repository location for a workspace root.
    pub fn init_for_workspace(workspace_root: &Path) -> Result<Self, GitStoreError> {
        let git_dir = Self::default_git_dir(workspace_root);
        Self::init(workspace_root, &git_dir)
    }

    /// Ephemeral store under the system temp dir (tests / in-memory use).
    pub fn init_temp() -> Result<Self, GitStoreError> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("wf-checkpoint-git-{}-{}", std::process::id(), id));
        let workspace = dir.join("work");
        fs::create_dir_all(&workspace)?;
        let mut store = Self::init(&workspace, &dir.join("repo"))?;
        store.owned_temp = Some(dir);
        Ok(store)
    }

    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
}

mod commit;
mod errors;
mod merge;
mod objects;
mod refs;
#[cfg(test)]
mod tests;

pub use commit::{commit_message, GitCommit};
pub use errors::GitStoreError;
pub use merge::{merge_file_contents, merge_file_maps};
pub use objects::{is_hex_id, CommitOnRefOutcome};
pub(crate) use refs::sanitize_ref_component;
pub use refs::{edit_ref_for_actor, feat_ref_for_name, review_ref_for_id};
