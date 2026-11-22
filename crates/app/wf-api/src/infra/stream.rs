use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{Stream, StreamExt};
use serde_json::Value;

use crate::infra::context::ApiContext;
use tokio::sync::mpsc::{self, error::TrySendError};

use wf_agent::stream::{AgentEventStream, AgentStreamEvent};
use wf_core::EventBus;
pub use wf_types::events::ExecutionStreamEvent;

/// Async stream of execution events (SSE/WS friendly).
pub struct ExecutionEventStream {
    rx: mpsc::Receiver<ExecutionStreamEvent>,
    /// Handle to the spawned workflow driver task. Dropping the stream aborts
    /// the driver so a disconnected consumer stops the workflow instead of
    /// letting it run out the execution timeout (mirrors `AgentEventStream`).
    task: Option<tokio::task::JoinHandle<()>>,
    /// Optional conversation persistence hook for agent streams: when a
    /// terminal event is delivered, the agent loop's conversation is
    /// persisted to the message store so stream turns are resumable.
    persist: Option<(Arc<ApiContext>, String)>,
}

impl ExecutionEventStream {
    pub fn new(rx: mpsc::Receiver<ExecutionStreamEvent>) -> Self {
        Self {
            rx,
            task: None,
            persist: None,
        }
    }

    pub(crate) fn with_task(mut self, task: tokio::task::JoinHandle<()>) -> Self {
        self.task = Some(task);
        self
    }

    /// Attach a conversation-persistence hook to an agent stream. The hook
    /// fires once when a terminal event is delivered to the consumer.
    pub fn with_persist(mut self, ctx: Arc<ApiContext>, agent_loop_id: String) -> Self {
        self.persist = Some((ctx, agent_loop_id));
        self
    }

    /// Adapt a raw [`AgentEventStream`] (engine-internal agent loop events)
    /// into the execution stream protocol, mapping the loop's outcome
    /// events onto the protocol terminal events.
    ///
    /// Dropping the returned stream (a disconnecting SSE consumer) drops the
    /// receiver, which makes the forwarder stop and therefore aborts the
    /// underlying engine task. Use [`Self::from_agent_stream_detached`] when
    /// the run must outlive its observer.
    pub fn from_agent_stream(agent: AgentEventStream) -> Self {
        Self::from_agent_stream_inner(agent, false)
    }

    /// Like [`Self::from_agent_stream`], but the forwarder keeps draining the
    /// engine stream to its terminal event after the consumer disconnects.
    /// Engine events still reach the shared event bus via `AgentEventSink`,
    /// so a later `attach` can replay them; only the SSE delivery is dropped.
    /// This is the transport primitive behind `stream_detach_policy = keep`.
    pub fn from_agent_stream_detached(agent: AgentEventStream) -> Self {
        Self::from_agent_stream_inner(agent, true)
    }

    fn from_agent_stream_inner(agent: AgentEventStream, keep_on_detach: bool) -> Self {
        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(async move {
            let mut agent = agent;
            // Once the consumer is gone in `keep` mode, keep draining the
            // engine stream (discarding mapped events) so the driver task
            // runs to completion instead of being aborted on drop.
            let mut detached = false;
            while let Some(event) = agent.next().await {
                let mapped = match event {
                    AgentStreamEvent::IterationStart {
                        iteration,
                        message_count,
                        array_version,
                    } => ExecutionStreamEvent::IterationStart {
                        iteration,
                        message_count,
                        array_version,
                    },
                    AgentStreamEvent::LlmDelta { content } => {
                        ExecutionStreamEvent::LlmDelta { content }
                    }
                    AgentStreamEvent::ToolStart {
                        tool_call_id,
                        tool_name,
                    } => ExecutionStreamEvent::ToolStart {
                        tool_call_id,
                        tool_name,
                    },
                    AgentStreamEvent::ToolEnd {
                        tool_call_id,
                        tool_name,
                        success,
                        result,
                        error,
                        error_code,
                        display,
                        background,
                    } => ExecutionStreamEvent::ToolEnd {
                        tool_call_id,
                        tool_name,
                        success,
                        result,
                        error,
                        error_code,
                        display,
                        background,
                    },
                    AgentStreamEvent::IterationEnd {
                        iteration,
                        message_count,
                        array_version,
                    } => ExecutionStreamEvent::IterationEnd {
                        iteration,
                        message_count,
                        array_version,
                    },
                    AgentStreamEvent::Interrupted { reason } => {
                        ExecutionStreamEvent::Interrupted { reason }
                    }
                    AgentStreamEvent::Completed { result, iterations } => {
                        ExecutionStreamEvent::Completed { result, iterations }
                    }
                    AgentStreamEvent::Failed { error, .. } => {
                        ExecutionStreamEvent::Failed { error }
                    }
                    AgentStreamEvent::ReasoningDelta { content } => {
                        ExecutionStreamEvent::ReasoningDelta { content }
                    }
                    AgentStreamEvent::Usage {
                        prompt_tokens,
                        completion_tokens,
                        cost,
                        reasoning_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        total_tokens,
                    } => ExecutionStreamEvent::Usage {
                        prompt_tokens,
                        completion_tokens,
                        cost,
                        reasoning_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        total_tokens,
                    },
                    AgentStreamEvent::SubAgentStarted {
                        id,
                        name,
                        tool_call_id,
                        depth,
                        preview,
                        background,
                    } => ExecutionStreamEvent::SubAgentStarted {
                        id,
                        name,
                        tool_call_id,
                        depth,
                        preview,
                        background: Some(background),
                    },
                    AgentStreamEvent::SubAgentEnded {
                        id,
                        name,
                        success,
                        tool_call_id,
                        background,
                    } => ExecutionStreamEvent::SubAgentEnded {
                        id,
                        name,
                        success,
                        tool_call_id,
                        background: Some(background),
                    },
                    AgentStreamEvent::SubAgentProgress {
                        id,
                        tool_call_id,
                        activity,
                        output_tokens,
                        tool_uses,
                    } => ExecutionStreamEvent::SubAgentProgress {
                        id,
                        tool_call_id,
                        activity,
                        output_tokens,
                        tool_uses,
                    },
                };
                let terminal = matches!(
                    mapped,
                    ExecutionStreamEvent::Completed { .. }
                        | ExecutionStreamEvent::Failed { .. }
                        | ExecutionStreamEvent::Interrupted { .. }
                );
                if terminal {
                    // Terminal events must always reach a slow consumer,
                    // otherwise the stream never ends. A closed receiver in
                    // keep mode simply ends delivery; the engine task already
                    // reached its terminal event, so nothing is aborted.
                    if tx.send(mapped).await.is_err() && !keep_on_detach {
                        break;
                    }
                    break;
                }
                if detached {
                    // Consumer gone but the run must continue: discard the
                    // mapped event and keep polling the engine stream.
                    continue;
                }
                // Never stall reading the agent stream on a slow consumer:
                // drop overflow instead.
                match tx.try_send(mapped) {
                    Ok(()) => {}
                    Err(TrySendError::Closed(_)) => {
                        if keep_on_detach {
                            detached = true;
                        } else {
                            break;
                        }
                    }
                    Err(TrySendError::Full(_)) => {}
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
        let polled = Pin::new(&mut self.rx).poll_recv(cx);
        if let Poll::Ready(Some(event)) = &polled {
            let is_terminal = matches!(
                event,
                ExecutionStreamEvent::Completed { .. }
                    | ExecutionStreamEvent::Failed { .. }
                    | ExecutionStreamEvent::Interrupted { .. }
            );
            if is_terminal {
                if let Some((ctx, id)) = self.persist.take() {
                    tokio::spawn(async move {
                        crate::agent::agent_execution::checkpoint::persist_stream_conversation(
                            &ctx, &id,
                        )
                        .await;
                    });
                }
            }
        }
        polled
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

#[cfg(test)]
mod detached_tests {
    use super::*;
    use wf_agent::stream::AgentStreamEvent;

    fn delta_event(text: &str) -> AgentStreamEvent {
        AgentStreamEvent::LlmDelta {
            content: text.to_string(),
        }
    }

    /// With `keep` semantics, dropping the consumer stream must NOT make the
    /// forwarder stop reading the engine stream: the underlying run keeps
    /// progressing after the observer disconnects.
    #[tokio::test]
    async fn keep_forwarder_survives_consumer_drop() {
        let (engine_tx, engine_rx) = mpsc::channel::<AgentStreamEvent>(8);
        let engine = AgentEventStream::new(engine_rx);
        let consumer = ExecutionEventStream::from_agent_stream_detached(engine);

        drop(consumer); // observer disconnects

        // The engine producer can still publish: if the forwarder had stopped
        // on the closed receiver, engine_rx would have been dropped and these
        // sends would fail immediately.
        for i in 0..8 {
            engine_tx
                .send(delta_event(&format!("chunk-{i}")))
                .await
                .expect("keep forwarder must keep draining after consumer drop");
        }
    }

    /// With `cancel` semantics (the default), dropping the consumer stops the
    /// forwarder, which drops the engine receiver. This preserves today's
    /// behavior for local single-user runs.
    #[tokio::test]
    async fn cancel_forwarder_stops_on_consumer_drop() {
        let (engine_tx, engine_rx) = mpsc::channel::<AgentStreamEvent>(8);
        let engine = AgentEventStream::new(engine_rx);
        let consumer = ExecutionEventStream::from_agent_stream(engine);
        drop(consumer);

        // The forwarder notices the closed receiver when it next tries to
        // deliver an event. One event triggers that; subsequent sends must
        // fail because the engine receiver was dropped (cancel semantics).
        let _ = engine_tx.send(delta_event("probe")).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            engine_tx.send(delta_event("late")).await.is_err(),
            "cancel forwarder must drop the engine receiver after consumer drop"
        );
    }
}
