use std::sync::Arc;

use async_trait::async_trait;
use wf_types::events::BaseEvent;
use wf_types::trigger::TriggerTemplate;
use wf_types::Id;

#[async_trait]
pub trait TriggerExecutionRecorder: Send + Sync {
    async fn record(
        &self,
        metadata: wf_types::TriggerExecutionStorageMetadata,
    ) -> Result<(), wf_storage::error::StorageError>;
}

#[async_trait]
impl<S> TriggerExecutionRecorder for S
where
    S: wf_storage::adapter::trigger_execution::TriggerExecutionStorageAdapter,
{
    async fn record(
        &self,
        metadata: wf_types::TriggerExecutionStorageMetadata,
    ) -> Result<(), wf_storage::error::StorageError> {
        self.save(&metadata).await
    }
}

/// Durable-ledger collaborators shared by every trigger-action runner: the
/// optional execution recorder and the checkpoint trigger-state registry.
/// Wrapped in an `Arc` so all runners and the listener handle observe one
/// shared write-failure count.
#[derive(Default)]
pub struct TriggerLedger {
    pub storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    pub trigger_state_registry: Option<Arc<wf_workflow::TriggerStateRegistry>>,
    /// Ledger write failures (recorder present but the durable write
    /// errored). The ledger stays best-effort — a failed write never blocks
    /// the emitter — but the count makes "the fallback itself is broken"
    /// observable through the listener handle.
    pub(crate) write_failures: std::sync::atomic::AtomicU64,
}

impl TriggerLedger {
    pub fn new(
        storage: Option<Arc<dyn TriggerExecutionRecorder>>,
        trigger_state_registry: Option<Arc<wf_workflow::TriggerStateRegistry>>,
    ) -> Self {
        Self {
            storage,
            trigger_state_registry,
            write_failures: std::sync::atomic::AtomicU64::default(),
        }
    }

    pub fn write_failures(&self) -> u64 {
        self.write_failures
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Result of one trigger-action execution, recorded to the durable ledger.
pub(crate) struct TriggerOutcome<'a> {
    pub(crate) action_type: &'a str,
    pub(crate) outcome: wf_types::TriggerExecutionOutcome,
    pub(crate) error: Option<String>,
    pub(crate) execution_time_ms: i64,
    pub(crate) child_execution_id: Option<Id>,
}

/// Record a trigger execution in the optional durable ledger (management
/// surface). Best-effort: storage failures never propagate to the emitter,
/// but each failed write is logged at `error` level and counted on the
/// ledger.
pub(crate) async fn record_trigger_execution(
    ledger: &Option<Arc<TriggerLedger>>,
    template: &TriggerTemplate,
    event: &BaseEvent,
    outcome: TriggerOutcome<'_>,
) {
    let TriggerOutcome {
        action_type,
        outcome,
        error,
        execution_time_ms,
        child_execution_id,
    } = outcome;
    let Some(ledger) = ledger.as_ref() else {
        return;
    };
    let Some(storage) = ledger.storage.as_ref() else {
        return;
    };
    let metadata = wf_types::TriggerExecutionStorageMetadata {
        id: Id::new(),
        trigger_name: template.name.clone(),
        trigger_type: "event".to_string(),
        event: event.r#type.as_str().to_string(),
        execution_id: child_execution_id.or_else(|| event.execution_id.clone()),
        workflow_id: event.workflow_id.clone(),
        outcome,
        result: None,
        error,
        action_type: Some(action_type.to_string()),
        execution_time_ms,
        triggered_at: event.timestamp,
    };
    if let Err(e) = storage.record(metadata).await {
        ledger
            .write_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        tracing::error!(
            "Failed to record trigger execution '{}' ({}): {}",
            template.name,
            action_type,
            e
        );
    }
}
