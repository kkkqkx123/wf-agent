use std::sync::Arc;

use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};

use crate::agent::agent_execution::RunAgentLoopParams;
use crate::ApiContext;

pub struct AgentExecutionBuilder {
    config: AgentLoopConfig,
    message: String,
    context: std::collections::HashMap<String, serde_json::Value>,
    on_completed: Option<AgentCompletedCallback>,
}

/// Callback invoked when an agent loop completes.
type AgentCompletedCallback = Arc<dyn Fn(&str) + Send + Sync>;

impl AgentExecutionBuilder {
    /// Start building an agent execution from a built loop config.
    pub fn new(config: AgentLoopConfig) -> Self {
        Self {
            config,
            message: String::new(),
            context: Default::default(),
            on_completed: None,
        }
    }

    /// Set the initial user message driving the loop.
    pub fn with_input(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Attach a callback run with the produced `agent_loop_id` on completion.
    pub fn on_completed(mut self, on_completed: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_completed = Some(Arc::new(on_completed));
        self
    }

    /// Run the loop to completion through [`crate::agent::agent_execution::run`].
    ///
    /// Composition boundary: the template is resolved before execution so
    /// builder-driven runs share the HTTP/CLI defaults.
    pub async fn execute(
        self,
        ctx: &Arc<ApiContext>,
    ) -> crate::ApiResult<wf_tools::callback::AgentLoopOutput> {
        let input = AgentLoopInput {
            message: self.message,
            context: self.context,
            conversation: Vec::new(),
        };
        let env = wf_execution_shared::agent_prompt::PromptEnvironment::new(
            Some(ctx.registries.as_ref()),
            Some(ctx.tool_registry.as_ref()),
            ctx.metrics.as_deref(),
        );
        let params = crate::agent::composition::resolve_run_params(
            &env,
            RunAgentLoopParams::new(self.config.clone(), input),
        )?;
        let output = crate::agent::agent_execution::run(ctx, params).await?;
        if let Some(callback) = &self.on_completed {
            callback(&output.agent_loop_id.to_string());
        }
        Ok(output)
    }
}
