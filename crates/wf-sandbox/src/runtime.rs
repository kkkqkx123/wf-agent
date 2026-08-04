use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use wf_types::script::sandbox::{
    AuditEvent, AuditEventType, SandboxConfig, SandboxGlobalConfig, SandboxMode, SandboxPolicy,
    SandboxProfile, ScriptExecutionResult,
};

use crate::default_policy::default_sandbox_policy;
use crate::policy::SandboxPolicyManager;
use crate::resolver::{
    DefaultStrategyResolver, StrategyExecuteOptions, StrategyKind, StrategyResolver,
};
use crate::vfs::overlay::OverlayVFS;

pub struct SandboxRuntime {
    resolver: Arc<dyn StrategyResolver>,
    default_policy: SandboxPolicy,
    global_config: Option<SandboxGlobalConfig>,
    audit_log: Mutex<Vec<AuditEvent>>,
}

impl Default for SandboxRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxRuntime {
    pub fn new() -> Self {
        Self {
            resolver: Arc::new(DefaultStrategyResolver::with_defaults()),
            default_policy: default_sandbox_policy().clone(),
            global_config: None,
            audit_log: Mutex::new(Vec::new()),
        }
    }

    pub fn with_global_config(global_config: SandboxGlobalConfig) -> Self {
        Self {
            resolver: Arc::new(DefaultStrategyResolver::with_defaults()),
            default_policy: default_sandbox_policy().clone(),
            global_config: Some(global_config),
            audit_log: Mutex::new(Vec::new()),
        }
    }

    pub fn with_resolver(resolver: Arc<dyn StrategyResolver>) -> Self {
        Self {
            resolver,
            default_policy: default_sandbox_policy().clone(),
            global_config: None,
            audit_log: Mutex::new(Vec::new()),
        }
    }

    pub fn with_policy(mut self, policy: SandboxPolicy) -> Self {
        self.default_policy = policy;
        self
    }

    pub fn get_audit_log(&self) -> Vec<AuditEvent> {
        self.audit_log
            .lock()
            .map(|log| log.clone())
            .unwrap_or_default()
    }

    pub fn clear_audit_log(&self) {
        if let Ok(mut log) = self.audit_log.lock() {
            log.clear();
        }
    }

    fn merge_profile_into_config(
        profile: &SandboxProfile,
        config: &SandboxConfig,
    ) -> SandboxConfig {
        SandboxConfig {
            mode: config.mode.clone().or(profile.mode.clone()),
            policy: config.policy.clone().or(profile.policy.clone()),
            shell_strategy: config
                .shell_strategy
                .clone()
                .or(profile.shell_strategy.clone()),
            python_strategy: config
                .python_strategy
                .clone()
                .or(profile.python_strategy.clone()),
            javascript_strategy: config
                .javascript_strategy
                .clone()
                .or(profile.javascript_strategy.clone()),
            lua_strategy: config.lua_strategy.clone().or(profile.lua_strategy.clone()),
            vfs: config.vfs.clone().or(profile.vfs.clone()),
            legacy_type: config.legacy_type.clone(),
            resource_limits: config.resource_limits.clone(),
        }
    }

    fn resolve_config(&self, config: &SandboxConfig) -> SandboxConfig {
        let mut resolved = config.clone();
        if let Some(ref global) = self.global_config {
            if let Some(ref default_profile_name) = global.default_profile {
                if let Some(profile) = global
                    .profiles
                    .iter()
                    .find(|p| p.name == *default_profile_name)
                {
                    resolved = Self::merge_profile_into_config(profile, &resolved);
                }
            }
        }
        self.apply_legacy_mappings(&mut resolved);
        resolved
    }

    fn apply_legacy_mappings(&self, config: &mut SandboxConfig) {
        match config.legacy_type.as_deref() {
            Some("docker") => {
                config.shell_strategy = Some(vec!["container".to_string()]);
            }
            Some("nodejs") => {
                config.javascript_strategy = Some(vec!["vm-context".to_string()]);
            }
            Some("python") => {
                config.python_strategy =
                    Some(vec!["ast-analyzer".to_string(), "builtin-hook".to_string()]);
            }
            _ => {}
        }
    }

    fn record_audit(
        &self,
        event_type: AuditEventType,
        language: &str,
        script_name: &str,
        violation: Option<String>,
        strategy_id: Option<String>,
        allowed: bool,
    ) {
        let should_record = self
            .global_config
            .as_ref()
            .map(|g| g.audit_logging)
            .unwrap_or(false);
        if !should_record {
            return;
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());
        let event = AuditEvent {
            timestamp,
            event_type,
            language: language.to_string(),
            script_name: script_name.to_string(),
            violation,
            strategy_id,
            allowed,
        };
        if let Ok(mut log) = self.audit_log.lock() {
            log.push(event);
        }
    }

    fn failed_result(
        &self,
        language: &str,
        mode: &SandboxMode,
        error: String,
        strategy_id: Option<String>,
    ) -> ScriptExecutionResult {
        ScriptExecutionResult {
            success: false,
            script_name: format!("sandbox-{language}"),
            stdout: None,
            stderr: Some(error.clone()),
            exit_code: Some(1),
            execution_time: 0,
            error: Some(error),
            sandbox_mode: Some(format!("{:?}", mode)),
            strategy_id,
            violations: None,
        }
    }

    pub async fn execute(
        &self,
        language: &str,
        command: &str,
        config: &SandboxConfig,
    ) -> ScriptExecutionResult {
        let resolved_config = self.resolve_config(config);

        let mode = resolved_config
            .mode
            .clone()
            .or(self.default_policy.mode.clone())
            .unwrap_or(SandboxMode::Strict);

        if mode == SandboxMode::Disabled {
            let result = self.execute_direct(command).await;
            self.record_audit(
                AuditEventType::ExecutionAllowed,
                language,
                "direct-exec",
                None,
                None,
                true,
            );
            return result;
        }

        let policy = resolved_config
            .policy
            .as_ref()
            .map(|p| SandboxPolicyManager::merge(&self.default_policy, p))
            .unwrap_or_else(|| self.default_policy.clone());

        let preferred_ids: Vec<String> = match language {
            "shell" => resolved_config.shell_strategy.clone().unwrap_or_default(),
            "python" => resolved_config.python_strategy.clone().unwrap_or_default(),
            "javascript" | "js" => resolved_config
                .javascript_strategy
                .clone()
                .unwrap_or_default(),
            "lua" => resolved_config.lua_strategy.clone().unwrap_or_default(),
            _ => vec![],
        };

        let chain = match self.resolver.resolve_chain(language, &preferred_ids) {
            Ok(chain) => chain,
            Err(e) => {
                self.record_audit(
                    AuditEventType::StrategyFallback,
                    language,
                    &format!("sandbox-{language}"),
                    Some(e.clone()),
                    None,
                    false,
                );
                return self.failed_result(language, &mode, e, None);
            }
        };

        let vfs = if let Some(ref vfs_config) = resolved_config.vfs {
            if vfs_config.enabled {
                let base = vfs_config
                    .workspace_root
                    .as_ref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| std::env::temp_dir().join("sandbox-vfs"));
                let path_policy = vfs_config.path_policy.clone().unwrap_or(
                    wf_types::script::sandbox::PathPolicy {
                        allowed_read: vec![],
                        allowed_write: vec![],
                    },
                );
                Some(Arc::new(OverlayVFS::new(base, path_policy))
                    as Arc<dyn crate::resolver::VfsProvider>)
            } else {
                None
            }
        } else {
            None
        };

        let options = StrategyExecuteOptions {
            command: command.to_string(),
            shell_type: None,
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: policy.resource.as_ref().and_then(|r| r.timeout_limit_ms),
            vfs,
        };

        // Phase 1: run all analysis gates in chain order. Strict rejects on
        // the first denial; Lenient records violations and continues so the
        // execution layer still runs for real.
        let mut analysis_violations: Vec<String> = Vec::new();
        for s in chain.iter().filter(|s| s.kind() == StrategyKind::Analysis) {
            if !s.is_available() {
                let e = format!(
                    "Analysis strategy '{}' is not available on this platform; refusing to run without the gate",
                    s.id()
                );
                self.record_audit(
                    AuditEventType::ExecutionDenied,
                    language,
                    &format!("sandbox-{language}"),
                    Some(e.clone()),
                    Some(s.id().to_string()),
                    false,
                );
                return self.failed_result(language, &mode, e, Some(s.id().to_string()));
            }
            match s.execute(options.clone(), &policy).await {
                Ok(res) if !res.success => {
                    if mode == SandboxMode::Strict {
                        let mut res = res;
                        res.sandbox_mode = Some(format!("{:?}", mode));
                        self.record_audit(
                            AuditEventType::ExecutionDenied,
                            language,
                            &res.script_name,
                            res.error.clone(),
                            Some(s.id().to_string()),
                            false,
                        );
                        return res;
                    }
                    let new_violations: Vec<String> = res
                        .violations
                        .clone()
                        .or_else(|| res.error.clone().map(|e| vec![e]))
                        .unwrap_or_default();
                    analysis_violations.extend(new_violations);
                }
                Ok(_) => {}
                Err(e) => {
                    self.record_audit(
                        AuditEventType::ExecutionDenied,
                        language,
                        &format!("sandbox-{language}"),
                        Some(e.to_string()),
                        Some(s.id().to_string()),
                        false,
                    );
                    return self.failed_result(
                        language,
                        &mode,
                        format!("Analysis strategy '{}' failed: {e}", s.id()),
                        Some(s.id().to_string()),
                    );
                }
            }
        }

        // Phase 2: run the first available execution strategy in chain order.
        let mut unavailable: Vec<String> = Vec::new();
        for s in chain.iter().filter(|s| s.kind() == StrategyKind::Execution) {
            if !s.is_available() {
                unavailable.push(s.id().to_string());
                self.record_audit(
                    AuditEventType::StrategyFallback,
                    language,
                    &format!("sandbox-{language}"),
                    Some(format!(
                        "Execution strategy '{}' unavailable, skipping",
                        s.id()
                    )),
                    Some(s.id().to_string()),
                    true,
                );
                continue;
            }

            let result = s.execute(options.clone(), &policy).await;
            return match result {
                Ok(mut res) => {
                    res.sandbox_mode = Some(format!("{:?}", mode));
                    res.strategy_id = Some(s.id().to_string());
                    if mode == SandboxMode::Lenient && !analysis_violations.is_empty() {
                        let mut all = analysis_violations.clone();
                        if let Some(mut existing) = res.violations.take() {
                            all.append(&mut existing);
                        }
                        res.violations = Some(all);
                    }

                    if !res.success {
                        self.record_audit(
                            AuditEventType::ExecutionDenied,
                            language,
                            &res.script_name,
                            res.error.clone(),
                            Some(s.id().to_string()),
                            false,
                        );
                    } else if res.violations.is_some() {
                        self.record_audit(
                            AuditEventType::ExecutionViolation,
                            language,
                            &res.script_name,
                            res.violations.as_ref().and_then(|v| v.first().cloned()),
                            Some(s.id().to_string()),
                            true,
                        );
                    } else {
                        self.record_audit(
                            AuditEventType::ExecutionAllowed,
                            language,
                            &res.script_name,
                            None,
                            Some(s.id().to_string()),
                            true,
                        );
                    }
                    res
                }
                Err(e) => {
                    self.record_audit(
                        AuditEventType::ExecutionDenied,
                        language,
                        &format!("sandbox-{language}"),
                        Some(e.to_string()),
                        Some(s.id().to_string()),
                        false,
                    );
                    self.failed_result(
                        language,
                        &mode,
                        format!("Execution strategy '{}' failed: {e}", s.id()),
                        Some(s.id().to_string()),
                    )
                }
            };
        }

        let e = if unavailable.is_empty() {
            format!("No execution strategy in chain for language: {language}")
        } else {
            format!(
                "No available execution strategy for language '{language}': {} unavailable",
                unavailable.join(", ")
            )
        };
        self.record_audit(
            AuditEventType::StrategyFallback,
            language,
            &format!("sandbox-{language}"),
            Some(e.clone()),
            None,
            false,
        );
        self.failed_result(language, &mode, e, None)
    }

    async fn execute_direct(&self, command: &str) -> ScriptExecutionResult {
        let start = std::time::Instant::now();

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .await;

        match output {
            Ok(out) => ScriptExecutionResult {
                success: out.status.success(),
                script_name: "direct-exec".to_string(),
                stdout: Some(String::from_utf8_lossy(&out.stdout).to_string()),
                stderr: Some(String::from_utf8_lossy(&out.stderr).to_string()),
                exit_code: out.status.code(),
                execution_time: start.elapsed().as_millis() as u64,
                error: if out.status.success() {
                    None
                } else {
                    Some("Command failed".to_string())
                },
                sandbox_mode: Some("Disabled".to_string()),
                strategy_id: None,
                violations: None,
            },
            Err(e) => ScriptExecutionResult {
                success: false,
                script_name: "direct-exec".to_string(),
                stdout: None,
                stderr: Some(e.to_string()),
                exit_code: None,
                execution_time: start.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
                sandbox_mode: Some("Disabled".to_string()),
                strategy_id: None,
                violations: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::script::sandbox::{ResourcePolicy, ShellPolicy};

    fn make_config(mode: Option<SandboxMode>) -> SandboxConfig {
        SandboxConfig {
            mode,
            policy: None,
            shell_strategy: None,
            python_strategy: None,
            javascript_strategy: None,
            lua_strategy: None,
            vfs: None,
            legacy_type: None,
            resource_limits: None,
        }
    }

    #[tokio::test]
    async fn test_disabled_mode_executes_directly() {
        let runtime = SandboxRuntime::new();
        let config = make_config(Some(SandboxMode::Disabled));

        let result = runtime.execute("shell", "echo hello", &config).await;
        assert!(result.success);
        assert_eq!(result.sandbox_mode, Some("Disabled".to_string()));
    }

    #[tokio::test]
    async fn test_strict_default_chain_denies_rm_rf() {
        let runtime = SandboxRuntime::new();
        let config = make_config(Some(SandboxMode::Strict));

        let result = runtime.execute("shell", "rm -rf /", &config).await;
        assert!(
            !result.success,
            "static-analyzer must gate rm -rf before seccomp sees it"
        );
        assert_eq!(result.strategy_id.as_deref(), Some("static-analyzer"));
        assert!(result.violations.is_some());
    }

    #[tokio::test]
    async fn test_strict_default_chain_allows_safe_command() {
        let runtime = SandboxRuntime::new();
        let config = make_config(Some(SandboxMode::Strict));

        let result = runtime.execute("shell", "echo hello", &config).await;
        assert!(
            result.success,
            "safe command should execute: {:?}",
            result.stderr
        );
        assert_eq!(result.strategy_id.as_deref(), Some("os-hook"));
        assert!(result.stdout.unwrap_or_default().contains("hello"));
    }

    #[tokio::test]
    async fn test_lenient_records_violation_but_executes_for_real() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            mode: Some(SandboxMode::Lenient),
            policy: Some(SandboxPolicy {
                mode: Some(SandboxMode::Lenient),
                shell: Some(ShellPolicy {
                    allowed_commands: None,
                    denied_commands: None,
                    dangerous_patterns: Some(vec!["MARKER123".to_string()]),
                    allow_pipe: None,
                    allow_redirect: None,
                }),
                ..default_sandbox_policy().clone()
            }),
            ..make_config(None)
        };

        let result = runtime.execute("shell", "echo MARKER123", &config).await;
        assert!(
            result.success,
            "lenient must still run the command: {:?}",
            result.stderr
        );
        assert_eq!(result.strategy_id.as_deref(), Some("os-hook"));
        assert!(result.stdout.unwrap_or_default().contains("MARKER123"));
        let violations = result.violations.unwrap_or_default();
        assert!(
            violations.iter().any(|v| v.contains("MARKER123")),
            "violations must be recorded: {violations:?}"
        );
    }

    #[tokio::test]
    async fn test_timeout_policy_enforced() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            mode: Some(SandboxMode::Strict),
            policy: Some(SandboxPolicy {
                mode: Some(SandboxMode::Strict),
                resource: Some(ResourcePolicy {
                    cpu_limit_ms: None,
                    memory_limit_mb: None,
                    disk_limit_mb: None,
                    timeout_limit_ms: Some(100),
                }),
                ..default_sandbox_policy().clone()
            }),
            python_strategy: Some(vec!["direct".to_string()]),
            ..make_config(None)
        };

        let result = runtime
            .execute("python", "import time; time.sleep(5)", &config)
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("timed out"));
    }

    #[tokio::test]
    async fn test_fail_closed_on_unknown_strategy() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            python_strategy: Some(vec!["nonexistent-strategy".to_string()]),
            ..make_config(None)
        };

        let result = runtime.execute("python", "print(1)", &config).await;
        assert!(!result.success);
        assert!(
            result.error.unwrap_or_default().contains("unregistered"),
            "must report the missing strategy id"
        );
    }

    #[tokio::test]
    async fn test_default_chain_denies_python_os_import_in_strict() {
        let runtime = SandboxRuntime::new();
        let config = make_config(Some(SandboxMode::Strict));

        let result = runtime.execute("python", "import os", &config).await;
        assert!(
            !result.success,
            "ast-analyzer must gate import os: {:?}",
            result.stderr
        );
        assert_eq!(result.strategy_id.as_deref(), Some("ast-analyzer"));
    }
}
