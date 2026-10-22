//! SSE streaming layer for endpoints the generator cannot express.
//!
//! The snapshot marks six operations as `text/event-stream`; progenitor
//! only models JSON, so they are removed from codegen (see `normalize`) and
//! streamed here with `reqwest` bytes + `eventsource-stream` parsing.

use futures::Stream;
use futures::StreamExt;

use crate::error::{from_status_body, ClientError, ClientResult};
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

/// Known streaming endpoints owned by this layer. The set mirrors the
/// `text/event-stream` operations in the committed snapshot.
pub mod endpoints {
    /// `POST /api/v1/workflows/{id}/execute/stream`
    pub fn workflow_execute(id: &str) -> String {
        format!("/api/v1/workflows/{id}/execute/stream")
    }

    /// `POST /api/v1/agent-loops/{id}/stream`
    pub fn agent_loop_stream(id: &str) -> String {
        format!("/api/v1/agent-loops/{id}/stream")
    }

    /// `POST /api/v1/llm/generate-stream`
    pub const LLM_GENERATE_STREAM: &str = "/api/v1/llm/generate-stream";

    /// `POST /api/v1/chat/stream`
    pub const CHAT_STREAM: &str = "/api/v1/chat/stream";

    /// `GET /api/v1/events/stream`
    pub const EVENTS_STREAM: &str = "/api/v1/events/stream";

    /// `GET /api/v1/executions/{id}/error-analysis/stream`
    pub fn error_chain_stream(id: &str) -> String {
        format!("/api/v1/executions/{id}/error-analysis/stream")
    }

    /// Session shortcut `POST /api/v1/agent-loops/cli/stream`.
    ///
    /// The server route is `/agent-loops/{id}/stream` and its handler
    /// ignores the path id, so `cli` reuses that slot as a session marker.
    pub fn agent_cli_stream() -> String {
        "/api/v1/agent-loops/cli/stream".to_string()
    }

    /// Session shortcut `POST /api/v1/agent-loops/cli/run`, same id-reuse
    /// note as [`agent_cli_stream`].
    pub fn agent_cli_run() -> String {
        "/api/v1/agent-loops/cli/run".to_string()
    }
}

impl WfClient {
    /// POST a JSON body and stream back SSE frames.
    pub async fn post_stream(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> ClientResult<impl Stream<Item = Result<SseFrame, ClientError>> + Send + use<>> {
        let url = format!("{}{}", self.base_url(), path);
        let resp = self
            .http()
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;
        stream_response(resp).await
    }

    /// GET an SSE endpoint and stream back frames.
    pub async fn get_stream(
        &self,
        path: &str,
    ) -> ClientResult<impl Stream<Item = Result<SseFrame, ClientError>> + Send + use<>> {
        let url = format!("{}{}", self.base_url(), path);
        let resp = self
            .http()
            .get(&url)
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;
        stream_response(resp).await
    }
}

/// Shared SSE status gate: the auth header already travels via the client
/// default headers, and failures preserve the envelope code and message.
async fn stream_response(
    resp: reqwest::Response,
) -> ClientResult<impl Stream<Item = Result<SseFrame, ClientError>> + Send> {
    let status = resp.status().as_u16();
    if status == 401 {
        return Err(ClientError::Unauthorized);
    }
    if !(200..300).contains(&status) {
        let body = resp.text().await.unwrap_or_default();
        let value: serde_json::Value =
            serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
        return Err(from_status_body(status, &value));
    }
    Ok(byte_stream(resp))
}

fn byte_stream(
    resp: reqwest::Response,
) -> impl Stream<Item = Result<SseFrame, ClientError>> + Send {
    use eventsource_stream::Eventsource;
    use tokio_util::io::StreamReader;

    let bytes = resp
        .bytes_stream()
        .map(|r| r.map_err(|e| std::io::Error::other(e.to_string())));
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

#[cfg(test)]
mod tests {
    use super::endpoints;
    use std::collections::BTreeSet;

    #[test]
    fn streaming_endpoints_match_snapshot_paths() {
        assert_eq!(
            endpoints::workflow_execute("w1"),
            "/api/v1/workflows/w1/execute/stream"
        );
        assert_eq!(
            endpoints::agent_loop_stream("a1"),
            "/api/v1/agent-loops/a1/stream"
        );
        assert_eq!(
            endpoints::error_chain_stream("e1"),
            "/api/v1/executions/e1/error-analysis/stream"
        );
        assert_eq!(
            endpoints::LLM_GENERATE_STREAM,
            "/api/v1/llm/generate-stream"
        );
        assert_eq!(endpoints::EVENTS_STREAM, "/api/v1/events/stream");
        assert_eq!(endpoints::CHAT_STREAM, "/api/v1/chat/stream");
    }

    #[test]
    fn cli_shortcuts_reuse_the_loop_id_slot() {
        assert_eq!(
            endpoints::agent_cli_stream(),
            "/api/v1/agent-loops/cli/stream"
        );
        assert_eq!(endpoints::agent_cli_run(), "/api/v1/agent-loops/cli/run");
    }

    /// The streaming endpoint table mirrors the snapshot `text/event-stream`
    /// set: any snapshot drift fails here instead of silently diverging.
    #[test]
    fn streaming_table_matches_snapshot_sse_set() {
        let text = std::fs::read_to_string("../wf-server/openapi.json")
            .expect("read wf-server openapi.json snapshot");
        let doc: serde_json::Value =
            serde_json::from_str(&text).expect("snapshot must be valid JSON");
        let mut snapshot: BTreeSet<String> = BTreeSet::new();
        if let Some(paths) = doc.get("paths").and_then(|p| p.as_object()) {
            for (path, item) in paths {
                let Some(obj) = item.as_object() else {
                    continue;
                };
                for op in obj.values() {
                    let Some(responses) = op.get("responses") else {
                        continue;
                    };
                    let Some(map) = responses.as_object() else {
                        continue;
                    };
                    for response in map.values() {
                        if response
                            .get("content")
                            .and_then(|c| c.as_object())
                            .is_some_and(|c| c.contains_key("text/event-stream"))
                        {
                            // Normalize templated ids so concrete endpoint
                            // strings compare against snapshot templates.
                            let normalized = path
                                .replace("{id}", "w1")
                                .replace("{workflow}", "w1")
                                .replace("{nodeId}", "n1")
                                .replace("{errorId}", "e1");
                            snapshot.insert(normalized);
                            break;
                        }
                    }
                }
            }
        }
        let mut table: BTreeSet<String> = BTreeSet::new();
        table.insert(endpoints::workflow_execute("w1"));
        table.insert(endpoints::agent_loop_stream("a1").replace("a1", "w1"));
        table.insert(endpoints::LLM_GENERATE_STREAM.to_string());
        table.insert(endpoints::CHAT_STREAM.to_string());
        table.insert(endpoints::EVENTS_STREAM.to_string());
        table.insert(endpoints::error_chain_stream("e1").replace("e1", "w1"));
        // Normalize templated ids in the snapshot the same way the table
        // does, so the two sets are directly comparable.
        let snapshot: BTreeSet<String> = snapshot
            .into_iter()
            .map(|path| {
                path.replace("{id}", "w1")
                    .replace("{workflow}", "w1")
                    .replace("{nodeId}", "n1")
                    .replace("{errorId}", "e1")
            })
            .collect();

        assert!(!snapshot.is_empty(), "snapshot SSE set must not be empty");
        assert_eq!(
            table, snapshot,
            "streaming endpoint table must exactly mirror the snapshot SSE set"
        );
    }
}
