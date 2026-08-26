//! Execution recovery: scan incomplete executions and drive them back to a
//! consistent state through the checkpoint + resume path.

pub mod orchestrator;
pub mod scanner;

#[cfg(feature = "checkpoint")]
pub mod api_executor;

pub use orchestrator::RecoveryOrchestrator;
pub use scanner::RecoveryScanner;

#[cfg(feature = "checkpoint")]
pub use api_executor::ApiRecoveryExecutor;

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
    ) -> crate::error::RuntimeResult<RecoveryItem>;
}
