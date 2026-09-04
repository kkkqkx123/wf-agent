//! Shared checkpoint APIs: generic checkpoint CRUD, file-level checkpoint
//! operations, provenance queries, and approval flow. Used by both agent
//! and workflow execution paths.

pub mod approval;
pub mod file;
pub mod provenance;
pub mod record;

pub use approval::{approve_changes, list_pending_approvals, reject_changes};
pub use file::{
    create_file_checkpoint, diff_actors, diff_against_staged, get_actor_workspace,
    list_conflicts, list_file_changes, list_partitions, restore_workspace_from_checkpoint,
    scan_workspace, FileCheckpointSummary, WorkspaceScanResult,
};
pub use provenance::{
    get_actor_workspace as provenance_get_actor_workspace, list_changes_by_actor,
    list_changes_by_path, list_partitions as provenance_list_partitions, run_gc,
};
pub use record::{
    delete_checkpoint, delete_checkpoints_by_entity, get_checkpoint, get_checkpoint_chain,
    get_checkpoint_entity_metadata, get_latest_checkpoint, list_checkpoints,
    list_checkpoints_by_entities, list_checkpoints_by_entity, list_checkpoints_by_time_range,
    save_checkpoint, set_checkpoint_entity_metadata, CheckpointChainAnalysisView,
    CheckpointTimeRangeView, CheckpointTransitionView,
};
