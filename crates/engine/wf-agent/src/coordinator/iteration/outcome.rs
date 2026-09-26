use std::collections::HashMap;

use serde_json::Value;

use wf_execution_shared::interruption::check_execution_interruption;
use wf_execution_shared::types::interruption::ExecutionInterruptionCheckResult;

use super::AgentIterationCoordinator;
use crate::entity::AgentLoopEntity;
use crate::hook::AgentHookEmitter;
use crate::stream::AgentStreamEvent;

impl AgentIterationCoordinator {
    /// Read the conversation position at the current boundary: message count
    /// and ledger version, published as the turn anchor on iteration events.
    pub(super) async fn conversation_anchor(&self, entity: &AgentLoopEntity) -> (usize, u64) {
        let conversation = entity.conversation().read().await;
        (
            conversation.messages().len(),
            conversation.conversation_version(),
        )
    }

    /// Check for an interruption after an LLM call or tool execution; when
    /// interrupted the iteration is closed and a terminal result returned.
    pub(super) async fn interrupted(
        &self,
        entity: &AgentLoopEntity,
        tool_call_count: u32,
    ) -> Option<super::IterationResult> {
        let interruption = check_execution_interruption(
            entity.interruption(),
            Some(entity.state.read().await.current_iteration()),
        );
        if matches!(interruption, ExecutionInterruptionCheckResult::Continue) {
            return None;
        }

        entity.state.write().await.end_iteration();
        if self.is_streaming() {
            if let Some(ref sink) = self.event_sink {
                let iteration = entity.state.read().await.current_iteration();
                let (message_count, array_version) = self.conversation_anchor(entity).await;
                let _ = sink
                    .emit(
                        entity.id(),
                        AgentStreamEvent::IterationEnd {
                            iteration,
                            message_count,
                            array_version,
                        },
                    )
                    .await;
            }
        }
        Some(super::IterationResult {
            should_continue: false,
            content: Value::String("Execution interrupted".to_string()),
            completion_data: None,
            tool_call_count,
            finish_reason: wf_tools::callback::LoopFinishReason::Interrupted,
        })
    }

    /// Close the iteration (state, stream events, AFTER_ITERATION hook) and
    /// assemble the result.
    pub(super) async fn finish_iteration(
        &self,
        entity: &AgentLoopEntity,
        content: String,
        completion_data: Option<Value>,
        tool_call_count: u32,
        should_continue: bool,
    ) -> crate::error::AgentResult<super::IterationResult> {
        entity
            .state
            .write()
            .await
            .end_iteration_with_content(Some(content.clone()));

        if self.is_streaming() {
            if let Some(ref sink) = self.event_sink {
                let iteration = entity.state.read().await.current_iteration();
                let (message_count, array_version) = self.conversation_anchor(entity).await;
                sink.emit(
                    entity.id(),
                    AgentStreamEvent::IterationEnd {
                        iteration,
                        message_count,
                        array_version,
                    },
                )
                .await?;
            }
        }

        AgentHookEmitter::fire_agent_point_with_checkpoint(
            entity,
            "AFTER_ITERATION",
            HashMap::new(),
            self.hook_handler_registry.as_deref(),
            self.event_bus.as_deref(),
            self.checkpoint.as_deref(),
        )
        .await;

        Ok(super::IterationResult {
            should_continue,
            content: Value::String(content),
            completion_data,
            tool_call_count,
            finish_reason: wf_tools::callback::LoopFinishReason::Completed,
        })
    }
}
