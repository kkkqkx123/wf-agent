//! Ports through which the trigger machinery reaches runtime services.
//!
//! wf-workflow stays decoupled from wf-resource and from concrete trigger
//! business: templates are looked up through [`TriggerTemplateRegistry`],
//! sub-workflow execution through [`SubworkflowRunner`] and the action itself
//! through [`TriggerActionRunner`], all implemented by wf-runtime during
//! assembly.

use async_trait::async_trait;

use wf_types::events::BaseEvent;
use wf_types::trigger::TriggerTemplate;

use crate::error::WorkflowResult;

/// Lookup source for trigger templates. Implemented by wf-runtime over the
/// wf-resource registrar, where the predefined templates are registered.
pub trait TriggerTemplateRegistry: Send + Sync {
    /// All enabled trigger templates to match events against.
    fn templates(&self) -> Vec<TriggerTemplate>;
}

/// Executes a triggered sub-workflow and returns its final output.
/// Implemented by wf-runtime over the workflow coordinator. Used by
/// concrete [`TriggerActionRunner`] implementations (e.g. the compression
/// chain runner) that need to run a workflow from a trigger event.
#[async_trait]
pub trait SubworkflowRunner: Send + Sync {
    async fn run(
        &self,
        workflow_id: &str,
        input: serde_json::Value,
    ) -> WorkflowResult<serde_json::Value>;
}

/// Executes the action of a matched trigger template for one event.
///
/// Implemented by wf-runtime: `SubworkflowActionRunner` handles the
/// user-template sub-workflow action; other runners can be assembled for
/// other event/action combinations. The context-compression chain itself no
/// longer runs through the listener — the engine dispatches the
/// `CONTEXT_COMPRESSION_REQUESTED` signal to hook receivers registered on
/// the shared hook registry (see `wf-runtime`'s `CompressionService`).
#[async_trait]
pub trait TriggerActionRunner: Send + Sync {
    /// Run the action of `template` for `event`. Best-effort: the listener
    /// logs failures, never propagates them to the emitting execution.
    async fn run(&self, template: &TriggerTemplate, event: &BaseEvent) -> WorkflowResult<()>;
}
