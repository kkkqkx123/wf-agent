//! Restricted string-value enumerations shared by the API layer.
//!
//! These enums replace free-form `String` fields whose values are drawn from
//! a fixed, closed set. Serialization uses `snake_case` so the JSON output is
//! byte-identical to the historical string values (see `wf-api` consumers:
//! variable history sources, script languages, error severity buckets and
//! performance/trend classifications).

use serde::{Deserialize, Serialize};

/// Source of a variable history entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableSource {
    /// The current storage record.
    Storage,
    /// The live execution entity.
    Live,
    /// The persisted execution record.
    Persisted,
    /// The latest checkpoint variable state.
    Checkpoint,
}

impl VariableSource {
    /// Canonical snake_case string of the source.
    pub fn as_str(&self) -> &'static str {
        match self {
            VariableSource::Storage => "storage",
            VariableSource::Live => "live",
            VariableSource::Persisted => "persisted",
            VariableSource::Checkpoint => "checkpoint",
        }
    }
}

/// Script executor language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptLanguage {
    Shell,
    Python,
    /// Accepted alias `js` on input (deserialization).
    #[serde(alias = "js")]
    JavaScript,
    Lua,
}

impl ScriptLanguage {
    /// Canonical snake_case string of the language.
    pub fn as_str(&self) -> &'static str {
        match self {
            ScriptLanguage::Shell => "shell",
            ScriptLanguage::Python => "python",
            ScriptLanguage::JavaScript => "javascript",
            ScriptLanguage::Lua => "lua",
        }
    }
}

/// Coarse error severity bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Warning,
    Critical,
}

impl ErrorSeverity {
    /// Canonical snake_case string of the severity.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorSeverity::Warning => "warning",
            ErrorSeverity::Critical => "critical",
        }
    }
}

/// Performance bottleneck severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BottleneckSeverity {
    Low,
    Medium,
    High,
}

impl BottleneckSeverity {
    /// Canonical snake_case string of the severity.
    pub fn as_str(&self) -> &'static str {
        match self {
            BottleneckSeverity::Low => "low",
            BottleneckSeverity::Medium => "medium",
            BottleneckSeverity::High => "high",
        }
    }
}

/// Error count trend direction across an execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorTrend {
    Stable,
    Increasing,
    Decreasing,
}

impl ErrorTrend {
    /// Canonical snake_case string of the trend.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorTrend::Stable => "stable",
            ErrorTrend::Increasing => "increasing",
            ErrorTrend::Decreasing => "decreasing",
        }
    }
}

/// Performance trend of node durations across an execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceTrend {
    Stable,
    Improving,
    Degrading,
}

impl PerformanceTrend {
    /// Canonical snake_case string of the trend.
    pub fn as_str(&self) -> &'static str {
        match self {
            PerformanceTrend::Stable => "stable",
            PerformanceTrend::Improving => "improving",
            PerformanceTrend::Degrading => "degrading",
        }
    }
}

/// Known plugin hook type plus unknown fallback.
///
/// Plugins register hooks under arbitrary strings (the plugin registry is a
/// `MultiRegistry<String, ...>`); this enum types the known set while
/// `Other` keeps any plugin-registered key representable without rejecting or
/// erroring on registration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookType {
    OnWorkflowStart,
    OnWorkflowEnd,
    OnNodeStart,
    OnNodeEnd,
    OnError,
    /// Any other plugin-registered hook type (round-trips as-is).
    #[serde(untagged)]
    Other(String),
}

impl HookType {
    /// Registration string of the hook type. `Other` returns its raw value,
    /// so an unknown plugin key round-trips unchanged.
    pub fn as_str(&self) -> &str {
        match self {
            HookType::OnWorkflowStart => "on_workflow_start",
            HookType::OnWorkflowEnd => "on_workflow_end",
            HookType::OnNodeStart => "on_node_start",
            HookType::OnNodeEnd => "on_node_end",
            HookType::OnError => "on_error",
            HookType::Other(value) => value,
        }
    }
}

impl From<&str> for HookType {
    fn from(value: &str) -> Self {
        match value {
            "on_workflow_start" => HookType::OnWorkflowStart,
            "on_workflow_end" => HookType::OnWorkflowEnd,
            "on_node_start" => HookType::OnNodeStart,
            "on_node_end" => HookType::OnNodeEnd,
            "on_error" => HookType::OnError,
            other => HookType::Other(other.to_string()),
        }
    }
}

impl From<String> for HookType {
    fn from(value: String) -> Self {
        HookType::from(value.as_str())
    }
}

/// Known middleware lifecycle phase plus unknown fallback.
///
/// Values serialize in kebab-case, matching the historical
/// `middleware_phase` string constants; unknown plugin-registered phases fall
/// into `Other` and round-trip unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MiddlewarePhase {
    BeforeWorkflowExecution,
    AfterWorkflowExecution,
    BeforeNodeExecution,
    AfterNodeExecution,
    BeforeLlmInvocation,
    AfterLlmInvocation,
    BeforeToolExecution,
    AfterToolExecution,
    OnError,
    OnCheckpoint,
    OnResume,
    /// Any other plugin-registered phase (round-trips as-is).
    #[serde(untagged)]
    Other(String),
}

impl MiddlewarePhase {
    /// Registration string of the phase. `Other` returns its raw value, so an
    /// unknown plugin phase round-trips unchanged.
    pub fn as_str(&self) -> &str {
        match self {
            MiddlewarePhase::BeforeWorkflowExecution => "before-workflow-execution",
            MiddlewarePhase::AfterWorkflowExecution => "after-workflow-execution",
            MiddlewarePhase::BeforeNodeExecution => "before-node-execution",
            MiddlewarePhase::AfterNodeExecution => "after-node-execution",
            MiddlewarePhase::BeforeLlmInvocation => "before-llm-invocation",
            MiddlewarePhase::AfterLlmInvocation => "after-llm-invocation",
            MiddlewarePhase::BeforeToolExecution => "before-tool-execution",
            MiddlewarePhase::AfterToolExecution => "after-tool-execution",
            MiddlewarePhase::OnError => "on-error",
            MiddlewarePhase::OnCheckpoint => "on-checkpoint",
            MiddlewarePhase::OnResume => "on-resume",
            MiddlewarePhase::Other(value) => value,
        }
    }
}

impl From<&str> for MiddlewarePhase {
    fn from(value: &str) -> Self {
        match value {
            "before-workflow-execution" => MiddlewarePhase::BeforeWorkflowExecution,
            "after-workflow-execution" => MiddlewarePhase::AfterWorkflowExecution,
            "before-node-execution" => MiddlewarePhase::BeforeNodeExecution,
            "after-node-execution" => MiddlewarePhase::AfterNodeExecution,
            "before-llm-invocation" => MiddlewarePhase::BeforeLlmInvocation,
            "after-llm-invocation" => MiddlewarePhase::AfterLlmInvocation,
            "before-tool-execution" => MiddlewarePhase::BeforeToolExecution,
            "after-tool-execution" => MiddlewarePhase::AfterToolExecution,
            "on-error" => MiddlewarePhase::OnError,
            "on-checkpoint" => MiddlewarePhase::OnCheckpoint,
            "on-resume" => MiddlewarePhase::OnResume,
            other => MiddlewarePhase::Other(other.to_string()),
        }
    }
}

impl From<String> for MiddlewarePhase {
    fn from(value: String) -> Self {
        MiddlewarePhase::from(value.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_type_known_strings_round_trip() {
        assert_eq!(
            HookType::from("on_workflow_start"),
            HookType::OnWorkflowStart
        );
        assert_eq!(HookType::from("on_workflow_end"), HookType::OnWorkflowEnd);
        assert_eq!(HookType::from("on_node_start"), HookType::OnNodeStart);
        assert_eq!(HookType::from("on_node_end"), HookType::OnNodeEnd);
        assert_eq!(HookType::from("on_error"), HookType::OnError);
        assert_eq!(HookType::OnWorkflowStart.as_str(), "on_workflow_start");
        assert_eq!(HookType::OnError.as_str(), "on_error");
    }

    #[test]
    fn hook_type_unknown_string_falls_to_other_and_round_trips() {
        let unknown = HookType::from("custom-plugin-hook");
        assert_eq!(unknown, HookType::Other("custom-plugin-hook".into()));
        assert_eq!(unknown.as_str(), "custom-plugin-hook");
        assert_eq!(
            serde_json::to_value(&unknown).unwrap(),
            "custom-plugin-hook"
        );
        assert_eq!(
            serde_json::from_value::<HookType>("custom-plugin-hook".into()).unwrap(),
            unknown
        );
    }

    #[test]
    fn hook_type_serde_uses_snake_case() {
        assert_eq!(
            serde_json::to_string(&HookType::OnNodeStart).unwrap(),
            "\"on_node_start\""
        );
    }

    #[test]
    fn middleware_phase_known_strings_round_trip() {
        assert_eq!(
            MiddlewarePhase::from("before-workflow-execution"),
            MiddlewarePhase::BeforeWorkflowExecution
        );
        assert_eq!(
            MiddlewarePhase::from("after-llm-invocation"),
            MiddlewarePhase::AfterLlmInvocation
        );
        assert_eq!(
            MiddlewarePhase::from("on-checkpoint"),
            MiddlewarePhase::OnCheckpoint
        );
        assert_eq!(
            MiddlewarePhase::BeforeWorkflowExecution.as_str(),
            "before-workflow-execution"
        );
        assert_eq!(MiddlewarePhase::OnResume.as_str(), "on-resume");
    }

    #[test]
    fn middleware_phase_unknown_string_falls_to_other() {
        let unknown = MiddlewarePhase::from("custom-phase");
        assert_eq!(unknown, MiddlewarePhase::Other("custom-phase".into()));
        assert_eq!(unknown.as_str(), "custom-phase");
    }

    #[test]
    fn middleware_phase_serde_uses_kebab_case() {
        assert_eq!(
            serde_json::to_string(&MiddlewarePhase::BeforeNodeExecution).unwrap(),
            "\"before-node-execution\""
        );
        assert_eq!(
            serde_json::from_value::<MiddlewarePhase>("on-error".into()).unwrap(),
            MiddlewarePhase::OnError
        );
    }
}
