use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::sync::mpsc;

use wf_core::event::EventBus;
use wf_types::events::{BaseEvent, EventType};

use crate::error::{AgentError, AgentResult};

/// Event emitted by a streaming agent loop execution.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentStreamEvent {
    IterationStart {
        iteration: u32,
        /// Conversation message count at the iteration boundary (turn
        /// anchor position for trigger conditions and nested-agent input
        /// slicing).
        message_count: usize,
        /// Conversation ledger version at the iteration boundary (turn
        /// anchor version for version-checked write-backs).
        array_version: u64,
    },
    /// Incremental LLM text delta.
    LlmDelta {
        content: String,
    },
    ToolStart {
        tool_call_id: String,
        tool_name: String,
    },
    ToolEnd {
        tool_call_id: String,
        tool_name: String,
        success: bool,
        result: String,
    },
    IterationEnd {
        iteration: u32,
        /// Conversation message count at the iteration boundary (turn
        /// anchor position; includes the iteration's own messages).
        message_count: usize,
        /// Conversation ledger version at the iteration boundary.
        array_version: u64,
    },
    Completed {
        result: serde_json::Value,
        iterations: u32,
    },
    Failed {
        error: String,
    },
    Interrupted {
        reason: String,
    },
}

/// Async stream of agent loop events (message deltas, tool lifecycle,
/// iteration boundaries and the final outcome).
pub struct AgentEventStream {
    rx: mpsc::Receiver<AgentStreamEvent>,
    /// Handle to the spawned loop task. Dropping the stream aborts the task
    /// so a disconnected consumer stops the loop instead of letting it run
    /// every remaining iteration to completion.
    task: Option<tokio::task::JoinHandle<()>>,
}

impl AgentEventStream {
    pub fn new(rx: mpsc::Receiver<AgentStreamEvent>) -> Self {
        Self { rx, task: None }
    }

    pub(crate) fn with_task(mut self, task: tokio::task::JoinHandle<()>) -> Self {
        self.task = Some(task);
        self
    }
}

impl Drop for AgentEventStream {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl futures::Stream for AgentEventStream {
    type Item = AgentStreamEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_recv(cx)
    }
}

/// Publish an agent stream event onto the shared event bus (best effort).
fn publish_to_bus(event_bus: Option<&EventBus>, agent_loop_id: &str, event: &AgentStreamEvent) {
    let Some(bus) = event_bus else {
        return;
    };
    let event_type = match event {
        AgentStreamEvent::IterationStart { .. } => EventType::AgentIterationStarted,
        AgentStreamEvent::LlmDelta { .. } => EventType::LlmStreamChunk,
        AgentStreamEvent::ToolStart { .. } => EventType::AgentToolExecutionStarted,
        AgentStreamEvent::ToolEnd { .. } => EventType::AgentToolExecutionCompleted,
        AgentStreamEvent::IterationEnd { .. } => EventType::AgentIterationCompleted,
        AgentStreamEvent::Completed { .. } => EventType::AgentCompleted,
        AgentStreamEvent::Failed { .. } => EventType::AgentFailed,
        AgentStreamEvent::Interrupted { .. } => EventType::AgentCancelled,
    };
    let bus_event = BaseEvent {
        id: wf_common::generate_id(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(agent_loop_id.to_string()),
        agent_loop_id: Some(agent_loop_id.to_string()),
        event_name: None,
        metadata: Some(
            serde_json::to_value(event)
                .ok()
                .and_then(|v| {
                    v.as_object()
                        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                })
                .unwrap_or_default(),
        ),
    };
    let _ = bus.publish(bus_event);
}

/// Event sink attached to the iteration coordinator when the agent loop runs
/// in streaming mode. Forwards events into the consumer channel and mirrors
/// them onto the shared event bus.
#[derive(Clone)]
pub struct AgentEventSink {
    tx: mpsc::Sender<AgentStreamEvent>,
    event_bus: Option<Arc<EventBus>>,
}

impl AgentEventSink {
    pub fn new(tx: mpsc::Sender<AgentStreamEvent>, event_bus: Option<Arc<EventBus>>) -> Self {
        Self { tx, event_bus }
    }

    /// Send a structural event (iteration/tool boundaries). A dropped
    /// receiver fails the iteration, mirroring the pre-unification driver.
    pub async fn emit(&self, agent_loop_id: &str, event: AgentStreamEvent) -> AgentResult<()> {
        publish_to_bus(self.event_bus.as_deref(), agent_loop_id, &event);
        self.tx
            .send(event)
            .await
            .map_err(|_| AgentError::ExecutionError("stream receiver dropped".to_string()))
    }

    /// Send a content delta; a dropped receiver is tolerated (best effort).
    pub async fn emit_quiet(&self, agent_loop_id: &str, event: AgentStreamEvent) {
        publish_to_bus(self.event_bus.as_deref(), agent_loop_id, &event);
        let _ = self.tx.send(event).await;
    }
}
