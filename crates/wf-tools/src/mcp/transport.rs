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
    McpServerConfig, McpStdioConfig, McpSseConfig, McpStreamableHttpConfig,
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
        fn metadata_to_map(
            m: &wf_types::Metadata,
        ) -> HashMap<String, String> {
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
    pub id: String,
    pub method: String,
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: impl Into<String>, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: id.into(),
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

pub struct McpTransportHandle {
    pub request_tx: mpsc::Sender<JsonRpcRequest>,
    pub response_rx: mpsc::Receiver<JsonRpcResponse>,
}

#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn start(&mut self) -> ToolResult<()>;
    async fn send(&mut self, request: JsonRpcRequest) -> ToolResult<()>;
    async fn receive(&mut self) -> ToolResult<Option<JsonRpcResponse>>;
    async fn close(&mut self) -> ToolResult<()>;
    fn is_connected(&self) -> bool;
}

pub struct StdioTransport {
    config: McpStdioConfig,
    connected: bool,
    child: Option<tokio::process::Child>,
    request_tx: Option<mpsc::Sender<JsonRpcRequest>>,
    response_rx: Option<mpsc::Receiver<JsonRpcResponse>>,
}

impl StdioTransport {
    pub fn new(config: McpStdioConfig) -> Self {
        Self {
            config,
            connected: false,
            child: None,
            request_tx: None,
            response_rx: None,
        }
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn start(&mut self) -> ToolResult<()> {
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

        let mut child = cmd.spawn().map_err(|e| ToolError::TransportError(format!(
            "Failed to spawn MCP server process: {}",
            e
        )))?;

        let stdin = child.stdin.take().ok_or_else(|| {
            ToolError::TransportError("Failed to open stdin".into())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            ToolError::TransportError("Failed to open stdout".into())
        })?;

        let (request_tx, mut request_rx) = mpsc::channel::<JsonRpcRequest>(64);
        let (response_tx, response_rx) = mpsc::channel::<JsonRpcResponse>(64);

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
                if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(&line) {
                    if response_tx.send(resp).await.is_err() {
                        break;
                    }
                }
            }
        });

        self.child = Some(child);
        self.request_tx = Some(request_tx);
        self.response_rx = Some(response_rx);
        self.connected = true;
        Ok(())
    }

    async fn send(&mut self, request: JsonRpcRequest) -> ToolResult<()> {
        match &self.request_tx {
            Some(tx) => tx
                .send(request)
                .await
                .map_err(|_| ToolError::TransportError("Send channel closed".into())),
            None => Err(ToolError::TransportError("Transport not started".into())),
        }
    }

    async fn receive(&mut self) -> ToolResult<Option<JsonRpcResponse>> {
        match &mut self.response_rx {
            Some(rx) => Ok(rx.recv().await),
            None => Err(ToolError::TransportError("Transport not started".into())),
        }
    }

    async fn close(&mut self) -> ToolResult<()> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }
        self.connected = false;
        self.request_tx = None;
        self.response_rx = None;
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
    endpoint_url: Option<String>,
    response_rx: Option<mpsc::Receiver<ToolResult<JsonRpcResponse>>>,
    response_tx: Option<mpsc::Sender<ToolResult<JsonRpcResponse>>>,
}

impl SseTransport {
    pub fn new(config: McpSseConfig) -> Self {
        Self {
            config,
            connected: false,
            client: None,
            endpoint_url: None,
            response_rx: None,
            response_tx: None,
        }
    }

    async fn post_request(&self, request: &JsonRpcRequest) -> ToolResult<()> {
        let endpoint = self.endpoint_url.as_ref().ok_or_else(|| {
            ToolError::TransportError("SSE: no endpoint URL received yet".into())
        })?;
        let client = self.client.as_ref().ok_or_else(|| {
            ToolError::TransportError("SSE transport not started".into())
        })?;

        let mut req = client.post(endpoint).json(request);
        if let Some(headers) = &self.config.headers {
            for (k, v) in headers {
                if let Some(val) = v.as_str() {
                    req = req.header(k, val);
                }
            }
        }

        req.send().await?;
        Ok(())
    }
}

#[async_trait]
impl McpTransport for SseTransport {
    async fn start(&mut self) -> ToolResult<()> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        let (response_tx, response_rx) = mpsc::channel::<ToolResult<JsonRpcResponse>>(64);

        let mut request = client.get(&self.config.url);
        if let Some(headers) = &self.config.headers {
            for (k, v) in headers {
                if let Some(val) = v.as_str() {
                    request = request.header(k, val);
                }
            }
        }

        let response = request.send().await.map_err(|e| {
            ToolError::TransportError(format!("Failed to connect SSE: {}", e))
        })?;

        if !response.status().is_success() {
            return Err(ToolError::TransportError(format!(
                "SSE connection failed with status: {}",
                response.status()
            )));
        }

        let response_tx_clone = response_tx.clone();
        let (endpoint_tx, endpoint_rx) = oneshot::channel::<String>();
        let endpoint_tx = Arc::new(tokio::sync::Mutex::new(Some(endpoint_tx)));

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
                        "message" => {
                            if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(&event.data) {
                                let _ = response_tx_clone.send(Ok(resp)).await;
                            }
                        }
                        _ => {}
                    },
                    Err(_) => {
                        let _ = response_tx_clone.send(Err(ToolError::TransportError(
                            "SSE stream error".into(),
                        ))).await;
                        break;
                    }
                }
            }
        });

        self.client = Some(client);
        self.response_tx = Some(response_tx);
        self.response_rx = Some(response_rx);

        match tokio::time::timeout(Duration::from_secs(10), endpoint_rx).await {
            Ok(Ok(url)) => {
                self.endpoint_url = Some(url);
                self.connected = true;
                Ok(())
            }
            Ok(Err(_)) => {
                self.connected = true;
                Ok(())
            }
            Err(_) => Err(ToolError::TransportError(
                "SSE endpoint not received within 10s".into(),
            )),
        }
    }

    async fn send(&mut self, request: JsonRpcRequest) -> ToolResult<()> {
        self.post_request(&request).await
    }

    async fn receive(&mut self) -> ToolResult<Option<JsonRpcResponse>> {
        match &mut self.response_rx {
            Some(rx) => match rx.recv().await {
                Some(result) => result.map(Some),
                None => Ok(None),
            },
            None => Err(ToolError::TransportError("Transport not started".into())),
        }
    }

    async fn close(&mut self) -> ToolResult<()> {
        self.connected = false;
        self.client = None;
        self.endpoint_url = None;
        self.response_tx = None;
        self.response_rx = None;
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
    response_rx: Option<mpsc::Receiver<ToolResult<JsonRpcResponse>>>,
    response_tx: Option<mpsc::Sender<ToolResult<JsonRpcResponse>>>,
}

impl StreamableHttpTransport {
    pub fn new(config: McpStreamableHttpConfig) -> Self {
        Self {
            config,
            connected: false,
            client: None,
            response_rx: None,
            response_tx: None,
        }
    }

    async fn send_request(&self, body: &Value) -> ToolResult<Value> {
        let client = self.client.as_ref().ok_or_else(|| {
            ToolError::TransportError("Transport not started".into())
        })?;

        let mut req = client.post(&self.config.url).json(body);

        if let Some(headers) = &self.config.headers {
            for (k, v) in headers {
                if let Some(val) = v.as_str() {
                    req = req.header(k, val);
                }
            }
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(ToolError::RestError {
                url: self.config.url.clone(),
                status: status.as_u16(),
            });
        }

        let body = resp.json::<Value>().await?;
        Ok(body)
    }
}

#[async_trait]
impl McpTransport for StreamableHttpTransport {
    async fn start(&mut self) -> ToolResult<()> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let (response_tx, response_rx) = mpsc::channel::<ToolResult<JsonRpcResponse>>(64);

        self.client = Some(client);
        self.response_tx = Some(response_tx);
        self.response_rx = Some(response_rx);
        self.connected = true;
        Ok(())
    }

    async fn send(&mut self, request: JsonRpcRequest) -> ToolResult<()> {
        let response = self.send_request(&serde_json::to_value(&request)?).await?;
        let json_resp: JsonRpcResponse = serde_json::from_value(response)?;
        if let Some(tx) = &self.response_tx {
            let _ = tx.send(Ok(json_resp)).await;
        }
        Ok(())
    }

    async fn receive(&mut self) -> ToolResult<Option<JsonRpcResponse>> {
        match &mut self.response_rx {
            Some(rx) => match rx.recv().await {
                Some(result) => result.map(Some),
                None => Ok(None),
            },
            None => Err(ToolError::TransportError("Transport not started".into())),
        }
    }

    async fn close(&mut self) -> ToolResult<()> {
        self.connected = false;
        self.client = None;
        self.response_tx = None;
        self.response_rx = None;
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
