pub(crate) mod adapter;
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
pub(crate) mod scope;
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
pub use script_capture::{CollectedChange, CollectedChangeKind, WorkspaceChangeCollector};
pub use session::CheckpointSession;
pub use watcher::{
    normalize_absolute_path, FileChangeKind, FileChangeRecord, FileWatcher, ManualChangeService,
};
