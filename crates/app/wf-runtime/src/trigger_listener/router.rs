//! Routes matched trigger actions to their concrete runners.
//!
//! The listener holds a single [`TriggerActionRunner`]; this router is the
//! runtime assembly point dispatching by action type so compression
//! sub-workflows, nested agent executions, cold-start runs and in-context
//! actions (variable writes, stop/pause/skip/notification/script) can coexist
//! on one listener.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::warn;
use wf_types::events::BaseEvent;
use wf_types::trigger::{TriggerAction, TriggerTemplate};
use wf_workflow::error::WorkflowResult;
use wf_workflow::trigger::TriggerActionRunner;

use super::agent_runner::AgentTriggerRunner;
use super::compression::CompressionPipeline;
use super::context_runner::ContextTriggerRunner;
use super::creation_runner::CreationRunner;

/// Routes matched trigger actions to their concrete runners.
///
/// The listener holds a single [`TriggerActionRunner`]; this router is the
/// runtime assembly point dispatching by action type so compression
/// sub-workflows, nested agent executions, cold-start runs and in-context
/// actions (variable writes, stop/pause/skip/notification/script) can coexist
/// on one listener.
pub struct TriggerActionRouter {
    compression: Arc<dyn TriggerActionRunner>,
    agent: Option<Arc<AgentTriggerRunner>>,
    creation: Arc<CreationRunner>,
    context: Arc<ContextTriggerRunner>,
    /// Trigger-side pipeline serving the builtin compression route
    /// (`ExecuteContextCompression`). Absent when the listener was built
    /// without the route; a routed match then fails loudly instead of
    /// silently dropping the handoff.
    routed_compression: Option<Arc<CompressionPipeline>>,
}

impl TriggerActionRouter {
    pub fn new(
        compression: Arc<dyn TriggerActionRunner>,
        agent: Option<Arc<AgentTriggerRunner>>,
        creation: Arc<CreationRunner>,
        context: Arc<ContextTriggerRunner>,
    ) -> Self {
        Self {
            compression,
            agent,
            creation,
            context,
            routed_compression: None,
        }
    }

    pub(crate) fn with_routed_compression(mut self, pipeline: Arc<CompressionPipeline>) -> Self {
        self.routed_compression = Some(pipeline);
        self
    }
}

#[async_trait]
impl TriggerActionRunner for TriggerActionRouter {
    async fn run(&self, template: &TriggerTemplate, event: &BaseEvent) -> WorkflowResult<()> {
        // Defense in depth: the matcher already drops execution-less events
        // for non-creation actions, but templates can be registered around
        // validation. Fail loudly instead of letting the context runner
        // silently skip on the missing execution id.
        if event.execution_id.is_none()
            && !template
                .action
                .as_ref()
                .is_some_and(|action| action.is_execution_creating())
        {
            return Err(wf_workflow::error::WorkflowError::TriggerError(format!(
                "Trigger '{}' matched an execution-less event without an execution-creating action; skipping",
                template.name
            )));
        }
        match &template.action {
            Some(TriggerAction::ExecuteTriggeredSubworkflow { .. }) => {
                self.compression.run(template, event).await
            }
            // Builtin compression handoffs run through the trigger-side
            // pipeline (claim, retry, write-back, terminal events).
            Some(TriggerAction::ExecuteContextCompression {}) => match &self.routed_compression {
                Some(pipeline) => pipeline.run_routed(template, event).await,
                None => Err(wf_workflow::error::WorkflowError::TriggerError(format!(
                    "Trigger '{}' matched the builtin compression route without a wired pipeline; skipping",
                    template.name
                ))),
            },
            // Cold-start workflow runs need no emitting execution.
            Some(TriggerAction::ExecuteWorkflow { .. }) => self.creation.run(template, event).await,
            Some(
                TriggerAction::ExecuteTriggeredAgentExecution { .. }
                | TriggerAction::ExecuteAgent { .. },
            ) => match &self.agent {
                Some(agent) => agent.run(template, event).await,
                None => {
                    warn!(
                        "Trigger '{}' matched with a nested agent action but no agent executor is \
                         wired; skipping",
                        template.name
                    );
                    Ok(())
                }
            },
            // Remaining actions (set_variable, stop/pause/resume, skip_node,
            // send_notification, execute_script, set/append_message_context)
            // run against the emitting execution's live context.
            _ => self.context.run(template, event).await,
        }
    }
}
