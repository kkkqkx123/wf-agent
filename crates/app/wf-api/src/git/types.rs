use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DangerLevel {
    Readonly,
    Recoverable,
    Destructive,
    HistoryRewrite,
}

impl DangerLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Readonly => "readonly",
            Self::Recoverable => "recoverable",
            Self::Destructive => "destructive",
            Self::HistoryRewrite => "history_rewrite",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Untracked,
    Staged,
    Unstaged,
    Renamed,
    Conflicted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepoMeta {
    pub name: String,
    pub root: String,
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub head: String,
    pub bare: bool,
    pub merge_head: bool,
    pub rebase_head: bool,
    pub cherry_pick_head: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusView {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub conflicted: usize,
    pub merge_head: bool,
    pub rebase_head: bool,
    pub cherry_pick_head: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    pub path: String,
    pub staged: String,
    pub unstaged: String,
    pub kind: FileKind,
    pub renamed_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<String>,
}

/// Maximum rendered diff lines per file; longer diffs are cut with
/// `truncated: true`. The file-checkpoint diffs share the same cap, so
/// both surfaces keep one contract.
pub const MAX_DIFF_LINES: usize = 400;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffFileView {
    pub path: String,
    /// `added` | `modified` | `deleted`, derived from the `---`/`+++`
    /// headers (`modified` when the headers are inconclusive).
    pub kind: String,
    pub hunks: Vec<DiffHunk>,
    pub additions: usize,
    pub deletions: usize,
    /// True when git reports binary content (no hunks rendered).
    pub binary: bool,
    /// True when hunk lines were cut at the render cap.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitEntry {
    pub id: String,
    pub short_id: String,
    pub author: String,
    pub email: String,
    pub timestamp: i64,
    pub subject: String,
    pub body: String,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitFileEntry {
    pub path: String,
    pub status: String,
    pub renamed_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchEntry {
    pub name: String,
    pub current: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub commit: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteEntry {
    pub name: String,
    pub url: String,
    pub branches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TagEntry {
    pub name: String,
    pub commit: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StashEntry {
    pub index: usize,
    pub name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubmoduleEntry {
    pub path: String,
    pub commit: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorktreeEntry {
    pub path: String,
    pub branch: String,
    pub commit: String,
    pub bare: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReflogEntry {
    pub id: String,
    pub short_id: String,
    pub selector: String,
    pub message: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fingerprint {
    pub status_hash: String,
    pub head: String,
    pub branch_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegistryEntry {
    pub id: String,
    pub danger: DangerLevel,
    pub params: Vec<String>,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("user git is not enabled; set user_git.enabled=true with repo_root")]
    NotEnabled,
    #[error("user git repository is not initialized: {0}")]
    NotInitialized(String),
    #[error("path escapes the repository: {0}")]
    PathEscape(String),
    #[error("unknown git action: {0}")]
    UnknownAction(String),
    #[error("git action unavailable: {0}")]
    Unavailable(String),
    #[error("git precondition failed: {0}")]
    Precondition(String),
    #[error(
        "confirmation required for destructive action '{0}'; pass confirm matching the target"
    )]
    ConfirmRequired(String),
    #[error("git command failed: {0}")]
    Command(String),
    #[error("git command timed out after {0}ms")]
    Timeout(u64),
}

pub type GitResult<T> = Result<T, GitError>;

impl From<GitError> for crate::ApiError {
    fn from(err: GitError) -> Self {
        match &err {
            GitError::UnknownAction(_) | GitError::PathEscape(_) | GitError::Precondition(_) => {
                crate::ApiError::Validation(err.to_string())
            }
            GitError::NotInitialized(_) => crate::ApiError::NotFound {
                entity_type: "user-git".to_string(),
                id: err.to_string(),
            },
            GitError::NotEnabled | GitError::Unavailable(_) | GitError::ConfirmRequired(_) => {
                crate::ApiError::Conflict(err.to_string())
            }
            GitError::Timeout(_) => crate::ApiError::Timeout(err.to_string()),
            GitError::Command(_) => crate::ApiError::execution(err.to_string()),
        }
    }
}
