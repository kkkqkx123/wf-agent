use serde_json::Value;
use wf_execution_shared::context::NodeExecutionContext;
use wf_types::llm::{
    generation::LlmGenerationParams, DeadLoopDetectionConfig, LlmExecutionConfig,
    ToolCallProtocolConfig, ToolCallProtocolViolationPolicy,
};

use crate::error::{WorkflowError, WorkflowResult};

/// Parsed and validated LLM node configuration. Pure data; parsing warns
/// (never silently degrades) for unvalidated graphs, mirroring static
/// validation for registered graphs.
#[derive(Debug, Clone)]
pub struct LlmNodeConfig {
    pub profile_id: String,
    pub tool_call_protocol: Option<ToolCallProtocolConfig>,
    pub violation_policy: Option<ToolCallProtocolViolationPolicy>,
    pub stream_enabled: bool,
    pub max_interactions: u64,
    pub max_tools_per_response: Option<u64>,
    pub exec_config: LlmExecutionConfig,
    pub token_tracking_enabled: bool,
    pub token_warning_threshold: u64,
    pub dead_loop_detection: Option<DeadLoopDetectionConfig>,
    pub node_generation: Option<LlmGenerationParams>,
}

pub fn parse_llm_node_config(ctx: &NodeExecutionContext) -> WorkflowResult<LlmNodeConfig> {
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

    let profile_id = config
        .get("profile_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            WorkflowError::OperationError("LLM node requires a profile_id".to_string())
        })?;

    // Node-level tool call format (canonical string) is applied at runtime
    // so validation and execution agree on the effective protocol. An
    // unknown or non-string value is ignored as before, but now warns:
    // registered graphs are statically rejected for this, so reaching
    // here means an unvalidated graph where silence would hide a typo.
    let tool_call_protocol = match config.get("tool_call_protocol") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_str() {
            Some(s) => match ToolCallProtocolConfig::from_protocol_str(s) {
                Some(format) => Some(format),
                None => {
                    tracing::warn!(
                        node_id = %ctx.node_id,
                        field = "inner.tool_call_protocol",
                        value = %s,
                        "unknown tool call format, ignoring node-level override"
                    );
                    None
                }
            },
            None => {
                tracing::warn!(
                    node_id = %ctx.node_id,
                    field = "inner.tool_call_protocol",
                    "tool_call_protocol must be a canonical format string, ignoring"
                );
                None
            }
        },
    };
    let violation_policy = match config.get("violation_policy") {
        None => None,
        Some(v) => crate::config_parse::parse_node_config_or_warn(
            &ctx.node_id,
            "inner.violation_policy",
            v,
            None,
        ),
    };

    let stream_enabled = config
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // Interaction budget: maximum model generations per node
    // execution. Static validation rejects an explicit zero; the
    // runtime guard below covers unvalidated graphs the same way
    // instead of running zero generations and returning nothing.
    let max_interactions = config
        .get("max_interactions")
        .and_then(|v| v.as_u64())
        .unwrap_or(5);
    if max_interactions == 0 {
        return Err(WorkflowError::OperationError(format!(
            "LLM node '{}' has max_interactions 0; it must be >= 1",
            ctx.node_id
        )));
    }
    // Per-response tool cap: at most this many tool calls of a single
    // model response are executed; the rest fail with an error result
    // so the next generation sees what was skipped. Absent means no
    // cap. Zero would execute nothing while declaring tools, which
    // masks a misconfiguration, so it is rejected: omit the tools
    // list for text-only calls.
    let max_tools_per_response: Option<u64> = config
        .get("max_tool_calls_per_request")
        .and_then(|v| v.as_u64());
    if max_tools_per_response == Some(0) {
        return Err(WorkflowError::OperationError(format!(
            "LLM node '{}' has max_tool_calls_per_request 0; omit the tools list for text-only calls",
            ctx.node_id
        )));
    }

    // Token tracking: the node-level settings consolidate through
    // `LlmExecutionConfig` (typed extraction from the node config JSON);
    // a parse failure degrades to defaults (tracking on, default
    // threshold, no limit) with an explicit warning. The shared execution
    // tracker is updated after every LLM call unless tracking is
    // explicitly disabled.
    let exec_config: LlmExecutionConfig = crate::config_parse::parse_node_config_or_warn(
        &ctx.node_id,
        "inner (LlmExecutionConfig)",
        config,
        LlmExecutionConfig::default(),
    );
    let token_tracking_enabled = exec_config.enable_token_tracking.unwrap_or(true);
    let token_warning_threshold = exec_config
        .token_warning_threshold
        .map(u64::from)
        .unwrap_or(wf_execution_shared::DEFAULT_TOKEN_WARNING_THRESHOLD as u64);

    // Dead-loop detection config: parsed once from the node config,
    // carried on every request in the multi-round tool loop. The gateway
    // detects repeated output and errors out (wf-llm `DeadLoopDetector`).
    let dead_loop_detection: Option<DeadLoopDetectionConfig> =
        match config.get("dead_loop_detection") {
            None => None,
            Some(v) => crate::config_parse::parse_node_config_or_warn(
                &ctx.node_id,
                "inner.dead_loop_detection",
                v,
                None,
            ),
        };

    // An invalid node-level `generation` warns and falls through to the
    // execution defaults, mirroring the previous `.ok()` fallback with
    // observability added. Registered graphs are statically rejected
    // for this; this path only serves unvalidated graphs.
    let parsed_generation: Option<LlmGenerationParams> = match config.get("generation") {
        None | Some(Value::Null) => None,
        Some(v) => match serde_json::from_value(v.clone()) {
            Ok(parsed) => Some(parsed),
            Err(e) => {
                tracing::warn!(
                    node_id = %ctx.node_id,
                    field = "inner.generation",
                    error = %e,
                    "invalid node generation config, falling back to execution defaults"
                );
                None
            }
        },
    };
    let node_generation = parsed_generation.or_else(|| exec_config.generation.clone());

    Ok(LlmNodeConfig {
        profile_id,
        tool_call_protocol,
        violation_policy,
        stream_enabled,
        max_interactions,
        max_tools_per_response,
        exec_config,
        token_tracking_enabled,
        token_warning_threshold,
        dead_loop_detection,
        node_generation,
    })
}
