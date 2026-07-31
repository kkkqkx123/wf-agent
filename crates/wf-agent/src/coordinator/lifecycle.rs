use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use wf_checkpoint::event::CheckpointEventBus;
use wf_core::event::EventBus;
use wf_execution_shared::hooks::executor::HookExecutor;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_llm::LlmWrapper;
use wf_metrics::MetricsRegistry;
use wf_storage::backend::StorageBackend;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput};
use wf_tools::registry::ToolRegistry;
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::message::Message;

use crate::checkpoint::{AgentCheckpointIntegration, AgentCheckpointStrategy};
use crate::coordinator::execution::AgentExecutionCoordinator;
use crate::coordinator::iteration::AgentIterationCoordinator;
use crate::coordinator::state_transitor::AgentLoopStateTransitor;
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::hook::AgentHookHandler;

pub struct AgentLoopCoordinator {
    llm_wrapper: Arc<LlmWrapper>,
    tool_registry: Arc<ToolRegistry>,
    hook_executor: Arc<HookExecutor>,
    event_bus: Option<Arc<EventBus>>,
    store: Arc<StorageBackend>,
    checkpoint_strategy: Option<AgentCheckpointStrategy>,
    checkpoint_event_bus: Option<CheckpointEventBus>,
    metrics: Option<Arc<MetricsRegistry>>,
}

impl AgentLoopCoordinator {
    pub fn new(llm_wrapper: Arc<LlmWrapper>, tool_registry: Arc<ToolRegistry>) -> Self {
        Self::with_store(
            llm_wrapper,
            tool_registry,
            Arc::new(StorageBackend::new_memory()),
        )
    }

    pub fn with_store(
        llm_wrapper: Arc<LlmWrapper>,
        tool_registry: Arc<ToolRegistry>,
        store: Arc<StorageBackend>,
    ) -> Self {
        let hook_executor = Arc::new(HookExecutor::new());
        Self {
            llm_wrapper,
            tool_registry,
            hook_executor,
            event_bus: None,
            store,
            checkpoint_strategy: None,
            checkpoint_event_bus: None,
            metrics: None,
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

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub async fn execute(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        let entity = self.build_entity(&config, input).await?;
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

        let iteration_coordinator = Arc::new(AgentIterationCoordinator::new(
            self.llm_wrapper.clone(),
            self.tool_registry.clone(),
            self.hook_executor.clone(),
            self.metrics.clone(),
        ));
        let execution_coordinator = AgentExecutionCoordinator::new(iteration_coordinator)
            .with_checkpoint(checkpoint)
            .with_metrics(self.metrics.clone());

        let profile_id = entity
            .model()
            .map(|m| m.to_string())
            .unwrap_or_else(|| "default".to_string());
        if let Some(ref metrics) = self.metrics {
            metrics
                .agent()
                .record_execution_start(&profile_id, &execution_id);
            metrics.agent_loop().record_execution_start(&execution_id);
        }

        let max_iterations = config.max_iterations.unwrap_or(10);
        let start = wf_common::now();
        match execution_coordinator.execute(&entity, max_iterations).await {
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

                Ok(AgentLoopOutput {
                    result: result.content,
                    iterations,
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

        let mut entity = AgentLoopEntity::new(config.agent_id.clone()).with_hooks(hooks);

        if let Some(ref model) = config.model {
            entity = entity.with_model(model.clone());
        }

        if !config.available_tool_names.is_empty() {
            entity = entity.with_available_tool_names(config.available_tool_names.clone());
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
}
