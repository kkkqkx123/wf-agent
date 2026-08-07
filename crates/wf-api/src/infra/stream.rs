use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{Stream, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;

use wf_agent::stream::{AgentEventStream, AgentStreamEvent};
use wf_core::EventBus;
use wf_types::events::BaseEvent;

/// Event produced by a streaming execution.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionStreamEvent {
    /// Engine lifecycle event published on the shared event bus
    /// (workflow/agent/checkpoint lifecycle).
    Engine(BaseEvent),
    /// Raw agent loop stream event (message deltas, tool lifecycle, ...).
    Agent(AgentStreamEvent),
    /// Terminal success payload.
    Completed { result: Value, iterations: u32 },
    /// Terminal failure payload.
    Failed { error: String },
}

/// Async stream of execution events (SSE/WS friendly).
pub struct ExecutionEventStream {
    rx: mpsc::Receiver<ExecutionStreamEvent>,
}

impl ExecutionEventStream {
    pub fn new(rx: mpsc::Receiver<ExecutionStreamEvent>) -> Self {
        Self { rx }
    }

    /// Adapt a raw [`AgentEventStream`] into the unified stream, mapping its
    /// terminal events onto `Completed` / `Failed`.
    pub fn from_agent_stream(agent: AgentEventStream) -> Self {
        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(async move {
            let mut agent = agent;
            while let Some(event) = agent.next().await {
                let mapped = match event {
                    AgentStreamEvent::Completed { result, iterations } => {
                        ExecutionStreamEvent::Completed { result, iterations }
                    }
                    AgentStreamEvent::Failed { error } => ExecutionStreamEvent::Failed { error },
                    other => ExecutionStreamEvent::Agent(other),
                };
                if tx.send(mapped).await.is_err() {
                    break;
                }
            }
        });
        Self { rx }
    }
}

impl Stream for ExecutionEventStream {
    type Item = ExecutionStreamEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_recv(cx)
    }
}

/// Terminal-event sender paired with an [`ExecutionEventStream`].
///
/// The execution driver holds this handle and emits `Completed` / `Failed`
/// once the engine finishes, while a background forwarder pipes matching
/// engine events from the shared event bus into the stream.
#[derive(Clone)]
pub struct ExecutionStreamSink {
    tx: mpsc::Sender<ExecutionStreamEvent>,
}

impl ExecutionStreamSink {
    pub async fn completed(&self, result: Value, iterations: u32) {
        let _ = self
            .tx
            .send(ExecutionStreamEvent::Completed { result, iterations })
            .await;
    }

    pub async fn failed(&self, error: String) {
        let _ = self.tx.send(ExecutionStreamEvent::Failed { error }).await;
    }
}

/// Spawn a stream that forwards engine events published on the event bus for
/// `execution_id` and hands back a sink for the terminal event.
///
/// The returned [`ExecutionStreamSink`] must be kept alive by the caller;
/// dropping the stream (receiver) stops the forwarder on the next send.
pub fn spawn_execution_stream(
    bus: Option<Arc<EventBus>>,
    execution_id: String,
) -> (ExecutionEventStream, ExecutionStreamSink) {
    let (tx, rx) = mpsc::channel(256);
    let forward_tx = tx.clone();

    // Subscribe synchronously so no engine event is lost when the caller
    // spawns the workflow execution immediately after this returns.
    let Some(bus) = bus else {
        return (ExecutionEventStream { rx }, ExecutionStreamSink { tx });
    };
    let mut sub = bus.subscribe();
    let filter = crate::infra::subscription::EventSubscriptionOptions {
        execution_id: Some(execution_id.clone()),
        ..crate::infra::subscription::EventSubscriptionOptions::default()
    };
    tokio::spawn(async move {
        while let Ok(event) = sub.recv().await {
            if !filter.matches(&event) {
                continue;
            }
            let terminal =
                crate::infra::subscription::EventSubscriptionOptions::is_terminal(&event);
            if forward_tx
                .send(ExecutionStreamEvent::Engine(event))
                .await
                .is_err()
            {
                break;
            }
            if terminal {
                break;
            }
        }
    });

    (ExecutionEventStream { rx }, ExecutionStreamSink { tx })
}
