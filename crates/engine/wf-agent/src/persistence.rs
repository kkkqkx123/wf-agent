//! Execution record construction for the agent engine.
//!
//! The coordinator turns its live [`AgentLoopEntity`] state into a persisted
//! [`wf_types::AgentExecution`] record keyed by the per-run `agent_loop_id`,
//! keeping the iteration history / tool calls / status the downstream agent
//! queries read.

use wf_types::agent_execution::{
    AgentExecutionStatus, AgentRuntimeConfig, IterationRecord as PersistedIterationRecord,
    ToolCallRecord,
};
use wf_types::AgentExecution;

use crate::entity::AgentLoopEntity;

/// Build a persisted `AgentExecution` record from the entity's live state.
pub async fn build_agent_execution(entity: &AgentLoopEntity) -> AgentExecution {
    let state = entity.state.read().await;
    let status: AgentExecutionStatus = state.status().into();

    let iteration_history = state
        .iteration_history()
        .iter()
        .enumerate()
        .map(|(index, record)| PersistedIterationRecord {
            iteration: record.iteration,
            started_at: record.start_time,
            completed_at: record.end_time,
            tool_calls: Some(
                record
                    .tool_calls
                    .iter()
                    .enumerate()
                    .map(|(call_index, call)| ToolCallRecord {
                        // Reuse the LLM tool call id when available so the
                        // persisted audit trail matches the conversation and
                        // checkpoint records.
                        id: call
                            .tool_call_id
                            .clone()
                            .unwrap_or_else(|| format!("tool-{}-{}", index, call_index)),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                        result: call.result.clone(),
                        error: call.error.clone(),
                        started_at: record.start_time,
                        completed_at: record.end_time,
                    })
                    .collect(),
            ),
            response_content: record.response_content.clone(),
            // The runtime and persisted types share the same
            // `LlmCallRecord` shape; only non-empty trails are persisted.
            llm_calls: if record.llm_calls.is_empty() {
                None
            } else {
                Some(record.llm_calls.clone())
            },
            error: None,
        })
        .collect();

    AgentExecution {
        id: entity.id().clone(),
        definition_id: entity.definition_id().clone(),
        status,
        current_iteration: state.current_iteration(),
        tool_call_count: state.tool_call_count(),
        iteration_history: Some(iteration_history),
        started_at: state.start_time(),
        completed_at: state.end_time(),
        error: state.error().map(String::from),
        context: Some(AgentRuntimeConfig {
            profile_id: Some(entity.model().to_string()),
            system_prompt: None,
            max_iterations: None,
            max_execution_time: None,
            max_retries: None,
            execution_timeout: None,
            max_pause_duration: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            initial_messages: None,
            available_tools: Some(entity.available_tool_names().to_vec()),
            discoverable_tool_names: Some(entity.discoverable_tool_names().to_vec()),
            hidden_tool_names: Some(entity.hidden_tool_names().to_vec()),
            stream: None,
            tool_call_format: entity.tool_call_format().cloned(),
            on_failure: None,
            fallback_output: None,
            hooks: None,
            dynamic_context_config: None,
            checkpoint_config: None,
        }),
    }
}
