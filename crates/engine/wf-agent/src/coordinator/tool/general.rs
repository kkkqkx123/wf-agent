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

    /// Re-run the shared approval gate for one inner call so tools reached
    /// through the discoverable path face the same approval strength as
    /// direct calls. Returns the (possibly parameter-edited) call to
    /// execute; approval denials are returned as errors so the proxy
    /// surfaces them per inner call instead of executing.
    async fn approve_inner(
        &self,
        call: &wf_types::message::LlmToolCall,
    ) -> Result<wf_types::message::LlmToolCall, String> {
        use super::approval::ToolApprovalGate;
        use super::types::ApprovalOutcome;

        if self.ctx.approval_options.is_none() && self.ctx.approval_handler.is_none() {
            return Ok(call.clone());
        }
        let gate = ToolApprovalGate::new(
            self.ctx.approval_options.clone(),
            self.ctx.approval_handler.clone(),
        );
        let outcomes = gate
            .approve_tool_calls(&self.entity, std::slice::from_ref(call), &self.ctx.registry)
            .await;
        match outcomes.into_iter().next() {
            Some(ApprovalOutcome::Execute { edited_parameters }) => {
                let mut approved = call.clone();
                if let Some(edited) = edited_parameters {
                    approved.function.arguments =
                        serde_json::to_string(&edited).unwrap_or(approved.function.arguments);
                }
                Ok(approved)
            }
            Some(ApprovalOutcome::Rejected { reason }) => Err(reason),
            None => Err("internal: missing approval outcome".to_string()),
        }
    }

    /// Execute one inner invocation through the shared pipeline and return
    /// its result value. Per-call failures (exposure denial, approval
    /// rejection, execution error) surface as `{"error": reason}` values so
    /// a batch behaves like direct-call batches: every call yields exactly
    /// one result and failures never swallow their siblings.
    async fn invoke_inner(&self, call: &wf_types::message::LlmToolCall) -> serde_json::Value {
        let tool_name = call.function.name.clone();
        if let Err(reason) = self.check_inner_tool_allowed(&tool_name).await {
            return serde_json::json!({"error": reason});
        }
        let call = match self.approve_inner(call).await {
            Ok(approved) => approved,
            Err(reason) => return serde_json::json!({"error": reason}),
        };

        let started = wf_common::now();
        let is_first_discovery = {
            let mut state = self.entity.state.write().await;
            state
                .tool_discovery_mut()
                .record_general_discovery(&tool_name)
        };

        let outcome = run_tool(&self.ctx, &call, self.entity.id(), &self.entity.state).await;
        let duration_ms = (wf_common::now() - started) as f64;
        // Success comes from the pipeline outcome itself, never from
        // sniffing the payload text (a normal result may mention "error").
        let success = outcome.is_ok();

        if let Some(ref metrics) = self.ctx.metrics {
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

        match outcome {
            Ok(msg) => content_to_value(&msg.content),
            Err(reason) => serde_json::json!({"error": reason}),
        }
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

/// Convert a tool result message body into a JSON value: JSON payloads keep
/// their native shape, plain text stays a string, and rich (multi-modal)
/// blocks serialize to their JSON form instead of being dropped.
fn content_to_value(content: &wf_types::message::MessageContentValue) -> serde_json::Value {
    match content {
        wf_types::message::MessageContentValue::Text(t) => {
            serde_json::from_str(t).unwrap_or(serde_json::Value::String(t.clone()))
        }
        wf_types::message::MessageContentValue::Rich(blocks) => {
            serde_json::to_value(blocks).unwrap_or(serde_json::Value::Null)
        }
    }
}

/// Fallback outer id when the pipeline did not stamp one (direct handler
/// tests, DevTools one-shots): a deterministic hash of the request body, so
/// replays of the same body still map to the same inner keys.
fn fallback_outer_id(request: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    request.hash(&mut hasher);
    format!("general-{:016x}", hasher.finish())
}

#[async_trait::async_trait]
impl wf_tools::general::GeneralToolInvoker for GeneralToolContext {
    async fn invoke_request(&self, request: &str) -> wf_tools::ToolResult<serde_json::Value> {
        self.invoke_request_with_outer(&fallback_outer_id(request), request)
            .await
    }

    async fn invoke_request_with_outer(
        &self,
        outer_call_id: &str,
        request: &str,
    ) -> wf_tools::ToolResult<serde_json::Value> {
        // Explicit per-item errors: the whole body failing parses to the
        // format hint; individual bad items become positioned error values
        // while their siblings still execute.
        let items = match wf_llm::tool_call_parser::parse_invoke_json_calls_detailed(request) {
            Ok(items) => items,
            Err(_) => return Err(wf_tools::general::build_format_error()),
        };
        if items.is_empty() {
            return Err(wf_tools::general::build_format_error());
        }

        let mut results = Vec::with_capacity(items.len());
        for (index, item) in items.into_iter().enumerate() {
            match item {
                Ok(mut call) => {
                    // Stable inner keys (`outer#index#tool`): checkpoint
                    // replay of this outer call hits the inner result cache
                    // instead of re-executing side effects.
                    call.id = wf_tools::general_history::derive_inner_call_id(
                        outer_call_id,
                        index,
                        &call.function.name,
                    );
                    results.push(self.invoke_inner(&call).await);
                }
                Err(parse_error) => {
                    results.push(serde_json::json!({"error": parse_error.to_string()}));
                }
            }
        }
        if results.len() == 1 {
            Ok(results.pop().expect("len checked above"))
        } else {
            Ok(serde_json::Value::Array(results))
        }
    }
}
