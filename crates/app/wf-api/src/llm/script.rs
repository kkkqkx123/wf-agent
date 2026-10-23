//! Direct script execution entry points.
//!
//! Execution renders the template (via `wf-script`) and runs the resulting
//! command through the shared `wf-sandbox` runtime of the context, so ad-hoc
//! executions get exactly the same sandbox profile routing and gate checks as
//! `SCRIPT` nodes inside a workflow.

use std::collections::HashMap;

use serde::Serialize;

use wf_types::enums::ScriptLanguage;
use wf_types::script::sandbox::SandboxConfig;

mod execution;
mod references;
mod storage;
#[cfg(test)]
mod tests;

/// Result of a script validation.
#[derive(Debug, Clone, Serialize)]
pub struct ScriptValidation {
    pub valid: bool,
    pub errors: Vec<String>,
}

/// Parameters for a direct script execution.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScriptExecuteParams {
    /// Script name (used for audit, profile routing and lookup fallback).
    pub name: String,
    /// Executor language: `shell` / `python` / `javascript` / `lua`.
    pub language: Option<ScriptLanguage>,
    /// Inline code to run (takes precedence over `template`).
    pub code: Option<String>,
    /// Template rendered with `args` before execution.
    pub template: Option<String>,
    /// Template arguments (only meaningful together with `template`).
    pub args: HashMap<String, serde_json::Value>,
    /// Optional sandbox config; defaults to a Lenient config.
    pub sandbox: Option<SandboxConfig>,
    /// Working directory passed to the sandbox strategy.
    pub working_directory: Option<String>,
    /// Environment variables passed to the sandbox strategy.
    pub environment: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    /// Cap captured stdout/stderr at this many bytes each (tail-kept).
    pub max_output_bytes: Option<u64>,
    /// When output is truncated, spill the full stream to this directory.
    pub output_spill_dir: Option<String>,
}

pub use execution::{execute, is_enabled, validate};
pub use references::{check_script_delete_references, ScriptReference};
pub use storage::{
    delete_script, disable_script, enable_script, get_script, is_script_enabled, list_scripts,
    list_scripts_by_language, save_script, save_script_with_impact, search_scripts,
    set_script_enabled,
};
