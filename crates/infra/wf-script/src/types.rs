use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorMode {
    Direct,
    Shared,
    Pty,
    SandboxShell,
    SandboxPython,
    SandboxJavaScript,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScriptArgumentType {
    String,
    Number,
    Boolean,
    File,
}

/// Source of the argument value at runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentValueSource {
    /// Static/default value provided at definition time.
    Static,
    /// Resolved from context variables at runtime.
    Variable,
    /// Resolved from a dynamic expression at runtime.
    Expression,
}

/// Script risk level for security evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScriptRiskLevel {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

/// Script Security Policy — controls which scripts are allowed to run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptSecurityPolicy {
    /// Maximum allowed risk level (scripts above this are rejected).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_risk_level: Option<ScriptRiskLevel>,
    /// Whether to require human review before execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_review: Option<bool>,
    /// Allowed script languages (empty = all allowed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_languages: Option<Vec<String>>,
    /// Regex patterns that are blocked in script content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_patterns: Option<Vec<String>>,
    /// Forbidden commands (shell-level, matched against the command).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forbidden_commands: Option<Vec<String>>,
    /// Forbidden path patterns in arguments (e.g. directory traversal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forbidden_path_patterns: Option<Vec<String>>,
    /// Maximum script content size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_script_size: Option<usize>,
    /// Whether to allow dynamic (runtime-generated) scripts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_dynamic_scripts: Option<bool>,
}

/// Interaction mode for interactive scripts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    /// Wait for user input at each interaction point.
    Blocking,
    /// LLM provides automatic responses.
    LlmAssisted,
    /// LLM suggests, user confirms or modifies.
    Hybrid,
}

/// A point during script execution where interaction is needed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptInteractionPoint {
    /// Prompt text to display to the user/LLM.
    pub prompt: String,
    /// Expected input type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_input_type: Option<String>,
    /// Available options (for choice type).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    /// Timeout in milliseconds for this interaction point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// Configuration for scripts that require user interaction during execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InteractiveScriptConfig {
    /// Interaction mode.
    pub mode: InteractionMode,
    /// Maximum interaction rounds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rounds: Option<u32>,
    /// Predefined interaction points (optional, can be auto-detected).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_points: Option<Vec<ScriptInteractionPoint>>,
    /// Prompt patterns to detect (regex strings that indicate waiting for input).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_patterns: Option<Vec<String>>,
    /// Timeout per interaction round (milliseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round_timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptArgument {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<ScriptArgumentType>,
    /// Human-readable label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// Source of the value at runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ArgumentValueSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Allowed options (for enum-like selection).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<Value>>,
    /// Regex pattern for string validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<ScriptArgument>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor_mode: Option<ExecutorMode>,
    /// Interactive script configuration (for scripts requiring user interaction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactive: Option<InteractiveScriptConfig>,
    /// Security policy for this script.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_policy: Option<ScriptSecurityPolicy>,
    /// Script description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the script is enabled (default: true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ScriptExecutionOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor_mode: Option<ExecutorMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Number of retries on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<u32>,
    /// Retry delay (milliseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_delay_ms: Option<u64>,
    /// Whether to enable exponential backoff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exponential_backoff: Option<bool>,
    /// Interactive script configuration (overrides script-level config).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactive: Option<InteractiveScriptConfig>,
    /// Security policy (overrides script-level policy).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_policy: Option<ScriptSecurityPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptExecutionResult {
    pub success: bool,
    pub script_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub execution_time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleRef {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowBranch {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
    pub modules: Vec<ModuleRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptFlow {
    pub name: String,
    pub branches: Vec<FlowBranch>,
}

#[derive(Debug, Clone)]
pub struct FlowBranchExecutionResult {
    pub success: bool,
    pub module_key: String,
    pub output: Option<String>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct BranchExecutionResult {
    pub success: bool,
    pub modules: Vec<FlowBranchExecutionResult>,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct FlowExecutionResult {
    pub success: bool,
    pub branches: HashMap<String, BranchExecutionResult>,
    pub total_execution_time_ms: u64,
    pub error: Option<String>,
}
