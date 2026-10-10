//! File-content checkpointing: per-workspace file history backed by an
//! independent bare Git object store plus a small SQLite metadata database.
//!
//! File bytes, trees, history and file-branch refs (`refs/wf/*`) live in
//! the bare repository and are the only truth for content. SQLite keeps
//! what Git cannot express cheaply: review states, empty-directory
//! manifests, the source index cache, state-to-file links and key-value
//! metadata. The user's own repository is never read or written.
//!
//! Write attribution has three paths: precise tool reports (primary),
//! script before/after scans (one atomic commit per run) and periodic
//! polling for external human edits (see `watcher::ManualChangeService`,
//! the production pump). Reads go through `provenance` (partitions,
//! diffs, timelines, conflict enumeration) with an index fast path and a
//! bounded commit-graph fallback.
pub mod approval;
pub(crate) mod branch;
pub mod event;
pub mod file;
pub mod gc;
pub(crate) mod git_store;
pub(crate) mod manager_store;
pub(crate) mod precise;
pub mod provenance;
pub mod scan;
pub mod scope;
pub mod script_capture;
pub mod session;
pub(crate) mod storage;
pub mod watcher;

pub use approval::{ConflictView, MergeOutcome, PendingApproval};
pub use event::{CheckpointEvent, CheckpointEventBus};
pub use file::actor::{PreciseApplyStats, PreciseFileEvent, PreciseFileEventKind};
pub use file::merge::MergeCommitResult;
pub use file::util::sha256_hex;
pub use file::{
    FileCheckpoint, FileCheckpointManager, FileCheckpointMetadata, FileCheckpointOptions,
    FileContentEntry, FileState, WorkspaceRestoreResult,
};
pub use provenance::{DeltaSummary, FileDiffKind, FileDiffView, PartitionView, WorkspaceFile};
pub use scan::{ScanConfig, WorkspaceScan, WorkspaceScanner};
pub use scope::ScopeEndOutcome;
pub use script_capture::{CollectedChange, CollectedChangeKind, WorkspaceChangeCollector};
pub use session::CheckpointSession;
pub use watcher::{normalize_absolute_path, FileChangeKind, FileChangeRecord, ManualChangeService};
