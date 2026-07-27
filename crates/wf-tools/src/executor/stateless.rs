use async_trait::async_trait;
use serde_json::Value;
use std::time::Instant;

use crate::error::ToolResult;
use crate::executor::base::BaseExecutor;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;

#[derive(Debug, Clone)]
pub struct StatelessToolRuntime {
    pub endpoint: Option<String>,
    pub method: Option<String>,
}

pub struct StatelessExecutor {
    #[allow(dead_code)]
    runtime: Option<StatelessToolRuntime>,
}

impl StatelessExecutor {
    pub fn new() -> Self {
        Self { runtime: None }
    }

    pub fn with_runtime(runtime: StatelessToolRuntime) -> Self {
        Self {
            runtime: Some(runtime),
        }
    }

    pub fn from_tool_config(tool: &wf_types::tool::Tool) -> Self {
        let runtime = tool.config.as_ref().map(|config| StatelessToolRuntime {
                endpoint: config
                    .get("endpoint")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                method: config
                    .get("method")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            });

        Self { runtime }
    }
}

impl Default for StatelessExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for StatelessExecutor {
    async fn execute(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        options: &ToolExecutionOptions,
        context: &ToolExecutionContext,
    ) -> ToolResult<ToolExecutionResult> {
        let start = Instant::now();
        BaseExecutor::validate_parameters(tool, parameters)?;

        let result = self.run_stateless(tool, parameters, options, context);

        let execution_time = start.elapsed().as_millis() as i64;
        match result {
            Ok(value) => Ok(BaseExecutor::build_result(true, Some(value), None, execution_time, 0)),
            Err(e) => Ok(BaseExecutor::build_result(
                false,
                None,
                Some(e.to_string()),
                execution_time,
                0,
            )),
        }
    }

    fn executor_type(&self) -> &str {
        "stateless"
    }
}

impl StatelessExecutor {
    fn run_stateless(
        &self,
        _tool: &wf_types::tool::Tool,
        parameters: &Value,
        _options: &ToolExecutionOptions,
        _context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        Ok(serde_json::json!({
            "status": "executed",
            "parameters": parameters,
        }))
    }
}
