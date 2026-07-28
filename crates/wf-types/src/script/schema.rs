use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxModeSchema {
    Strict,
    Lenient,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScriptLanguageSchema {
    Shell,
    Python,
    JavaScript,
    Lua,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScriptRiskLevelSchema {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilesystemPolicySchema {
    pub read_allowed: Option<Vec<String>>,
    pub write_allowed: Option<Vec<String>>,
    pub read_blocked: Option<Vec<String>>,
    pub write_blocked: Option<Vec<String>>,
    pub max_file_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessPolicySchema {
    pub max_processes: Option<u32>,
    pub max_cpu_time: Option<u64>,
    pub allowed_binaries: Option<Vec<String>>,
    pub blocked_binaries: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkPolicySchema {
    pub enabled: bool,
    pub allowed_hosts: Option<Vec<String>>,
    pub blocked_hosts: Option<Vec<String>>,
    pub allowed_ports: Option<Vec<u16>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourcePolicySchema {
    pub max_memory_mb: Option<u64>,
    pub max_disk_mb: Option<u64>,
    pub max_open_files: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShellPolicySchema {
    pub allowed_commands: Option<Vec<String>>,
    pub blocked_commands: Option<Vec<String>>,
    pub allow_pipe: Option<bool>,
    pub allow_redirect: Option<bool>,
    pub allow_background: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PythonPolicySchema {
    pub allowed_modules: Option<Vec<String>>,
    pub blocked_modules: Option<Vec<String>>,
    pub allow_import: Option<bool>,
    pub allow_exec: Option<bool>,
    pub allow_eval: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JavaScriptPolicySchema {
    pub allowed_globals: Option<Vec<String>>,
    pub blocked_globals: Option<Vec<String>>,
    pub allow_fetch: Option<bool>,
    pub allow_worker: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LuaPolicySchema {
    pub allowed_modules: Option<Vec<String>>,
    pub blocked_modules: Option<Vec<String>>,
    pub allow_os: Option<bool>,
    pub allow_io: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxPolicySchema {
    pub filesystem: Option<FilesystemPolicySchema>,
    pub process: Option<ProcessPolicySchema>,
    pub network: Option<NetworkPolicySchema>,
    pub resource: Option<ResourcePolicySchema>,
    pub shell: Option<ShellPolicySchema>,
    pub python: Option<PythonPolicySchema>,
    pub javascript: Option<JavaScriptPolicySchema>,
    pub lua: Option<LuaPolicySchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VfsConfigSchema {
    pub mount_points: Option<Vec<String>>,
    pub root_path: Option<String>,
    pub max_size_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxConfigSchema {
    pub mode: SandboxModeSchema,
    pub policy: Option<SandboxPolicySchema>,
    pub vfs: Option<VfsConfigSchema>,
    pub timeout_seconds: Option<u64>,
    pub max_memory_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptArgumentSchema {
    pub name: String,
    pub value: String,
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerConfigSchema {
    pub image: String,
    pub command: Option<Vec<String>>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub volumes: Option<Vec<String>>,
    pub network_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SshConfigSchema {
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub key_path: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeConfigSchema {
    pub docker: Option<DockerConfigSchema>,
    pub ssh: Option<SshConfigSchema>,
    pub timeout_seconds: Option<u64>,
    pub max_retries: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptExecutorConfigSchema {
    pub language: ScriptLanguageSchema,
    pub sandbox: Option<SandboxConfigSchema>,
    pub runtime: Option<RuntimeConfigSchema>,
    pub arguments: Option<Vec<ScriptArgumentSchema>>,
    pub environment: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxProfileRuleSchema {
    pub path: String,
    pub permission: String,
    pub recursive: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxProfileSchema {
    pub name: String,
    pub description: Option<String>,
    pub rules: Vec<SandboxProfileRuleSchema>,
    pub policy: Option<SandboxPolicySchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxGlobalConfigSchema {
    pub default_profile: Option<String>,
    pub profiles: Option<Vec<SandboxProfileSchema>>,
    pub global_policy: Option<SandboxPolicySchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptExecutionOptionsSchema {
    pub timeout_seconds: Option<u64>,
    pub max_memory_mb: Option<u64>,
    pub capture_output: Option<bool>,
    pub environment: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptMetadataSchema {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub language: ScriptLanguageSchema,
    pub risk_level: Option<ScriptRiskLevelSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptSchema {
    pub metadata: ScriptMetadataSchema,
    pub content: String,
    pub executor: Option<ScriptExecutorConfigSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationResultSchema {
    pub valid: bool,
    pub errors: Option<Vec<String>>,
    pub warnings: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityViolationSchema {
    pub rule: String,
    pub severity: String,
    pub message: String,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityCheckResultSchema {
    pub passed: bool,
    pub violations: Vec<SecurityViolationSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptSecurityPolicySchema {
    pub max_risk_level: Option<ScriptRiskLevelSchema>,
    pub require_review: Option<bool>,
    pub allowed_languages: Option<Vec<ScriptLanguageSchema>>,
    pub blocked_patterns: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEventSchema {
    pub timestamp: i64,
    pub action: String,
    pub script_id: Option<String>,
    pub user: Option<String>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptExecutionResultSchema {
    pub exit_code: i32,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub duration_ms: i64,
    pub memory_used_mb: Option<u64>,
}
