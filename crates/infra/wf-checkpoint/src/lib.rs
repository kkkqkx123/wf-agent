pub mod actor;
pub mod approval;
pub mod branch;
pub mod cache;
pub mod scope;
pub mod checkpoint_graph;
pub mod cleanup_policy;
pub mod common;
pub mod config_resolver;
pub mod coordinator;
pub mod delta;
pub mod error;
pub mod error_handling;
pub mod event;
pub mod execution_events;
pub mod file;
pub mod layertwine;
pub mod manager_store;
pub mod metadata;
pub mod metrics_collector;
pub mod precise;
pub mod provenance;
pub mod recent_agent_writes;
pub mod restore;
pub mod scan;
pub mod script_capture;
pub mod serializer;
pub mod session;
pub mod state;
pub mod strategy;
pub mod version_manager;
pub mod watcher;

pub use ::layertwine::core::edit_session::EditSession;
pub use ::layertwine::git_sync::{GcRetention, GcStats};
pub use actor::id::{ActorId, ActorIdError, ActorKind};
pub use approval::{ConflictView, MergeOutcome, PendingApproval};
pub use cache::CheckpointCache;
pub use config_resolver::CheckpointConfigResolver;
pub use common::{
    content_hash, diff_stats_for_text, inline_word_diff, is_binary, unified_diff_text, DiffStats,
};
pub use wf_types::effect::{
    normalize_effect_path, FileMutation, FileOperation, ScopeOutcome, SessionBoundary, ToolEffect,
    ToolEffectPayload,
};
pub use error::CheckpointError;
pub use error_handling::{CheckpointErrorHandler, ErrorHandlingOutcome};
pub use event::{CheckpointEvent, CheckpointEventBus};
pub use file::{
    FileCheckpoint, FileCheckpointManager, FileCheckpointMetadata, FileCheckpointOptions,
    FileContentEntry, FileState, WorkspaceRestoreResult,
};
pub use file::actor::{PreciseApplyStats, PreciseFileEvent, PreciseFileEventKind};
pub use file::merge::MergeCommitResult;
pub use file::util::sha256_hex;
pub use metadata::builder::{build_checkpoint_state, CheckpointMetadataBuilder};
pub use provenance::{DeltaSummary, FileDiffKind, FileDiffView, PartitionView, WorkspaceFile};
pub use scan::{ScanConfig, WorkspaceScan, WorkspaceScanner};
pub use script_capture::{CollectedChange, CollectedChangeKind, WorkspaceChangeCollector};
pub use serializer::{CheckpointCodec, CheckpointSerializer};
pub use session::CheckpointSession;
pub use watcher::{
    normalize_absolute_path, FileChangeKind, FileChangeRecord, FileWatcher, ManualChangeService,
};
