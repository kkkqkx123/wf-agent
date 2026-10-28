use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use wf_core::hierarchy::manager::ExecutionHierarchyManager;
use wf_core::interruption::{InterruptionSignal, InterruptionState};
use wf_execution_shared::types::execution_entity::{ExecutionEntity, ExecutionStatus};
use wf_types::Id;

use crate::state::WorkflowExecutionState;

/// Internal variable key carrying the execution options from the API boundary
/// to the persistence layer. Exposed so the wf-api layer can read/write it
/// through the shared entity variable map.
pub const EXECUTION_OPTIONS_VAR: &str = "__execution_options";

/// Internal variable key carrying the debug marker from the API boundary to
/// the persistence layer. Kept private to this module so callers go through
/// the typed accessors below.
const DEBUG_RUN_VAR: &str = "__debug_run";

/// Application policy of a pending workflow variable injection: when the
/// buffered update is applied to the live variable map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableInjectionPolicy {
    /// Applied at the next node boundary of the running execution.
    NextBoundary,
    /// Held back until an explicit pause/resume cycle applies it.
    OnResume,
}

impl VariableInjectionPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::NextBoundary => "next_boundary",
            Self::OnResume => "on_resume",
        }
    }

    /// Parse a caller-supplied policy label; hyphens and underscores are
    /// interchangeable, `None` means the default (`NextBoundary`).
    pub fn parse(raw: Option<&str>) -> Option<Self> {
        let normalized = raw?.trim().replace('-', "_");
        match normalized.as_str() {
            "next_boundary" | "next" | "boundary" => Some(Self::NextBoundary),
            "on_resume" | "resume" => Some(Self::OnResume),
            _ => None,
        }
    }
}

/// One externally submitted variable update waiting for its application
/// policy to trigger. The audit source and submission time travel with the
/// update so the apply-time event matches the submit-time record.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PendingVariableInjection {
    pub id: String,
    pub variable_name: String,
    pub value: Value,
    pub source: String,
    pub submitted_at: i64,
    pub policy: VariableInjectionPolicy,
}

/// Upper bound of buffered injections per execution. The buffer is a
/// short-lived handoff, not storage: submissions past the bound are
/// rejected so a stalled consumer cannot grow memory without limit.
pub const MAX_PENDING_VARIABLE_INJECTIONS: usize = 128;

/// Longest accepted variable name for an injected update.
pub const MAX_INJECTION_VARIABLE_NAME_LEN: usize = 128;

/// Audit source labels accepted on submission; an absent source is
/// attributed to the generic API entry point.
pub const VARIABLE_INJECTION_SOURCES: &[&str] = &["user", "cli", "tui", "web", "workflow-console"];
pub const DEFAULT_VARIABLE_INJECTION_SOURCE: &str = "api";

/// Normalize a caller-supplied audit source against the allowlist.
pub fn normalize_injection_source(source: Option<&str>) -> Result<String, String> {
    match source {
        None => Ok(DEFAULT_VARIABLE_INJECTION_SOURCE.to_string()),
        Some(raw) if VARIABLE_INJECTION_SOURCES.contains(&raw) => Ok(raw.to_string()),
        Some(raw) => Err(format!("unknown variable injection source: {raw}")),
    }
}

/// Validate an injected variable name. Injection writes top-level map keys
/// only: dotted paths would imply nested-path semantics the drain does not
/// implement, and the `__` prefix is engine-owned state (the same boundary
/// the node execution context and trigger actions enforce).
pub fn validate_injection_variable_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("variable name must not be empty".to_string());
    }
    if name.len() > MAX_INJECTION_VARIABLE_NAME_LEN {
        return Err(format!(
            "variable name exceeds {MAX_INJECTION_VARIABLE_NAME_LEN} characters"
        ));
    }
    if wf_execution_shared::context::is_internal_variable(name) {
        return Err(format!(
            "refusing to inject internal variable '{name}' (reserved '__' prefix)"
        ));
    }
    if name.contains('.') {
        return Err(format!(
            "variable injection accepts top-level names only: '{name}'"
        ));
    }
    Ok(())
}

pub struct WorkflowExecutionEntity {
    id: Id,
    workflow_id: Id,
    /// Pinned workflow version this execution runs against. `None` means the
    /// stored definition as it stands; `Some` is a saved version identifier.
    workflow_version: Option<wf_types::Version>,
    pub state: Arc<tokio::sync::RwLock<WorkflowExecutionState>>,
    interruption: InterruptionState,
    cancellation: tokio_util::sync::CancellationToken,
    variables: Arc<DashMap<String, Value>>,
    node_results: Arc<DashMap<String, Value>>,
    /// Externally submitted variable updates waiting for their application
    /// policy (next node boundary or pause/resume). Keyed by injection id;
    /// never part of the state snapshot, so a crash drops pending items
    /// while already-applied values survive through the variable map.
    pending_injections: Arc<DashMap<String, PendingVariableInjection>>,
    pub current_node_id: Arc<tokio::sync::RwLock<Option<String>>>,
    hierarchy: Arc<ExecutionHierarchyManager>,
    execution_type: Option<wf_types::workflow_execution::WorkflowExecutionType>,
    /// Final result of the execution, written on completion (both sync and
    /// spawned paths). `None` until the execution settles.
    output: Arc<tokio::sync::RwLock<Option<Value>>>,
}

impl WorkflowExecutionEntity {
    pub fn new(id: Id, workflow_id: Id) -> Self {
        let hierarchy = Arc::new(ExecutionHierarchyManager::new(
            id.clone(),
            wf_types::execution::ExecutionType::Workflow,
        ));
        Self {
            id,
            workflow_id,
            workflow_version: None,
            state: Arc::new(tokio::sync::RwLock::new(WorkflowExecutionState::new())),
            interruption: InterruptionState::new(),
            cancellation: tokio_util::sync::CancellationToken::new(),
            variables: Arc::new(DashMap::new()),
            node_results: Arc::new(DashMap::new()),
            pending_injections: Arc::new(DashMap::new()),
            current_node_id: Arc::new(tokio::sync::RwLock::new(None)),
            hierarchy,
            execution_type: None,
            output: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    pub fn hierarchy_manager(&self) -> Arc<ExecutionHierarchyManager> {
        self.hierarchy.clone()
    }

    pub fn with_hierarchy_manager(mut self, manager: Arc<ExecutionHierarchyManager>) -> Self {
        self.hierarchy = manager;
        self
    }

    pub fn with_execution_type(
        mut self,
        execution_type: wf_types::workflow_execution::WorkflowExecutionType,
    ) -> Self {
        self.execution_type = Some(execution_type);
        self
    }

    pub fn execution_type(&self) -> Option<wf_types::workflow_execution::WorkflowExecutionType> {
        self.execution_type.clone()
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub fn workflow_id(&self) -> &Id {
        &self.workflow_id
    }

    pub fn with_workflow_version(mut self, version: Option<wf_types::Version>) -> Self {
        self.workflow_version = version;
        self
    }

    pub fn workflow_version(&self) -> Option<wf_types::Version> {
        self.workflow_version.clone()
    }

    pub fn variables(&self) -> &Arc<DashMap<String, Value>> {
        &self.variables
    }

    pub fn interruption(&self) -> &InterruptionState {
        &self.interruption
    }

    pub fn node_results(&self) -> &Arc<DashMap<String, Value>> {
        &self.node_results
    }

    pub fn child_ids(&self) -> Vec<Id> {
        let mut ids: Vec<Id> = self
            .hierarchy
            .children()
            .into_iter()
            .map(|c| c.child_id)
            .collect();
        ids.sort();
        ids
    }

    pub fn parent_execution_id(&self) -> Option<Id> {
        self.hierarchy.parent_id()
    }

    pub fn ancestors(&self) -> Vec<Id> {
        self.hierarchy.ancestors()
    }

    /// The final execution output; `None` until the execution settles.
    pub async fn output(&self) -> Option<Value> {
        self.output.read().await.clone()
    }

    /// Record the final execution output (completion path).
    pub async fn set_output(&self, output: Value) {
        *self.output.write().await = Some(output);
    }

    pub fn get_variable(&self, name: &str) -> Option<Value> {
        self.variables.get(name).map(|v| v.clone())
    }

    pub fn set_variable(&self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }

    /// Set the debug marker for this run. `None` clears it.
    pub fn set_debug_run(&self, debug: Option<bool>) {
        match debug {
            Some(value) => {
                self.variables
                    .insert(DEBUG_RUN_VAR.to_string(), Value::Bool(value));
            }
            None => {
                self.variables.remove(DEBUG_RUN_VAR);
            }
        }
    }

    /// Read the debug marker carried by this run, if any.
    pub fn debug_run(&self) -> Option<bool> {
        self.variables
            .get(DEBUG_RUN_VAR)
            .and_then(|entry| entry.value().as_bool())
    }

    /// Whether `name` is engine-owned bookkeeping state excluded from
    /// public persisted views. Delegates to the shared prefix predicate
    /// so persistence, branch snapshots and injection validation agree.
    pub fn is_internal_variable(name: &str) -> bool {
        wf_execution_shared::context::is_internal_variable(name)
    }

    /// Buffer an externally submitted variable update. The update waits for
    /// its application policy instead of touching the live map, so a
    /// running node never observes a half-applied change.
    pub fn submit_variable_injection(
        &self,
        variable_name: impl Into<String>,
        value: Value,
        source: Option<&str>,
        policy: Option<VariableInjectionPolicy>,
    ) -> Result<PendingVariableInjection, String> {
        use wf_execution_shared::types::execution_entity::ExecutionEntity;
        if self.status().is_terminal() {
            return Err(format!(
                "execution {} is terminal and cannot accept variable injections",
                self.id()
            ));
        }
        let variable_name = variable_name.into();
        validate_injection_variable_name(&variable_name)?;
        let source = normalize_injection_source(source)?;
        if self.pending_injections.len() >= MAX_PENDING_VARIABLE_INJECTIONS {
            return Err(format!(
                "pending variable injection buffer is full ({MAX_PENDING_VARIABLE_INJECTIONS} items)"
            ));
        }
        let injection = PendingVariableInjection {
            id: wf_common::generate_id(),
            variable_name,
            value,
            source,
            submitted_at: wf_common::now(),
            policy: policy.unwrap_or(VariableInjectionPolicy::NextBoundary),
        };
        self.pending_injections
            .insert(injection.id.clone(), injection.clone());
        Ok(injection)
    }

    /// Pending updates ordered by submission time (ties by id) for stable
    /// listing and deterministic last-writer-wins draining.
    pub fn list_variable_injections(&self) -> Vec<PendingVariableInjection> {
        let mut items: Vec<PendingVariableInjection> = self
            .pending_injections
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        items.sort_by(|a, b| {
            a.submitted_at
                .cmp(&b.submitted_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        items
    }

    /// Drop one pending update; `false` means the id was unknown (already
    /// applied or never submitted). Applied values are never removed: only
    /// the pending buffer entry goes away.
    pub fn remove_variable_injection(&self, id: &str) -> bool {
        self.pending_injections.remove(id).is_some()
    }

    /// Take and return every pending update of `policy`, ordered by
    /// submission time. The caller applies the returned items and publishes
    /// the audit events; removal here makes the application idempotent.
    pub fn drain_variable_injections(
        &self,
        policy: VariableInjectionPolicy,
    ) -> Vec<PendingVariableInjection> {
        let mut items: Vec<PendingVariableInjection> = Vec::new();
        for entry in self.pending_injections.iter() {
            if entry.value().policy == policy {
                items.push(entry.value().clone());
            }
        }
        for item in &items {
            self.pending_injections.remove(&item.id);
        }
        items.sort_by(|a, b| {
            a.submitted_at
                .cmp(&b.submitted_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        items
    }

    pub fn get_node_result(&self, node_id: &str) -> Option<Value> {
        self.node_results.get(node_id).map(|v| v.clone())
    }

    pub fn set_node_result(&self, node_id: impl Into<String>, value: Value) {
        self.node_results.insert(node_id.into(), value);
    }

    pub async fn register_child(&self, child_id: Id) {
        let already = self
            .hierarchy
            .children()
            .iter()
            .any(|c| c.child_id == child_id);
        if already {
            return;
        }
        self.hierarchy
            .register_child_ref(wf_core::ChildExecutionReference {
                child_type: wf_types::execution::ExecutionType::Workflow,
                child_id,
                created_at: wf_common::now(),
                fork_path: None,
            });
    }

    pub async fn register_child_ref(&self, child_ref: wf_core::ChildExecutionReference) {
        self.hierarchy.register_child_ref(child_ref);
    }

    pub async fn unregister_child(&self, child_id: &Id) {
        for child_type in [
            wf_types::execution::ExecutionType::Workflow,
            wf_types::execution::ExecutionType::Agent,
        ] {
            if self.hierarchy.remove_child(child_id, &child_type) {
                break;
            }
        }
    }

    /// Read the shared status from a synchronous context. Tries a non-blocking
    /// `try_read` first (works on any runtime); when the lock is contended it
    /// blocks on the tokio runtime (multi-thread only, where `block_in_place`
    /// is safe). When no suitable runtime context exists — `block_in_place`
    /// would panic on a current-thread runtime and blocking outside tokio
    /// would deadlock — it infers a coherent status from the sync-visible
    /// signals (cancellation / interruption) instead of fabricating a fresh
    /// state.
    fn sync_status(&self) -> ExecutionStatus {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread {
                return tokio::task::block_in_place(|| {
                    handle.block_on(async { self.state.read().await.status() })
                });
            }
        }
        if self.cancellation.is_cancelled() {
            return ExecutionStatus::Cancelled;
        }
        match self.interruption.check() {
            Some(InterruptionSignal::Stop) => return ExecutionStatus::Cancelled,
            Some(InterruptionSignal::Pause) => return ExecutionStatus::Paused,
            _ => {}
        }
        ExecutionStatus::Running
    }
}

#[async_trait::async_trait]
impl ExecutionEntity for WorkflowExecutionEntity {
    fn id(&self) -> &Id {
        &self.id
    }

    fn status(&self) -> ExecutionStatus {
        if let Ok(state) = self.state.try_read() {
            return state.status();
        }
        self.sync_status()
    }

    fn is_running(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Running)
    }

    fn is_paused(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Paused)
    }

    fn is_completed(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Completed)
    }

    fn is_failed(&self) -> bool {
        matches!(self.status(), ExecutionStatus::Failed)
    }

    fn is_cancelled(&self) -> bool {
        matches!(
            self.status(),
            ExecutionStatus::Cancelled | ExecutionStatus::Stopped
        )
    }

    async fn pause(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.state.write().await.pause()?;
        self.interruption.pause()?;
        Ok(())
    }

    async fn resume(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        self.state.write().await.resume()?;
        self.interruption.resume()?;
        Ok(())
    }

    async fn stop(&self) -> Result<(), wf_execution_shared::error::ExecutionSharedError> {
        if self.state.read().await.status().is_terminal() {
            return Ok(());
        }
        self.state.write().await.cancel()?;
        self.interruption.stop()?;
        self.cancellation.cancel();
        Ok(())
    }

    async fn abort(&self) {
        self.cancellation.cancel();
    }

    fn get_abort_signal(&self) -> tokio_util::sync::CancellationToken {
        self.cancellation.clone()
    }

    fn get_hierarchy_depth(&self) -> u32 {
        self.hierarchy.depth()
    }

    fn get_root_execution_id(&self) -> Option<Id> {
        Some(self.hierarchy.root_execution_id())
    }

    fn get_ancestors(&self) -> Vec<Id> {
        self.hierarchy.ancestors()
    }

    fn hierarchy_manager(
        &self,
    ) -> Option<Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>> {
        Some(self.hierarchy.clone())
    }
}

impl wf_execution_shared::execution_loop::HasInterruption for WorkflowExecutionEntity {
    fn interruption(&self) -> &InterruptionState {
        &self.interruption
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_entity_starts_in_created_state() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        assert_eq!(entity.id(), "exec-1");
        assert_eq!(entity.workflow_id(), "wf-1");
        assert_eq!(entity.status(), ExecutionStatus::Created);
        assert!(!entity.is_running());
        assert!(entity.output().await.is_none());
    }

    #[tokio::test]
    async fn variables_and_node_results_round_trip() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        assert!(entity.get_variable("missing").is_none());
        entity.set_variable("count", serde_json::json!(3));
        assert_eq!(entity.get_variable("count"), Some(serde_json::json!(3)));

        assert!(entity.get_node_result("n1").is_none());
        entity.set_node_result("n1", serde_json::json!({"ok": true}));
        assert_eq!(
            entity.get_node_result("n1"),
            Some(serde_json::json!({"ok": true}))
        );
    }

    #[tokio::test]
    async fn output_can_be_set_once_and_read_back() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.set_output(serde_json::json!({"final": 42})).await;
        assert_eq!(
            entity.output().await,
            Some(serde_json::json!({"final": 42}))
        );
    }

    #[tokio::test]
    async fn pause_resume_flow_through_state_and_interruption() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.state.write().await.start().expect("start");

        entity.pause().await.expect("pause");
        assert_eq!(entity.status(), ExecutionStatus::Paused);
        assert!(entity.is_paused());
        assert!(!entity.is_running());

        entity.resume().await.expect("resume");
        assert_eq!(entity.status(), ExecutionStatus::Running);
        assert!(entity.is_running());
    }

    #[tokio::test]
    async fn stop_is_idempotent_and_cancels_the_token() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.state.write().await.start().expect("start");

        let signal = entity.get_abort_signal();
        assert!(!signal.is_cancelled());
        entity.stop().await.expect("stop");
        assert!(signal.is_cancelled());
        assert!(entity.is_cancelled());

        // A second stop on the already-terminal state must be a no-op Ok.
        entity.stop().await.expect("second stop");
    }

    #[tokio::test]
    async fn abort_cancels_without_touching_the_state() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.state.write().await.start().expect("start");
        entity.abort().await;
        assert!(entity.get_abort_signal().is_cancelled());
        // abort() only cancels the token; the state machine stays Running.
        assert!(entity.is_running());
    }

    #[test]
    fn root_execution_id_falls_back_through_the_hierarchy() {
        use std::sync::Arc;
        use wf_core::hierarchy::manager::ExecutionHierarchyManager;
        use wf_types::execution::ExecutionHierarchy;
        // Root: no parent -> self.
        let root = WorkflowExecutionEntity::new("root".to_string(), "wf-1".to_string());
        assert_eq!(root.get_root_execution_id(), Some("root".to_string()));
        assert_eq!(root.get_ancestors(), Vec::<Id>::new());
        assert_eq!(root.get_hierarchy_depth(), 0);

        // Child derived from the root manager -> parent is the root.
        let child_manager = Arc::new(ExecutionHierarchyManager::new(
            "root".to_string(),
            wf_types::execution::ExecutionType::Workflow,
        ))
        .derive_child(
            "child".to_string(),
            wf_types::execution::ExecutionType::Workflow,
            None,
        )
        .expect("derive");
        let child = WorkflowExecutionEntity::new("child".to_string(), "wf-1".to_string())
            .with_hierarchy_manager(child_manager);
        assert_eq!(child.get_root_execution_id(), Some("root".to_string()));

        // Grandchild restored from a snapshot hierarchy -> oldest ancestor
        // is the root, depth and chain preserved.
        let hierarchy = ExecutionHierarchy::new(
            "wf-1".to_string(),
            "gc".to_string(),
            vec!["root".to_string(), "child".to_string()],
            Some(wf_types::execution::ExecutionType::Workflow),
            Some(wf_types::execution::ExecutionType::Workflow),
            None,
        );
        let restored_manager = ExecutionHierarchyManager::restore(
            "gc".to_string(),
            wf_types::execution::ExecutionType::Workflow,
            &hierarchy,
            wf_types::execution::ExecutionType::Workflow,
        );
        let grandchild = WorkflowExecutionEntity::new("gc".to_string(), "wf-1".to_string())
            .with_hierarchy_manager(restored_manager);
        assert_eq!(grandchild.get_root_execution_id(), Some("root".to_string()));
        assert_eq!(
            grandchild.get_ancestors(),
            vec!["root".to_string(), "child".to_string()]
        );
        assert_eq!(grandchild.get_hierarchy_depth(), 2);
    }

    #[tokio::test]
    async fn child_registration_can_be_reversed() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.register_child("child-1".to_string()).await;
        entity.register_child("child-2".to_string()).await;
        assert_eq!(
            entity.child_ids().as_slice(),
            &["child-1".to_string(), "child-2".to_string()]
        );
        entity.unregister_child(&"child-1".to_string()).await;
        assert_eq!(entity.child_ids().as_slice(), &["child-2".to_string()]);
    }

    #[tokio::test]
    async fn failed_status_reports_through_the_entity_trait() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.state.write().await.start().expect("start");
        entity
            .state
            .write()
            .await
            .fail("boom".to_string())
            .expect("fail");
        assert!(entity.is_failed());
        assert!(!entity.is_completed());
        assert!(!entity.is_cancelled());
    }

    #[test]
    fn injection_policy_labels_parse() {
        assert_eq!(
            VariableInjectionPolicy::NextBoundary.label(),
            "next_boundary"
        );
        assert_eq!(VariableInjectionPolicy::OnResume.label(), "on_resume");
        assert_eq!(VariableInjectionPolicy::parse(None), None);
        assert_eq!(
            VariableInjectionPolicy::parse(Some("next_boundary")),
            Some(VariableInjectionPolicy::NextBoundary)
        );
        assert_eq!(
            VariableInjectionPolicy::parse(Some("on-resume")),
            Some(VariableInjectionPolicy::OnResume)
        );
        assert_eq!(VariableInjectionPolicy::parse(Some("nope")), None);
    }

    #[test]
    fn injection_sources_and_names_validate() {
        assert_eq!(
            normalize_injection_source(None).expect("default"),
            "api".to_string()
        );
        assert_eq!(
            normalize_injection_source(Some("tui")).expect("allowlisted"),
            "tui".to_string()
        );
        assert!(normalize_injection_source(Some("evil")).is_err());
        assert!(validate_injection_variable_name("speed").is_ok());
        assert!(validate_injection_variable_name("").is_err());
        assert!(validate_injection_variable_name("__loop").is_err());
        assert!(validate_injection_variable_name("a.b").is_err());
    }

    #[tokio::test]
    async fn injections_buffer_list_drain_and_remove() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        entity.state.write().await.start().expect("start");
        let first = entity
            .submit_variable_injection(
                "speed",
                serde_json::json!(1),
                Some("cli"),
                Some(VariableInjectionPolicy::NextBoundary),
            )
            .expect("submit");
        assert_eq!(first.source, "cli".to_string());
        let second = entity
            .submit_variable_injection(
                "held",
                serde_json::json!(true),
                None,
                Some(VariableInjectionPolicy::OnResume),
            )
            .expect("submit");
        assert_eq!(second.source, "api".to_string());
        assert_eq!(entity.list_variable_injections().len(), 2);

        // The boundary drain only takes its own policy and leaves the rest.
        let drained = entity.drain_variable_injections(VariableInjectionPolicy::NextBoundary);
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].variable_name, "speed".to_string());
        assert_eq!(entity.list_variable_injections().len(), 1);

        assert!(entity.remove_variable_injection(&second.id));
        assert!(!entity.remove_variable_injection(&second.id));
        assert!(entity.list_variable_injections().is_empty());
    }

    #[tokio::test]
    async fn injections_reject_bad_names_and_terminal_executions() {
        let entity = WorkflowExecutionEntity::new("exec-1".to_string(), "wf-1".to_string());
        assert!(entity
            .submit_variable_injection("", serde_json::json!(1), None, None)
            .is_err());
        assert!(entity
            .submit_variable_injection("__x", serde_json::json!(1), None, None)
            .is_err());
        assert!(entity
            .submit_variable_injection("x", serde_json::json!(1), Some("evil"), None)
            .is_err());
        entity.state.write().await.start().expect("start");
        entity.state.write().await.complete().expect("complete");
        assert!(entity
            .submit_variable_injection("x", serde_json::json!(1), None, None)
            .is_err());
    }
}
