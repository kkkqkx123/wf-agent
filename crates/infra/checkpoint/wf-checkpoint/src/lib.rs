pub mod coordinator;

pub use checkpoint_base::error;
pub use checkpoint_file::approval;
pub use checkpoint_file::event;
pub use checkpoint_file::file;
pub use checkpoint_file::gc;
pub use checkpoint_file::provenance;
pub use checkpoint_file::scan;
pub use checkpoint_file::watcher;

pub use checkpoint_base::actor::id::{ActorId, ActorIdError, ActorKind};
pub use checkpoint_base::cache::CheckpointCache;
pub use checkpoint_base::clock::{CheckpointClock, ManualClock};
pub use checkpoint_base::common::content::ContentFilter;
pub use checkpoint_base::common::{
    diff_stats_for_text, inline_word_diff, unified_diff_text, DiffStats,
};
pub use checkpoint_base::delta::calculator::{AgentDiffCalculator, WorkflowDiffCalculator};
pub use checkpoint_base::delta::restorer::GenericDeltaRestorer;
pub use checkpoint_base::delta::{CheckpointLoader, DeltaRestorer, DiffCalculator};
pub use checkpoint_base::error::CheckpointError;
pub use checkpoint_base::error_handling::CheckpointErrorHandler;
pub use checkpoint_base::execution_events::ExecutionEventBus;
pub use checkpoint_base::metadata::builder::{
    build_checkpoint_metadata, build_checkpoint_state, custom_fields_equal, fingerprint_entries,
    fingerprint_option, trigger_description, trigger_tag, CheckpointMetadataBuilder,
    CHAIN_POSITION_FIELD, PROGRESS_COORD_KEYS, WF_PROGRESS_COORD_KEYS,
};
pub use checkpoint_base::serializer::CheckpointSerializer;
pub use checkpoint_base::strategy::cadenced::{
    CadencedCheckpointStrategy, CheckpointTimingVariant,
};
pub use checkpoint_base::strategy::{
    create_checkpoint_strategy, CheckpointStrategy, StandardStrategy,
};
pub use checkpoint_base::version_manager::{VersionManager, CURRENT_FORMAT_VERSION};
pub use checkpoint_file::event::{CheckpointEvent, CheckpointEventBus as FileEventBus};
pub use checkpoint_file::file::FileCheckpointManager;
pub use checkpoint_file::scope::ScopeEndOutcome;
pub use checkpoint_file::scope::ShellWindowOutcome;
pub use checkpoint_file::script_capture::WorkspaceChangeCollector;
pub use checkpoint_file::session::CheckpointSession;
pub use checkpoint_file::sha256_hex;
pub use checkpoint_file::MergeCommitResult;
pub use checkpoint_file::{
    BaselineRebaseOutcome, BaselineSeedOutcome, BaselineStatus, ExportOptions, ExportOutcome,
    ExternalUpdateHint, ImportOutcome, ReconcilePoint, SYNC_RECONCILE_KEY,
};
pub use checkpoint_state::restore::{
    ChildDiscovery, ChildDiscoveryLoader, ChildDiscoverySummary, InMemoryChildResolver,
    RestoreStrategyRegistry,
};
pub use checkpoint_state::state::agent::{AgentCheckpoint, AgentCheckpointStateManager};
pub use checkpoint_state::state::base::CheckpointStateManager;
pub use checkpoint_state::state::storage::{parse_storage_metadata, StorageBackedStateManager};
pub use checkpoint_state::state::workflow::{WorkflowCheckpoint, WorkflowCheckpointStateManager};
pub use coordinator::base::{
    CheckpointBlob, CheckpointCoordinator, CheckpointId, ChildDiscoveryIndex, ChildMetadataIndex,
};
pub use wf_types::config::file_scope::{
    check_child_scope_subset, expand_file_scope, is_file_channel_closed, is_scope_subset,
    merge_file_scope, merge_file_scope_chain, normalize_scope_path, resolve_execution_scope,
    resolve_execution_scope_checked, scope_path_contains, validate_file_scope,
    validate_resolved_file_scope, validate_scope_path, ExecutionFileScope, ResolvedFileScope,
    MAX_SCOPE_PATH_LEN,
};

#[cfg(test)]
mod scope_tests {
    use super::*;
    fn exec(
        enabled: Option<bool>,
        read: Option<Vec<&str>>,
        write: Option<Vec<&str>>,
        checkpoint: Option<Vec<&str>>,
    ) -> ExecutionFileScope {
        let owned =
            |list: Option<Vec<&str>>| list.map(|v| v.into_iter().map(str::to_string).collect());
        ExecutionFileScope {
            enabled,
            read_prefixes: owned(read),
            write_prefixes: owned(write),
            checkpoint_prefixes: owned(checkpoint),
            workdir: None,
        }
    }
    #[test]
    fn missing_checkpoint_copies_write_and_inherits() {
        let global = exec(Some(true), Some(vec!["/repo"]), Some(vec!["/repo"]), None);
        let call = exec(None, None, Some(vec!["/repo/frontend"]), None);
        let resolved =
            resolve_execution_scope_checked(Some(&global), None, None, Some(&call), Some("/repo"))
                .unwrap();
        assert_eq!(resolved.write_prefixes, vec!["/repo/frontend".to_string()]);
        assert_eq!(
            resolved.checkpoint_prefixes,
            vec!["/repo/frontend".to_string()]
        );
    }
    #[test]
    fn checkpoint_outside_write_fails() {
        let call = exec(
            None,
            None,
            Some(vec!["/repo/frontend"]),
            Some(vec!["/repo/backend"]),
        );
        assert!(
            resolve_execution_scope_checked(None, None, None, Some(&call), Some("/repo")).is_err()
        );
    }
    #[test]
    fn child_outside_parent_fails() {
        let parent = ResolvedFileScope {
            enabled: true,
            read_prefixes: vec!["/repo".to_string()],
            write_prefixes: vec!["/repo/frontend".to_string()],
            checkpoint_prefixes: vec!["/repo/frontend".to_string()],
            workdir: None,
        };
        let call = exec(None, None, Some(vec!["/repo/backend"]), None);
        assert!(resolve_execution_scope_checked(
            None,
            Some(&parent),
            None,
            Some(&call),
            Some("/repo")
        )
        .is_err());
    }
    #[test]
    fn closed_channel_skips_projection() {
        let closed = ResolvedFileScope {
            enabled: false,
            read_prefixes: vec![],
            write_prefixes: vec![],
            checkpoint_prefixes: vec![],
            workdir: None,
        };
        assert!(is_file_channel_closed(Some(&closed)));
        assert!(crate::coordinator::projection::should_skip_file_projection(
            Some(&closed)
        ));
        assert!(!is_file_channel_closed(None));
    }
}
