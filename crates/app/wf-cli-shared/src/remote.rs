use std::pin::Pin;

use futures::Stream;
use futures::StreamExt;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::args::Cli;

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("http error: {0}")]
    Http(String),
    #[error("remote error {code}: {message}")]
    Remote { code: String, message: String },
    #[error("invalid response: {0}")]
    Invalid(String),
    #[error("not configured")]
    NotConfigured,
}

#[derive(Debug, Clone)]
pub struct RemoteClient {
    base: String,
    client: reqwest::Client,
    api_key: Option<String>,
}

impl RemoteClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let base = base_url.into().trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .user_agent("wf-cli/remote")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let api_key = std::env::var("WF_API_KEY").ok();
        Self {
            base,
            client,
            api_key,
        }
    }

    pub fn with_api_key(mut self, key: Option<String>) -> Self {
        if let Some(k) = key {
            if !k.is_empty() {
                self.api_key = Some(k);
            }
        }
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn from_cli(cli: &Cli) -> Option<Self> {
        if let Some(url) = Self::resolve_url(cli) {
            let client = Self::new(url).with_api_key(cli.api_key.clone());
            Some(client)
        } else {
            None
        }
    }

    pub fn resolve_url(cli: &Cli) -> Option<String> {
        if let Some(url) = cli.remote.clone() {
            if !url.trim().is_empty() {
                return Some(url);
            }
        }
        if let Ok(url) = std::env::var("WF_REMOTE") {
            if !url.trim().is_empty() {
                return Some(url);
            }
        }
        if let Some(crate::args::Command::Run { remote, .. }) = &cli.command {
            if let Some(url) = remote.clone() {
                if !url.trim().is_empty() {
                    return Some(url);
                }
            }
        }
        None
    }

    pub fn is_remote(cli: &Cli) -> bool {
        Self::resolve_url(cli).is_some()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn builder(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut b = self.client.request(method, self.url(path));
        if let Some(key) = &self.api_key {
            b = b.header("x-api-key", key);
        }
        b
    }

    async fn parse_envelope<T: DeserializeOwned>(
        &self,
        resp: reqwest::Response,
    ) -> Result<T, RemoteError> {
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| RemoteError::Http(e.to_string()))?;
        let v: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| RemoteError::Invalid(e.to_string()))?;
        if !status.is_success() {
            let code = v
                .get("error")
                .and_then(|e| e.get("code"))
                .and_then(|c| c.as_str())
                .unwrap_or("UNKNOWN")
                .to_string();
            let message = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|c| c.as_str())
                .unwrap_or(&format!("http {status}"))
                .to_string();
            return Err(RemoteError::Remote { code, message });
        }
        if let Some(success) = v.get("success").and_then(|s| s.as_bool()) {
            if !success {
                let code = v
                    .get("error")
                    .and_then(|e| e.get("code"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("UNKNOWN")
                    .to_string();
                let message = v
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("remote error")
                    .to_string();
                return Err(RemoteError::Remote { code, message });
            }
            let data = v.get("data").cloned().unwrap_or(serde_json::Value::Null);
            serde_json::from_value(data).map_err(|e| RemoteError::Invalid(e.to_string()))
        } else {
            // No envelope, try direct deserialize
            serde_json::from_value(v).map_err(|e| RemoteError::Invalid(e.to_string()))
        }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, RemoteError> {
        let resp = self
            .builder(reqwest::Method::GET, path)
            .send()
            .await
            .map_err(|e| RemoteError::Http(e.to_string()))?;
        self.parse_envelope(resp).await
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, RemoteError> {
        let resp = self
            .builder(reqwest::Method::POST, path)
            .json(body)
            .send()
            .await
            .map_err(|e| RemoteError::Http(e.to_string()))?;
        self.parse_envelope(resp).await
    }

    pub async fn list_workflows(&self) -> Result<serde_json::Value, RemoteError> {
        self.get_json("/api/v1/workflows").await
    }

    pub async fn get_workflow(&self, id: &str) -> Result<serde_json::Value, RemoteError> {
        self.get_json(&format!("/api/v1/workflows/{}", id)).await
    }

    pub async fn list_executions(
        &self,
        limit: Option<usize>,
    ) -> Result<serde_json::Value, RemoteError> {
        let mut path = "/api/v1/executions".to_string();
        if let Some(l) = limit {
            path = format!("{path}?limit={l}");
        }
        self.get_json(&path).await
    }

    pub async fn get_execution(&self, id: &str) -> Result<serde_json::Value, RemoteError> {
        self.get_json(&format!("/api/v1/executions/{}", id)).await
    }

    pub async fn post_json_raw(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<serde_json::Value, RemoteError> {
        let resp = self
            .builder(method, path)
            .send()
            .await
            .map_err(|e| RemoteError::Http(e.to_string()))?;
        self.parse_envelope(resp).await
    }

    pub async fn delete_json(&self, path: &str) -> Result<serde_json::Value, RemoteError> {
        let resp = self
            .builder(reqwest::Method::DELETE, path)
            .send()
            .await
            .map_err(|e| RemoteError::Http(e.to_string()))?;
        self.parse_envelope(resp).await
    }

    pub async fn put_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, RemoteError> {
        let resp = self
            .builder(reqwest::Method::PUT, path)
            .json(body)
            .send()
            .await
            .map_err(|e| RemoteError::Http(e.to_string()))?;
        self.parse_envelope(resp).await
    }

    pub async fn create_workflow(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, RemoteError> {
        self.post_json("/api/v1/workflows", body).await
    }

    pub async fn update_workflow(
        &self,
        id: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, RemoteError> {
        self.put_json(&format!("/api/v1/workflows/{}", id), body)
            .await
    }

    pub async fn delete_workflow(&self, id: &str) -> Result<serde_json::Value, RemoteError> {
        self.delete_json(&format!("/api/v1/workflows/{}", id)).await
    }

    pub async fn clone_workflow(
        &self,
        id: &str,
        new_id: Option<&str>,
    ) -> Result<serde_json::Value, RemoteError> {
        let body = if let Some(nid) = new_id {
            serde_json::json!({ "new_id": nid })
        } else {
            serde_json::json!({})
        };
        self.post_json(&format!("/api/v1/workflows/{}/clone", id), &body)
            .await
    }

    pub async fn export_workflow(&self, id: &str) -> Result<serde_json::Value, RemoteError> {
        self.get_json(&format!("/api/v1/workflows/{}/export", id))
            .await
    }

    pub async fn import_workflow(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, RemoteError> {
        self.post_json("/api/v1/workflows/import", body).await
    }

    pub async fn list_workflow_versions(&self, id: &str) -> Result<serde_json::Value, RemoteError> {
        self.get_json(&format!("/api/v1/workflows/{}/versions", id))
            .await
    }

    pub async fn get_workflow_version(
        &self,
        id: &str,
        version: &str,
    ) -> Result<serde_json::Value, RemoteError> {
        self.get_json(&format!("/api/v1/workflows/{}/versions/{}", id, version))
            .await
    }

    pub async fn rollback_workflow(
        &self,
        id: &str,
        version: &str,
    ) -> Result<serde_json::Value, RemoteError> {
        let body = serde_json::json!({ "version": version });
        self.post_json(&format!("/api/v1/workflows/{}/rollback", id), &body)
            .await
    }

    pub async fn execute_workflow(
        &self,
        id: &str,
        input: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, RemoteError> {
        let body = serde_json::json!({ "input": input });
        self.post_json(&format!("/api/v1/workflows/{}/execute", id), &body)
            .await
    }

    pub async fn cancel_execution(&self, id: &str) -> Result<serde_json::Value, RemoteError> {
        self.post_json_raw(
            reqwest::Method::POST,
            &format!("/api/v1/executions/{}/cancel", id),
        )
        .await
    }

    pub async fn pause_execution(&self, id: &str) -> Result<serde_json::Value, RemoteError> {
        self.post_json_raw(
            reqwest::Method::POST,
            &format!("/api/v1/executions/{}/pause", id),
        )
        .await
    }

    pub async fn resume_execution(&self, id: &str) -> Result<serde_json::Value, RemoteError> {
        self.post_json_raw(
            reqwest::Method::POST,
            &format!("/api/v1/executions/{}/resume", id),
        )
        .await
    }

    pub async fn delete_execution(&self, id: &str) -> Result<serde_json::Value, RemoteError> {
        self.delete_json(&format!("/api/v1/executions/{}", id))
            .await
    }

    pub async fn health(&self) -> Result<serde_json::Value, RemoteError> {
        let resp = self
            .builder(reqwest::Method::GET, "/health")
            .send()
            .await
            .map_err(|e| RemoteError::Http(e.to_string()))?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| RemoteError::Http(e.to_string()))?;
        let v: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| RemoteError::Invalid(e.to_string()))?;
        if !status.is_success() {
            return Err(RemoteError::Http(format!("health {status}: {v}")));
        }
        Ok(v)
    }

    /// Stream workflow execution events via SSE.
    /// POST /api/v1/workflows/{id}/execute/stream with body {input}
    pub async fn stream_workflow_execution(
        &self,
        workflow_id: &str,
        input: Option<serde_json::Value>,
    ) -> Result<
        Pin<
            Box<
                dyn Stream<Item = Result<wf_api::infra::stream::ExecutionStreamEvent, RemoteError>>
                    + Send,
            >,
        >,
        RemoteError,
    > {
        let body = serde_json::json!({ "input": input });
        let resp = self
            .builder(
                reqwest::Method::POST,
                &format!("/api/v1/workflows/{}/execute/stream", workflow_id),
            )
            .json(&body)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .map_err(|e| RemoteError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let bytes = resp.bytes().await.unwrap_or_default();
            let v: serde_json::Value =
                serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
            let msg = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("stream failed");
            return Err(RemoteError::Remote {
                code: status.to_string(),
                message: msg.to_string(),
            });
        }

        let byte_stream = resp.bytes_stream();
        let stream = futures::stream::unfold(
            (byte_stream, String::new()),
            |(mut byte_stream, mut buffer)| async move {
                loop {
                    if let Some(pos) = buffer.find("\n\n") {
                        let frame = buffer[..pos].to_string();
                        buffer = buffer[pos + 2..].to_string();
                        let mut data_lines: Vec<String> = Vec::new();
                        let mut event_type = String::new();
                        for line in frame.lines() {
                            if let Some(d) = line.strip_prefix("data: ") {
                                data_lines.push(d.to_string());
                            } else if let Some(d) = line.strip_prefix("data:") {
                                data_lines.push(d.trim_start().to_string());
                            } else if let Some(e) = line.strip_prefix("event: ") {
                                event_type = e.to_string();
                            } else if let Some(e) = line.strip_prefix("event:") {
                                event_type = e.trim_start().to_string();
                            }
                        }
                        if event_type == "metadata" {
                            continue;
                        }
                        let data = data_lines.join("\n");
                        if data.trim().is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<wf_api::infra::stream::ExecutionStreamEvent>(
                            &data,
                        ) {
                            Ok(evt) => return Some((Ok(evt), (byte_stream, buffer))),
                            Err(e) => {
                                return Some((
                                    Err(RemoteError::Invalid(format!("sse parse: {e}: {data}"))),
                                    (byte_stream, buffer),
                                ))
                            }
                        }
                    }
                    match byte_stream.next().await {
                        Some(Ok(bytes)) => {
                            buffer.push_str(&String::from_utf8_lossy(&bytes));
                        }
                        Some(Err(e)) => {
                            return Some((
                                Err(RemoteError::Http(e.to_string())),
                                (byte_stream, buffer),
                            ))
                        }
                        None => {
                            if buffer.trim().is_empty() {
                                return None;
                            }
                            let frame = std::mem::take(&mut buffer);
                            let mut data_lines: Vec<String> = Vec::new();
                            let mut event_type = String::new();
                            for line in frame.lines() {
                                if let Some(d) = line.strip_prefix("data: ") {
                                    data_lines.push(d.to_string());
                                } else if let Some(d) = line.strip_prefix("data:") {
                                    data_lines.push(d.trim_start().to_string());
                                } else if let Some(e) = line.strip_prefix("event: ") {
                                    event_type = e.to_string();
                                }
                            }
                            if event_type == "metadata" || data_lines.is_empty() {
                                return None;
                            }
                            let data = data_lines.join("\n");
                            match serde_json::from_str::<wf_api::infra::stream::ExecutionStreamEvent>(
                                &data,
                            ) {
                                Ok(evt) => return Some((Ok(evt), (byte_stream, buffer))),
                                Err(e) => {
                                    return Some((
                                        Err(RemoteError::Invalid(format!(
                                            "sse parse: {e}: {data}"
                                        ))),
                                        (byte_stream, buffer),
                                    ))
                                }
                            }
                        }
                    }
                }
            },
        );
        Ok(Box::pin(stream))
    }

    /// Stream agent loop execution via SSE.
    /// POST /api/v1/agent-loops/{id}/stream with body containing agent config
    pub async fn stream_agent_execution(
        &self,
        params: crate::turn::TurnParams,
    ) -> Result<
        Pin<
            Box<
                dyn Stream<Item = Result<wf_api::infra::stream::ExecutionStreamEvent, RemoteError>>
                    + Send,
            >,
        >,
        RemoteError,
    > {
        let prompt = match &params.kind {
            crate::turn::TurnKind::Agent { prompt } => prompt.clone(),
            crate::turn::TurnKind::Workflow { .. } => String::new(),
        };
        let sanitized = crate::sanitize::sanitize_user_text(&prompt);
        let body = serde_json::json!({
            "agent_id": params.agent.clone().unwrap_or_else(|| crate::config::DEFAULT_AGENT.to_string()),
            "model": params.model.clone().unwrap_or_else(|| crate::config::DEFAULT_MODEL.to_string()),
            "message": sanitized,
            "max_iterations": 50,
            "context": {},
        });
        let resp = self
            .builder(reqwest::Method::POST, "/api/v1/agent-loops/cli/stream")
            .json(&body)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .map_err(|e| RemoteError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let bytes = resp.bytes().await.unwrap_or_default();
            let v: serde_json::Value =
                serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
            let msg = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("agent stream failed");
            return Err(RemoteError::Remote {
                code: status.to_string(),
                message: msg.to_string(),
            });
        }
        let byte_stream = resp.bytes_stream();
        let stream = futures::stream::unfold(
            (byte_stream, String::new()),
            |(mut byte_stream, mut buffer)| async move {
                loop {
                    if let Some(pos) = buffer.find("\n\n") {
                        let frame = buffer[..pos].to_string();
                        buffer = buffer[pos + 2..].to_string();
                        let mut data_lines: Vec<String> = Vec::new();
                        for line in frame.lines() {
                            if let Some(d) = line.strip_prefix("data: ") {
                                data_lines.push(d.to_string());
                            } else if let Some(d) = line.strip_prefix("data:") {
                                data_lines.push(d.trim_start().to_string());
                            }
                        }
                        let data = data_lines.join("\n");
                        if data.trim().is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<wf_api::infra::stream::ExecutionStreamEvent>(
                            &data,
                        ) {
                            Ok(evt) => return Some((Ok(evt), (byte_stream, buffer))),
                            Err(e) => {
                                return Some((
                                    Err(RemoteError::Invalid(format!("sse parse: {e}: {data}"))),
                                    (byte_stream, buffer),
                                ))
                            }
                        }
                    }
                    match byte_stream.next().await {
                        Some(Ok(bytes)) => buffer.push_str(&String::from_utf8_lossy(&bytes)),
                        Some(Err(e)) => {
                            return Some((
                                Err(RemoteError::Http(e.to_string())),
                                (byte_stream, buffer),
                            ))
                        }
                        None => {
                            if buffer.trim().is_empty() {
                                return None;
                            }
                            let frame = std::mem::take(&mut buffer);
                            let mut data_lines: Vec<String> = Vec::new();
                            for line in frame.lines() {
                                if let Some(d) = line.strip_prefix("data: ") {
                                    data_lines.push(d.to_string());
                                }
                            }
                            let data = data_lines.join("\n");
                            if data.trim().is_empty() {
                                return None;
                            }
                            match serde_json::from_str::<wf_api::infra::stream::ExecutionStreamEvent>(
                                &data,
                            ) {
                                Ok(evt) => return Some((Ok(evt), (byte_stream, buffer))),
                                Err(e) => {
                                    return Some((
                                        Err(RemoteError::Invalid(format!(
                                            "sse parse: {e}: {data}"
                                        ))),
                                        (byte_stream, buffer),
                                    ))
                                }
                            }
                        }
                    }
                }
            },
        );
        Ok(Box::pin(stream))
    }
}

impl From<RemoteError> for crate::error::CliError {
    fn from(e: RemoteError) -> Self {
        match e {
            RemoteError::Remote { code, message } => {
                crate::error::CliError::Business(format!("remote {code}: {message}"))
            }
            RemoteError::Http(m) => {
                crate::error::CliError::Configuration(format!("remote http: {m}"))
            }
            RemoteError::Invalid(m) => {
                crate::error::CliError::Configuration(format!("remote invalid: {m}"))
            }
            RemoteError::NotConfigured => {
                crate::error::CliError::Arguments("remote not configured".into())
            }
        }
    }
}
