use std::collections::HashMap;
use std::sync::Arc;

use wf_core::EventBus;

use crate::entity::AgentLoopEntity;

use super::runner::run_tool;
use super::types::ToolRunCtx;

/// Per-run context backing the `general` tool.
///
/// Holds no reference to the coordinator (no Arc cycle): it snapshots the
/// immutable execution context and the run entity, then routes every inner
/// invocation through the shared [`run_tool`] pipeline so all controls
/// (visibility, approval, checkpoint, failure protection, timeout) apply
/// exactly as to direct calls.
pub struct GeneralToolContext {
    ctx: ToolRunCtx,
    entity: Arc<AgentLoopEntity>,
    event_bus: Option<Arc<EventBus>>,
}

impl GeneralToolContext {
    pub(crate) fn new(
        ctx: ToolRunCtx,
        entity: Arc<AgentLoopEntity>,
        event_bus: Option<Arc<EventBus>>,
    ) -> Self {
        Self {
            ctx,
            entity,
            event_bus,
        }
    }

    /// Validate the inner tool against the run's exposure state using the
    /// same resolution the schema assembly consumes (single decision
    /// source): hidden tools are always rejected, gated (not yet activated)
    /// tools are rejected, and anything outside the resolved visible or
    /// discoverable buckets is rejected. The `general` tool itself cannot be
    /// invoked through `general` (recursion guard). Runtime blocks are
    /// enforced by the shared pipeline.
    async fn check_inner_tool_allowed(&self, tool_name: &str) -> Result<(), String> {
        if tool_name == wf_tools::general::GENERAL_TOOL_NAME {
            return Err(format!(
                "Tool '{}' cannot be invoked through the general tool; call the target tool directly",
                tool_name
            ));
        }
        // The hidden blocklist is the strongest exposure layer (mirrors
        // `effective_exposure`): a hidden tool is rejected even when it is
        // outside the available pool, which the pooled resolution below
        // filters out entirely.
        if self
            .entity
            .hidden_tool_names()
            .contains(&tool_name.to_string())
        {
            return Err(format!(
                "Tool '{}' is not callable in this execution",
                tool_name
            ));
        }
        let activated_tools = {
            let state = self.entity.state.read().await;
            state.tool_discovery().activated_tools.clone()
        };
        let resolution = wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
            registry: self.ctx.registry.as_ref(),
            available_names: self.entity.available_tool_names(),
            initial_names: self.entity.initial_tool_names(),
            discoverable_names: self.entity.discoverable_tool_names(),
            hidden_names: self.entity.hidden_tool_names(),
            enable_general_tool: self.entity.enable_general_tool(),
            activated_tools: &activated_tools,
            exposure_overrides: &self.entity.exposure_overrides().iter().cloned().collect(),
        });
        if resolution.hidden.iter().any(|t| t.name == tool_name) {
            return Err(format!(
                "Tool '{}' is not callable in this execution",
                tool_name
            ));
        }
        if resolution.gated.iter().any(|t| t.name == tool_name) {
            return Err(format!(
                "Tool '{}' is not activated yet; wait until it is explicitly enabled",
                tool_name
            ));
        }
        if !wf_tools::is_tool_callable(&resolution, tool_name) {
            return Err(format!(
                "Tool '{}' is not in the available tool set",
                tool_name
            ));
        }
        Ok(())
    }

    /// Execute one inner invocation through the shared pipeline.
    async fn invoke_inner(
        &self,
        call: &wf_types::message::LlmToolCall,
    ) -> wf_tools::ToolResult<serde_json::Value> {
        use wf_tools::error::ToolError;

        let tool_name = call.function.name.clone();
        self.check_inner_tool_allowed(&tool_name)
            .await
            .map_err(ToolError::ValidationFailed)?;

        let started = wf_common::now();
        let is_first_discovery = {
            let mut state = self.entity.state.write().await;
            state
                .tool_discovery_mut()
                .record_general_discovery(&tool_name)
        };

        let msg = run_tool(&self.ctx, call, self.entity.id(), &self.entity.state)
            .await
            .map_err(ToolError::ExecutionError)?;
        let duration_ms = (wf_common::now() - started) as f64;

        if let Some(ref metrics) = self.ctx.metrics {
            let success = !matches!(&msg.content, wf_types::message::MessageContentValue::Text(t) if t.contains("\"error\""));
            metrics
                .tool()
                .record_general_invoke(&tool_name, success, duration_ms);
            if is_first_discovery {
                metrics.tool().record_discovery(&tool_name, "general");
            }
        }

        if is_first_discovery {
            self.emit_discovery_event(&tool_name, "general");
        }

        let content = match &msg.content {
            wf_types::message::MessageContentValue::Text(t) => t.clone(),
            wf_types::message::MessageContentValue::Rich(_) => String::new(),
        };
        // Return the inner tool's native result shape when it was JSON.
        Ok(serde_json::from_str(&content).unwrap_or(serde_json::Value::String(content)))
    }

    fn emit_discovery_event(&self, tool_name: &str, method: &str) {
        let Some(bus) = self.event_bus.as_ref() else {
            return;
        };
        let _ = bus.publish(wf_types::events::BaseEvent {
            id: wf_types::Id::new(),
            r#type: wf_types::events::EventType::NodeCustomEvent,
            timestamp: wf_common::now(),
            workflow_id: None,
            execution_id: Some(self.entity.id().clone()),
            agent_loop_id: Some(self.entity.id().clone()),
            event_name: None,
            metadata: Some(HashMap::from([
                (
                    "event".to_string(),
                    serde_json::Value::String("tool_discovery_state_changed".to_string()),
                ),
                (
                    "tool".to_string(),
                    serde_json::Value::String(tool_name.to_string()),
                ),
                (
                    "method".to_string(),
                    serde_json::Value::String(method.to_string()),
                ),
            ])),
        });
    }
}

#[async_trait::async_trait]
impl wf_tools::general::GeneralToolInvoker for GeneralToolContext {
    async fn invoke_request(&self, request: &str) -> wf_tools::ToolResult<serde_json::Value> {
        let calls = wf_llm::tool_call_parser::parse_invoke_json_calls(request);
        if calls.is_empty() {
            return Err(wf_tools::general::build_format_error());
        }

        let mut results = Vec::with_capacity(calls.len());
        for call in &calls {
            results.push(self.invoke_inner(call).await?);
        }
        if results.len() == 1 {
            Ok(results.pop().expect("len checked above"))
        } else {
            Ok(serde_json::Value::Array(results))
        }
    }
}
