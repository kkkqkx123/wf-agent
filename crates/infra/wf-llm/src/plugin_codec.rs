use std::sync::Arc;

use wf_plugin_sdk::{CodecHttpRequest, PluginLlmCodec};
use wf_types::llm::{LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent};
use wf_types::tool::Tool;

use crate::error::{LlmError, LlmResult};
use crate::codecs::LlmCodec;

/// Adapt a plugin-provided [`PluginLlmCodec`] to the host [`LlmCodec`].
///
/// Structured values cross the boundary as JSON: the request and profile are
/// serialized before entering the codec, and the codec answers are
/// deserialized into the host LLM types. `build_request` answers are request
/// descriptions; the HTTP request itself is always constructed here, so
/// plugins never touch the HTTP client.
pub struct PluginCodecAdapter {
    codec: Arc<dyn PluginLlmCodec>,
}

impl PluginCodecAdapter {
    pub fn new(codec: Arc<dyn PluginLlmCodec>) -> Self {
        Self { codec }
    }

    pub fn codec(&self) -> &Arc<dyn PluginLlmCodec> {
        &self.codec
    }
}

fn plugin_error(error: wf_plugin_sdk::PluginError) -> LlmError {
    LlmError::ConfigError(error.to_string())
}

fn http_request_from_description(described: &CodecHttpRequest) -> LlmResult<reqwest::Request> {
    let method: reqwest::Method = described.method.parse().map_err(|_| {
        LlmError::ConfigError(format!(
            "Invalid codec request method '{}'",
            described.method
        ))
    })?;
    let mut builder = reqwest::Client::new().request(method, &described.url);
    if !described.body.is_null() {
        builder = builder.json(&described.body);
    }
    for (name, value) in &described.headers {
        builder = builder.header(name, value.as_str());
    }
    builder.build().map_err(LlmError::HttpError)
}

impl LlmCodec for PluginCodecAdapter {
    fn build_request(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<reqwest::Request> {
        let request = serde_json::to_value(request)?;
        let profile = serde_json::to_value(profile)?;
        let described = self
            .codec
            .build_request(request, profile)
            .map_err(plugin_error)?;
        http_request_from_description(&described)
    }

    fn parse_response(&self, body: &str, request: &LlmRequest) -> LlmResult<LlmResponseType> {
        let request = serde_json::to_value(request)?;
        let value = self
            .codec
            .parse_response(body, request)
            .map_err(plugin_error)?;
        serde_json::from_value(value).map_err(LlmError::from)
    }

    fn parse_stream_chunk(&self, data: &str) -> LlmResult<Option<MessageStreamEvent>> {
        let value = self.codec.parse_stream_chunk(data).map_err(plugin_error)?;
        value
            .map(|event| serde_json::from_value(event).map_err(LlmError::from))
            .transpose()
    }

    fn convert_tools(&self, tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>> {
        let tools = serde_json::to_value(tools)?;
        let value = self.codec.convert_tools(tools).map_err(plugin_error)?;
        value.as_array().cloned().ok_or_else(|| {
            LlmError::InvalidResponse("Codec convert_tools must return a JSON array".to_string())
        })
    }

    fn parse_tool_calls(&self, result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall> {
        let request = match serde_json::to_value(result) {
            Ok(request) => request,
            Err(e) => {
                tracing::warn!("plugin codec parse_tool_calls: result serialization failed: {e}");
                return Vec::new();
            }
        };
        let value = match self.codec.parse_tool_calls(request) {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!("plugin codec parse_tool_calls failed: {e}");
                return Vec::new();
            }
        };
        serde_json::from_value(value).unwrap_or_default()
    }

    fn build_count_tokens_request(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
    ) -> LlmResult<Option<reqwest::Request>> {
        let request = serde_json::to_value(request)?;
        let profile = serde_json::to_value(profile)?;
        self.codec
            .build_count_tokens_request(request, profile)
            .map_err(plugin_error)?
            .map(|described| http_request_from_description(&described))
            .transpose()
    }

    fn parse_count_tokens_response(&self, body: &serde_json::Value) -> LlmResult<u32> {
        self.codec
            .parse_count_tokens_response(body.clone())
            .map_err(plugin_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_plugin_sdk::PluginResult;

    struct EchoCodec;

    impl PluginLlmCodec for EchoCodec {
        fn build_request(
            &self,
            _request: serde_json::Value,
            _profile: serde_json::Value,
        ) -> PluginResult<CodecHttpRequest> {
            Ok(CodecHttpRequest {
                method: "POST".to_string(),
                url: "https://api.test.test/v1/chat".to_string(),
                headers: [("Content-Type".to_string(), "application/json".to_string())]
                    .into_iter()
                    .collect(),
                body: serde_json::json!({"model": "m"}),
            })
        }

        fn parse_response(
            &self,
            _body: &str,
            _request: serde_json::Value,
        ) -> PluginResult<serde_json::Value> {
            Err(wf_plugin_sdk::PluginError::Internal(
                "no response".to_string(),
            ))
        }
    }

    #[test]
    fn adapter_builds_http_request_from_description() {
        let adapter = PluginCodecAdapter::new(Arc::new(EchoCodec));
        let request = LlmRequest {
            profile_id: "p1".to_string(),
            messages: Vec::new(),
            parameters: None,
            generation: None,
            tools: None,
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        };
        let profile = LlmProfile {
            id: "p1".to_string(),
            name: "p1".to_string(),
            format: wf_types::llm::LlmFormat::OpenaiChat,
            provider_id: None,
            model: "m".to_string(),
            api_key: None,
            base_url: None,
            parameters: None,
            generation: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_protocol: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
        };
        let http = adapter.build_request(&request, &profile).unwrap();
        assert_eq!(http.method(), reqwest::Method::POST);
        assert_eq!(http.url().as_str(), "https://api.test.test/v1/chat");
        assert!(adapter.parse_response("{}", &request).is_err());
        assert!(adapter.parse_stream_chunk("{}").unwrap().is_none());
    }
}
