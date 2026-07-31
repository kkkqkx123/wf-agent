use async_trait::async_trait;
use serde_json::Value;
use std::time::Instant;

use crate::error::{ToolError, ToolResult};
use crate::executor::base::BaseExecutor;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;

pub struct McpExecutor {
    server_name: String,
    connection_manager: Option<crate::mcp::connection::McpConnectionManager>,
}

impl McpExecutor {
    pub fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            connection_manager: None,
        }
    }

    pub fn with_connection_manager(
        mut self,
        manager: crate::mcp::connection::McpConnectionManager,
    ) -> Self {
        self.connection_manager = Some(manager);
        self
    }

    pub fn from_tool_config(tool: &wf_types::tool::Tool) -> ToolResult<Self> {
        let config = tool.config.as_ref().ok_or_else(|| {
            ToolError::ValidationFailed("MCP tool requires config with server_name".into())
        })?;

        let server_name = config
            .get("server_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::ValidationFailed("MCP tool config missing 'server_name'".into())
            })?;

        Ok(Self::new(server_name))
    }
}

#[async_trait]
impl ToolExecutor for McpExecutor {
    async fn execute(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        options: &ToolExecutionOptions,
        _context: &ToolExecutionContext,
    ) -> ToolResult<ToolExecutionResult> {
        let start = Instant::now();
        BaseExecutor::validate_parameters(tool, parameters)?;

        let timeout_ms = options.timeout.unwrap_or(30000);

        let result = if let Some(manager) = &self.connection_manager {
            manager.call_tool(&tool.name, parameters, timeout_ms).await
        } else {
            Err(ToolError::McpError(format!(
                "No connection manager for MCP server '{}'",
                self.server_name
            )))
        };

        let execution_time = start.elapsed().as_millis() as i64;
        match result {
            Ok(value) => Ok(BaseExecutor::build_result(
                true,
                Some(value),
                None,
                execution_time,
                0,
            )),
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
        "mcp"
    }
}
