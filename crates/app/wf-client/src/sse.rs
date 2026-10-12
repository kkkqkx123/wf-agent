//! SSE streaming layer for endpoints the generator cannot express.
//!
//! Five operations serve `text/event-stream`; progenitor only models JSON, so
//! they are removed from codegen (see `normalize`) and streamed here with
//! `reqwest` bytes + `eventsource-stream` parsing.

use futures::Stream;
use futures::StreamExt;

use crate::error::{ClientError, ClientResult};
use crate::WfClient;

/// One parsed SSE frame.
#[derive(Debug, Clone)]
pub struct SseFrame {
    /// Event name (`event:` field, empty for message-only frames).
    pub event: String,
    /// Decoded JSON payload when the data parses, else the raw string.
    pub data: serde_json::Value,
}

impl SseFrame {
    /// True when the frame carries a terminal error payload.
    pub fn is_error(&self) -> bool {
        self.event.eq_ignore_ascii_case("error")
    }
}

/// Known streaming endpoints owned by this layer.
pub mod endpoints {
    /// `POST /api/v1/workflows/{id}/execute/stream`
    pub fn workflow_execute(id: &str) -> String {
        format!("/api/v1/workflows/{id}/execute/stream")
    }

    /// `POST /api/v1/agent-loops/cli/stream`
    pub const AGENT_CLI_STREAM: &str = "/api/v1/agent-loops/cli/stream";

    /// `POST /api/v1/agent-loops/{id}/stream`
    pub fn agent_loop_stream(id: &str) -> String {
        format!("/api/v1/agent-loops/{id}/stream")
    }

    /// `POST /api/v1/llm/generate-stream`
    pub const LLM_GENERATE_STREAM: &str = "/api/v1/llm/generate-stream";

    /// `GET /api/v1/events/stream`
    pub const EVENTS_STREAM: &str = "/api/v1/events/stream";
}

impl WfClient {
    /// POST a JSON body and stream back SSE frames.
    pub async fn post_stream(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> ClientResult<impl Stream<Item = Result<SseFrame, ClientError>> + Send + use<>> {
        let url = format!("{}{}", self.base_url(), path);
        let mut request = self.http().post(&url).json(body);
        if let Some(key) = self.api_key() {
            request = request.header("x-api-key", key);
        }
        let resp = request
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        if !resp.status().is_success() {
            return Err(ClientError::Http(format!("HTTP {}", resp.status())));
        }
        Ok(byte_stream(resp))
    }

    /// GET an SSE endpoint and stream back frames.
    pub async fn get_stream(
        &self,
        path: &str,
    ) -> ClientResult<impl Stream<Item = Result<SseFrame, ClientError>> + Send + use<>> {
        let url = format!("{}{}", self.base_url(), path);
        let mut request = self.http().get(&url);
        if let Some(key) = self.api_key() {
            request = request.header("x-api-key", key);
        }
        let resp = request
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        if !resp.status().is_success() {
            return Err(ClientError::Http(format!("HTTP {}", resp.status())));
        }
        Ok(byte_stream(resp))
    }
}

fn byte_stream(
    resp: reqwest::Response,
) -> impl Stream<Item = Result<SseFrame, ClientError>> + Send {
    use eventsource_stream::Eventsource;
    use tokio_util::io::StreamReader;

    let bytes = resp.bytes_stream().map(|r| {
        r.map_err(|e| std::io::Error::other(e.to_string()))
    });
    let reader = StreamReader::new(bytes);
    tokio_util::io::ReaderStream::new(reader)
        .eventsource()
        .map(|event| match event {
            Ok(event) => {
                let data = serde_json::from_str(&event.data)
                    .unwrap_or_else(|_| serde_json::Value::String(event.data.clone()));
                Ok(SseFrame {
                    event: event.event,
                    data,
                })
            }
            Err(e) => Err(ClientError::Stream(e.to_string())),
        })
}
