use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorMode {
    Direct,
    Shared,
    Pty,
    SandboxShell,
    SandboxPython,
    SandboxJavaScript,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScriptArgumentType {
    String,
    Number,
    Boolean,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptArgument {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<ScriptArgumentType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptExecutionOptions {
    pub executor_mode: Option<ExecutorMode>,
    pub working_directory: Option<String>,
    pub environment: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
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
