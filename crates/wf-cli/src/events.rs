//! Unified event stream feeding the CLI output/render pipeline.
//!
//! Defines the unified event shape and the conversions from the agent
//! streaming events; the shared `EventBus` subscription helper lets the
//! interactive forms (mini / TUI) consume lifecycle events as a `Stream`.
//! Full
//! execution lifecycle events (`ExecutionEvent`) travel on the checkpoint
//! `ExecutionEventBus`; the `Execution` variant keeps the door open for that
//! adapter.

use std::sync::Arc;

use wf_agent::stream::AgentStreamEvent;
use wf_api::infra::subscription::{spawn_event_subscription, EventSubscription};
use wf_api::infra::subscription::EventSubscriptionOptions;
use wf_core::event::EventBus;
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
    ToolStart { tool_call_id: String, tool_name: String },
    /// Tool invocation finished. `duration_ms` is `None` when the source
    /// event carries no timing (e.g. the agent stream events).
    ToolEnd {
        tool_call_id: String,
        tool_name: String,
        success: bool,
        duration_ms: Option<u64>,
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

/// Convert an agent stream event into the unified shape.
impl From<AgentStreamEvent> for UnifiedEvent {
    fn from(event: AgentStreamEvent) -> Self {
        match event {
            AgentStreamEvent::IterationStart { iteration, .. } => {
                Self::IterationStarted { index: iteration }
            }
            AgentStreamEvent::IterationEnd { iteration, .. } => {
                Self::IterationEnded { index: iteration }
            }
            AgentStreamEvent::LlmDelta { content } => Self::TextDelta { content },
            AgentStreamEvent::ToolStart {
                tool_call_id,
                tool_name,
            } => Self::ToolStart {
                tool_call_id,
                tool_name,
            },
            AgentStreamEvent::ToolEnd {
                tool_call_id,
                tool_name,
                success,
                ..
            } => Self::ToolEnd {
                tool_call_id,
                tool_name,
                success,
                duration_ms: None,
            },
            AgentStreamEvent::Completed { result, iterations } => Self::Completed { result, iterations },
            AgentStreamEvent::Failed { error } => Self::Failed { error },
            AgentStreamEvent::Interrupted { reason } => Self::Interrupted { reason },
        }
    }
}

/// Convert an execution stream event (the wf-api unified stream) into the
/// CLI unified shape. Engine lifecycle events carry no agent-loop payload
/// for a headless run and are filtered out (`None`).
pub fn unified_from_execution_stream(
    event: wf_api::infra::stream::ExecutionStreamEvent,
) -> Option<UnifiedEvent> {
    match event {
        wf_api::infra::stream::ExecutionStreamEvent::Agent(agent) => Some(agent.into()),
        wf_api::infra::stream::ExecutionStreamEvent::Completed { result, iterations } => {
            Some(UnifiedEvent::Completed { result, iterations })
        }
        wf_api::infra::stream::ExecutionStreamEvent::Failed { error } => {
            Some(UnifiedEvent::Failed { error })
        }
        wf_api::infra::stream::ExecutionStreamEvent::Engine(_) => None,
    }
}

/// Subscribe to lifecycle events on the shared runtime `EventBus`.
///
/// The returned subscription is a `Stream<Item = BaseEvent>` and can be
/// consumed directly or filtered further by the caller.
pub fn subscribe_event_bus(
    bus: Arc<EventBus>,
    options: &EventSubscriptionOptions,
) -> EventSubscription {
    spawn_event_subscription(bus, options)
}

/// Convenience: subscribe to every lifecycle event without filtering.
pub fn subscribe_all(bus: Arc<EventBus>) -> EventSubscription {
    subscribe_event_bus(bus, &EventSubscriptionOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_events_map_into_unified_events() {
        let cases = [
            (
                AgentStreamEvent::IterationStart {
                    iteration: 3,
                    message_count: 10,
                    array_version: 5,
                },
                UnifiedEvent::IterationStarted { index: 3 },
            ),
            (
                AgentStreamEvent::IterationEnd {
                    iteration: 3,
                    message_count: 12,
                    array_version: 6,
                },
                UnifiedEvent::IterationEnded { index: 3 },
            ),
            (
                AgentStreamEvent::LlmDelta {
                    content: "hi".to_string(),
                },
                UnifiedEvent::TextDelta {
                    content: "hi".to_string(),
                },
            ),
            (
                AgentStreamEvent::ToolStart {
                    tool_call_id: "t1".to_string(),
                    tool_name: "bash".to_string(),
                },
                UnifiedEvent::ToolStart {
                    tool_call_id: "t1".to_string(),
                    tool_name: "bash".to_string(),
                },
            ),
            (
                AgentStreamEvent::ToolEnd {
                    tool_call_id: "t1".to_string(),
                    tool_name: "bash".to_string(),
                    success: true,
                    result: "ok".to_string(),
                },
                UnifiedEvent::ToolEnd {
                    tool_call_id: "t1".to_string(),
                    tool_name: "bash".to_string(),
                    success: true,
                    duration_ms: None,
                },
            ),
            (
                AgentStreamEvent::Completed {
                    result: serde_json::Value::Null,
                    iterations: 2,
                },
                UnifiedEvent::Completed {
                    result: serde_json::Value::Null,
                    iterations: 2,
                },
            ),
            (
                AgentStreamEvent::Failed {
                    error: "boom".to_string(),
                },
                UnifiedEvent::Failed {
                    error: "boom".to_string(),
                },
            ),
            (
                AgentStreamEvent::Interrupted {
                    reason: "user".to_string(),
                },
                UnifiedEvent::Interrupted {
                    reason: "user".to_string(),
                },
            ),
        ];
        for (source, expected) in cases {
            let unified: UnifiedEvent = source.into();
            assert_eq!(unified, expected, "kind mismatch: {}", expected.kind());
        }
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
        use wf_api::infra::stream::ExecutionStreamEvent;
        use wf_agent::stream::AgentStreamEvent;

        let agent = ExecutionStreamEvent::Agent(AgentStreamEvent::LlmDelta {
            content: "hi".into(),
        });
        assert_eq!(
            unified_from_execution_stream(agent),
            Some(UnifiedEvent::TextDelta { content: "hi".into() })
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
            Some(UnifiedEvent::Failed { error: "boom".into() })
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
