use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::{ToolError, ToolResult};
use crate::mcp::transport::{
    JsonRpcRequest, JsonRpcResponse, McpTransport,
};

pub struct McpClient {
    transport: Arc<Mutex<Box<dyn McpTransport>>>,
    #[allow(dead_code)]
    next_id: Arc<Mutex<u64>>,
    server_name: String,
}

impl McpClient {
    pub fn new(server_name: impl Into<String>, transport: Box<dyn McpTransport>) -> Self {
        Self {
            transport: Arc::new(Mutex::new(transport)),
            next_id: Arc::new(Mutex::new(0)),
            server_name: server_name.into(),
        }
    }

    pub async fn connect(&self) -> ToolResult<()> {
        let mut transport = self.transport.lock().await;
        transport.start().await
    }

    pub async fn disconnect(&self) -> ToolResult<()> {
        let mut transport = self.transport.lock().await;
        transport.close().await
    }

    pub async fn is_connected(&self) -> bool {
        let transport = self.transport.lock().await;
        transport.is_connected()
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: &Value,
        timeout_ms: u64,
    ) -> ToolResult<Value> {
        let request = self.build_request("tools/call", serde_json::json!({
            "name": tool_name,
            "arguments": arguments,
        }));

        let request_id = request.id.clone();
        let response = self.send_request(request, timeout_ms).await?;

        if response.id.as_ref() != Some(&request_id) {
            return Err(ToolError::McpError(format!(
                "Response ID mismatch: expected {}, got {:?}",
                request_id, response.id
            )));
        }

        if let Some(error) = response.error {
            return Err(ToolError::McpError(format!(
                "JSON-RPC error {}: {}",
                error.code, error.message
            )));
        }

        response
            .result
            .ok_or_else(|| ToolError::McpError("Empty response result".into()))
    }

    pub async fn list_tools(&self, timeout_ms: u64) -> ToolResult<Vec<McpToolInfo>> {
        let request = self.build_request("tools/list", Value::Null);
        let request_id = request.id.clone();

        let response = self.send_request(request, timeout_ms).await?;

        if response.id.as_ref() != Some(&request_id) {
            return Err(ToolError::McpError("Response ID mismatch".into()));
        }

        if let Some(error) = response.error {
            return Err(ToolError::McpError(format!(
                "JSON-RPC error {}: {}",
                error.code, error.message
            )));
        }

        let result = response.result.unwrap_or(Value::Null);
        let tools = result
            .get("tools")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(McpToolInfo {
                            name: t.get("name")?.as_str()?.to_string(),
                            description: t
                                .get("description")
                                .and_then(|d| d.as_str())
                                .map(String::from),
                            input_schema: t.get("inputSchema").cloned(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(tools)
    }

    pub async fn initialize(&self, timeout_ms: u64) -> ToolResult<Value> {
        let request = self.build_request("initialize", serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "wf-tools",
                "version": "0.1.0",
            },
        }));

        let request_id = request.id.clone();
        let response = self.send_request(request, timeout_ms).await?;

        if response.id.as_ref() != Some(&request_id) {
            return Err(ToolError::McpError("Response ID mismatch".into()));
        }

        if let Some(error) = response.error {
            return Err(ToolError::McpError(format!(
                "Initialize error {}: {}",
                error.code, error.message
            )));
        }

        response
            .result
            .ok_or_else(|| ToolError::McpError("Empty initialize response".into()))
    }

    fn build_request(&self, method: &str, params: Value) -> JsonRpcRequest {
        let id = wf_common::generate_id();
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params: Some(params),
        }
    }

    async fn send_request(
        &self,
        request: JsonRpcRequest,
        timeout_ms: u64,
    ) -> ToolResult<JsonRpcResponse> {
        {
            let mut transport = self.transport.lock().await;
            transport.send(request.clone()).await?;
        }

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            self.wait_for_response(request.id),
        )
        .await;

        match result {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ToolError::Timeout {
                tool_id: self.server_name.clone(),
                timeout_ms,
            }),
        }
    }

    async fn wait_for_response(&self, request_id: String) -> ToolResult<JsonRpcResponse> {
        loop {
            let mut transport = self.transport.lock().await;
            match transport.receive().await? {
                Some(response) if response.id.as_ref() == Some(&request_id) => {
                    return Ok(response);
                }
                Some(_) => continue,
                None => {
                    return Err(ToolError::McpError("Connection closed".into()));
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::transport::*;

    struct MockTransport {
        responses: Vec<JsonRpcResponse>,
        last_request_id: std::sync::Mutex<Option<String>>,
    }

    impl MockTransport {
        fn new(responses: Vec<JsonRpcResponse>) -> Self {
            Self {
                responses,
                last_request_id: std::sync::Mutex::new(None),
            }
        }
    }

    #[async_trait::async_trait]
    impl McpTransport for MockTransport {
        async fn start(&mut self) -> ToolResult<()> {
            Ok(())
        }
        async fn send(&mut self, request: JsonRpcRequest) -> ToolResult<()> {
            *self.last_request_id.lock().unwrap() = Some(request.id);
            Ok(())
        }
        async fn receive(&mut self) -> ToolResult<Option<JsonRpcResponse>> {
            if !self.responses.is_empty() {
                let mut resp = self.responses[0].clone();
                if let Some(ref req_id) = *self.last_request_id.lock().unwrap() {
                    resp.id = Some(req_id.clone());
                }
                Ok(Some(resp))
            } else {
                Ok(None)
            }
        }
        async fn close(&mut self) -> ToolResult<()> {
            Ok(())
        }
        fn is_connected(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_list_tools() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some("test-1".into()),
            result: Some(serde_json::json!({
                "tools": [
                    {"name": "read_file", "description": "Read a file"},
                    {"name": "write_file", "description": "Write a file"},
                ]
            })),
            error: None,
        };

        let transport = Box::new(MockTransport::new(vec![response]));
        let client = McpClient::new("test_server", transport);

        let tools = client.list_tools(5000).await.unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(tools[1].name, "write_file");
    }

    #[tokio::test]
    async fn test_call_tool() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some("test-2".into()),
            result: Some(serde_json::json!({
                "content": [{"type": "text", "text": "Hello"}]
            })),
            error: None,
        };

        let transport = Box::new(MockTransport::new(vec![response]));
        let client = McpClient::new("test_server", transport);

        let result = client
            .call_tool("greet", &serde_json::json!({"name": "world"}), 5000)
            .await;
        assert!(result.is_ok());
    }
}
