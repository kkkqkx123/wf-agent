//! Cold-start workflow runner behind `TriggerAction::ExecuteWorkflow`.
//!
//! Unlike the compression runner (which runs a sub-workflow over the emitting
//! execution's message snapshot and writes the result back into it), this
//! runner starts a fresh run: the input comes from the action itself, falling
//! back to the producer-stamped `trigger_input` event metadata (scheduler
//! creation targets and webhook ingress share that contract). There is no
//! emitting execution, so there is no write-back and no trigger-state audit
//! entry; the run is recorded in the durable trigger ledger only.

use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use wf_types::events::BaseEvent;
use wf_types::trigger::{TriggerAction, TriggerTemplate};
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::trigger::{SubworkflowRunner, TriggerActionRunner};

use super::scheduler::TRIGGER_INPUT_METADATA_KEY;
use super::{
    record_trigger_execution, TriggerExecutionRecorder, TriggerOutcome, DEFAULT_TRIGGER_TIMEOUT_MS,
};

/// Cold-start workflow runner behind `TriggerAction::ExecuteWorkflow`.
pub struct CreationRunner {
    runner: Arc<dyn SubworkflowRunner>,
    shutdown: CancellationToken,
    storage: Option<Arc<dyn TriggerExecutionRecorder>>,
}

impl CreationRunner {
    pub fn new(
        runner: Arc<dyn SubworkflowRunner>,
        shutdown: CancellationToken,
        storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    ) -> Self {
        Self {
            runner,
            shutdown,
            storage,
        }
    }
}

#[async_trait]
impl TriggerActionRunner for CreationRunner {
    async fn run(&self, template: &TriggerTemplate, event: &BaseEvent) -> WorkflowResult<()> {
        let Some(TriggerAction::ExecuteWorkflow {
            workflow_id,
            input,
            timeout,
        }) = &template.action
        else {
            return Ok(());
        };
        let input = input
            .clone()
            .or_else(|| {
                event
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.get(TRIGGER_INPUT_METADATA_KEY).cloned())
            })
            .unwrap_or(serde_json::Value::Null);
        let timeout_ms = timeout.unwrap_or(DEFAULT_TRIGGER_TIMEOUT_MS);
        let start = wf_common::now();

        let run = self.runner.run(workflow_id, input);
        let outcome = tokio::select! {
            output = tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                run,
            ) => match output {
                Ok(Ok(_)) => (true, None),
                Ok(Err(e)) => (false, Some(e.to_string())),
                Err(_) => (false, Some(format!(
                    "Cold-started workflow '{}' timed out after {}ms",
                    workflow_id, timeout_ms
                ))),
            },
            _ = self.shutdown.cancelled() => {
                (false, Some("aborted at listener shutdown".to_string()))
            }
        };
        let (success, error) = outcome;
        if let Some(error) = &error {
            warn!(
                "Cold-started workflow '{}' for trigger '{}' failed: {}",
                workflow_id, template.name, error
            );
        }
        record_trigger_execution(
            &self.storage,
            template,
            event,
            TriggerOutcome {
                action_type: "execute_workflow",
                success,
                error: error.clone(),
                execution_time_ms: wf_common::now() - start,
                child_execution_id: None,
            },
        )
        .await;
        if success {
            Ok(())
        } else {
            Err(WorkflowError::TriggerError(error.unwrap_or_else(|| {
                "cold-start workflow failed".to_string()
            })))
        }
    }
}
