use wf_types::llm::{
    LlmProfile, LlmRequest, ToolCallProtocol, ToolCallProtocolViolationPolicy,
    DEFAULT_TOOL_CALL_PROTOCOL_POLICY,
};

use crate::error::{LlmError, LlmResult};

/// Single-point merge of profile defaults with request overrides:
/// - `parameters`: request wins per key, profile fills the rest
/// - `tool_call_protocol`: request wins, otherwise the profile default
/// - `locked_tool_call_protocol`: always wins when present, governed by the
///   violation policy (fail / warn / auto-convert / ignore)
///
/// Pipeline contract: this is the only raw key-merge site. Typed generation
/// normalization happens exactly once downstream in
/// `generation::resolve_generation` (invoked by codecs); no other layer
/// merges legacy parameter keys.
pub fn merge_request(request: &LlmRequest, profile: &LlmProfile) -> LlmResult<LlmRequest> {
    let mut effective = request.clone();

    if effective.tool_call_protocol.is_none() {
        effective.tool_call_protocol = profile
            .tool_call_protocol
            .as_ref()
            .map(|config| config.format.clone());
    }

    if let Some(locked) = effective.locked_tool_call_protocol.clone() {
        if let Some(attempted) = effective.tool_call_protocol {
            // Compatible formats (e.g. both JSON-based) proceed silently;
            // a genuine protocol conflict is governed by the policy.
            if attempted != locked.format && !attempted.is_compatible_with(&locked.format) {
                let policy = effective
                    .violation_policy
                    .clone()
                    .unwrap_or(DEFAULT_TOOL_CALL_PROTOCOL_POLICY);
                handle_protocol_violation(
                    request,
                    profile,
                    &locked.format,
                    attempted,
                    policy.clone(),
                )?;
                if policy == ToolCallProtocolViolationPolicy::AutoConvert {
                    effective.protocol_auto_converted = Some(true);
                }
            }
        }
        effective.tool_call_protocol = Some(locked.format);
    }

    let merged = crate::codecs::helpers::merge_parameters(profile, &effective.parameters);
    effective.parameters = if merged.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(merged.into_iter().collect()))
    };

    Ok(effective)
}

fn handle_protocol_violation(
    request: &LlmRequest,
    profile: &LlmProfile,
    locked: &ToolCallProtocol,
    attempted: ToolCallProtocol,
    policy: ToolCallProtocolViolationPolicy,
) -> LlmResult<()> {
    match policy {
        ToolCallProtocolViolationPolicy::Fail => Err(LlmError::ConfigError(format!(
            "Tool call protocol conflict: locked \"{}\" but profile \"{}\" attempted \"{}\". Execution interrupted per fail policy.",
            locked, profile.id, attempted
        ))),
        ToolCallProtocolViolationPolicy::Warn => {
            tracing::warn!(
                profile_id = %profile.id,
                locked_format = %locked,
                attempted_format = %attempted,
                execution_id = ?request.execution_id,
                "Tool call protocol violation detected, using the locked format"
            );
            Ok(())
        }
        ToolCallProtocolViolationPolicy::AutoConvert => {
            tracing::info!(
                profile_id = %profile.id,
                locked_format = %locked,
                attempted_format = %attempted,
                execution_id = ?request.execution_id,
                "Auto-converting tool call protocol to locked format"
            );
            Ok(())
        }
        ToolCallProtocolViolationPolicy::Ignore => {
            // Silently use the locked protocol
            Ok(())
        }
    }
}
