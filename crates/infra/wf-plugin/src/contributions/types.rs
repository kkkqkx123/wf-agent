use async_trait::async_trait;
use futures::future::BoxFuture;
use serde_json::Value;

use crate::error::PluginResult;

use serde::{Deserialize, Serialize};

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
    Middleware,
    // Declarative resource contributions (payloads from wf-types, no new dependencies)
    Workflow,
    Prompt,
    Fragment,
    AgentTemplate,
    NodeTemplate,
    Trigger,
    ToolDescription,
    Tool,
}

// ============================================================
// Context Types
// ============================================================

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
pub struct PluginLlmRequest {
    pub messages: Vec<PluginMessage>,
    pub config: Option<PluginLlmConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLlmConfig {
    pub model: String,
    pub provider: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLlmResponse {
    pub content: String,
    pub usage: Option<PluginLlmUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLlmUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEventData {
    pub event_type: String,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub struct PluginMiddlewareDef {
    pub phase: String,
    pub priority: i32,
}

/// Next function type for middleware chain
pub type NextFn = Box<dyn FnOnce() -> BoxFuture<'static, PluginResult<()>> + Send>;

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
pub trait PluginLlmFormatter: Send + Sync {
    async fn format(&self, request: PluginLlmRequest) -> PluginResult<PluginLlmResponse>;
}

#[async_trait]
pub trait PluginEventHandler: Send + Sync {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()>;
}

#[async_trait]
pub trait PluginMiddlewareHandler: Send + Sync {
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<()>;
}
