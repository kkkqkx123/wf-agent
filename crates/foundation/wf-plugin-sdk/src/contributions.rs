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

/// Next function type for middleware chains.
pub type NextFn = Box<dyn FnOnce() -> BoxFuture<'static, PluginResult<()>> + Send>;

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
