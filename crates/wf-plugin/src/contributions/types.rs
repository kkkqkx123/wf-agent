use async_trait::async_trait;
use serde_json::Value;

use crate::error::PluginResult;

// ============================================================
// Contribution Type
// ============================================================

#[derive(Debug, Clone)]
pub enum ContributionType {
    NodeType,
    ToolType,
    LlmProvider,
    Formatter,
    EventHandler,
    HookHandler,
    Middleware,
}

// ============================================================
// Context Types
// ============================================================

#[derive(Debug, Clone)]
pub struct PluginExecutionContext {
    pub node_id: String,
    pub inputs: Value,
    pub config: Value,
}

#[derive(Debug, Clone)]
pub struct PluginNodeResult {
    pub outputs: Value,
}

#[derive(Debug, Clone)]
pub struct PluginToolContext {
    pub args: Value,
}

#[derive(Debug, Clone)]
pub struct PluginToolResult {
    pub result: Value,
}

#[derive(Debug, Clone)]
pub struct PluginLLMRequest {
    pub messages: Vec<PluginMessage>,
    pub config: Option<PluginLLMConfig>,
}

#[derive(Debug, Clone)]
pub struct PluginMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct PluginLLMConfig {
    pub model: String,
    pub provider: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct PluginLLMResponse {
    pub content: String,
    pub usage: Option<PluginLLMUsage>,
}

#[derive(Debug, Clone)]
pub struct PluginLLMUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct PluginEventData {
    pub event_type: String,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub struct PluginMiddlewareDef {
    pub phase: String,
    pub priority: i32,
}

// ============================================================
// Plugin-Agnostic Handler Traits
// ============================================================

#[async_trait]
pub trait PluginNodeHandler: Send + Sync {
    async fn execute(&self, ctx: PluginExecutionContext) -> PluginResult<PluginNodeResult>;
}

#[async_trait]
pub trait PluginToolExecutor: Send + Sync {
    async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult>;
}

#[async_trait]
pub trait PluginLLMFormatter: Send + Sync {
    async fn format(&self, request: PluginLLMRequest) -> PluginResult<PluginLLMResponse>;
}

#[async_trait]
pub trait PluginEventHandler: Send + Sync {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()>;
}

#[async_trait]
pub trait PluginHookHandler: Send + Sync {
    async fn handle(&self, context: Value) -> PluginResult<()>;
}

#[async_trait]
pub trait PluginMiddlewareHandler: Send + Sync {
    async fn handle(&self, context: Value, next: Box<dyn FnOnce() -> PluginResult<()> + Send>) -> PluginResult<()>;
}
