use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::{ToolError, ToolResult};
use crate::mcp::transport::{JsonRpcRequest, JsonRpcResponse, McpTransport};

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
        self.call_method(
            "tools/call",
            serde_json::json!({
                "name": tool_name,
                "arguments": arguments,
            }),
            timeout_ms,
        )
        .await
    }

    pub async fn list_tools(&self, timeout_ms: u64) -> ToolResult<Vec<McpToolInfo>> {
        let result = self
            .call_method("tools/list", Value::Null, timeout_ms)
            .await?;

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

    pub async fn list_resources(
        &self,
        timeout_ms: u64,
    ) -> ToolResult<Vec<wf_types::tool::McpResource>> {
        let result = self
            .call_method("resources/list", Value::Null, timeout_ms)
            .await?;

        let resources = result
            .get("resources")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| {
                        Some(wf_types::tool::McpResource {
                            uri: r.get("uri")?.as_str()?.to_string(),
                            name: r.get("name")?.as_str()?.to_string(),
                            description: r
                                .get("description")
                                .and_then(|d| d.as_str())
                                .map(String::from),
                            mime_type: r.get("mimeType").and_then(|m| m.as_str()).map(String::from),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(resources)
    }

    pub async fn list_resource_templates(
        &self,
        timeout_ms: u64,
    ) -> ToolResult<Vec<wf_types::tool::McpResourceTemplate>> {
        let result = self
            .call_method("resources/templates/list", Value::Null, timeout_ms)
            .await?;

        let templates = result
            .get("resourceTemplates")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(wf_types::tool::McpResourceTemplate {
                            uri_template: t.get("uriTemplate")?.as_str()?.to_string(),
                            name: t.get("name")?.as_str()?.to_string(),
                            description: t
                                .get("description")
                                .and_then(|d| d.as_str())
                                .map(String::from),
                            mime_type: t.get("mimeType").and_then(|m| m.as_str()).map(String::from),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(templates)
    }

    pub async fn read_resource(
        &self,
        uri: &str,
        timeout_ms: u64,
    ) -> ToolResult<wf_types::tool::McpResourceReadResult> {
        let result = self
            .call_method(
                "resources/read",
                serde_json::json!({ "uri": uri }),
                timeout_ms,
            )
            .await?;

        let contents = result
            .get("contents")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| {
                        Some(wf_types::tool::McpResourceContent {
                            uri: c.get("uri")?.as_str()?.to_string(),
                            mime_type: c.get("mimeType").and_then(|m| m.as_str()).map(String::from),
                            text: c.get("text").and_then(|t| t.as_str()).map(String::from),
                            blob: c.get("blob").and_then(|b| b.as_str()).map(String::from),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(wf_types::tool::McpResourceReadResult { contents })
    }

    pub async fn initialize(&self, timeout_ms: u64) -> ToolResult<Value> {
        self.call_method(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "wf-tools",
                    "version": "0.1.0",
                },
            }),
            timeout_ms,
        )
        .await
    }

    async fn call_method(&self, method: &str, params: Value, timeout_ms: u64) -> ToolResult<Value> {
        let request = self.build_request(method, params);
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

    #[tokio::test]
    async fn test_list_resources() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some("test-3".into()),
            result: Some(serde_json::json!({
                "resources": [
                    {
                        "uri": "file:///etc/hosts",
                        "name": "hosts",
                        "description": "Hosts file",
                        "mimeType": "text/plain",
                    },
                    {"uri": "db://users/1", "name": "user 1"},
                ]
            })),
            error: None,
        };

        let transport = Box::new(MockTransport::new(vec![response]));
        let client = McpClient::new("test_server", transport);

        let resources = client.list_resources(5000).await.unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0].uri, "file:///etc/hosts");
        assert_eq!(resources[0].name, "hosts");
        assert_eq!(resources[0].mime_type.as_deref(), Some("text/plain"));
        assert_eq!(resources[1].description, None);
    }

    #[tokio::test]
    async fn test_list_resource_templates() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some("test-4".into()),
            result: Some(serde_json::json!({
                "resourceTemplates": [
                    {
                        "uriTemplate": "db://users/{id}",
                        "name": "User by id",
                        "description": "Fetch a user",
                    }
                ]
            })),
            error: None,
        };

        let transport = Box::new(MockTransport::new(vec![response]));
        let client = McpClient::new("test_server", transport);

        let templates = client.list_resource_templates(5000).await.unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0].uri_template, "db://users/{id}");
        assert_eq!(templates[0].name, "User by id");
    }

    #[tokio::test]
    async fn test_read_resource() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some("test-5".into()),
            result: Some(serde_json::json!({
                "contents": [
                    {
                        "uri": "file:///etc/hosts",
                        "mimeType": "text/plain",
                        "text": "127.0.0.1 localhost",
                    }
                ]
            })),
            error: None,
        };

        let transport = Box::new(MockTransport::new(vec![response]));
        let client = McpClient::new("test_server", transport);

        let result = client
            .read_resource("file:///etc/hosts", 5000)
            .await
            .unwrap();
        assert_eq!(result.contents.len(), 1);
        assert_eq!(result.contents[0].uri, "file:///etc/hosts");
        assert_eq!(
            result.contents[0].text.as_deref(),
            Some("127.0.0.1 localhost")
        );
    }
}
