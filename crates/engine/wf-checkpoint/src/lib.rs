pub mod actor_id;
pub mod approval;
pub mod branch;
pub mod cache;
pub mod checkpoint_graph;
pub mod cleanup;
pub mod config_resolver;
pub mod content;
pub mod coordinator;
pub mod delta;
pub mod diff;
pub mod error;
pub mod error_handling;
pub mod event;
pub mod execution_events;
pub mod file;
pub mod file_actor;
pub mod file_approval;
pub mod file_checkpoint;
pub mod file_merge;
pub mod file_restore;
pub mod file_util;
pub mod file_workspace;
pub mod layertwine;
pub mod metadata_builder;
pub mod metrics;
pub mod provenance;
pub mod recent_agent_writes;
pub mod restore;
pub mod scan;
pub mod script_capture;
pub mod serializer;
pub mod state;
pub mod strategy;
pub mod version;
pub mod watcher;

pub use actor_id::{ActorId, ActorIdError, ActorKind};
pub use approval::{ConflictView, MergeOutcome, PendingApproval};
pub use cache::CheckpointCache;
pub use config_resolver::CheckpointConfigResolver;
pub use diff::{
    DiffEngine, DiffHunk, DiffOp, DiffOpKind, DiffResult, DiffStats, HunkLine, HunkLineKind,
};
pub use error::CheckpointError;
pub use error_handling::{CheckpointErrorHandler, ErrorHandlingOutcome};
pub use event::{CheckpointEvent, CheckpointEventBus};
pub use file::{
    FileCheckpoint, FileCheckpointManager, FileCheckpointMetadata, FileCheckpointOptions,
    FileContentEntry, FileContentStore, FileState, LayertwineFileContentStore,
    WorkspaceRestoreResult,
};
pub use file_merge::MergeCommitResult;
pub use file_util::sha256_hex;
pub use metadata_builder::{build_checkpoint_state, CheckpointMetadataBuilder};
pub use provenance::{DeltaSummary, FileDiffKind, FileDiffView, PartitionView, WorkspaceFile};
pub use scan::{ScanConfig, WorkspaceScan, WorkspaceScanner};
pub use serializer::{CheckpointCodec, CheckpointSerializer};
pub use watcher::{FileChangeKind, FileChangeRecord, FileWatcher, ManualChangeService};
