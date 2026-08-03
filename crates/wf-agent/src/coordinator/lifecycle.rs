use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use wf_checkpoint::event::CheckpointEventBus;
use wf_checkpoint::execution_events::ExecutionEventBus;
use wf_core::event::EventBus;
use wf_execution_shared::hooks::executor::HookExecutor;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_llm::messaging::conversation_session::ConversationSession;
use wf_llm::LlmGateway;
use wf_metrics::MetricsRegistry;
use wf_storage::backend::StorageBackend;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput};
use wf_tools::registry::ToolRegistry;
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::message::Message;
use wf_types::tool::approval::ToolApprovalOptions;

use crate::approval::ToolApprovalHandler;
use crate::checkpoint::{AgentCheckpointIntegration, AgentCheckpointStrategy};
use crate::conversation_compression::spawn_conversation_compression_consumer;
use crate::coordinator::execution::AgentExecutionCoordinator;
use crate::coordinator::iteration::{AgentIterationCoordinator, DEFAULT_TOKEN_WARNING_THRESHOLD};
use crate::coordinator::state_transitor::AgentLoopStateTransitor;
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::hook::AgentHookHandler;
use crate::stream::{AgentEventSink, AgentEventStream, AgentStreamEvent};
use tokio::sync::RwLock;

pub struct AgentLoopCoordinator {
    gateway: Arc<LlmGateway>,
    tool_registry: Arc<ToolRegistry>,
    hook_executor: Arc<HookExecutor>,
    event_bus: Option<Arc<EventBus>>,
    store: Arc<StorageBackend>,
    checkpoint_strategy: Option<AgentCheckpointStrategy>,
    checkpoint_event_bus: Option<CheckpointEventBus>,
    checkpoint_execution_events: Option<ExecutionEventBus>,
    metrics: Option<Arc<MetricsRegistry>>,
    approval_options: Option<ToolApprovalOptions>,
    approval_handler: Option<Arc<dyn ToolApprovalHandler>>,
    max_pause_duration: Option<u64>,
}

impl AgentLoopCoordinator {
    pub fn new(gateway: Arc<LlmGateway>, tool_registry: Arc<ToolRegistry>) -> Self {
        Self::with_store(
            gateway,
            tool_registry,
            Arc::new(StorageBackend::new_memory()),
        )
    }

    pub fn with_store(
        gateway: Arc<LlmGateway>,
        tool_registry: Arc<ToolRegistry>,
        store: Arc<StorageBackend>,
    ) -> Self {
        let hook_executor = Arc::new(HookExecutor::new());
        Self {
            gateway,
            tool_registry,
            hook_executor,
            event_bus: None,
            store,
            checkpoint_strategy: None,
            checkpoint_event_bus: None,
            checkpoint_execution_events: None,
            metrics: None,
            approval_options: None,
            approval_handler: None,
            max_pause_duration: None,
        }
    }

    pub fn with_hook_executor(mut self, hook_executor: Arc<HookExecutor>) -> Self {
        self.hook_executor = hook_executor;
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_checkpoint_strategy(mut self, strategy: AgentCheckpointStrategy) -> Self {
        self.checkpoint_strategy = Some(strategy);
        self
    }

    pub fn with_checkpoint_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.checkpoint_event_bus = Some(bus);
        self
    }

    /// Register the execution event bus; `state_changed` events are published
    /// after every checkpoint creation.
    pub fn with_checkpoint_execution_events(mut self, bus: ExecutionEventBus) -> Self {
        self.checkpoint_execution_events = Some(bus);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Register an external tool approval handler. Tools that require
    /// confirmation are routed through it; without a handler and without
    /// explicit options every tool call is auto-approved.
    pub fn with_approval_handler(mut self, handler: Arc<dyn ToolApprovalHandler>) -> Self {
        self.approval_handler = Some(handler);
        self
    }

    pub fn with_approval_options(mut self, options: ToolApprovalOptions) -> Self {
        self.approval_options = Some(options);
        self
    }

    /// Max duration the agent loop may stay paused before it is stopped
    /// (mirrors TS maxPauseDuration).
    pub fn with_max_pause_duration(mut self, duration_ms: u64) -> Self {
        self.max_pause_duration = Some(duration_ms);
        self
    }

    /// Spawn the conversation compression consumer for the live session
    /// (self-consumption, compression chain closure): completed compression
    /// events matching `agent_loop_id` are applied to the conversation with
    /// a version check. Returns the task handle, aborted on every exit path
    /// of the execution.
    fn spawn_compression_consumer(
        &self,
        agent_loop_id: &str,
        conversation: Arc<RwLock<ConversationSession>>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        self.event_bus.as_ref().map(|bus| {
            spawn_conversation_compression_consumer(
                bus.clone(),
                agent_loop_id.to_string(),
                conversation,
            )
        })
    }

    pub async fn execute(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        let entity = self.build_entity(&config, input).await?;
        let execution_id = entity.id().clone();

        // The conversation applies compression results itself (it subscribes
        // to COMPLETED events on the bus); the consumer is aborted once the
        // loop finishes.
        let consumer =
            self.spawn_compression_consumer(&execution_id, entity.conversation().clone());
        let outcome = self.execute_inner(config, entity).await;
        if let Some(handle) = consumer {
            handle.abort();
        }
        outcome
    }

    async fn execute_inner(
        &self,
        config: AgentLoopConfig,
        entity: AgentLoopEntity,
    ) -> AgentResult<AgentLoopOutput> {
        let execution_id = entity.id().clone();

        AgentLoopStateTransitor::start_agent_loop(&entity, self.event_bus.as_deref()).await?;

        let checkpoint = self.build_checkpoint_integration();
        if let Some(ref cp) = checkpoint {
            cp.create_checkpoint(&entity, CheckpointTrigger::Manual)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to create agent start checkpoint: {}", e);
                });
        }

        let mut coordinator = AgentIterationCoordinator::new(
            self.gateway.clone(),
            self.tool_registry.clone(),
            self.hook_executor.clone(),
            self.metrics.clone(),
        )
        .with_approval(self.approval_options.clone(), self.approval_handler.clone())
        .with_token_warning_threshold(
            config
                .token_warning_threshold
                .unwrap_or(DEFAULT_TOKEN_WARNING_THRESHOLD),
        )
        .with_token_tracking_enabled(config.enable_token_tracking.unwrap_or(true));
        if let Some(ref bus) = self.event_bus {
            coordinator = coordinator.with_event_bus(bus.clone());
        }
        let iteration_coordinator = Arc::new(coordinator);
        let execution_coordinator = AgentExecutionCoordinator::new(iteration_coordinator)
            .with_checkpoint(checkpoint)
            .with_metrics(self.metrics.clone());

        let profile_id = entity.model().to_string();
        if let Some(ref metrics) = self.metrics {
            metrics
                .agent()
                .record_execution_start(&profile_id, &execution_id);
            metrics.agent_loop().record_execution_start(&execution_id);
        }

        let max_iterations = config.max_iterations.unwrap_or(10);
        let start = wf_common::now();
        match execution_coordinator
            .execute(&entity, max_iterations, config.max_execution_time)
            .await
        {
            Ok((result, iterations)) => {
                let duration_ms = (wf_common::now() - start) as f64;
                if result.completion_data.is_some() || !result.should_continue {
                    AgentLoopStateTransitor::complete_agent_loop(
                        &entity,
                        self.event_bus.as_deref(),
                    )
                    .await?;
                }
                if let Some(ref metrics) = self.metrics {
                    metrics.agent().record_execution_complete(
                        &profile_id,
                        &execution_id,
                        true,
                        duration_ms,
                    );
                    metrics.agent_loop().record_execution_complete(
                        &execution_id,
                        true,
                        duration_ms,
                    );
                }
                let mut hook_data = HashMap::new();
                hook_data.insert(
                    "total_iterations".to_string(),
                    Value::Number(iterations.into()),
                );
                AgentHookHandler::execute_agent_hook(
                    &self.hook_executor,
                    &entity,
                    "AFTER_AGENT",
                    hook_data,
                )
                .await
                .map_err(|e| AgentError::HookError(e.to_string()))?;

                let conversation = entity.conversation().read().await.messages().to_vec();
                Ok(AgentLoopOutput {
                    result: result.content,
                    iterations,
                    conversation,
                })
            }
            Err(e) => {
                let duration_ms = (wf_common::now() - start) as f64;
                AgentLoopStateTransitor::fail_agent_loop(
                    &entity,
                    e.to_string(),
                    self.event_bus.as_deref(),
                )
                .await?;
                if let Some(ref metrics) = self.metrics {
                    metrics.agent().record_execution_complete(
                        &profile_id,
                        &execution_id,
                        false,
                        duration_ms,
                    );
                    metrics.agent_loop().record_execution_complete(
                        &execution_id,
                        false,
                        duration_ms,
                    );
                    metrics
                        .agent_loop()
                        .record_error(&execution_id, "agent_loop");
                }
                Err(e)
            }
        }
    }

    fn build_checkpoint_integration(&self) -> Option<AgentCheckpointIntegration> {
        let strategy = self.checkpoint_strategy.as_ref()?;
        let mut cp = AgentCheckpointIntegration::new(self.store.clone());
        if let Some(ref bus) = self.checkpoint_event_bus {
            cp = cp.with_event_bus(bus.clone());
        }
        if let Some(ref bus) = self.checkpoint_execution_events {
            cp = cp.with_execution_event_bus(bus.clone());
        }
        let _ = strategy;
        Some(cp)
    }

    async fn build_entity(
        &self,
        config: &AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopEntity> {
        let hooks: Vec<BaseHookDefinition> = config
            .hooks
            .iter()
            .map(|h| BaseHookDefinition {
                id: wf_common::generate_id(),
                hook_type: h.hook_type.clone(),
                weight: 0,
                condition: h.condition.clone(),
                enabled: h.enabled,
                parallel: h.parallel.unwrap_or(true),
                continue_on_error: h.continue_on_error.unwrap_or(true),
            })
            .collect();

        let mut entity = AgentLoopEntity::new(config.agent_id.clone())
            .with_hooks(hooks)
            .with_model(config.model.clone());

        if !config.available_tool_names.is_empty() {
            entity = entity.with_available_tool_names(config.available_tool_names.clone());
        }

        if let Some(ref format) = config.tool_call_format {
            entity = entity.with_tool_call_format(format.clone());
        }

        if let Some(duration) = self.max_pause_duration {
            entity = entity.with_max_pause_duration(duration);
        }

        if let Some(ref bus) = self.event_bus {
            entity.interruption().set_event_bus(bus.clone());
        }

        for msg in &input.conversation {
            entity.conversation().write().await.add_message(msg.clone());
        }

        if config.enable_token_tracking.unwrap_or(true) {
            if let Some(token_limit) = config.token_limit.filter(|&l| l > 0) {
                entity
                    .conversation()
                    .write()
                    .await
                    .set_token_limit(token_limit);
            }
        }

        if !input.message.is_empty() {
            let msg = Message {
                id: wf_common::generate_id(),
                role: wf_types::message::MessageRole::User,
                content: wf_types::message::MessageContentValue::Text(input.message),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            };
            entity.conversation().write().await.add_message(msg);
        }

        Ok(entity)
    }

    /// Stream execution of the agent loop. Events (message deltas, tool
    /// lifecycle, iteration boundaries, final outcome) flow through the
    /// returned stream; execution state is updated as with `execute`.
    pub async fn execute_stream(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentEventStream {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let bus = self.event_bus.clone();
        let gateway = self.gateway.clone();
        let tool_registry = self.tool_registry.clone();
        let hook_executor = self.hook_executor.clone();
        let store = self.store.clone();
        let checkpoint_strategy = self.checkpoint_strategy.clone();
        let checkpoint_event_bus = self.checkpoint_event_bus.clone();
        let metrics = self.metrics.clone();
        let approval_options = self.approval_options.clone();
        let approval_handler = self.approval_handler.clone();
        let max_pause_duration = self.max_pause_duration;
        let hooks: Vec<BaseHookDefinition> = config
            .hooks
            .iter()
            .map(|h| BaseHookDefinition {
                id: wf_common::generate_id(),
                hook_type: h.hook_type.clone(),
                weight: 0,
                condition: h.condition.clone(),
                enabled: h.enabled,
                parallel: h.parallel.unwrap_or(true),
                continue_on_error: h.continue_on_error.unwrap_or(true),
            })
            .collect();

        tokio::spawn(async move {
            let mut entity = AgentLoopEntity::new(config.agent_id.clone())
                .with_hooks(hooks)
                .with_model(config.model.clone());
            if !config.available_tool_names.is_empty() {
                entity = entity.with_available_tool_names(config.available_tool_names.clone());
            }
            if let Some(ref format) = config.tool_call_format {
                entity = entity.with_tool_call_format(format.clone());
            }
            if let Some(duration) = max_pause_duration {
                entity = entity.with_max_pause_duration(duration);
            }
            if let Some(ref eb) = bus {
                entity.interruption().set_event_bus(eb.clone());
            }
            for msg in &input.conversation {
                entity.conversation().write().await.add_message(msg.clone());
            }
            if !input.message.is_empty() {
                let msg = Message {
                    id: wf_common::generate_id(),
                    role: wf_types::message::MessageRole::User,
                    content: wf_types::message::MessageContentValue::Text(input.message),
                    timestamp: wf_common::now(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: None,
                    thinking: None,
                    metadata: None,
                };
                entity.conversation().write().await.add_message(msg);
            }

            // The conversation applies compression results itself (it
            // subscribes to COMPLETED events on the bus); the consumer is
            // aborted on every exit path of the spawned task.
            let execution_id = entity.id().clone();
            let consumer = bus.as_ref().map(|eb| {
                spawn_conversation_compression_consumer(
                    eb.clone(),
                    execution_id.clone(),
                    entity.conversation().clone(),
                )
            });

            if let Err(e) = AgentLoopStateTransitor::start_agent_loop(&entity, bus.as_deref()).await
            {
                if let Some(handle) = consumer {
                    handle.abort();
                }
                let _ = tx
                    .send(AgentStreamEvent::Failed {
                        error: e.to_string(),
                    })
                    .await;
                return;
            }

            let checkpoint = {
                let strategy = checkpoint_strategy.as_ref();
                let mut cp = AgentCheckpointIntegration::new(store);
                if let Some(ref ceb) = checkpoint_event_bus {
                    cp = cp.with_event_bus(ceb.clone());
                }
                let _ = strategy;
                Some(cp)
            };

            let sink = AgentEventSink::new(tx.clone(), bus.clone());
            let iteration = Arc::new(
                AgentIterationCoordinator::new(
                    gateway,
                    tool_registry,
                    hook_executor.clone(),
                    metrics.clone(),
                )
                .with_approval(approval_options, approval_handler)
                .with_streaming(sink),
            );
            let execution_coordinator = AgentExecutionCoordinator::new(iteration)
                .with_checkpoint(checkpoint)
                .with_metrics(metrics.clone());

            let max_iterations = config.max_iterations.unwrap_or(10);
            match execution_coordinator
                .execute(&entity, max_iterations, config.max_execution_time)
                .await
            {
                Ok((result, iterations)) => {
                    if result.completion_data.is_some() || !result.should_continue {
                        let _ =
                            AgentLoopStateTransitor::complete_agent_loop(&entity, bus.as_deref())
                                .await;
                    }
                    let mut hook_data = HashMap::new();
                    hook_data.insert(
                        "total_iterations".to_string(),
                        Value::Number(iterations.into()),
                    );
                    let _ = AgentHookHandler::execute_agent_hook(
                        &hook_executor,
                        &entity,
                        "AFTER_AGENT",
                        hook_data,
                    )
                    .await;
                    if let Some(handle) = consumer {
                        handle.abort();
                    }
                    let _ = tx
                        .send(AgentStreamEvent::Completed {
                            result: result.content,
                            iterations,
                        })
                        .await;
                }
                Err(e) => {
                    let _ = AgentLoopStateTransitor::fail_agent_loop(
                        &entity,
                        e.to_string(),
                        bus.as_deref(),
                    )
                    .await;
                    if let Some(handle) = consumer {
                        handle.abort();
                    }
                    if let Some(ref metrics) = metrics {
                        metrics.agent_loop().record_error(entity.id(), "agent_loop");
                    }
                    let _ = tx
                        .send(AgentStreamEvent::Failed {
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        });

        AgentEventStream::new(rx)
    }
}
