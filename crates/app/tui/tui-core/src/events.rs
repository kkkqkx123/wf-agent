//! Unified event stream feeding the CLI output/render pipeline.
//!
//! Defines the unified event shape and the conversion from execution
//! stream events; lifecycle events travel over server SSE and are adapted
//! by the frontends before entering this shape.

use wf_types::events::ExecutionStreamEvent;
use wf_types::execution::events::ExecutionEvent;

/// Normalized event consumed by the output layers of every CLI form.
#[derive(Debug, Clone, PartialEq)]
pub enum UnifiedEvent {
    /// Agent iteration started (status bar iteration anchor).
    IterationStarted { index: u32 },
    /// Agent iteration ended (scrollback flush boundary).
    IterationEnded { index: u32 },
    /// Incremental LLM text delta.
    TextDelta { content: String },
    /// Tool invocation started.
    ToolStart {
        tool_call_id: String,
        tool_name: String,
    },
    /// Tool invocation finished. `duration_ms` is `None` when the source
    /// event carries no timing. The result excerpt and error travel with the
    /// event so renderers can show collapsed previews.
    ToolEnd {
        tool_call_id: String,
        tool_name: String,
        success: bool,
        duration_ms: Option<u64>,
        result: String,
        error: Option<String>,
    },
    /// Agent session completed successfully.
    Completed {
        /// Final loop result (assistant answer / JSON payload).
        result: serde_json::Value,
        iterations: u32,
    },
    /// Agent session failed.
    Failed { error: String },
    /// Agent session was interrupted.
    Interrupted { reason: String },
    /// Incremental reasoning / thinking text delta.
    ReasoningDelta { content: String },
    /// Cumulative token usage + estimated cost for the run.
    Usage {
        prompt_tokens: u32,
        completion_tokens: u32,
        cost: Option<f64>,
    },
    /// A sub-agent (triggered child agent) started.
    SubAgentStarted { id: String, name: String },
    /// A sub-agent finished.
    SubAgentEnded {
        id: String,
        name: String,
        success: bool,
    },
    /// Execution lifecycle event (checkpoint bus adapter).
    Execution(ExecutionEvent),
}

impl UnifiedEvent {
    /// Short human-readable label for logging / diagnostics.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::IterationStarted { .. } => "iteration_started",
            Self::IterationEnded { .. } => "iteration_ended",
            Self::TextDelta { .. } => "text_delta",
            Self::ToolStart { .. } => "tool_start",
            Self::ToolEnd { .. } => "tool_end",
            Self::Completed { .. } => "completed",
            Self::Failed { .. } => "failed",
            Self::Interrupted { .. } => "interrupted",
            Self::ReasoningDelta { .. } => "reasoning_delta",
            Self::Usage { .. } => "usage",
            Self::SubAgentStarted { .. } => "subagent_started",
            Self::SubAgentEnded { .. } => "subagent_ended",
            Self::Execution(e) => match e {
                ExecutionEvent::StateChanged(_) => "execution_state_changed",
                ExecutionEvent::ErrorOccurred(_) => "execution_error",
                ExecutionEvent::InterruptionOccurred(_) => "execution_interrupted",
                ExecutionEvent::ToolExecuted(_) => "execution_tool",
                ExecutionEvent::IterationStarted(_) => "execution_iteration_started",
                ExecutionEvent::IterationCompleted(_) => "execution_iteration_completed",
            },
        }
    }
}

/// Convert an execution stream event (the wire protocol shared with the
/// server) into the CLI unified shape. Engine lifecycle events carry no
/// agent-loop payload for a run and are filtered out (`None`).
pub fn unified_from_execution_stream(event: ExecutionStreamEvent) -> Option<UnifiedEvent> {
    match event {
        ExecutionStreamEvent::IterationStart { iteration, .. } => {
            Some(UnifiedEvent::IterationStarted { index: iteration })
        }
        ExecutionStreamEvent::IterationEnd { iteration, .. } => {
            Some(UnifiedEvent::IterationEnded { index: iteration })
        }
        ExecutionStreamEvent::ToolStart {
            tool_call_id,
            tool_name,
        } => Some(UnifiedEvent::ToolStart {
            tool_call_id,
            tool_name,
        }),
        ExecutionStreamEvent::ToolEnd {
            tool_call_id,
            tool_name,
            success,
            result,
            error,
            ..
        } => Some(UnifiedEvent::ToolEnd {
            tool_call_id,
            tool_name,
            success,
            duration_ms: None,
            result,
            error,
        }),
        ExecutionStreamEvent::Completed { result, iterations } => {
            Some(UnifiedEvent::Completed { result, iterations })
        }
        ExecutionStreamEvent::Failed { error } => Some(UnifiedEvent::Failed { error }),
        ExecutionStreamEvent::Interrupted { reason } => Some(UnifiedEvent::Interrupted { reason }),
        ExecutionStreamEvent::ReasoningDelta { content } => {
            Some(UnifiedEvent::ReasoningDelta { content })
        }
        ExecutionStreamEvent::Usage {
            prompt_tokens,
            completion_tokens,
            cost,
        } => Some(UnifiedEvent::Usage {
            prompt_tokens,
            completion_tokens,
            cost,
        }),
        ExecutionStreamEvent::SubAgentStarted { id, name } => {
            Some(UnifiedEvent::SubAgentStarted { id, name })
        }
        ExecutionStreamEvent::SubAgentEnded { id, name, success } => {
            Some(UnifiedEvent::SubAgentEnded { id, name, success })
        }
        ExecutionStreamEvent::LlmDelta { content } => Some(UnifiedEvent::TextDelta { content }),
        ExecutionStreamEvent::Engine(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_end_conversion_carries_result_and_error() {
        let end = ExecutionStreamEvent::ToolEnd {
            tool_call_id: "t1".into(),
            tool_name: "bash".into(),
            success: false,
            result: "out".into(),
            error: Some("boom".into()),
            display: None,
        };
        assert_eq!(
            unified_from_execution_stream(end),
            Some(UnifiedEvent::ToolEnd {
                tool_call_id: "t1".into(),
                tool_name: "bash".into(),
                success: false,
                duration_ms: None,
                result: "out".into(),
                error: Some("boom".into()),
            })
        );
    }

    #[test]
    fn execution_event_passthrough_preserves_payload() {
        let exec = ExecutionEvent::ErrorOccurred(wf_types::execution::events::ErrorOccurredEvent {
            execution_id: "e1".to_string(),
            timestamp: 1,
            message: "nope".to_string(),
            error_type: None,
            iteration: None,
            node_id: None,
        });
        let unified = UnifiedEvent::Execution(exec.clone());
        assert_eq!(unified.kind(), "execution_error");
        assert_eq!(unified, UnifiedEvent::Execution(exec));
    }

    #[test]
    fn execution_stream_events_map_and_engine_filters_out() {
        let llm_delta = ExecutionStreamEvent::LlmDelta {
            content: "hi".into(),
        };
        assert_eq!(
            unified_from_execution_stream(llm_delta),
            Some(UnifiedEvent::TextDelta {
                content: "hi".into()
            })
        );

        let completed = ExecutionStreamEvent::Completed {
            result: serde_json::json!("done"),
            iterations: 3,
        };
        assert_eq!(
            unified_from_execution_stream(completed),
            Some(UnifiedEvent::Completed {
                result: serde_json::json!("done"),
                iterations: 3,
            })
        );

        let failed = ExecutionStreamEvent::Failed {
            error: "boom".into(),
        };
        assert_eq!(
            unified_from_execution_stream(failed),
            Some(UnifiedEvent::Failed {
                error: "boom".into()
            })
        );

        let engine = ExecutionStreamEvent::Engine(wf_types::events::BaseEvent {
            id: wf_types::Id::new(),
            r#type: wf_types::events::EventType::Heartbeat,
            timestamp: wf_common::now(),
            event_name: None,
            workflow_id: None,
            execution_id: None,
            agent_loop_id: None,
            metadata: None,
        });
        assert_eq!(unified_from_execution_stream(engine), None);
    }
}
