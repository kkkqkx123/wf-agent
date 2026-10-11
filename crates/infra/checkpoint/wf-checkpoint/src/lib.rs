pub mod coordinator;

pub use checkpoint_base::error;
pub use checkpoint_file::approval;
pub use checkpoint_file::event;
pub use checkpoint_file::file;
pub use checkpoint_file::gc;
pub use checkpoint_file::provenance;
pub use checkpoint_file::scan;
pub use checkpoint_file::watcher;

pub use checkpoint_base::cache::CheckpointCache;
pub use checkpoint_base::clock::{CheckpointClock, ManualClock};
pub use checkpoint_base::common::content::ContentFilter;
pub use checkpoint_base::delta::calculator::{AgentDiffCalculator, WorkflowDiffCalculator};
pub use checkpoint_base::delta::restorer::GenericDeltaRestorer;
pub use checkpoint_base::delta::{CheckpointLoader, DeltaRestorer, DiffCalculator};
pub use checkpoint_base::error::CheckpointError;
pub use checkpoint_base::error_handling::CheckpointErrorHandler;
pub use checkpoint_base::execution_events::ExecutionEventBus;
pub use checkpoint_base::metadata::builder::{
    build_checkpoint_metadata, build_checkpoint_state, custom_fields_equal, trigger_description,
    trigger_tag, CHAIN_POSITION_FIELD, PROGRESS_COORD_KEYS, WF_PROGRESS_COORD_KEYS,
};
pub use checkpoint_base::strategy::{
    create_checkpoint_strategy, CheckpointStrategy, StandardStrategy,
};
pub use checkpoint_base::version_manager::{VersionCompatibility, VersionManager};
pub use checkpoint_file::event::{CheckpointEvent, CheckpointEventBus as FileEventBus};
pub use checkpoint_file::file::FileCheckpointManager;
pub use checkpoint_file::scope::ScopeEndOutcome;
pub use checkpoint_file::script_capture::WorkspaceChangeCollector;
pub use checkpoint_file::session::CheckpointSession;
pub use checkpoint_file::sha256_hex;
pub use checkpoint_file::MergeCommitResult;
pub use wf_types::config::file_scope::{
    is_scope_subset, normalize_scope_path, scope_path_contains, validate_scope_path,
    ExecutionFileScope, ResolvedFileScope, MAX_SCOPE_PATH_LEN,
};

/// Resolve the effective execution scope from the layered inputs.
///
/// Priority from low to high is global default, parent execution, node
/// declaration, then call argument. Missing values inherit the next lower
/// layer; an explicit empty list denies everything. A missing checkpoint
/// list copies the resolved write list. The resolved value is returned
/// without validation; callers validate the expansion before use.
pub fn resolve_execution_scope(
    global_default: Option<&ExecutionFileScope>,
    parent: Option<&ExecutionFileScope>,
    node: Option<&ExecutionFileScope>,
    call: Option<&ExecutionFileScope>,
    workspace_root: Option<&str>,
) -> ResolvedFileScope {
    let merged = merge_layer(
        merge_layer(
            merge_layer(
                global_default.cloned().unwrap_or_default(),
                parent.cloned().unwrap_or_default(),
            ),
            node.cloned().unwrap_or_default(),
        ),
        call.cloned().unwrap_or_default(),
    );
    expand_scope(&merged, global_default, workspace_root)
}

/// Check that a child scope stays inside its parent. An empty child set is
/// always contained; any out-of-bounds entry reports the offending path.
pub fn check_child_scope_subset(
    child: &ResolvedFileScope,
    parent: &ResolvedFileScope,
) -> Result<(), String> {
    for prefix in child
        .read_prefixes
        .iter()
        .chain(child.write_prefixes.iter())
        .chain(child.checkpoint_prefixes.iter())
    {
        let inside_read = parent
            .read_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, prefix));
        let inside_write = parent
            .write_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, prefix));
        if !inside_read && !inside_write {
            return Err(format!(
                "child scope entry '{prefix}' is outside parent read and write prefixes"
            ));
        }
    }
    for prefix in &child.checkpoint_prefixes {
        if !child
            .write_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, prefix))
        {
            return Err(format!(
                "checkpoint prefix '{prefix}' is outside child write prefixes"
            ));
        }
    }
    if let Some(workdir) = child.workdir.as_deref() {
        let inside_read = child
            .read_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, workdir));
        let inside_write = child
            .write_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, workdir));
        if !inside_read && !inside_write {
            return Err(format!(
                "workdir '{workdir}' is outside child read and write prefixes"
            ));
        }
    }
    Ok(())
}

/// True when the resolved scope disables the file channel. Closed
/// executions create no session, run no scan, and record no file link.
pub fn is_file_channel_closed(scope: Option<&ResolvedFileScope>) -> bool {
    scope.is_some_and(|s| !s.enabled)
}

/// Resolve and validate the effective scope. Lexical failures, checkpoint
/// outside write, workdir outside bounds, and child outside parent all
/// fail instead of silently narrowing.
pub fn resolve_execution_scope_checked(
    global_default: Option<&ExecutionFileScope>,
    parent_resolved: Option<&ResolvedFileScope>,
    node: Option<&ExecutionFileScope>,
    call: Option<&ExecutionFileScope>,
    workspace_root: Option<&str>,
) -> Result<ResolvedFileScope, String> {
    let parent_exec = parent_resolved.map(|p| ExecutionFileScope {
        enabled: Some(p.enabled),
        read_prefixes: Some(p.read_prefixes.clone()),
        write_prefixes: Some(p.write_prefixes.clone()),
        checkpoint_prefixes: Some(p.checkpoint_prefixes.clone()),
        workdir: p.workdir.clone(),
    });
    let resolved = resolve_execution_scope(
        global_default,
        parent_exec.as_ref(),
        node,
        call,
        workspace_root,
    );
    for path in resolved
        .read_prefixes
        .iter()
        .chain(resolved.write_prefixes.iter())
        .chain(resolved.checkpoint_prefixes.iter())
    {
        validate_scope_path(path, "scope.prefix").map_err(|e| e.to_string())?;
    }
    if let Some(workdir) = resolved.workdir.as_deref() {
        validate_scope_path(workdir, "scope.workdir").map_err(|e| e.to_string())?;
    }
    for prefix in &resolved.checkpoint_prefixes {
        if !resolved
            .write_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, prefix))
        {
            return Err(format!(
                "checkpoint prefix '{prefix}' is outside write prefixes"
            ));
        }
    }
    if let Some(workdir) = resolved.workdir.as_deref() {
        let inside = resolved
            .read_prefixes
            .iter()
            .chain(resolved.write_prefixes.iter())
            .any(|bound| scope_path_contains(bound, workdir));
        if !inside {
            return Err(format!(
                "workdir '{workdir}' is outside read and write prefixes"
            ));
        }
    }
    if let Some(parent) = parent_resolved {
        check_child_scope_subset(&resolved, parent)?;
    }
    Ok(resolved)
}

fn merge_layer(mut base: ExecutionFileScope, upper: ExecutionFileScope) -> ExecutionFileScope {
    if upper.enabled.is_some() {
        base.enabled = upper.enabled;
    }
    if upper.read_prefixes.is_some() {
        base.read_prefixes = upper.read_prefixes;
    }
    if upper.write_prefixes.is_some() {
        base.write_prefixes = upper.write_prefixes;
    }
    if upper.checkpoint_prefixes.is_some() {
        base.checkpoint_prefixes = upper.checkpoint_prefixes;
    }
    if upper.workdir.is_some() {
        base.workdir = upper.workdir;
    }
    base
}

fn expand_scope(
    scope: &ExecutionFileScope,
    global_default: Option<&ExecutionFileScope>,
    workspace_root: Option<&str>,
) -> ResolvedFileScope {
    let pick = |own: Option<&Vec<String>>, global: Option<&Vec<String>>| {
        own.cloned().or_else(|| global.cloned()).unwrap_or_else(|| {
            workspace_root
                .map(|root| vec![root.to_string()])
                .unwrap_or_default()
        })
    };
    let read_prefixes = pick(
        scope.read_prefixes.as_ref(),
        global_default.and_then(|g| g.read_prefixes.as_ref()),
    );
    let write_prefixes = pick(
        scope.write_prefixes.as_ref(),
        global_default.and_then(|g| g.write_prefixes.as_ref()),
    );
    let checkpoint_prefixes = scope
        .checkpoint_prefixes
        .clone()
        .or_else(|| global_default.and_then(|g| g.checkpoint_prefixes.clone()))
        .unwrap_or_else(|| write_prefixes.clone());
    let enabled = scope
        .enabled
        .or_else(|| global_default.and_then(|g| g.enabled))
        .unwrap_or(true);
    let workdir = scope
        .workdir
        .clone()
        .or_else(|| global_default.and_then(|g| g.workdir.clone()));
    ResolvedFileScope {
        enabled,
        read_prefixes,
        write_prefixes,
        checkpoint_prefixes,
        workdir,
    }
}
pub use checkpoint_state::restore::{
    ChildDiscovery, ChildDiscoveryLoader, ChildDiscoverySummary, InMemoryChildResolver,
    RestoreStrategyRegistry,
};
pub use checkpoint_state::state::agent::{AgentCheckpoint, AgentCheckpointStateManager};
pub use checkpoint_state::state::base::CheckpointStateManager;
pub use checkpoint_state::state::storage::StorageBackedStateManager;
pub use checkpoint_state::state::workflow::{WorkflowCheckpoint, WorkflowCheckpointStateManager};
pub use coordinator::base::{
    CheckpointBlob, CheckpointCoordinator, CheckpointId, ChildDiscoveryIndex, ChildMetadataIndex,
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
