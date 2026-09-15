//! Helpers shared by the core-module (`plugin.rs`) and component-model
//! (`component.rs`) guest hosts.
//!
//! Both paths move the same JSON documents across the host/guest boundary
//! and apply the same middleware envelope, differing only in their
//! transport calls (`invoke_dispatch` vs `invoke_component_dispatch`).
//! These stateless helpers keep that shared logic (and its error wording)
//! in one place; session handling stays with each path because pooled
//! core-module stores and per-call component stores have fundamentally
//! different lifetimes.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::contributions::NextFn;
use crate::error::{PluginError, PluginResult};

/// Serialize one guest-call input document to JSON.
pub fn encode_call_input<T: Serialize>(value: &T, what: &str) -> PluginResult<String> {
    serde_json::to_string(value)
        .map_err(|e| PluginError::WasmError(format!("serialize {what} failed: {e}")))
}

/// Deserialize one guest-call output document from JSON.
pub fn decode_call_output<T: DeserializeOwned>(bytes: &[u8], what: &str) -> PluginResult<T> {
    serde_json::from_slice(bytes)
        .map_err(|e| PluginError::WasmError(format!("deserialize {what} failed: {e}")))
}

/// Resolve a middleware guest answer into the chain decision.
///
/// Middleware answers with a boolean (legacy) or a `{proceed, context}`
/// envelope; the rewritten context threads downstream. Returns the final
/// context: either the short-circuit value or whatever the rest of the
/// chain produces.
pub async fn resolve_middleware_output(
    output: &[u8],
    incoming: &Value,
    next: NextFn,
) -> PluginResult<Value> {
    let response: Value = serde_json::from_slice(output).unwrap_or(Value::Null);
    let outcome = wf_plugin_sdk::contributions::parse_middleware_outcome(&response, incoming);
    if outcome.proceed {
        next(outcome.context).await
    } else {
        Ok(outcome.context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn call_input_output_roundtrip() {
        let input = json!({"q": 1});
        let encoded = encode_call_input(&input, "probe").expect("encodes");
        let decoded: Value = decode_call_output(encoded.as_bytes(), "probe").expect("decodes");
        assert_eq!(decoded, input);
    }

    #[test]
    fn call_codec_failures_carry_what() {
        // JSON objects require string keys, so a tuple-keyed map cannot
        // serialize.
        let bad = std::collections::HashMap::from([((1u32, 2u32), 3u32)]);
        let err = encode_call_input(&bad, "probe").expect_err("tuple keys");
        assert!(err.to_string().contains("serialize probe failed"));

        let err = decode_call_output::<Value>(b"not json", "probe").expect_err("bad json");
        assert!(err.to_string().contains("deserialize probe failed"));
    }

    #[tokio::test]
    async fn middleware_legacy_bool_threads_through_next() {
        let next: NextFn = Box::new(|ctx| Box::pin(async move { Ok(ctx) }));
        let out = resolve_middleware_output(b"true", &json!({"a": 1}), next)
            .await
            .expect("runs");
        assert_eq!(out, json!({"a": 1}));

        let next: NextFn = Box::new(|_| {
            Box::pin(async move { panic!("short-circuited chain must not call next") })
        });
        let out = resolve_middleware_output(b"false", &json!({"a": 1}), next)
            .await
            .expect("runs");
        assert_eq!(out, json!({"a": 1}));
    }

    #[tokio::test]
    async fn middleware_envelope_rewrites_and_short_circuits() {
        let next: NextFn = Box::new(|ctx| Box::pin(async move { Ok(ctx) }));
        let out = resolve_middleware_output(
            br#"{"proceed":true,"context":{"patched":1}}"#,
            &json!({"a": 1}),
            next,
        )
        .await
        .expect("runs");
        assert_eq!(out, json!({"patched": 1}));

        let next: NextFn = Box::new(|_| {
            Box::pin(async move { panic!("short-circuited chain must not call next") })
        });
        let out = resolve_middleware_output(
            br#"{"proceed":false,"context":{"stopped":true}}"#,
            &json!({"a": 1}),
            next,
        )
        .await
        .expect("runs");
        assert_eq!(out, json!({"stopped": true}));
    }

    #[tokio::test]
    async fn middleware_malformed_output_falls_back_to_legacy_proceed() {
        let next: NextFn = Box::new(|ctx| Box::pin(async move { Ok(ctx) }));
        let incoming = json!({"a": 1});
        let out = resolve_middleware_output(b"[1,2,3]", &incoming, next)
            .await
            .expect("runs");
        assert_eq!(out, incoming);
    }
}
