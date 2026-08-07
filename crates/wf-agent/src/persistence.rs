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
                        id: format!("tool-{}-{}", index, call_index),
                        name: call.name.clone(),
                        arguments: serde_json::Value::Null,
                        result: None,
                        error: if call.success {
                            None
                        } else {
                            Some("tool call failed".to_string())
                        },
                        started_at: record.start_time,
                        completed_at: record.end_time,
                    })
                    .collect(),
            ),
            response_content: None,
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
            initial_messages: None,
            available_tools: Some(entity.available_tool_names().to_vec()),
            stream: None,
            tool_call_format: entity.tool_call_format().cloned(),
            on_failure: None,
            fallback_output: None,
            hooks: None,
            triggers: None,
            dynamic_context_config: None,
            checkpoint_config: None,
        }),
    }
}
