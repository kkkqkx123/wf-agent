use std::sync::Arc;
use std::time::Duration;

use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_agent::entity::AgentLoopEntity;
use wf_agent::registry::AgentLoopRegistry;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput};

use crate::context::ApiContext;
use crate::error::ApiError;
use crate::stream::ExecutionEventStream;

/// Default wall-clock timeout for an agent loop when the config sets neither
/// `max_execution_time` nor `max_iterations` (30s per iteration, 3 default
/// iterations). An elapse maps onto `ApiError::Timeout`.
const DEFAULT_AGENT_TIMEOUT_MS: u64 = 90_000;

/// Per-iteration time budget used to derive the default agent timeout from
/// `max_iterations` (aligned with TS `max_iterations × 30s`).
const AGENT_TIMEOUT_PER_ITERATION_MS: u64 = 30_000;

/// Parameters for running an agent loop.
#[derive(Debug, Clone)]
pub struct RunAgentLoopParams {
    /// Agent loop config (model, tool visibility, token limits, ...).
    pub config: AgentLoopConfig,
    /// Loop input (initial user message, context, imported conversation).
    pub input: AgentLoopInput,
}

/// Application-facing agent loop API.
///
/// Launches agent loops through the `wf-agent` engine. The live entity is
/// registered in the shared [`AgentLoopRegistry`] so `pause` / `resume` /
/// `cancel` and status queries apply to the running loop.
/// Run an agent loop to completion and await the final output.
///
/// Every run gets a fresh `agent_loop_id`; `config.agent_id` only
/// identifies the agent definition. Bounded by a wall-clock timeout: the
/// config's `max_execution_time` when set, otherwise `max_iterations ×
/// 30s` (default 90s). An elapse maps onto `ApiError::Timeout`.
pub async fn run(ctx: &ApiContext, params: RunAgentLoopParams) -> crate::error::ApiResult<AgentLoopOutput> {
    let agent_loop_id = wf_types::Id::from(wf_common::generate_id());
    let coordinator = coordinator(ctx).with_agent_loop_id(agent_loop_id.clone());
    let timeout_ms = agent_timeout_ms(&params.config);
    let config = params.config.clone();
    let input = params.input.clone();
    let outcome = crate::error::with_timeout(Duration::from_millis(timeout_ms), async move {
        coordinator.execute(config, input).await.map_err(Into::into)
    })
    .await;
    match outcome {
        Ok(output) => {
            // Persist the produced conversation so agent-loop messages are
            // queryable through `message::save` (storage injected via the
            // shared context, mirroring the checkpoint wiring pattern).
            // Messages are scoped to the per-run agent loop id.
            persist_conversation(ctx, &output.agent_loop_id, &output.conversation).await;
            Ok(output)
        }
        Err(e) => {
            // A wall-clock timeout drops the coordinator mid-loop: the
            // start record stays `Running`. Mark the live entity failed
            // and persist the terminal record so the agent execution store
            // reflects the failure.
            if let Some(entity) = ctx.agent_loop(&agent_loop_id.to_string()) {
                entity.state.write().await.fail(e.to_string());
                let record = wf_agent::build_agent_execution(&entity).await;
                ctx.state_manager.persist_agent(&record).await;
            }
            Err(e)
        }
    }
}

/// Run an agent loop in streaming mode. Events (message deltas, tool
/// lifecycle, iteration boundaries, terminal outcome) flow through the
/// returned stream.
pub async fn stream(
    ctx: &ApiContext,
    params: RunAgentLoopParams,
) -> crate::error::ApiResult<ExecutionEventStream> {
    let coordinator = coordinator(ctx);
    let stream = coordinator
        .execute_stream(params.config, params.input)
        .await;
    Ok(ExecutionEventStream::from_agent_stream(stream))
}

/// Pause a running agent loop (checked between iterations).
pub async fn pause(ctx: &ApiContext, agent_loop_id: &str) -> crate::error::ApiResult<()> {
    let entity = live_entity(ctx, agent_loop_id)?;
    entity.interruption().pause()?;
    entity.state.write().await.pause();
    Ok(())
}

/// Resume a paused agent loop.
pub async fn resume(ctx: &ApiContext, agent_loop_id: &str) -> crate::error::ApiResult<()> {
    let entity = live_entity(ctx, agent_loop_id)?;
    entity.interruption().resume()?;
    entity.state.write().await.resume();
    Ok(())
}

/// Cancel (stop) a running agent loop.
pub async fn cancel(ctx: &ApiContext, agent_loop_id: &str) -> crate::error::ApiResult<()> {
    let entity = live_entity(ctx, agent_loop_id)?;
    entity.interruption().stop()?;
    entity.state.write().await.cancel();
    Ok(())
}

/// Query the live status of an agent loop execution.
///
/// Returns the typed [`wf_types::ExecutionStatus`] (the persisted status
/// contract) instead of a Debug string, so callers can match without
/// string parsing. A timeout in the engine state reads as `Failed`.
pub async fn status(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> crate::error::ApiResult<wf_types::ExecutionStatus> {
    let entity = live_entity(ctx, agent_loop_id)?;
    let status: wf_types::ExecutionStatus = entity.state.read().await.status().into();
    Ok(status)
}

/// Access the shared agent loop registry (query/records/cleanup).
pub fn registry(ctx: &ApiContext) -> Arc<AgentLoopRegistry> {
    ctx.agent_loops.clone()
}

fn coordinator(ctx: &ApiContext) -> AgentLoopCoordinator {
    let mut coordinator = AgentLoopCoordinator::with_store(
        ctx.llm_gateway.clone(),
        ctx.tool_registry.clone(),
        ctx.checkpoint_store.clone(),
    )
    .with_event_bus(ctx.event_bus.clone())
    .with_entity_registry(ctx.agent_loops.clone())
    .with_state_manager(ctx.state_manager.clone());
    if let Some(ref metrics) = ctx.metrics {
        coordinator = coordinator.with_metrics(metrics.clone());
    }
    coordinator
}

fn live_entity(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> crate::error::ApiResult<Arc<AgentLoopEntity>> {
    ctx.agent_loop(agent_loop_id)
        .ok_or_else(|| ApiError::execution_not_found(agent_loop_id))
}

/// Persist the final conversation of an agent loop into the message
/// adapter, scoped to the per-run agent loop id. Idempotent: message ids
/// are the storage keys, so re-running a loop never duplicates a message.
async fn persist_conversation(
    ctx: &ApiContext,
    agent_loop_id: &wf_types::Id,
    conversation: &[wf_types::message::Message],
) {
    let agent_loop_id = agent_loop_id.to_string();
    for message in conversation {
        let record = wf_types::MessageStorageMetadata {
            id: message.id.clone(),
            execution_id: agent_loop_id.clone(),
            agent_loop_id: Some(agent_loop_id.clone()),
            message: message.clone(),
        };
        if let Err(err) = ctx.storage.message.save(&record).await {
            tracing::warn!(
                target: "wf_api",
                agent_loop_id = %agent_loop_id,
                error = %err,
                "failed to persist agent conversation message"
            );
        }
    }
}

/// Derive the default agent loop wall-clock timeout: `max_execution_time` when
/// set, otherwise `max_iterations × 30s`, falling back to the 90s default.
fn agent_timeout_ms(config: &AgentLoopConfig) -> u64 {
    if let Some(max_execution_time) = config.max_execution_time {
        return max_execution_time;
    }
    config
        .max_iterations
        .map(|iterations| iterations as u64 * AGENT_TIMEOUT_PER_ITERATION_MS)
        .unwrap_or(DEFAULT_AGENT_TIMEOUT_MS)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    fn gateway_with(mock: Arc<MockLlmClient>) -> Arc<LlmGateway> {
        let gateway = LlmGateway::new();
        gateway.register_mock("mock", mock);
        Arc::new(gateway)
    }

    fn make_ctx() -> Arc<ApiContext> {
        let mock = Arc::new(MockLlmClient::new());
        mock.script(LlmResponseSpec::text("hello from mock"));
        let mut ctx = ApiContext::from_runtime_parts(
            Arc::new(StorageContext::new_memory()),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
            Arc::new(wf_core::EventBus::new(64)),
            gateway_with(mock),
            Arc::new(wf_tools::create_default_tool_registry()),
            None,
        );
        ctx =
            ctx.with_checkpoint_store(Arc::new(wf_storage::backend::StorageBackend::new_memory()));
        Arc::new(ctx)
    }

    #[tokio::test]
    async fn runs_agent_loop_to_completion() {
        let ctx = make_ctx();
        let output = run(
            &ctx,
            RunAgentLoopParams {
                config: AgentLoopConfig {
                    agent_id: wf_types::Id::from("agent-1".to_string()),
                    model: "mock".to_string(),
                    max_iterations: Some(3),
                    max_execution_time: None,
                    hooks: Vec::new(),
                    available_tool_names: Vec::new(),
                    tool_call_format: None,
                    token_limit: None,
                    token_warning_threshold: None,
                    enable_token_tracking: None,
                },
                input: AgentLoopInput {
                    message: "hi".to_string(),
                    context: Default::default(),
                    conversation: Vec::new(),
                },
            })
            .await
            .expect("agent loop should complete");
        assert_eq!(output.result, serde_json::json!("hello from mock"));
        assert_eq!(output.iterations, 1);

        // The produced conversation is persisted and queryable via the
        // message adapter, scoped to the per-run agent loop id (not the agent
        // definition id).
        let persisted = ctx.storage.message.list(None).await.unwrap();
        assert!(
            !persisted.is_empty(),
            "agent conversation must be persisted"
        );
        let scoped = persisted[0].agent_loop_id.as_deref().unwrap();
        assert_ne!(scoped, "agent-1", "loop id must differ from definition id");
        assert!(persisted
            .iter()
            .all(|r| r.agent_loop_id.as_deref() == Some(scoped)));

        // Stage 0: an `AgentExecution` record is persisted, keyed by the
        // per-run agent loop id and linked to the definition.
        let executions = ctx.storage.agent_execution.list(None).await.unwrap();
        assert_eq!(executions.len(), 1, "agent execution must be persisted");
        assert_eq!(executions[0].definition_id, wf_types::Id::from("agent-1"));
        assert_eq!(executions[0].id.to_string(), scoped);
    }

    /// Stage 0 acceptance: after a real `run`, the persisted `AgentExecution`
    /// record is readable through a fresh context (empty live registries), so
    /// the persisted branches of the agent queries return real data.
    #[tokio::test]
    async fn persisted_agent_execution_readable_after_restart() {
        let storage = Arc::new(StorageContext::new_memory());
        let mock = Arc::new(MockLlmClient::new());
        mock.script(LlmResponseSpec::text("persisted agent"));
        let gateway = gateway_with(mock);

        let mut ctx1 = ApiContext::from_runtime_parts(
            storage.clone(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
            Arc::new(wf_core::EventBus::new(64)),
            gateway.clone(),
            Arc::new(wf_tools::create_default_tool_registry()),
            None,
        );
        ctx1 =
            ctx1.with_checkpoint_store(Arc::new(wf_storage::backend::StorageBackend::new_memory()));

        let ctx1 = Arc::new(ctx1);
        let output = run(
            &ctx1,
            RunAgentLoopParams {
                config: AgentLoopConfig {
                    agent_id: wf_types::Id::from("agent-persist".to_string()),
                    model: "mock".to_string(),
                    max_iterations: Some(3),
                    max_execution_time: None,
                    hooks: Vec::new(),
                    available_tool_names: Vec::new(),
                    tool_call_format: None,
                    token_limit: None,
                    token_warning_threshold: None,
                    enable_token_tracking: None,
                },
                input: AgentLoopInput {
                    message: "hi".to_string(),
                    context: Default::default(),
                    conversation: Vec::new(),
                },
            },
        )
        .await
        .expect("agent loop should complete");
        let agent_loop_id = output.agent_loop_id.to_string();
        assert_ne!(agent_loop_id, "agent-persist");

        let ctx2 = Arc::new(ApiContext::from_runtime_parts(
            storage,
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
            Arc::new(wf_core::EventBus::new(64)),
            gateway,
            Arc::new(wf_tools::create_default_tool_registry()),
            None,
        ));

        use crate::execution_state::agent_execution_get_state;
        let view = agent_execution_get_state(&ctx2, &agent_loop_id)
            .await
            .expect("persisted agent state query");
        assert_eq!(view.source, "persisted");
        assert_eq!(view.status, wf_types::ExecutionStatus::Completed);
        assert_eq!(view.agent_loop_id, agent_loop_id);

        let executions = crate::agent::list_agent_executions(&ctx2.storage, None)
            .await
            .unwrap();
        assert_eq!(executions.len(), 1);
        assert_eq!(
            executions[0].definition_id,
            wf_types::Id::from("agent-persist")
        );
        assert_eq!(executions[0].id.to_string(), agent_loop_id);
    }

    #[tokio::test]
    async fn streams_agent_loop_events() {
        let ctx = make_ctx();
        let mut stream = stream(
            &ctx,
            RunAgentLoopParams {
                config: AgentLoopConfig {
                    agent_id: wf_types::Id::from("agent-2".to_string()),
                    model: "mock".to_string(),
                    max_iterations: Some(3),
                    max_execution_time: None,
                    hooks: Vec::new(),
                    available_tool_names: Vec::new(),
                    tool_call_format: None,
                    token_limit: None,
                    token_warning_threshold: None,
                    enable_token_tracking: None,
                },
                input: AgentLoopInput {
                    message: "hi".to_string(),
                    context: Default::default(),
                    conversation: Vec::new(),
                },
            },
        )
        .await
        .expect("agent stream");
        use futures::StreamExt;
        let mut saw_completed = false;
        while let Some(event) = stream.next().await {
            if let crate::stream::ExecutionStreamEvent::Completed { .. } = event {
                saw_completed = true;
            }
        }
        assert!(saw_completed, "stream must end with Completed");
    }

    #[tokio::test]
    async fn pause_resume_cancel_control_handles() {
        let ctx = make_ctx();
        let id = wf_types::Id::from("agent-ctrl-1".to_string());
        let entity = Arc::new(AgentLoopEntity::new(id.clone()));
        ctx.agent_loops.register(entity.clone());

        pause(&ctx, &id.to_string()).await.expect("pause");
        assert!(entity.state.read().await.is_paused());
        assert!(entity.interruption().is_interrupted());

        resume(&ctx, &id.to_string()).await.expect("resume");
        assert!(!entity.interruption().is_interrupted());

        cancel(&ctx, &id.to_string()).await.expect("cancel");
        assert!(entity.state.read().await.is_cancelled());

        let err = pause(&ctx, "missing").await.expect_err("unknown loop");
        assert!(matches!(err, ApiError::ExecutionNotFound { .. }));
    }
}
