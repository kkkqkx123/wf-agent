use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use serde_json::Value;
use tokio::sync::mpsc;

use wf_core::event::EventBus;
use wf_execution_shared::hooks::executor::HookExecutor;
use wf_llm::LlmWrapper;
use wf_types::events::{BaseEvent, EventType};
use wf_types::llm::{LlmRequest, MessageStreamEvent};

use crate::coordinator::iteration::{IterationExecutor, IterationResult};
use crate::coordinator::tool::ToolExecutionCoordinator;
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::hook::AgentHookHandler;

/// Event emitted by a streaming agent loop execution.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentStreamEvent {
    IterationStart {
        iteration: u32,
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
    },
    Completed {
        result: Value,
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
}

impl AgentEventStream {
    pub fn new(rx: mpsc::Receiver<AgentStreamEvent>) -> Self {
        Self { rx }
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

fn text_of(content: &wf_types::message::MessageContentValue) -> String {
    match content {
        wf_types::message::MessageContentValue::Text(t) => t.clone(),
        wf_types::message::MessageContentValue::Rich(_) => String::new(),
    }
}

/// Streaming LLM dependency of the stream driver. Implemented by
/// `Arc<LlmWrapper>`; kept as a trait so streaming can be tested with a
/// scripted fake stream.
#[async_trait::async_trait]
pub trait StreamProvider: Send + Sync {
    async fn generate_stream(
        &self,
        request: &LlmRequest,
    ) -> AgentResult<Box<dyn wf_llm::message_stream::MessageStream>>;
}

#[async_trait::async_trait]
impl StreamProvider for LlmWrapper {
    async fn generate_stream(
        &self,
        request: &LlmRequest,
    ) -> AgentResult<Box<dyn wf_llm::message_stream::MessageStream>> {
        LlmWrapper::generate_stream(self, request)
            .await
            .map_err(Into::into)
    }
}

/// Streaming iteration driver shared by the iteration and loop coordinators.
pub struct StreamDriver;

impl StreamDriver {
    /// Run a single iteration against the LLM in stream mode, forwarding
    /// deltas and tool lifecycle events into `tx`.
    pub async fn run_iteration(
        provider: &dyn StreamProvider,
        tool_coordinator: &ToolExecutionCoordinator,
        entity: &AgentLoopEntity,
        tx: &mpsc::Sender<AgentStreamEvent>,
        event_bus: Option<&EventBus>,
    ) -> AgentResult<IterationResult> {
        let iteration = entity.state.read().await.current_iteration() + 1;
        let iteration_event = AgentStreamEvent::IterationStart { iteration };
        tx.send(iteration_event.clone())
            .await
            .map_err(|_| AgentError::ExecutionError("stream receiver dropped".to_string()))?;
        publish_to_bus(event_bus, entity.id(), &iteration_event);

        let execution_id = entity.id().clone();
        let messages = entity.conversation().read().await.messages().to_vec();
        let available_tools = entity.get_available_tools(tool_coordinator.tool_registry());
        let tools = if available_tools.is_empty() {
            None
        } else {
            Some(available_tools)
        };

        let request = LlmRequest {
            profile_id: entity.model().map(|m| m.to_string()),
            messages,
            parameters: Some(serde_json::json!({
                "temperature": 0.7,
                "max_tokens": 4096,
            })),
            tools,
            tool_call_format: entity.tool_call_format().map(|f| f.format.clone()),
            locked_tool_call_format: entity.tool_call_format().cloned(),
            violation_policy: None,
            execution_id: Some(execution_id.clone()),
            stream: Some(true),
            dead_loop_detection: None,
        };

        let mut stream = provider.generate_stream(&request).await?;
        let mut final_message: Option<wf_types::message::Message> = None;

        loop {
            let Some(event) = stream.next().await else {
                break;
            };
            match event {
                Ok(MessageStreamEvent::Text(t)) => {
                    let delta = AgentStreamEvent::LlmDelta {
                        content: t.text.clone(),
                    };
                    let _ = tx.send(delta.clone()).await;
                    publish_to_bus(event_bus, entity.id(), &delta);
                }
                Ok(MessageStreamEvent::Stream(chunk)) => {
                    let delta = AgentStreamEvent::LlmDelta {
                        content: chunk.content.clone(),
                    };
                    let _ = tx.send(delta.clone()).await;
                    publish_to_bus(event_bus, entity.id(), &delta);
                }
                Ok(MessageStreamEvent::ReasoningText(reasoning)) => {
                    let delta = AgentStreamEvent::LlmDelta {
                        content: reasoning.reasoning.clone(),
                    };
                    let _ = tx.send(delta).await;
                }
                Ok(MessageStreamEvent::Message(msg)) => {
                    final_message = Some(msg.message);
                }
                Ok(MessageStreamEvent::FinalMessage(msg)) => {
                    final_message = Some(msg.message);
                }
                Ok(MessageStreamEvent::Error(e)) => {
                    return Err(AgentError::LlmError(wf_llm::error::LlmError::StreamError(
                        e.error,
                    )));
                }
                Ok(MessageStreamEvent::Abort(a)) => {
                    return Err(AgentError::LlmError(wf_llm::error::LlmError::StreamError(
                        a.reason,
                    )));
                }
                Ok(MessageStreamEvent::End(_))
                | Ok(MessageStreamEvent::Connect(_))
                | Ok(MessageStreamEvent::InputJson(_)) => {}
                Err(e) => return Err(e.into()),
            }
        }

        let assistant_msg = final_message.ok_or_else(|| {
            AgentError::LlmError(wf_llm::error::LlmError::StreamError(
                "stream ended without a final message".to_string(),
            ))
        })?;
        let has_tool_calls = assistant_msg
            .tool_calls
            .as_ref()
            .map(|c| !c.is_empty())
            .unwrap_or(false);
        entity
            .conversation()
            .write()
            .await
            .add_message(assistant_msg.clone());

        let content = text_of(&assistant_msg.content);

        if !has_tool_calls {
            entity.state.write().await.end_iteration();
            let done = AgentStreamEvent::IterationEnd { iteration };
            tx.send(done.clone())
                .await
                .map_err(|_| AgentError::ExecutionError("stream receiver dropped".to_string()))?;
            publish_to_bus(event_bus, entity.id(), &done);
            return Ok(IterationResult {
                should_continue: false,
                content: Value::String(content),
                completion_data: None,
                tool_call_count: 0,
            });
        }

        let tool_calls = assistant_msg.tool_calls.unwrap_or_default();
        let mut tool_messages = Vec::with_capacity(tool_calls.len());
        for tc in &tool_calls {
            let start = AgentStreamEvent::ToolStart {
                tool_call_id: tc.id.clone(),
                tool_name: tc.function.name.clone(),
            };
            tx.send(start.clone())
                .await
                .map_err(|_| AgentError::ExecutionError("stream receiver dropped".to_string()))?;
            publish_to_bus(event_bus, entity.id(), &start);

            let msg = tool_coordinator
                .execute_single_tool_for_stream(entity, tc)
                .await;
            let result_text = text_of(&msg.content);
            let end = AgentStreamEvent::ToolEnd {
                tool_call_id: tc.id.clone(),
                tool_name: tc.function.name.clone(),
                success: !result_text.contains("\"error\""),
                result: result_text.clone(),
            };
            tx.send(end.clone())
                .await
                .map_err(|_| AgentError::ExecutionError("stream receiver dropped".to_string()))?;
            publish_to_bus(event_bus, entity.id(), &end);
            tool_messages.push(msg);
        }

        let tool_call_count = tool_calls.len() as u32;
        let mut completion_data = None;
        for tc in &tool_calls {
            if tc.function.name == "attempt_completion" {
                completion_data = Some(Value::String(tc.function.arguments.clone()));
            }
        }

        for msg in &tool_messages {
            entity.conversation().write().await.add_message(msg.clone());
        }

        entity.state.write().await.end_iteration();
        let done = AgentStreamEvent::IterationEnd { iteration };
        tx.send(done.clone())
            .await
            .map_err(|_| AgentError::ExecutionError("stream receiver dropped".to_string()))?;
        publish_to_bus(event_bus, entity.id(), &done);

        Ok(IterationResult {
            should_continue: completion_data.is_none(),
            content: Value::String(content),
            completion_data,
            tool_call_count,
        })
    }
}

/// Iteration executor variant for streaming runs.
pub struct StreamingIteration {
    provider: Arc<dyn StreamProvider>,
    tool_coordinator: ToolExecutionCoordinator,
    hook_executor: Arc<HookExecutor>,
    tx: mpsc::Sender<AgentStreamEvent>,
    event_bus: Option<Arc<EventBus>>,
}

impl StreamingIteration {
    pub fn new(
        provider: Arc<dyn StreamProvider>,
        tool_registry: Arc<wf_tools::registry::ToolRegistry>,
        hook_executor: Arc<HookExecutor>,
        tx: mpsc::Sender<AgentStreamEvent>,
        event_bus: Option<Arc<EventBus>>,
    ) -> Self {
        let tool_coordinator = ToolExecutionCoordinator::new(tool_registry, hook_executor.clone());
        Self {
            provider,
            tool_coordinator,
            hook_executor,
            tx,
            event_bus,
        }
    }

    pub fn with_approval(
        mut self,
        options: Option<wf_types::tool::approval::ToolApprovalOptions>,
        handler: Option<Arc<dyn crate::approval::ToolApprovalHandler>>,
    ) -> Self {
        let registry = self.tool_coordinator.tool_registry().clone();
        self.tool_coordinator = ToolExecutionCoordinator::new(registry, self.hook_executor.clone())
            .with_approval(options, handler);
        self
    }
}

#[async_trait::async_trait]
impl IterationExecutor for StreamingIteration {
    async fn execute_iteration(&self, entity: &AgentLoopEntity) -> AgentResult<IterationResult> {
        AgentHookHandler::execute_agent_hook(
            &self.hook_executor,
            entity,
            "BEFORE_ITERATION",
            HashMap::new(),
        )
        .await
        .map_err(|e| AgentError::HookError(e.to_string()))?;

        entity.state.write().await.start_iteration();

        let result = StreamDriver::run_iteration(
            self.provider.as_ref(),
            &self.tool_coordinator,
            entity,
            &self.tx,
            self.event_bus.as_deref(),
        )
        .await;

        AgentHookHandler::execute_agent_hook(
            &self.hook_executor,
            entity,
            "AFTER_ITERATION",
            HashMap::new(),
        )
        .await
        .map_err(|e| AgentError::HookError(e.to_string()))?;

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::{LlmFunctionCall, LlmToolCall, Message, MessageRole};
    use wf_types::Id;

    /// Scripted fake message stream.
    struct FakeStream {
        events: std::vec::IntoIter<Result<MessageStreamEvent, wf_llm::error::LlmError>>,
    }

    #[async_trait::async_trait]
    impl wf_llm::message_stream::MessageStream for FakeStream {
        async fn next(&mut self) -> Option<Result<MessageStreamEvent, wf_llm::error::LlmError>> {
            self.events.next()
        }
    }

    fn text_message(content: &str) -> Message {
        Message {
            id: Id::from(wf_common::generate_id()),
            role: MessageRole::Assistant,
            content: wf_types::message::MessageContentValue::Text(content.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn tool_message(tool_call_id: &str) -> Message {
        Message {
            id: Id::from(wf_common::generate_id()),
            role: MessageRole::Assistant,
            content: wf_types::message::MessageContentValue::Text(String::new()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Some(vec![LlmToolCall {
                id: tool_call_id.to_string(),
                r#type: "function".to_string(),
                function: LlmFunctionCall {
                    name: "mock_write".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
            thinking: None,
            metadata: None,
        }
    }

    /// Fake provider returning a scripted stream.
    struct FakeProvider {
        events: Arc<Vec<MessageStreamEvent>>,
        /// Optional trailing stream error (sent after the events).
        error: Option<String>,
    }

    #[async_trait::async_trait]
    impl StreamProvider for FakeProvider {
        async fn generate_stream(
            &self,
            _request: &LlmRequest,
        ) -> AgentResult<Box<dyn wf_llm::message_stream::MessageStream>> {
            let mut events: Vec<Result<MessageStreamEvent, wf_llm::error::LlmError>> =
                self.events.iter().map(|e| Ok(e.clone())).collect();
            if let Some(ref err) = self.error {
                events.push(Err(wf_llm::error::LlmError::StreamError(err.clone())));
            }
            Ok(Box::new(FakeStream {
                events: events.into_iter(),
            }))
        }
    }

    fn mock_tool_registry(
        executed: &Arc<std::sync::atomic::AtomicU32>,
    ) -> Arc<wf_tools::registry::ToolRegistry> {
        use std::sync::atomic::Ordering;
        let registry = Arc::new(wf_tools::registry::ToolRegistry::new());
        let handler: wf_tools::executor::stateless::StatelessHandler = {
            let executed = executed.clone();
            Arc::new(
                move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                    executed.fetch_add(1, Ordering::SeqCst);
                    Ok(Value::from("stream-tool-ok"))
                },
            )
        };
        registry.register_tool(wf_types::tool::Tool {
            id: "tool-1".to_string(),
            name: "mock_write".to_string(),
            description: "mock".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        registry.register_stateless_handler("tool-1", handler);
        registry
    }

    #[tokio::test]
    async fn test_stream_events_text_only() {
        let executed = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let provider = FakeProvider {
            events: Arc::new(vec![
                MessageStreamEvent::Text(wf_types::llm::MessageStreamText {
                    text: "hello ".to_string(),
                }),
                MessageStreamEvent::Text(wf_types::llm::MessageStreamText {
                    text: "world".to_string(),
                }),
                MessageStreamEvent::FinalMessage(wf_types::llm::MessageStreamFinal {
                    message: text_message("hello world"),
                    usage: None,
                }),
                MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {}),
            ]),
            error: None,
        };
        let entity = AgentLoopEntity::new(Id::from("agent-stream-1".to_string()));
        entity.state.write().await.start();

        let (tx, mut rx) = mpsc::channel(16);
        let coordinator = ToolExecutionCoordinator::new(registry, Arc::new(HookExecutor::new()));

        let result = StreamDriver::run_iteration(&provider, &coordinator, &entity, &tx, None)
            .await
            .expect("stream iteration must succeed");

        // Deltas forwarded in order.
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        assert_eq!(events.len(), 4); // IterationStart + 2 deltas + IterationEnd
        match &events[0] {
            AgentStreamEvent::IterationStart { iteration } => assert_eq!(*iteration, 1),
            other => panic!("expected IterationStart, got {:?}", other),
        }
        assert!(
            matches!(&events[1], AgentStreamEvent::LlmDelta { content } if content == "hello ")
        );
        assert!(matches!(&events[2], AgentStreamEvent::LlmDelta { content } if content == "world"));
        assert!(matches!(
            &events[3],
            AgentStreamEvent::IterationEnd { iteration: 1 }
        ));

        // No tool calls -> complete immediately with content.
        assert!(!result.should_continue);
        assert_eq!(result.content, Value::String("hello world".to_string()));
        assert_eq!(executed.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_stream_events_with_tool_call() {
        let executed = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let provider = FakeProvider {
            events: Arc::new(vec![
                MessageStreamEvent::Text(wf_types::llm::MessageStreamText {
                    text: "using tool".to_string(),
                }),
                MessageStreamEvent::FinalMessage(wf_types::llm::MessageStreamFinal {
                    message: tool_message("tc-9"),
                    usage: None,
                }),
                MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {}),
            ]),
            error: None,
        };
        let entity = AgentLoopEntity::new(Id::from("agent-stream-2".to_string()));
        entity.state.write().await.start();
        entity.state.write().await.start_iteration();

        let (tx, mut rx) = mpsc::channel(32);
        let coordinator = ToolExecutionCoordinator::new(registry, Arc::new(HookExecutor::new()));

        let result = StreamDriver::run_iteration(&provider, &coordinator, &entity, &tx, None)
            .await
            .expect("stream iteration with tool must succeed");

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        let tool_start = events
            .iter()
            .find(|e| matches!(e, AgentStreamEvent::ToolStart { .. }));
        let tool_end = events
            .iter()
            .find(|e| matches!(e, AgentStreamEvent::ToolEnd { .. }));
        assert!(tool_start.is_some(), "ToolStart missing: {:?}", events);
        assert!(tool_end.is_some(), "ToolEnd missing: {:?}", events);
        if let Some(AgentStreamEvent::ToolEnd {
            success, result: r, ..
        }) = tool_end
        {
            assert!(*success);
            assert!(r.contains("stream-tool-ok"), "unexpected result: {}", r);
        }
        assert_eq!(executed.load(std::sync::atomic::Ordering::SeqCst), 1);
        // Tool call was mock_write (not attempt_completion) -> keep looping.
        assert!(result.should_continue);
        assert_eq!(result.tool_call_count, 1);
        // Tool message added to conversation (assistant + tool).
        let messages = entity.conversation().read().await.messages().to_vec();
        assert_eq!(messages.len(), 2);
    }

    #[tokio::test]
    async fn test_stream_error_propagates() {
        let provider = FakeProvider {
            events: Arc::new(Vec::new()),
            error: Some("upstream exploded".to_string()),
        };
        let entity = AgentLoopEntity::new(Id::from("agent-stream-3".to_string()));
        entity.state.write().await.start();

        let (tx, _rx) = mpsc::channel(16);
        let coordinator = ToolExecutionCoordinator::new(
            Arc::new(wf_tools::registry::ToolRegistry::new()),
            Arc::new(HookExecutor::new()),
        );

        let err = StreamDriver::run_iteration(&provider, &coordinator, &entity, &tx, None)
            .await
            .expect_err("stream error must fail the iteration");
        assert!(err.to_string().contains("upstream exploded"));
    }
}
