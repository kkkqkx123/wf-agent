//! MCP protocol client.
//!
//! The client owns the transport's response stream and multiplexes
//! concurrent requests over a single connection: each request registers a
//! oneshot response slot keyed by its id, and a background dispatcher task
//! routes incoming responses to the matching slot. `connect` performs the
//! full MCP handshake (initialize + initialized notification) and captures
//! the server instructions.

use dashmap::DashMap;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

use crate::error::{ToolError, ToolResult};
use crate::mcp::transport::{JsonRpcRequest, JsonRpcResponse, McpTransport, TransportHandle};

pub struct McpClient {
    transport: Arc<tokio::sync::Mutex<Box<dyn McpTransport>>>,
    request_tx: std::sync::Mutex<Option<mpsc::Sender<JsonRpcRequest>>>,
    pending: Arc<DashMap<String, oneshot::Sender<ToolResult<JsonRpcResponse>>>>,
    next_id: Arc<AtomicU64>,
    server_name: String,
    instructions: std::sync::Mutex<Option<String>>,
    dispatcher: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl McpClient {
    pub fn new(server_name: impl Into<String>, transport: Box<dyn McpTransport>) -> Self {
        Self {
            transport: Arc::new(tokio::sync::Mutex::new(transport)),
            request_tx: std::sync::Mutex::new(None),
            pending: Arc::new(DashMap::new()),
            next_id: Arc::new(AtomicU64::new(0)),
            server_name: server_name.into(),
            instructions: std::sync::Mutex::new(None),
            dispatcher: std::sync::Mutex::new(None),
        }
    }

    /// Start the transport, run the MCP handshake and spawn the response
    /// dispatcher. Returns the raw `initialize` result.
    pub async fn connect(&self) -> ToolResult<Value> {
        let handle: TransportHandle = {
            let mut transport = self.transport.lock().await;
            transport.start().await?
        };

        *self.request_tx.lock().unwrap() = Some(handle.request_tx);
        self.spawn_dispatcher(handle.response_rx);

        let timeout_ms = 30000;
        let init = self
            .call_method(
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
            .await?;

        if let Some(instructions) = init.get("instructions").and_then(|v| v.as_str()) {
            *self.instructions.lock().unwrap() = Some(instructions.to_string());
        }

        // MCP handshake: the initialized notification follows initialize.
        self.send_notification("notifications/initialized", None)
            .await?;

        Ok(init)
    }

    pub async fn disconnect(&self) -> ToolResult<()> {
        if let Some(handle) = self.dispatcher.lock().unwrap().take() {
            handle.abort();
        }
        self.reject_all_pending("Connection closed");
        *self.request_tx.lock().unwrap() = None;
        let mut transport = self.transport.lock().await;
        transport.close().await
    }

    pub async fn is_connected(&self) -> bool {
        let transport = self.transport.lock().await;
        transport.is_connected()
    }

    /// Server instructions from the `initialize` response (if any).
    pub fn instructions(&self) -> Option<String> {
        self.instructions.lock().unwrap().clone()
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

    /// Explicit `initialize` (kept for callers that need the raw result).
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

    /// Spawn the response dispatcher: route responses to pending slots by id.
    fn spawn_dispatcher(&self, response_rx: mpsc::Receiver<ToolResult<JsonRpcResponse>>) {
        let pending = self.pending.clone();
        let dispatcher = tokio::spawn(async move {
            let mut response_rx = response_rx;
            while let Some(response) = response_rx.recv().await {
                match response {
                    Ok(resp) => {
                        if let Some(id) = resp.id.as_ref() {
                            if let Some((_, tx)) = pending.remove(id) {
                                let _ = tx.send(Ok(resp));
                            }
                        }
                    }
                    Err(e) => {
                        let message = e.to_string();
                        let keys: Vec<String> = pending.iter().map(|e| e.key().clone()).collect();
                        for key in keys {
                            if let Some((_, tx)) = pending.remove(&key) {
                                let _ = tx.send(Err(ToolError::McpError(message.clone())));
                            }
                        }
                    }
                }
            }
            // Response channel closed: reject everything still pending.
            let keys: Vec<String> = pending.iter().map(|e| e.key().clone()).collect();
            for key in keys {
                if let Some((_, tx)) = pending.remove(&key) {
                    let _ = tx.send(Err(ToolError::McpError("Connection closed".into())));
                }
            }
        });
        *self.dispatcher.lock().unwrap() = Some(dispatcher);
    }

    /// Reject every pending request with the given error message.
    fn reject_all_pending(&self, message: &str) {
        let keys: Vec<String> = self.pending.iter().map(|e| e.key().clone()).collect();
        for key in keys {
            if let Some((_, tx)) = self.pending.remove(&key) {
                let _ = tx.send(Err(ToolError::McpError(message.to_string())));
            }
        }
    }

    /// Send a JSON-RPC notification (no id, no response expected).
    async fn send_notification(&self, method: &str, params: Option<Value>) -> ToolResult<()> {
        let request = JsonRpcRequest::notification(method, params);
        let tx = self
            .request_tx
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| ToolError::McpError("Transport not connected".into()))?;
        tx.send(request)
            .await
            .map_err(|_| ToolError::McpError("Request channel closed".into()))
    }

    async fn call_method(&self, method: &str, params: Value, timeout_ms: u64) -> ToolResult<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst).to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.insert(id.clone(), tx);

        let request = JsonRpcRequest::new(id.clone(), method, Some(params));
        let Some(request_tx) = self.request_tx.lock().unwrap().clone() else {
            self.pending.remove(&id);
            return Err(ToolError::McpError("Transport not connected".into()));
        };

        eprintln!("[client] sending request id={} method={}", id, method);
        if request_tx.send(request).await.is_err() {
            self.pending.remove(&id);
            return Err(ToolError::McpError("Request channel closed".into()));
        }
        eprintln!("[client] request sent id={}", id);

        let result = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx).await;

        self.pending.remove(&id);

        match result {
            Ok(Ok(Ok(response))) => {
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
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => Err(ToolError::McpError("Connection closed".into())),
            Err(_) => Err(ToolError::Timeout {
                tool_id: self.server_name.clone(),
                timeout_ms,
            }),
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

    /// Mock transport that serves pre-programmed responses in order.
    struct MockTransport {
        responses: std::sync::Mutex<VecDeque<JsonRpcResponse>>,
    }

    impl MockTransport {
        fn new(responses: Vec<JsonRpcResponse>) -> Self {
            Self {
                responses: std::sync::Mutex::new(responses.into()),
            }
        }
    }

    #[async_trait::async_trait]
    impl McpTransport for MockTransport {
        async fn start(&mut self) -> ToolResult<TransportHandle> {
            let (request_tx, mut request_rx) = mpsc::channel::<JsonRpcRequest>(64);
            let (response_tx, response_rx) = mpsc::channel::<ToolResult<JsonRpcResponse>>(64);
            tokio::spawn(async move {
                while let Some(req) = request_rx.recv().await {
                    if let Some(id) = &req.id {
                        let _ = response_tx
                            .send(Ok(JsonRpcResponse {
                                jsonrpc: "2.0".into(),
                                id: Some(id.clone()),
                                result: Some(Value::Null),
                                error: None,
                            }))
                            .await;
                    }
                }
            });
            Ok(TransportHandle {
                request_tx,
                response_rx,
            })
        }
        async fn close(&mut self) -> ToolResult<()> {
            Ok(())
        }
        fn is_connected(&self) -> bool {
            true
        }
    }

    use std::collections::VecDeque;

    #[tokio::test]
    async fn test_list_tools() {
        let transport = Box::new(MockTransport::new(vec![]));
        let client = McpClient::new("test_server", transport);
        // connect runs initialize against the mock; response is Null, which
        // is an empty result for tools/list — acceptable for the mock.
        let _ = client.connect().await;
        let tools = client.list_tools(5000).await.unwrap();
        assert!(tools.is_empty());
    }

    #[tokio::test]
    async fn test_concurrent_calls_complete() {
        let transport = Box::new(MockTransport::new(vec![]));
        let client = Arc::new(McpClient::new("test_server", transport));
        let _ = client.connect().await;

        // Fire several concurrent calls: the dispatcher must route each
        // response to its own request.
        let mut handles = Vec::new();
        for i in 0..10 {
            let client = client.clone();
            handles.push(tokio::spawn(async move {
                let result = client
                    .call_tool(&format!("tool_{}", i), &Value::Null, 5000)
                    .await;
                assert!(result.is_ok(), "call {} failed: {:?}", i, result);
            }));
        }
        for handle in handles {
            handle.await.unwrap();
        }
    }
}
