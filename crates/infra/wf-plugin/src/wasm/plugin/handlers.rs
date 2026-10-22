use super::*;
use async_trait::async_trait;

pub struct WasmNodeHandler {
    pub inner: Arc<WasmPluginInner>,
    pub type_name: String,
}

pub struct WasmToolExecutor {
    pub inner: Arc<WasmPluginInner>,
    pub type_name: String,
}

pub struct WasmEventHandler {
    pub inner: Arc<WasmPluginInner>,
    pub event_type: String,
}

pub struct WasmMiddlewareHandler {
    pub inner: Arc<WasmPluginInner>,
    pub phase: String,
}

#[async_trait]
impl PluginNodeHandler for WasmNodeHandler {
    async fn execute(&self, ctx: PluginExecutionContext) -> PluginResult<PluginNodeResult> {
        let input = super::super::shared::encode_call_input(&ctx, "node ctx")?;
        let output = invoke_dispatch(&self.inner, "node", &self.type_name, &input).await?;
        super::super::shared::decode_call_output(&output, "node result")
    }
}

#[async_trait]
impl PluginToolExecutor for WasmToolExecutor {
    async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
        let input = super::super::shared::encode_call_input(&ctx, "tool ctx")?;
        let output = invoke_dispatch(&self.inner, "tool", &self.type_name, &input).await?;
        super::super::shared::decode_call_output(&output, "tool result")
    }
}

#[async_trait]
impl PluginEventHandler for WasmEventHandler {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()> {
        let input = super::super::shared::encode_call_input(&event, "event")?;
        invoke_dispatch(&self.inner, "event", &self.event_type, &input).await?;
        Ok(())
    }
}

#[async_trait]
impl PluginMiddlewareHandler for WasmMiddlewareHandler {
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value> {
        let input = super::super::shared::encode_call_input(&context, "middleware ctx")?;
        let output = invoke_dispatch(&self.inner, "mw", &self.phase, &input).await?;
        super::super::shared::resolve_middleware_output(&output, &context, next).await
    }
}