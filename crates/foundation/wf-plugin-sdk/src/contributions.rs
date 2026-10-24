//! Plugin-author contribution contracts: payload types and handler traits.
//!
//! These traits are what a plugin implements to contribute behavior
//! (nodes/tools/LLM/middleware) to the host. Registration-side machinery
//! (`ContributionManager`, `ContributionRegistrar`) stays in the host crate
//! `wf-plugin`.

use async_trait::async_trait;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::PluginResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginExecutionContext {
    pub node_id: String,
    pub inputs: Value,
    pub config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginNodeResult {
    pub outputs: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginToolContext {
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginToolResult {
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEventData {
    pub event_type: String,
    pub data: Value,
}

/// Next function type for middleware chains.
///
/// The caller passes the (possibly rewritten) context downstream; the
/// returned future resolves to the final context after the rest of the
/// chain (and any response-side rewrites on the way back up).
pub type NextFn = Box<dyn FnOnce(Value) -> BoxFuture<'static, PluginResult<Value>> + Send>;

/// Outcome of one middleware guest call: whether the chain continues and
/// which context travels onward.
#[derive(Debug, Clone, PartialEq)]
pub struct MiddlewareOutcome {
    pub proceed: bool,
    pub context: Value,
}

/// Parse a middleware guest response against the incoming context.
///
/// - JSON `true`/`false` selects proceed/skip with the context unchanged
///   (legacy boolean protocol).
/// - An object carrying `proceed` (default `true`) and/or `context`
///   (default: the incoming value) rewrites the request.
/// - Anything else keeps the legacy behavior: proceed with the incoming
///   context, so old guests are unaffected. Replacement is whole-value;
///   no merge-patch semantics are applied.
pub fn parse_middleware_outcome(response: &Value, incoming: &Value) -> MiddlewareOutcome {
    if let Some(proceed) = response.as_bool() {
        return MiddlewareOutcome {
            proceed,
            context: incoming.clone(),
        };
    }
    if let Some(object) = response.as_object() {
        if object.contains_key("proceed") || object.contains_key("context") {
            let proceed = object
                .get("proceed")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let context = object
                .get("context")
                .cloned()
                .unwrap_or_else(|| incoming.clone());
            return MiddlewareOutcome { proceed, context };
        }
    }
    MiddlewareOutcome {
        proceed: true,
        context: incoming.clone(),
    }
}

#[async_trait]
pub trait PluginNodeHandler: Send + Sync {
    async fn execute(&self, ctx: PluginExecutionContext) -> PluginResult<PluginNodeResult>;
}

#[async_trait]
pub trait PluginToolExecutor: Send + Sync {
    async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult>;
}

// ── low-level LLM codec ─────────────────────────────────────────────
// A codec contributes a full wire protocol format: request building,
// response and stream-chunk parsing, tool conversion and token counting.
// The contract is synchronous and JSON-based so Lua tables and native
// dispatch buffers share one shape. The host builds the actual HTTP
// request from the returned description and translates the returned
// values into its own LLM types; `parse_response` / `parse_stream_chunk`
// results must therefore follow the host `LlmResult` / stream-event JSON
// shapes documented by the runtime adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodecHttpRequest {
    /// HTTP method, e.g. `POST`.
    pub method: String,
    /// Fully joined URL, query parameters included.
    pub url: String,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    /// JSON body; `Value::Null` sends no body.
    #[serde(default)]
    pub body: Value,
}

pub trait PluginLlmCodec: Send + Sync {
    /// Describe the HTTP request for a structured LLM request and profile
    /// (both passed as JSON). The host constructs the request from it.
    fn build_request(&self, request: Value, profile: Value) -> PluginResult<CodecHttpRequest>;
    /// Parse a complete response body into the host `LlmResult` JSON shape.
    fn parse_response(&self, body: &str, request: Value) -> PluginResult<Value>;
    /// Parse one SSE data payload into the host stream-event JSON shape,
    /// or `None` for payloads carrying no event.
    fn parse_stream_chunk(&self, chunk: &str) -> PluginResult<Option<Value>> {
        let _ = chunk;
        Ok(None)
    }
    /// Convert tool definitions (JSON array) into the format's tool schemas.
    fn convert_tools(&self, tools: Value) -> PluginResult<Value> {
        Ok(tools)
    }
    /// Extract tool calls from a parsed result (host `LlmResult` JSON shape).
    fn parse_tool_calls(&self, result: Value) -> PluginResult<Value> {
        let _ = result;
        Ok(Value::Array(Vec::new()))
    }
    /// Describe a token-counting request, or `None` when the format exposes
    /// no counting endpoint (the host falls back to local estimation).
    fn build_count_tokens_request(
        &self,
        request: Value,
        profile: Value,
    ) -> PluginResult<Option<CodecHttpRequest>> {
        let _ = (request, profile);
        Ok(None)
    }
    /// Parse a counting response body into input tokens. No default: a codec
    /// that describes a counting request must implement this explicitly so a
    /// missing implementation fails loudly instead of reporting `0` tokens.
    fn parse_count_tokens_response(&self, body: Value) -> PluginResult<u32> {
        let _ = body;
        Err(crate::error::PluginError::Internal(
            "codec does not implement parse_count_tokens_response".to_string(),
        ))
    }
}

#[async_trait]
pub trait PluginEventHandler: Send + Sync {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()>;
}

// ── provider authentication ───────────────────────────────────────────
// A plugin contributes the login / refresh / resolve contract for one
// provider. The contract is synchronous and JSON-based so Lua tables,
// native dispatch buffers and wasm guest calls share one shape. The host
// owns the login session, the credential store and the request pipeline:
// it calls the plugin on its own schedule and persists the produced
// credential. Plugins stay stateless between calls; all flow state
// travels through the opaque `session` value.
/// Static metadata about a contributed auth implementation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthDescriptor {
    /// Display name, e.g. "Claude (Pro/Max Subscription)".
    pub name: String,
    /// True when access is subscription-backed (non-metered).
    #[serde(default)]
    pub is_subscription: bool,
    /// Selector label for the login option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_label: Option<String>,
}

/// One event rendered to the user during login. Mirrors the host's
/// auth-event vocabulary; the host maps it onto its own interaction type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginAuthEvent {
    /// Informational line.
    Info { message: String },
    /// Authorization URL to open, with optional instructions.
    AuthUrl {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<String>,
    },
    /// Device-code challenge (RFC 8628).
    DeviceCode {
        user_code: String,
        verification_uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Progress line.
    Progress { message: String },
}

/// One option of a select prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginAuthOption {
    pub label: String,
    pub value: String,
}

/// One input prompt the host must answer before the flow continues.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginAuthPrompt {
    Text {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    Secret {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    Select {
        message: String,
        options: Vec<PluginAuthOption>,
    },
    ManualCode {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
}

/// What a login step expects the host to do next.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "next", rename_all = "snake_case")]
pub enum PluginAuthNext {
    /// Waiting on an external event; the host polls.
    Pending,
    /// The host must collect one input, then call submit.
    AwaitingInput { prompt: PluginAuthPrompt },
    /// The flow produced a credential; the host persists it.
    Completed { credential: Value },
    /// The flow failed.
    Failed { error: String },
}

/// The first step of a login flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginLoginChallenge {
    /// Events to render immediately.
    #[serde(default)]
    pub events: Vec<PluginAuthEvent>,
    /// Opaque session state the host passes back on poll / submit.
    pub session: Value,
    /// What the host should do next.
    pub next: PluginAuthNext,
}

/// One transition after a poll or submit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginLoginStep {
    /// Events produced by this step.
    #[serde(default)]
    pub events: Vec<PluginAuthEvent>,
    /// Opaque session state for the following step.
    pub session: Value,
    /// What the host should do next.
    pub next: PluginAuthNext,
}

/// Plugin-contributed provider authentication.
pub trait PluginLlmAuth: Send + Sync {
    /// Static metadata for status UIs and login pickers.
    fn describe(&self) -> PluginResult<AuthDescriptor>;

    /// Begin the login flow: render the challenge and report what the host
    /// should do next. `config` carries the provider's declared auth
    /// parameters (endpoints, client id, scopes, flow) verbatim.
    /// Implementations must not block on the user.
    fn login_start(&self, config: Value) -> PluginResult<PluginLoginChallenge>;

    /// Advance a polling step; the host calls this on an interval while
    /// the flow is pending.
    fn login_poll(&self, session: Value) -> PluginResult<PluginLoginStep>;

    /// Accept user input for a step awaiting it.
    fn login_submit(&self, session: Value, input: String) -> PluginResult<PluginLoginStep>;

    /// Exchange a refresh token; returns the new credential JSON
    /// (`{access, refresh, expires, is_subscription, extra?}`).
    fn refresh(&self, credential: Value) -> PluginResult<Value>;

    /// Derive request auth from a valid credential JSON. The result
    /// carries `api_key` / `headers` / `base_url` / `is_subscription` /
    /// `source`; a null result means not configured.
    fn resolve(&self, credential: Value) -> PluginResult<Value>;
}

#[async_trait]
pub trait PluginMiddlewareHandler: Send + Sync {
    /// Handle one middleware phase. Implementations may rewrite `context`
    /// before passing it to `next`; the returned value is the final context
    /// after the rest of the chain. Returning without calling `next`
    /// short-circuits the chain with the returned context.
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn outcome_bool_is_legacy_proceed_flag() {
        let incoming = json!({"a": 1});
        let out = parse_middleware_outcome(&json!(true), &incoming);
        assert_eq!(
            out,
            MiddlewareOutcome {
                proceed: true,
                context: incoming.clone(),
            }
        );
        let out = parse_middleware_outcome(&json!(false), &incoming);
        assert!(!out.proceed);
        assert_eq!(out.context, incoming);
    }

    #[test]
    fn outcome_envelope_rewrites_context() {
        let incoming = json!({"a": 1});
        let response = json!({"proceed": true, "context": {"a": 2}});
        let out = parse_middleware_outcome(&response, &incoming);
        assert!(out.proceed);
        assert_eq!(out.context, json!({"a": 2}));

        let response = json!({"proceed": false, "context": {"stop": true}});
        let out = parse_middleware_outcome(&response, &incoming);
        assert!(!out.proceed);
        assert_eq!(out.context, json!({"stop": true}));
    }

    #[test]
    fn outcome_envelope_fields_default() {
        let incoming = json!({"a": 1});
        let out = parse_middleware_outcome(&json!({"context": {"b": 2}}), &incoming);
        assert!(out.proceed);
        assert_eq!(out.context, json!({"b": 2}));

        let out = parse_middleware_outcome(&json!({"proceed": false}), &incoming);
        assert!(!out.proceed);
        assert_eq!(out.context, incoming);
    }

    #[test]
    fn outcome_unknown_shapes_fall_back_to_legacy() {
        let incoming = json!({"a": 1});
        for response in [
            json!(null),
            json!(42),
            json!("x"),
            json!([1]),
            json!({"other": 1}),
        ] {
            let out = parse_middleware_outcome(&response, &incoming);
            assert!(out.proceed, "response: {response}");
            assert_eq!(out.context, incoming, "response: {response}");
        }
    }
}
