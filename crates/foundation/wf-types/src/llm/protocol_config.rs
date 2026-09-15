use serde::{Deserialize, Serialize};

use super::request::ToolCallProtocolViolationPolicy;

/// Default tool call protocol violation policy: warn and continue with the
/// locked protocol.
pub const DEFAULT_TOOL_CALL_PROTOCOL_POLICY: ToolCallProtocolViolationPolicy =
    ToolCallProtocolViolationPolicy::Warn;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CrossBoundaryMismatchStrategy {
    Convert,
    Inherit,
    Strict,
    WarnAndContinue,
}
