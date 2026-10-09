//! Execution recovery: scan incomplete executions and drive them back to a
//! consistent state through the checkpoint + resume path.

pub mod api_executor;
pub mod orchestrator;
pub mod scanner;

pub use api_executor::ApiRecoveryExecutor;
pub use orchestrator::RecoveryOrchestrator;
pub use scanner::RecoveryScanner;

/// Outcome of one recovery attempt. `recovered` is `false` when the
/// execution could not be restarted (no checkpoint available, recovery not
/// wired, ...); the `note` carries the reason so callers can distinguish a
/// genuine recovery from a skipped one.
#[derive(Debug, Clone)]
pub struct RecoveryItem {
    pub execution_id: String,
    pub status: String,
    pub current_node_id: Option<String>,
    pub recovered: bool,
    pub note: Option<String>,
}

#[derive(Debug, Default)]
pub struct RecoveryResult {
    pub recovered: Vec<RecoveryItem>,
    pub failed: Vec<(String, String)>,
    /// Executions left untouched (no checkpoint / recovery not wired).
    pub skipped: Vec<RecoveryItem>,
}

impl RecoveryResult {
    pub fn is_empty(&self) -> bool {
        self.recovered.is_empty() && self.failed.is_empty() && self.skipped.is_empty()
    }
}

/// Unified recovery target covering both engines. Sorted by root group then
/// depth parent-first so a parent re-registers before its children resume.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum RecoveryTarget {
    Workflow(wf_types::WorkflowExecution),
    Agent(wf_types::AgentExecution),
}

impl RecoveryTarget {
    pub fn execution_id(&self) -> &str {
        match self {
            Self::Workflow(e) => e.id.as_str(),
            Self::Agent(e) => e.id.as_str(),
        }
    }

    pub fn depth(&self) -> u32 {
        match self {
            Self::Workflow(e) => e.hierarchy.as_ref().map_or(0, |h| h.depth()),
            Self::Agent(e) => e.hierarchy.as_ref().map_or(0, |h| h.depth()),
        }
    }

    pub fn root_id(&self) -> String {
        match self {
            // A record that carries no hierarchy is a root that never linked to
            // a parent, so it is its own root.
            Self::Workflow(e) => e
                .hierarchy
                .as_ref()
                .map_or_else(|| e.id.clone(), |h| h.root_execution_id()),
            Self::Agent(e) => e
                .hierarchy
                .as_ref()
                .map_or_else(|| e.id.clone(), |h| h.root_execution_id()),
        }
    }

    pub fn parent_id(&self) -> Option<String> {
        match self {
            Self::Workflow(e) => e.hierarchy.as_ref().and_then(|h| h.parent_execution_id()),
            Self::Agent(e) => e.hierarchy.as_ref().and_then(|h| h.parent_execution_id()),
        }
    }

    pub fn is_workflow(&self) -> bool {
        matches!(self, Self::Workflow(_))
    }
}

/// Backend that actually restarts an incomplete execution. Injected into the
/// [`RecoveryOrchestrator`] so the orchestrator stays storage-agnostic; the
/// runtime provides an API-backed implementation over the checkpoint +
/// resume path. The shared `ApiContext` is passed in per call (the context
/// itself is not cloneable, and the runtime owns exactly one live copy).
#[async_trait::async_trait]
pub trait RecoveryExecutor: Send + Sync {
    /// Recover one incomplete execution. Returns a `RecoveryItem` describing
    /// what happened (`recovered: false` + note when the execution could not
    /// be restarted but the scan itself succeeded), or `Err` when recovery
    /// failed at the infrastructure level.
    async fn recover_execution(
        &self,
        ctx: &wf_api::ApiContext,
        execution: &wf_types::WorkflowExecution,
    ) -> crate::error::RuntimeResult<RecoveryItem> {
        let _ = (ctx, execution);
        Ok(RecoveryItem {
            execution_id: execution.id.to_string(),
            status: format!("{:?}", execution.status),
            current_node_id: execution.current_node_id.clone(),
            recovered: false,
            note: Some("recovery executor does not handle workflow targets".to_string()),
        })
    }

    /// Recover one unified target (workflow or agent). Defaults to the
    /// workflow path for workflow targets and skips agents so existing
    /// executors keep working; the API executor overrides this to
    /// auto-recover both.
    async fn recover_target(
        &self,
        ctx: &wf_api::ApiContext,
        target: &RecoveryTarget,
    ) -> crate::error::RuntimeResult<RecoveryItem> {
        match target {
            RecoveryTarget::Workflow(e) => self.recover_execution(ctx, e).await,
            RecoveryTarget::Agent(e) => Ok(RecoveryItem {
                execution_id: e.id.to_string(),
                status: format!("{:?}", e.status),
                current_node_id: None,
                recovered: false,
                note: Some("agent executions are not auto-recovered by this executor".to_string()),
            }),
        }
    }
}
