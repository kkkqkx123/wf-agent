use super::request::ToolCallProtocolViolationPolicy;

/// Default tool call protocol violation policy: warn and continue with the
/// locked protocol.
pub const DEFAULT_TOOL_CALL_PROTOCOL_POLICY: ToolCallProtocolViolationPolicy =
    ToolCallProtocolViolationPolicy::Warn;
