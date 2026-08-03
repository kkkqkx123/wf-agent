//! MCP transport layer: stdio / SSE / streamable-HTTP.
//!
//! Each transport starts its background I/O tasks in [`McpTransport::start`]
//! and hands the caller a [`TransportHandle`] with the request sender and
//! response receiver. The client owns the response receiver and dispatches
//! responses to pending requests by id, which allows concurrent requests
//! on a single connection.

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use crate::error::{ToolError, ToolResult};

use wf_types::tool::mcp_connection::{
    McpServerConfig, McpSseConfig, McpStdioConfig, McpStreamableHttpConfig,
};

#[derive(Debug, Clone)]
pub enum TransportConfig {
    Stdio {
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
        env: Option<HashMap<String, String>>,
    },
    Sse {
        url: String,
        headers: Option<HashMap<String, String>>,
    },
    StreamableHttp {
        url: String,
        headers: Option<HashMap<String, String>>,
    },
}

impl From<&McpServerConfig> for TransportConfig {
    fn from(config: &McpServerConfig) -> Self {
        fn metadata_to_map(m: &wf_types::Metadata) -> HashMap<String, String> {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        }

        match config {
            McpServerConfig::Stdio(c) => TransportConfig::Stdio {
                command: c.command.clone(),
                args: c.args.clone().unwrap_or_default(),
                cwd: c.cwd.clone(),
                env: c.env.as_ref().map(metadata_to_map),
            },
            McpServerConfig::Sse(c) => TransportConfig::Sse {
                url: c.url.clone(),
                headers: c.headers.as_ref().map(metadata_to_map),
            },
            McpServerConfig::StreamableHttp(c) => TransportConfig::StreamableHttp {
                url: c.url.clone(),
                headers: c.headers.as_ref().map(metadata_to_map),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    /// Request id; `None` for JSON-RPC notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: impl Into<String>, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id.into()),
            method: method.into(),
            params,
        }
    }

    /// Build a JSON-RPC notification (no id, no response expected).
    pub fn notification(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: None,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<String>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

/// Running transport handles handed back by [`McpTransport::start`]:
/// requests are sent through `request_tx`, responses arrive on `response_rx`.
pub struct TransportHandle {
    pub request_tx: mpsc::Sender<JsonRpcRequest>,
    pub response_rx: mpsc::Receiver<ToolResult<JsonRpcResponse>>,
}

/// A transport owns the connection to one MCP server. `start` spawns the
/// background I/O tasks and returns the communication channels; `close`
/// tears the connection down. The trait is intentionally channel-based so
/// the client can multiplex concurrent requests over one connection.
#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn start(&mut self) -> ToolResult<TransportHandle>;
    async fn close(&mut self) -> ToolResult<()>;
    fn is_connected(&self) -> bool;
}

pub struct StdioTransport {
    config: McpStdioConfig,
    connected: bool,
    child: Option<tokio::process::Child>,
}

impl StdioTransport {
    pub fn new(config: McpStdioConfig) -> Self {
        Self {
            config,
            connected: false,
            child: None,
        }
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn start(&mut self) -> ToolResult<TransportHandle> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::process::Command;

        let mut cmd = Command::new(&self.config.command);
        if let Some(args) = &self.config.args {
            cmd.args(args);
        }
        if let Some(cwd) = &self.config.cwd {
            cmd.current_dir(cwd);
        }
        if let Some(env) = &self.config.env {
            for (k, v) in env {
                if let Some(val) = v.as_str() {
                    cmd.env(k, val);
                }
            }
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());

        let mut child = cmd.spawn().map_err(|e| {
            ToolError::TransportError(format!("Failed to spawn MCP server process: {}", e))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ToolError::TransportError("Failed to open stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolError::TransportError("Failed to open stdout".into()))?;

        let (request_tx, mut request_rx) = mpsc::channel::<JsonRpcRequest>(64);
        let (response_tx, response_rx) = mpsc::channel::<ToolResult<JsonRpcResponse>>(64);

        let mut stdin_write = stdin;
        tokio::spawn(async move {
            while let Some(req) = request_rx.recv().await {
                let json = match serde_json::to_string(&req) {
                    Ok(j) => j,
                    Err(_) => continue,
                };
                let line = format!("{}\n", json);
                if stdin_write.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                if stdin_write.flush().await.is_err() {
                    break;
                }
            }
        });

        let stdout_read = BufReader::new(stdout);
        tokio::spawn(async move {
            let mut lines = stdout_read.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                match serde_json::from_str::<JsonRpcResponse>(&line) {
                    Ok(resp) => {
                        if response_tx.send(Ok(resp)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = response_tx
                            .send(Err(ToolError::TransportError(format!(
                                "Invalid JSON-RPC line from server: {}",
                                e
                            ))))
                            .await;
                    }
                }
            }
        });

        self.child = Some(child);
        self.connected = true;
        Ok(TransportHandle {
            request_tx,
            response_rx,
        })
    }

    async fn close(&mut self) -> ToolResult<()> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }
        self.connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

pub struct SseTransport {
    config: McpSseConfig,
    connected: bool,
    client: Option<reqwest::Client>,
}

impl SseTransport {
    pub fn new(config: McpSseConfig) -> Self {
        Self {
            config,
            connected: false,
            client: None,
        }
    }
}

#[async_trait]
impl McpTransport for SseTransport {
    async fn start(&mut self) -> ToolResult<TransportHandle> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        let (request_tx, mut request_rx) = mpsc::channel::<JsonRpcRequest>(64);
        let (response_tx, response_rx) = mpsc::channel::<ToolResult<JsonRpcResponse>>(64);

        let mut request = client.get(&self.config.url);
        if let Some(headers) = &self.config.headers {
            for (k, v) in headers {
                if let Some(val) = v.as_str() {
                    request = request.header(k, val);
                }
            }
        }

        let response = request
            .send()
            .await
            .map_err(|e| ToolError::TransportError(format!("Failed to connect SSE: {}", e)))?;

        if !response.status().is_success() {
            return Err(ToolError::TransportError(format!(
                "SSE connection failed with status: {}",
                response.status()
            )));
        }

        // The SSE stream announces the POST endpoint as its first event.
        let (endpoint_tx, endpoint_rx) = oneshot::channel::<String>();
        let endpoint_tx = Arc::new(tokio::sync::Mutex::new(Some(endpoint_tx)));

        let response_tx_clone = response_tx.clone();
        tokio::spawn(async move {
            let byte_stream = response.bytes_stream();
            let mut event_stream = byte_stream.eventsource();

            while let Some(event_result) = event_stream.next().await {
                match event_result {
                    Ok(event) => match event.event.as_str() {
                        "endpoint" => {
                            if let Some(tx) = endpoint_tx.lock().await.take() {
                                let _ = tx.send(event.data.clone());
                            }
                        }
                        "message" => match serde_json::from_str::<JsonRpcResponse>(&event.data) {
                            Ok(resp) => {
                                if response_tx_clone.send(Ok(resp)).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => {}
                        },
                        _ => {}
                    },
                    Err(_) => {
                        let _ = response_tx_clone
                            .send(Err(ToolError::TransportError("SSE stream error".into())))
                            .await;
                        break;
                    }
                }
            }
        });

        // POST loop: wait for the announced endpoint, then forward outgoing
        // requests to it. A shared client clone is moved into the task.
        let post_client = client.clone();
        let config_headers = self.config.headers.clone();
        tokio::spawn(async move {
            let endpoint = match tokio::time::timeout(Duration::from_secs(10), endpoint_rx).await {
                Ok(Ok(url)) => url,
                _ => return,
            };
            while let Some(req) = request_rx.recv().await {
                let mut post = post_client.post(&endpoint).json(&req);
                if let Some(headers) = &config_headers {
                    for (k, v) in headers {
                        if let Some(val) = v.as_str() {
                            post = post.header(k, val);
                        }
                    }
                }
                let _ = post.send().await;
            }
        });

        self.client = Some(client);
        self.connected = true;

        Ok(TransportHandle {
            request_tx,
            response_rx,
        })
    }

    async fn close(&mut self) -> ToolResult<()> {
        self.connected = false;
        self.client = None;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

pub struct StreamableHttpTransport {
    config: McpStreamableHttpConfig,
    connected: bool,
    client: Option<reqwest::Client>,
}

impl StreamableHttpTransport {
    pub fn new(config: McpStreamableHttpConfig) -> Self {
        Self {
            config,
            connected: false,
            client: None,
        }
    }
}

#[async_trait]
impl McpTransport for StreamableHttpTransport {
    async fn start(&mut self) -> ToolResult<TransportHandle> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let (request_tx, mut request_rx) = mpsc::channel::<JsonRpcRequest>(64);
        let (response_tx, response_rx) = mpsc::channel::<ToolResult<JsonRpcResponse>>(64);

        let post_client = client.clone();
        let config_headers = self.config.headers.clone();
        let url = self.config.url.clone();

        tokio::spawn(async move {
            while let Some(req) = request_rx.recv().await {
                let mut post = post_client.post(&url).json(&req);
                if let Some(headers) = &config_headers {
                    for (k, v) in headers {
                        if let Some(val) = v.as_str() {
                            post = post.header(k, val);
                        }
                    }
                }
                let result = match post.send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        if !status.is_success() {
                            Err(crate::error::ToolError::RestError {
                                url: url.clone(),
                                status: status.as_u16(),
                            })
                        } else {
                            resp.json::<JsonRpcResponse>()
                                .await
                                .map_err(ToolError::from)
                        }
                    }
                    Err(e) => Err(ToolError::HttpError(e)),
                };
                if response_tx.send(result).await.is_err() {
                    break;
                }
            }
        });

        self.client = Some(client);
        self.connected = true;
        Ok(TransportHandle {
            request_tx,
            response_rx,
        })
    }

    async fn close(&mut self) -> ToolResult<()> {
        self.connected = false;
        self.client = None;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

pub fn create_transport(config: &McpServerConfig) -> Box<dyn McpTransport> {
    match config {
        McpServerConfig::Stdio(c) => Box::new(StdioTransport::new(c.clone())),
        McpServerConfig::Sse(c) => Box::new(SseTransport::new(c.clone())),
        McpServerConfig::StreamableHttp(c) => Box::new(StreamableHttpTransport::new(c.clone())),
    }
}
