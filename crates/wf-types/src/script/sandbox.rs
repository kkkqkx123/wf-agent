use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SandboxMode {
    Disabled,
    Lenient,
    Strict,
}

/// Filesystem policy. Every field is `Option<T>`: `None` means "not
/// specified" and inherits from the base policy during merge; only an
/// explicit `Some(...)` overrides. Empty explicit lists (`Some(vec![])`) are
/// deliberate removals of default entries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FilesystemPolicy {
    pub allowed_read_paths: Option<Vec<String>>,
    pub allowed_write_paths: Option<Vec<String>>,
    pub allowed_remove_paths: Option<Vec<String>>,
    pub allowed_execute_paths: Option<Vec<String>>,
    pub copy_on_write: Option<bool>,
    pub max_file_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProcessPolicy {
    pub allowed_child_processes: Option<Vec<String>>,
    pub denied_child_processes: Option<Vec<String>>,
    pub max_child_processes: Option<u32>,
    pub allow_fork: Option<bool>,
    pub allow_exec: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct NetworkPolicy {
    #[serde(rename = "access")]
    pub access_type: Option<NetworkAccessType>,
    pub allowed_domains: Option<Vec<String>>,
    pub allowed_ports: Option<Vec<(u16, u16)>>,
    pub allow_dns: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkAccessType {
    None,
    Localhost,
    Specific,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourcePolicy {
    pub cpu_limit_ms: Option<u64>,
    pub memory_limit_mb: Option<u64>,
    pub disk_limit_mb: Option<u64>,
    pub timeout_limit_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ShellPolicy {
    pub allowed_commands: Option<Vec<String>>,
    pub denied_commands: Option<Vec<String>>,
    pub dangerous_patterns: Option<Vec<String>>,
    pub allow_pipe: Option<bool>,
    pub allow_redirect: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PythonPolicy {
    pub allowed_modules: Option<Vec<String>>,
    pub denied_modules: Option<Vec<String>>,
    pub allow_subprocess: Option<bool>,
    pub restrict_builtin_open: Option<bool>,
    pub allow_dynamic_eval: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct JavaScriptPolicy {
    pub allowed_modules: Option<Vec<String>>,
    pub denied_modules: Option<Vec<String>>,
    pub allow_child_process: Option<bool>,
    pub allow_fs_write: Option<bool>,
    pub allow_dynamic_eval: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LuaPolicy {
    pub allowed_modules: Option<Vec<String>>,
    pub denied_modules: Option<Vec<String>>,
    pub allow_os_execute: Option<bool>,
    pub restrict_io_open: Option<bool>,
    pub allow_dynamic_load: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxPolicy {
    /// `None` means "not specified" and inherits from the base policy during
    /// merge; only an explicit `Some(mode)` overrides. This eliminates the
    /// former Strict-as-unset sentinel semantics.
    pub mode: Option<SandboxMode>,
    pub shell: Option<ShellPolicy>,
    pub python: Option<PythonPolicy>,
    pub javascript: Option<JavaScriptPolicy>,
    pub lua: Option<LuaPolicy>,
    pub filesystem: Option<FilesystemPolicy>,
    pub process: Option<ProcessPolicy>,
    pub network: Option<NetworkPolicy>,
    pub resource: Option<ResourcePolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxConfig {
    pub mode: Option<SandboxMode>,
    pub policy: Option<SandboxPolicy>,
    pub shell_strategy: Option<Vec<String>>,
    pub python_strategy: Option<Vec<String>>,
    pub javascript_strategy: Option<Vec<String>>,
    pub lua_strategy: Option<Vec<String>>,
    pub vfs: Option<VfsConfig>,
    /// Working directory for the executed script; propagates to the
    /// subprocess used by the execution strategy.
    pub workdir: Option<String>,
    /// Environment variables for the executed script.
    pub env: Option<HashMap<String, String>>,

    #[serde(rename = "type")]
    pub legacy_type: Option<String>,
    pub resource_limits: Option<ResourceLimits>,
    /// Allow a strategy chain without any analysis gate for languages that
    /// have one by default (shell/python/lua). Defaults to `false`
    /// (fail-closed); only for advanced users accepting the risk.
    pub skip_gate_check: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VfsConfig {
    pub enabled: bool,
    pub storage: Option<VfsStorageType>,
    pub db_path: Option<String>,
    pub workspace_root: Option<String>,
    pub path_policy: Option<PathPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VfsStorageType {
    Memory,
    Sqlite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathPolicy {
    pub allowed_read: Vec<String>,
    pub allowed_write: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptExecutionResult {
    pub success: bool,
    pub script_name: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub execution_time: u64,
    pub error: Option<String>,
    pub sandbox_mode: Option<String>,
    pub strategy_id: Option<String>,
    pub violations: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceLimits {
    pub cpu: Option<u64>,
    pub memory: Option<u64>,
    pub disk: Option<u64>,
}

// ============================================================
// Profile & Global Configuration
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxProfile {
    pub name: String,
    pub description: Option<String>,
    pub mode: Option<SandboxMode>,
    pub shell_strategy: Option<Vec<String>>,
    pub python_strategy: Option<Vec<String>>,
    pub javascript_strategy: Option<Vec<String>>,
    pub lua_strategy: Option<Vec<String>>,
    pub policy: Option<SandboxPolicy>,
    pub vfs: Option<VfsConfig>,
    pub workdir: Option<String>,
    pub env: Option<HashMap<String, String>>,
}

/// Match target of a sandbox profile routing rule. Unknown fields are
/// rejected at deserialization time instead of at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxRuleMatchField {
    Language,
    ScriptName,
}

impl SandboxRuleMatchField {
    /// Canonical string form, as used in serialized configuration.
    pub fn as_str(self) -> &'static str {
        match self {
            SandboxRuleMatchField::Language => "language",
            SandboxRuleMatchField::ScriptName => "script_name",
        }
    }
}

impl std::fmt::Display for SandboxRuleMatchField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxProfileRule {
    pub match_field: SandboxRuleMatchField,
    pub match_pattern: String,
    pub profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxGlobalConfig {
    pub mode: Option<SandboxMode>,
    pub profiles: Vec<SandboxProfile>,
    pub rules: Vec<SandboxProfileRule>,
    pub default_profile: Option<String>,
    pub audit_logging: bool,
}

impl Default for SandboxGlobalConfig {
    fn default() -> Self {
        Self {
            mode: Some(SandboxMode::Strict),
            profiles: vec![],
            rules: vec![],
            default_profile: None,
            audit_logging: true,
        }
    }
}

/// Referential integrity error of a [`SandboxGlobalConfig`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SandboxGlobalConfigError {
    #[error("sandbox rule (match_field='{match_field}', pattern='{pattern}') references unknown profile '{profile}'")]
    UnknownProfile {
        match_field: SandboxRuleMatchField,
        pattern: String,
        profile: String,
    },

    #[error("sandbox default_profile '{0}' references unknown profile")]
    UnknownDefaultProfile(String),
}

impl SandboxGlobalConfig {
    /// Fail-fast referential validation: every rule must reference an
    /// existing profile and `default_profile` must exist.
    ///
    /// This is the single source of truth for config semantics. It is
    /// invoked at configuration load time (wf-config) and at runtime
    /// construction (wf-sandbox `SandboxProfileResolver::compile`) so both
    /// paths reject the same invalid configurations.
    pub fn validate(&self) -> Result<(), SandboxGlobalConfigError> {
        for rule in &self.rules {
            let exists = self.profiles.iter().any(|p| p.name == rule.profile);
            if !exists {
                return Err(SandboxGlobalConfigError::UnknownProfile {
                    match_field: rule.match_field,
                    pattern: rule.match_pattern.clone(),
                    profile: rule.profile.clone(),
                });
            }
        }
        if let Some(ref name) = self.default_profile {
            let exists = self.profiles.iter().any(|p| p.name == *name);
            if !exists {
                return Err(SandboxGlobalConfigError::UnknownDefaultProfile(name.clone()));
            }
        }
        Ok(())
    }
}

// ============================================================
// Audit & Metadata Types
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuditEventType {
    ExecutionAllowed,
    ExecutionDenied,
    ExecutionViolation,
    StrategyFallback,
    ConfigError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEvent {
    pub timestamp: String,
    pub event_type: AuditEventType,
    pub language: String,
    pub script_name: String,
    pub violation: Option<String>,
    pub strategy_id: Option<String>,
    pub allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptMetadata {
    pub name: String,
    pub language: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
}

// ============================================================
// Security Validation
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityViolation {
    pub field: String,
    pub reason: String,
    pub severity: SecuritySeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecuritySeverity {
    Info,
    Warning,
    Error,
    Critical,
}
