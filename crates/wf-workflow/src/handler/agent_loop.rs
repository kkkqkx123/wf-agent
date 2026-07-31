use async_trait::async_trait;
use serde_json::Value;
use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_llm::LlmWrapper;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_tools::registry::ToolRegistry;
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct AgentLoopHandler;

#[async_trait]
impl NodeHandler for AgentLoopHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::AgentLoop
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        let max_iterations = config
            .get("max_iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32;

        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let system_prompt = config
            .get("system_prompt")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let tool_names: Vec<String> = config
            .get("available_tools")
            .or_else(|| config.get("available_tool_names"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let text = if let Value::String(s) = &ctx.input {
            s.clone()
        } else {
            ctx.input.to_string()
        };

        let message = if let Some(ref sp) = system_prompt {
            format!("{}\n\n{}", sp, text)
        } else {
            text
        };

        let llm_wrapper = LlmWrapper::new();
        let tool_registry = ToolRegistry::new();

        let coordinator = AgentLoopCoordinator::new(
            std::sync::Arc::new(llm_wrapper),
            std::sync::Arc::new(tool_registry),
        );

        let loop_config = AgentLoopConfig {
            agent_id: ctx.node_id.clone(),
            model,
            available_tool_names: tool_names,
            hooks: vec![],
            max_iterations: Some(max_iterations),
        };

        let loop_input = AgentLoopInput {
            message,
            context: std::collections::HashMap::new(),
        };

        match coordinator.execute(loop_config, loop_input).await {
            Ok(output) => {
                let mut metadata = std::collections::HashMap::new();
                metadata.insert(
                    "iteration_count".to_string(),
                    Value::Number(output.iterations.into()),
                );
                if let Some(performance) = output.performance {
                    metadata.insert("performance".to_string(), performance);
                }
                metadata.insert("node_id".to_string(), Value::String(ctx.node_id.clone()));

                let final_content = output.result;

                Ok(NodeExecutionResult {
                    output: final_content,
                    next_node_ids: Vec::new(),
                    metadata,
                })
            }
            Err(e) => Err(WorkflowError::AgentError(e)),
        }
    }
}
