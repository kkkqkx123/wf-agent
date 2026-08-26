//! Routes matched trigger actions to their concrete runners.
//!
//! The listener holds a single [`TriggerActionRunner`]; this router is the
//! runtime assembly point dispatching by action type so compression
//! sub-workflows, nested agent executions and in-context actions (variable
//! writes, stop/pause/skip/notification/script) can coexist on one listener.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::warn;
use wf_types::events::BaseEvent;
use wf_types::trigger::{TriggerAction, TriggerTemplate};
use wf_workflow::error::WorkflowResult;
use wf_workflow::trigger_listener::TriggerActionRunner;

use super::agent_runner::AgentTriggerRunner;
use super::context_runner::ContextTriggerRunner;

/// Routes matched trigger actions to their concrete runners.
///
/// The listener holds a single [`TriggerActionRunner`]; this router is the
/// runtime assembly point dispatching by action type so compression
/// sub-workflows, nested agent executions and in-context actions (variable
/// writes, stop/pause/skip/notification/script) can coexist on one listener.
pub struct TriggerActionRouter {
    compression: Arc<dyn TriggerActionRunner>,
    agent: Option<Arc<AgentTriggerRunner>>,
    context: Arc<ContextTriggerRunner>,
}

impl TriggerActionRouter {
    pub fn new(
        compression: Arc<dyn TriggerActionRunner>,
        agent: Option<Arc<AgentTriggerRunner>>,
        context: Arc<ContextTriggerRunner>,
    ) -> Self {
        Self {
            compression,
            agent,
            context,
        }
    }
}

#[async_trait]
impl TriggerActionRunner for TriggerActionRouter {
    async fn run(&self, template: &TriggerTemplate, event: &BaseEvent) -> WorkflowResult<()> {
        match &template.action {
            Some(TriggerAction::ExecuteTriggeredSubworkflow { .. }) => {
                self.compression.run(template, event).await
            }
            Some(TriggerAction::ExecuteTriggeredAgentExecution { .. }) => match &self.agent {
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
