use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{Stream, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc::{self, error::TrySendError};

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
    /// Handle to the spawned workflow driver task. Dropping the stream aborts
    /// the driver so a disconnected consumer stops the workflow instead of
    /// letting it run out the execution timeout (mirrors `AgentEventStream`).
    task: Option<tokio::task::JoinHandle<()>>,
}

impl ExecutionEventStream {
    pub fn new(rx: mpsc::Receiver<ExecutionStreamEvent>) -> Self {
        Self { rx, task: None }
    }

    pub(crate) fn with_task(mut self, task: tokio::task::JoinHandle<()>) -> Self {
        self.task = Some(task);
        self
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
                let terminal = matches!(
                    mapped,
                    ExecutionStreamEvent::Completed { .. } | ExecutionStreamEvent::Failed { .. }
                );
                if terminal {
                    // Terminal events must always reach a slow consumer,
                    // otherwise the stream never ends.
                    if tx.send(mapped).await.is_err() {
                        break;
                    }
                } else {
                    // Never stall reading the agent stream on a slow consumer:
                    // drop overflow instead.
                    match tx.try_send(mapped) {
                        Ok(()) => {}
                        Err(TrySendError::Closed(_)) => break,
                        Err(TrySendError::Full(_)) => {}
                    }
                }
            }
        });
        Self::new(rx)
    }
}

impl Drop for ExecutionEventStream {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
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
        return (ExecutionEventStream::new(rx), ExecutionStreamSink { tx });
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
            if terminal {
                // Terminal events must always reach a slow consumer, otherwise
                // the stream never ends.
                if forward_tx
                    .send(ExecutionStreamEvent::Engine(event))
                    .await
                    .is_err()
                {
                    break;
                }
                break;
            }
            // Never stall the broadcast receiver on a slow consumer: blocking
            // here would lag the bus and silently drop events for this
            // subscriber. Drop overflow instead (broadcast-lag semantics).
            match forward_tx.try_send(ExecutionStreamEvent::Engine(event)) {
                Ok(()) => {}
                Err(TrySendError::Closed(_)) => break,
                Err(TrySendError::Full(_)) => {}
            }
        }
    });

    (ExecutionEventStream::new(rx), ExecutionStreamSink { tx })
}
