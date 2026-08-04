use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SandboxMode {
    Disabled,
    Lenient,
    Strict,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilesystemPolicy {
    pub allowed_read_paths: Vec<String>,
    pub allowed_write_paths: Vec<String>,
    pub allowed_remove_paths: Vec<String>,
    pub allowed_execute_paths: Vec<String>,
    pub copy_on_write: bool,
    pub max_file_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessPolicy {
    pub allowed_child_processes: Vec<String>,
    pub denied_child_processes: Vec<String>,
    pub max_child_processes: u32,
    pub allow_fork: bool,
    pub allow_exec: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkPolicy {
    #[serde(rename = "access")]
    pub access_type: NetworkAccessType,
    pub allowed_domains: Option<Vec<String>>,
    pub allowed_ports: Option<Vec<(u16, u16)>>,
    pub allow_dns: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PythonPolicy {
    pub allowed_modules: Vec<String>,
    pub denied_modules: Vec<String>,
    pub allow_subprocess: bool,
    pub restrict_builtin_open: bool,
    pub allow_dynamic_eval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JavaScriptPolicy {
    pub allowed_modules: Vec<String>,
    pub denied_modules: Vec<String>,
    pub allow_child_process: bool,
    pub allow_fs_write: bool,
    pub allow_dynamic_eval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LuaPolicy {
    pub allowed_modules: Vec<String>,
    pub denied_modules: Vec<String>,
    pub allow_os_execute: bool,
    pub restrict_io_open: bool,
    pub allow_dynamic_load: bool,
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

    #[serde(rename = "type")]
    pub legacy_type: Option<String>,
    pub resource_limits: Option<ResourceLimits>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxProfileRule {
    pub match_field: String,
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
