use super::*;

pub struct ComponentLlmCodec {
    pub(crate) inner: Arc<ComponentPluginInner>,
    pub(crate) name: String,
}

impl ComponentLlmCodec {
    /// Structured codec round-trip over the component dispatch channel
    /// (sync wrapper over the async guest call; see `WasmLlmCodec`).
    fn roundtrip<T: serde::de::DeserializeOwned>(&self, op: &str, input: Value) -> PluginResult<T> {
        let input_json = super::super::shared::encode_call_input(&input, "llm codec input")?;
        let inner = self.inner.clone();
        let handler = format!("{}/{}", self.name, op);
        let output = match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| {
                handle.block_on(invoke_component_dispatch(
                    &inner,
                    "llm-codec",
                    &handler,
                    &input_json,
                ))
            })?,
            Err(_) => {
                return Err(PluginError::WasmError(
                    "llm codec requires a tokio runtime".to_string(),
                ));
            }
        };
        super::super::shared::decode_call_output(&output, "llm codec output")
    }
}

impl PluginLlmCodec for ComponentLlmCodec {
    fn build_request(&self, request: Value, profile: Value) -> PluginResult<CodecHttpRequest> {
        self.roundtrip(
            "build_request",
            serde_json::json!({"request": request, "profile": profile}),
        )
    }

    fn parse_response(&self, body: &str, request: Value) -> PluginResult<Value> {
        self.roundtrip(
            "parse_response",
            serde_json::json!({"body": body, "request": request}),
        )
    }

    fn parse_stream_chunk(&self, chunk: &str) -> PluginResult<Option<Value>> {
        self.roundtrip("parse_stream_chunk", serde_json::json!({"chunk": chunk}))
    }

    fn convert_tools(&self, tools: Value) -> PluginResult<Value> {
        self.roundtrip("convert_tools", serde_json::json!({"tools": tools}))
    }

    fn parse_tool_calls(&self, result: Value) -> PluginResult<Value> {
        self.roundtrip("parse_tool_calls", serde_json::json!({"result": result}))
    }

    fn build_count_tokens_request(
        &self,
        request: Value,
        profile: Value,
    ) -> PluginResult<Option<CodecHttpRequest>> {
        self.roundtrip(
            "build_count_tokens_request",
            serde_json::json!({"request": request, "profile": profile}),
        )
    }

    fn parse_count_tokens_response(&self, body: Value) -> PluginResult<u32> {
        self.roundtrip(
            "parse_count_tokens_response",
            serde_json::json!({"body": body}),
        )
    }
}

pub struct ComponentLlmAuth {
    pub(crate) inner: Arc<ComponentPluginInner>,
    pub(crate) name: String,
}

impl ComponentLlmAuth {
    /// Structured auth round-trip over the component dispatch channel
    /// (sync wrapper over the async guest call; see `WasmLlmCodec`).
    fn roundtrip<T: serde::de::DeserializeOwned>(&self, op: &str, input: Value) -> PluginResult<T> {
        let input_json = super::super::shared::encode_call_input(&input, "llm auth input")?;
        let inner = self.inner.clone();
        let handler = format!("{}/{}", self.name, op);
        let output = match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| {
                handle.block_on(invoke_component_dispatch(&inner, "llm-auth", &handler, &input_json))
            })?,
            Err(_) => {
                return Err(PluginError::WasmError(
                    "llm auth requires a tokio runtime".to_string(),
                ));
            }
        };
        super::super::shared::decode_call_output(&output, "llm auth output")
    }
}

impl PluginLlmAuth for ComponentLlmAuth {
    fn describe(&self) -> PluginResult<AuthDescriptor> {
        self.roundtrip("describe", serde_json::json!({}))
    }

    fn login_start(&self, config: Value) -> PluginResult<PluginLoginChallenge> {
        self.roundtrip("login_start", serde_json::json!({"config": config}))
    }

    fn login_poll(&self, session: Value) -> PluginResult<PluginLoginStep> {
        self.roundtrip("login_poll", serde_json::json!({"session": session}))
    }

    fn login_submit(&self, session: Value, input: String) -> PluginResult<PluginLoginStep> {
        self.roundtrip(
            "login_submit",
            serde_json::json!({"session": session, "input": input}),
        )
    }

    fn refresh(&self, credential: Value) -> PluginResult<Value> {
        self.roundtrip("refresh", serde_json::json!({"credential": credential}))
    }

    fn resolve(&self, credential: Value) -> PluginResult<Value> {
        self.roundtrip("resolve", serde_json::json!({"credential": credential}))
    }
}
