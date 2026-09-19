pub mod adapter;
pub mod approval;
pub mod branch;
pub mod event;
pub mod file;
pub mod manager_store;
pub mod precise;
pub mod provenance;
pub mod scan;
pub mod scope;
pub mod script_capture;
pub mod session;
pub mod watcher;

pub use adapter::{LayertwineBackend, LayertwineCheckpointBridge};
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
