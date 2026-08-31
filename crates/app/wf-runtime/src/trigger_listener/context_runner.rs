//! Executes in-context trigger actions against the *emitting* workflow
//! execution (variable writes, stop/pause/resume, skip, notification,
//! script, message context) through the shared [`TriggerCoordinator`].

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_core::internal_signal::InternalSignalBus;
use wf_core::EventBus;
use wf_types::events::BaseEvent;
use wf_types::node::StaticNodeType;
use wf_types::trigger::TriggerTemplate;
use wf_types::Id;
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::handler::NodeHandler;
use wf_workflow::trigger_listener::TriggerActionRunner;
use wf_workflow::{TriggerContext, TriggerCoordinator};

use super::ExecutionContextRegistry;

/// Executes in-context trigger actions against the *emitting* workflow
/// execution: the actions the message node handlers support
/// (`SetVariable`, `StopWorkflowExecution`, `PauseWorkflowExecution`,
/// `ResumeWorkflowExecution`, `SkipNode`, `SendNotification`,
/// `ExecuteScript`, `SetMessageContext`, `AppendMessageContext`) but driven
/// event-driven through the listener.
///
/// The runner resolves the event's execution to its live variable map via
/// the [`ExecutionContextRegistry`] and runs the shared
/// [`TriggerCoordinator`] against it, so a variable write lands in the
/// executing workflow's map, a stop/pause publishes a typed signal the
/// coordinator consumes, and the emitted events carry the execution ids.
///
/// Events without a registered execution context are skipped (agent
/// sessions are not registered here; agent-facing actions use
/// [`crate::trigger_listener::AgentTriggerRunner`] instead).
/// Default timeout for context trigger actions (set_variable, stop, pause).
const CONTEXT_TRIGGER_DEFAULT_TIMEOUT_MS: u64 = 5_000;

#[derive(Clone)]
pub struct ContextTriggerRunnerConfig {
    /// Timeout per execute call. Defaults to 5 seconds.
    pub timeout: Option<std::time::Duration>,
}

impl Default for ContextTriggerRunnerConfig {
    fn default() -> Self {
        Self {
            timeout: Some(std::time::Duration::from_millis(
                CONTEXT_TRIGGER_DEFAULT_TIMEOUT_MS,
            )),
        }
    }
}

pub struct ContextTriggerRunner {
    bus: Arc<EventBus>,
    contexts: Arc<ExecutionContextRegistry>,
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
    shutdown: CancellationToken,
    config: ContextTriggerRunnerConfig,
    signal_bus: Option<Arc<InternalSignalBus>>,
}

impl ContextTriggerRunner {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        bus: Arc<EventBus>,
        contexts: Arc<ExecutionContextRegistry>,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
        tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            bus,
            contexts,
            handlers,
            tool_registry,
            shutdown,
            config: ContextTriggerRunnerConfig::default(),
            signal_bus: None,
        }
    }

    /// Override the default timeout configuration.
    pub fn with_config(mut self, config: ContextTriggerRunnerConfig) -> Self {
        self.config = config;
        self
    }

    /// Inject the typed signal bus: control actions (stop/pause/resume/skip)
    /// publish typed signals in addition to the legacy variable protocol.
    pub fn with_signal_bus(mut self, bus: Option<Arc<InternalSignalBus>>) -> Self {
        self.signal_bus = bus;
        self
    }
}

#[async_trait]
impl TriggerActionRunner for ContextTriggerRunner {
    async fn run(&self, template: &TriggerTemplate, event: &BaseEvent) -> WorkflowResult<()> {
        let Some(action) = &template.action else {
            return Ok(());
        };
        let Some(execution_id) = event.execution_id.clone() else {
            return Ok(());
        };
        let Some(variables) = self.contexts.variables_for(&execution_id) else {
            debug!(
                "Trigger '{}' matched but execution {} has no live context; skipping",
                template.name, execution_id
            );
            return Ok(());
        };

        let workflow_id = event
            .workflow_id
            .clone()
            .unwrap_or_else(|| Id::from(execution_id.clone()));
        let mut tctx = TriggerContext::new(Id::from(execution_id.clone()), workflow_id)
            .with_variables(variables)
            .with_event_bus(self.bus.clone())
            .with_handlers(self.handlers.clone())
            .with_cancellation(self.shutdown.clone());
        if let Some(registry) = &self.tool_registry {
            tctx = tctx.with_tool_registry(registry.clone());
        }
        if let Some(bus) = &self.signal_bus {
            tctx = tctx.with_signal_bus(bus.clone());
        }

        let timeout = self.config.timeout.unwrap_or_else(|| {
            std::time::Duration::from_millis(CONTEXT_TRIGGER_DEFAULT_TIMEOUT_MS)
        });
        let result = tokio::time::timeout(
            timeout,
            TriggerCoordinator::execute(action, &template.name, &tctx),
        )
        .await
        .unwrap_or_else(|_| {
            warn!(
                "Context trigger '{}' timed out after {:?}",
                template.name, timeout
            );
            wf_types::trigger::TriggerExecutionResult {
                trigger_id: wf_types::Id::from(template.name.clone()),
                success: false,
                execution_id: None,
                result: None,
                error: Some(format!(
                    "ContextTriggerRunner timed out after {:?}",
                    timeout
                )),
                execution_time: 0,
            }
        });
        if result.success {
            Ok(())
        } else {
            Err(WorkflowError::TriggerError(
                result
                    .error
                    .unwrap_or_else(|| "trigger action failed".to_string()),
            ))
        }
    }
}
