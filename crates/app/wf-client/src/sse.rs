//! SSE streaming layer for endpoints the generator cannot express.
//!
//! The snapshot marks six operations as `text/event-stream`; progenitor
//! only models JSON, so they are removed from codegen (see `normalize`) and
//! streamed here with `reqwest` bytes + `eventsource-stream` parsing.

use futures::Stream;
use futures::StreamExt;

use wf_types::events::{BaseEvent, ExecutionStreamEvent};

use crate::error::{from_status_body, ClientError, ClientResult};
use crate::WfClient;

/// One parsed SSE frame.
#[derive(Debug, Clone, PartialEq)]
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

/// Execution-stream preface: the single-field view the workflow execute
/// stream emits first (`event: metadata`). It lives server-side; this shape
/// mirrors it so the client decodes without importing server code.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StreamMetadata {
    pub execution_id: String,
}

/// System-stream hello: the `type: connected` envelope the events stream
/// emits first. The inner execution id is optional (scoped streams fill it,
/// unscoped streams send null).
#[derive(Debug, Clone, PartialEq)]
pub struct SystemConnected {
    pub execution_id: Option<String>,
}

/// Decoded execution-stream frame. Unknown payloads fall back to `Raw`,
/// which retains the event line and original data for forward compatibility.
/// Decoding never fails as a stream error; only transport failures use
/// `ClientError`.
#[derive(Debug, Clone)]
pub enum ExecutionFrame {
    Metadata(StreamMetadata),
    Engine(BaseEvent),
    Event(ExecutionStreamEvent),
    Raw(SseFrame),
}

/// Decoded system-event frame. The `connected` envelope stands alone; every
/// other frame decodes as a bare bus event, with `Raw` preserving unknown
/// payloads.
#[derive(Debug, Clone)]
pub enum SystemFrame {
    Connected(SystemConnected),
    Base(BaseEvent),
    Raw(SseFrame),
}

impl ExecutionFrame {
    /// Typed protocol event for renderers. Preface and raw frames have no
    /// progress payload, so they map to `None` and callers skip them.
    pub fn into_stream_event(self) -> Option<ExecutionStreamEvent> {
        match self {
            Self::Metadata(_) | Self::Raw(_) => None,
            Self::Engine(base) => Some(ExecutionStreamEvent::Engine(base)),
            Self::Event(event) => Some(event),
        }
    }
}

impl SystemFrame {
    /// Bus event for consumers. Hello and raw frames carry no routing
    /// payload, so they map to `None` and callers skip them.
    pub fn into_base_event(self) -> Option<BaseEvent> {
        match self {
            Self::Base(base) => Some(base),
            Self::Connected(_) | Self::Raw(_) => None,
        }
    }
}

/// Decode one execution-stream frame by its `event:` line, falling back to
/// the raw frame when the payload does not match the expected shape.
pub fn decode_execution_frame(frame: &SseFrame) -> ExecutionFrame {
    match frame.event.as_str() {
        "metadata" => match serde_json::from_value::<StreamMetadata>(frame.data.clone()) {
            Ok(meta) => ExecutionFrame::Metadata(meta),
            Err(_) => ExecutionFrame::Raw(frame.clone()),
        },
        "engine" => match serde_json::from_value::<BaseEvent>(frame.data.clone()) {
            Ok(base) => ExecutionFrame::Engine(base),
            Err(_) => ExecutionFrame::Raw(frame.clone()),
        },
        "" => match serde_json::from_value::<ExecutionStreamEvent>(frame.data.clone()) {
            Ok(event) => ExecutionFrame::Event(event),
            Err(_) => ExecutionFrame::Raw(frame.clone()),
        },
        _ => ExecutionFrame::Raw(frame.clone()),
    }
}

/// Decode one system-event frame. The hello envelope is detected by its
/// `type: connected` marker (either the event line or the payload field,
/// since the server sends it as a bare data frame); the rest decode as bare
/// bus events with raw fallback.
pub fn decode_system_frame(frame: &SseFrame) -> SystemFrame {
    let payload_connected = frame
        .data
        .get("type")
        .and_then(|v| v.as_str())
        .is_some_and(|t| t == "connected");
    if frame.event == "connected" || payload_connected {
        return SystemFrame::Connected(SystemConnected {
            execution_id: connected_execution_id(&frame.data),
        });
    }
    match serde_json::from_value::<BaseEvent>(frame.data.clone()) {
        Ok(base) => SystemFrame::Base(base),
        Err(_) => SystemFrame::Raw(frame.clone()),
    }
}

fn connected_execution_id(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("data")
        .and_then(|inner| {
            inner
                .get("executionId")
                .or_else(|| inner.get("execution_id"))
        })
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            payload
                .get("executionId")
                .or_else(|| payload.get("execution_id"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
}

// Future payload families keep the same branch-then-fallback shape when a
// CLI consumer appears. The LLM family branches on its `event_type` tag
// (`MessageStreamEvent` in `llm-suite-types`); the error-chain family
// decodes its bare error-record view. Neither is implemented here because
// no CLI surface consumes them.

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

    /// `GET /api/v1/events/stream` scoped to one execution, with optional
    /// workflow scope. Query strings stay here so callers never rebuild the
    /// literal by hand.
    pub fn events_stream_for_execution(id: &str, workflow: Option<&str>) -> String {
        let mut path = format!("{EVENTS_STREAM}?execution_id={id}");
        if let Some(workflow) = workflow {
            path.push_str(&format!("&workflow_id={workflow}"));
        }
        path
    }

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

    #[test]
    fn execution_scoped_event_stream_builds_query() {
        assert_eq!(
            endpoints::events_stream_for_execution("e1", None),
            "/api/v1/events/stream?execution_id=e1"
        );
        assert_eq!(
            endpoints::events_stream_for_execution("e1", Some("w1")),
            "/api/v1/events/stream?execution_id=e1&workflow_id=w1"
        );
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

    use super::{decode_execution_frame, decode_system_frame, ExecutionFrame, SseFrame, SystemFrame};
    use wf_types::events::{BaseEvent, EventType, ExecutionStreamEvent};

    fn base_fixture(event_type: EventType) -> serde_json::Value {
        let base = BaseEvent {
            id: "evt-1".to_string(),
            r#type: event_type,
            timestamp: 1_700_000_000,
            event_name: None,
            workflow_id: Some("w1".to_string()),
            execution_id: Some("exec-1".to_string()),
            agent_loop_id: None,
            metadata: None,
        };
        serde_json::to_value(&base).expect("base fixture serializes")
    }

    #[test]
    fn execution_preface_decodes_to_metadata() {
        let frame = SseFrame {
            event: "metadata".to_string(),
            data: serde_json::json!({"execution_id": "exec-1"}),
        };
        match decode_execution_frame(&frame) {
            ExecutionFrame::Metadata(meta) => assert_eq!(meta.execution_id, "exec-1"),
            other => panic!("expected metadata, got {other:?}"),
        }
    }

    #[test]
    fn execution_engine_line_decodes_to_base_event() {
        let frame = SseFrame {
            event: "engine".to_string(),
            data: base_fixture(EventType::NodeStarted),
        };
        match decode_execution_frame(&frame) {
            ExecutionFrame::Engine(base) => {
                assert_eq!(base.r#type, EventType::NodeStarted);
                assert_eq!(base.execution_id.as_deref(), Some("exec-1"));
            }
            other => panic!("expected engine, got {other:?}"),
        }
        match decode_execution_frame(&frame).into_stream_event() {
            Some(ExecutionStreamEvent::Engine(base)) => {
                assert_eq!(base.r#type, EventType::NodeStarted)
            }
            other => panic!("expected engine stream event, got {other:?}"),
        }
    }

    #[test]
    fn execution_protocol_event_decodes_to_typed_event() {
        let frame = SseFrame {
            event: String::new(),
            data: serde_json::json!({"type": "llm_delta", "content": "hi"}),
        };
        match decode_execution_frame(&frame) {
            ExecutionFrame::Event(ExecutionStreamEvent::LlmDelta { content }) => {
                assert_eq!(content, "hi")
            }
            other => panic!("expected protocol event, got {other:?}"),
        }
    }

    #[test]
    fn execution_unknown_tag_falls_back_to_raw_without_loss() {
        let data = serde_json::json!({"type": "future_unknown_kind", "content": "hi"});
        let frame = SseFrame {
            event: String::new(),
            data: data.clone(),
        };
        match decode_execution_frame(&frame) {
            ExecutionFrame::Raw(raw) => {
                assert_eq!(raw.event, "");
                assert_eq!(raw.data, data);
            }
            other => panic!("expected raw fallback, got {other:?}"),
        }
        assert!(decode_execution_frame(&frame).into_stream_event().is_none());
    }

    #[test]
    fn execution_malformed_engine_falls_back_to_raw() {
        let data = serde_json::json!({"not": "an event"});
        let frame = SseFrame {
            event: "engine".to_string(),
            data: data.clone(),
        };
        match decode_execution_frame(&frame) {
            ExecutionFrame::Raw(raw) => assert_eq!(raw.data, data),
            other => panic!("expected raw fallback, got {other:?}"),
        }
    }

    #[test]
    fn system_connected_envelope_decodes_alone() {
        let frame = SseFrame {
            event: String::new(),
            data: serde_json::json!({"type": "connected", "data": {"executionId": "exec-1"}}),
        };
        match decode_system_frame(&frame) {
            SystemFrame::Connected(hello) => {
                assert_eq!(hello.execution_id.as_deref(), Some("exec-1"))
            }
            other => panic!("expected connected, got {other:?}"),
        }
    }

    #[test]
    fn system_bus_event_decodes_to_base() {
        let frame = SseFrame {
            event: String::new(),
            data: base_fixture(EventType::CheckpointCreated),
        };
        match decode_system_frame(&frame) {
            SystemFrame::Base(base) => assert_eq!(base.r#type, EventType::CheckpointCreated),
            other => panic!("expected base, got {other:?}"),
        }
    }

    #[test]
    fn system_unknown_payload_falls_back_to_raw() {
        let data = serde_json::json!({"type": "future_unknown", "x": 1});
        let frame = SseFrame {
            event: String::new(),
            data: data.clone(),
        };
        match decode_system_frame(&frame) {
            SystemFrame::Raw(raw) => {
                assert_eq!(raw.event, "");
                assert_eq!(raw.data, data);
            }
            other => panic!("expected raw fallback, got {other:?}"),
        }
        assert!(decode_system_frame(&frame).into_base_event().is_none());
    }
}
