use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use wf_agent::entity::AgentLoopEntity;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_execution_shared::messaging::message_context_registry::MessageContextRegistry;
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;
use crate::handler::NodeHandler;

pub struct AgentLoopHandler;

#[async_trait]
impl NodeHandler for AgentLoopHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::AgentLoop
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        let _max_iterations = config.get("max_iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as u32;

        let agent_loop_id = wf_types::Id::new();
        let mut entity = AgentLoopEntity::new(agent_loop_id);

        if let Some(model) = config.get("model").and_then(|v| v.as_str()) {
            entity = entity.with_model(model.to_string());
        }

        if let Some(ref parent_id) = ctx.parent_execution_id {
            entity = entity.with_parent_execution_id(parent_id.clone());
        }

        let mut messages: Vec<Message> = Vec::new();

        if let Some(system_prompt) = config.get("system_prompt").and_then(|v| v.as_str()) {
            messages.push(Message {
                id: wf_types::Id::new(),
                role: MessageRole::System,
                content: MessageContentValue::Text(system_prompt.to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            });
        }

        let msg_inputs = config.get("messageInputs").and_then(|v| v.as_array());
        if let Some(inputs) = msg_inputs {
            for input_ref in inputs {
                if let Some(ctx_name) = input_ref.as_str() {
                    let registry = MessageContextRegistry::new();
                    if let Some(named_msgs) = registry.get(ctx_name) {
                        messages.extend(named_msgs);
                    }
                }
            }
        }

        if messages.is_empty() {
            let text = if let Value::String(s) = &ctx.input {
                s.clone()
            } else {
                ctx.input.to_string()
            };
            if !text.is_empty() {
                messages.push(Message {
                    id: wf_types::Id::new(),
                    role: MessageRole::User,
                    content: MessageContentValue::Text(text),
                    timestamp: wf_common::now(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: None,
                    thinking: None,
                    metadata: None,
                });
            }
        }

        for msg in messages {
            entity.conversation().write().await.add_message(msg);
        }

        let tool_names: Vec<String> = config.get("available_tools")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        entity = entity.with_available_tool_names(tool_names);

        let iteration_count = config.get("iteration_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let mut metadata = HashMap::new();
        metadata.insert("agent_loop_id".to_string(), Value::String(entity.id().to_string()));
        metadata.insert("iteration_count".to_string(), Value::Number(iteration_count.into()));

        let conversation = entity.conversation().read().await;
        let final_response = conversation.messages().iter().rev()
            .find(|m| m.role == MessageRole::Assistant)
            .map(|m| {
                if let MessageContentValue::Text(t) = &m.content {
                    Value::String(t.clone())
                } else {
                    Value::String(format!("{:?}", m.content))
                }
            })
            .unwrap_or(Value::Null);

        Ok(NodeExecutionResult {
            output: final_response,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}
