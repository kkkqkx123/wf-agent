//! Streamed agent-loop execution: forward lifecycle events to the workflow
//! event bus, aggregate the final result, and map the outcome to a node result.

use futures::StreamExt;
use serde_json::Value;

use wf_agent::{AgentEventStream, AgentStreamEvent};
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};

use crate::error::{WorkflowError, WorkflowResult};

/// Drain the agent-loop event stream, republishing each event on the workflow
/// bus and capturing the terminal `Completed`/`Failed` outcome. A `Failed`
/// event is surfaced as an execution error; otherwise the aggregated result and
/// iteration count become the node output.
pub(crate) async fn run_streaming(
    mut stream: AgentEventStream,
    ctx: &NodeExecutionContext,
) -> WorkflowResult<NodeExecutionResult> {
    let mut final_result = Value::Null;
    let mut iterations = 0u32;
    let mut last_error: Option<String> = None;
    while let Some(event) = stream.next().await {
        if let Some(ref bus) = ctx.event_bus {
            let event_type = match &event {
                AgentStreamEvent::LlmDelta { .. } => wf_types::events::EventType::LlmStreamChunk,
                AgentStreamEvent::ToolStart { .. } => {
                    wf_types::events::EventType::AgentToolExecutionStarted
                }
                AgentStreamEvent::ToolEnd { .. } => {
                    wf_types::events::EventType::AgentToolExecutionCompleted
                }
                AgentStreamEvent::IterationStart { .. } => {
                    wf_types::events::EventType::AgentIterationStarted
                }
                AgentStreamEvent::IterationEnd { .. } => {
                    wf_types::events::EventType::AgentIterationCompleted
                }
                AgentStreamEvent::Completed { .. } => wf_types::events::EventType::AgentCompleted,
                AgentStreamEvent::Failed { .. } => wf_types::events::EventType::AgentFailed,
                AgentStreamEvent::Interrupted { .. } => wf_types::events::EventType::AgentCancelled,
                AgentStreamEvent::ReasoningDelta { .. } => {
                    wf_types::events::EventType::LlmStreamChunk
                }
                AgentStreamEvent::Usage { .. } => wf_types::events::EventType::LlmStreamDone,
                AgentStreamEvent::SubAgentStarted { .. } => {
                    wf_types::events::EventType::AgentStarted
                }
                AgentStreamEvent::SubAgentEnded { .. } => {
                    wf_types::events::EventType::AgentCompleted
                }
            };
            let bus_event = wf_types::events::BaseEvent {
                id: wf_common::generate_id(),
                r#type: event_type,
                timestamp: wf_common::now(),
                workflow_id: Some(ctx.execution_id.clone()),
                execution_id: Some(ctx.execution_id.clone()),
                agent_loop_id: Some(ctx.node_id.clone()),
                event_name: None,
                metadata: serde_json::to_value(&event).ok().and_then(|v| {
                    v.as_object()
                        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                }),
            };
            bus.publish_logged(
                bus_event,
                &format!("workflow={} agent-loop={}", ctx.execution_id, ctx.node_id),
            )
            .ok();
        }
        match event {
            AgentStreamEvent::Completed {
                result,
                iterations: it,
            } => {
                final_result = result;
                iterations = it;
            }
            AgentStreamEvent::Failed { error } => {
                last_error = Some(error);
            }
            _ => {}
        }
    }

    if let Some(error) = last_error {
        return Err(WorkflowError::AgentError(
            wf_agent::AgentError::ExecutionError(error),
        ));
    }

    let mut metadata = std::collections::HashMap::new();
    metadata.insert(
        "iteration_count".to_string(),
        Value::Number(iterations.into()),
    );
    metadata.insert("node_id".to_string(), Value::String(ctx.node_id.clone()));
    Ok(NodeExecutionResult {
        output: final_result,
        next_node_ids: Vec::new(),
        metadata,
    })
}
