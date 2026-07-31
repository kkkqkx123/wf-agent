use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_llm::LlmWrapper;
use wf_types::llm::LlmRequest;
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct LlmHandler;

#[async_trait]
impl NodeHandler for LlmHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Llm
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        let profile_id = config
            .get("profile_id")
            .or_else(|| config.get("model"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let system_prompt = config.get("system_prompt").and_then(|v| v.as_str());

        let mut messages: Vec<Message> = Vec::new();

        if let Some(system) = system_prompt {
            messages.push(Message {
                id: wf_types::Id::new(),
                role: MessageRole::System,
                content: MessageContentValue::Text(system.to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            });
        }

        let input_messages = config.get("messages").and_then(|v| v.as_array());
        if let Some(msgs) = input_messages {
            for msg_val in msgs {
                if let Ok(msg) = serde_json::from_value::<Message>(msg_val.clone()) {
                    messages.push(msg);
                }
            }
        }

        if messages.is_empty() {
            let text = if let Value::String(s) = &ctx.input {
                s.clone()
            } else {
                ctx.input.to_string()
            };
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

        let request = LlmRequest {
            profile_id,
            messages,
            parameters: config.get("parameters").cloned(),
            tools: None,
            tool_call_format: None,
            locked_tool_call_format: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
        };

        let mut llm_wrapper = LlmWrapper::new();
        if let Some(metrics) = &ctx.metrics {
            llm_wrapper = llm_wrapper.with_token_metrics(metrics.token().as_ref().clone());
        }
        let response = llm_wrapper
            .generate(&request)
            .await
            .map_err(|e| WorkflowError::Internal(format!("LLM call failed: {}", e)))?;

        let output = response
            .content
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null);

        let mut metadata = HashMap::new();
        metadata.insert("model".to_string(), Value::String(response.model));
        if let Some(finish_reason) = response.finish_reason {
            metadata.insert("finish_reason".to_string(), Value::String(finish_reason));
        }
        if let Some(usage) = response.usage {
            metadata.insert(
                "prompt_tokens".to_string(),
                Value::Number(usage.prompt_tokens.into()),
            );
            metadata.insert(
                "completion_tokens".to_string(),
                Value::Number(usage.completion_tokens.into()),
            );
        }

        Ok(NodeExecutionResult {
            output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}
