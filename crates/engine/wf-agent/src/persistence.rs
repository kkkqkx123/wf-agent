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
            error: state
                .error_records()
                .iter()
                .find(|r| r.node_id.as_deref() == Some(record.iteration_tag()).as_deref())
                .map(|r| r.error.clone()),
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
        })
        .collect();

    let failed_tools = state.permanently_failed_tools();
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
        permanently_failed_tools: if failed_tools.is_empty() {
            None
        } else {
            Some(failed_tools)
        },
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
            tool_call_protocol: entity.tool_call_protocol().cloned(),
            on_failure: None,
            fallback_output: None,
            hooks: None,
            checkpoint_config: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::Id;

    use crate::error_analysis::analyze_error;
    use crate::error::AgentError;

    fn entity(id: &str) -> AgentLoopEntity {
        AgentLoopEntity::new(Id::from(id.to_string()))
    }

    #[tokio::test]
    async fn iteration_error_slot_carries_the_round_error() {
        let entity = entity("loop-iter-err");
        {
            let mut state = entity.state.write().await;
            state.start_iteration();
            state.end_iteration();
            state.start_iteration();
            let analysis = analyze_error(&AgentError::Internal("boom".to_string()));
            let record =
                analysis.to_error_record(entity.id(), Some(crate::state::iteration_tag(2)));
            state.record_error(record);
            state.end_iteration();
        }
        let persisted = build_agent_execution(&entity).await;
        let history = persisted.iteration_history.expect("history persisted");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].error, None);
        assert_eq!(history[1].error.as_deref(), Some("Internal error: boom"));
    }

    #[tokio::test]
    async fn permanently_failed_tools_reach_the_record() {
        let entity = entity("loop-tools");
        entity
            .state
            .write()
            .await
            .record_permanently_failed_tool("broken-tool".to_string());
        let persisted = build_agent_execution(&entity).await;
        assert_eq!(
            persisted.permanently_failed_tools,
            Some(vec!["broken-tool".to_string()])
        );
    }

    #[tokio::test]
    async fn budget_exhaustion_is_terminal_and_not_retryable() {
        let analysis = analyze_error(&AgentError::ContextBudgetExhausted(
            "context-length rejection recurred after compression".to_string(),
        ));
        assert_eq!(analysis.kind, wf_types::errors::ErrorKind::Resource);
        assert!(!analysis.retryable);
        assert_eq!(
            analysis.recovery_action,
            wf_types::errors::RecoveryAction::Abort
        );
    }
}
