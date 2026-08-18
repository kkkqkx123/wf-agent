use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use wf_types::script::sandbox::{
    AuditEvent, AuditEventType, SandboxConfig, SandboxGlobalConfig, SandboxMode, SandboxPolicy,
    SandboxProfile, ScriptExecutionResult,
};

use crate::default_policy::default_sandbox_policy;
use crate::policy::SandboxPolicyManager;
use crate::profile::{SandboxProfileError, SandboxProfileResolver};
use crate::resolver::{
    analysis_gate_required, DefaultStrategyResolver, StrategyExecuteOptions, StrategyKind,
    StrategyResolver,
};
use crate::vfs::overlay::OverlayVFS;

pub struct SandboxRuntime {
    resolver: Arc<dyn StrategyResolver>,
    default_policy: SandboxPolicy,
    profile_resolver: Option<SandboxProfileResolver>,
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
            profile_resolver: Some(SandboxProfileResolver::default()),
            audit_log: Mutex::new(Vec::new()),
        }
    }

    /// Build a runtime from a global sandbox configuration.
    ///
    /// The configuration is compiled and validated up front (fail-fast):
    /// rules referencing unknown profiles or a missing `default_profile`
    /// are rejected here instead of at script execution time.
    pub fn with_global_config(
        global_config: SandboxGlobalConfig,
    ) -> Result<Self, SandboxProfileError> {
        Ok(Self {
            resolver: Arc::new(DefaultStrategyResolver::with_defaults()),
            default_policy: default_sandbox_policy().clone(),
            profile_resolver: Some(SandboxProfileResolver::compile(global_config)?),
            audit_log: Mutex::new(Vec::new()),
        })
    }

    pub fn with_resolver(resolver: Arc<dyn StrategyResolver>) -> Self {
        Self {
            resolver,
            default_policy: default_sandbox_policy().clone(),
            profile_resolver: Some(SandboxProfileResolver::default()),
            audit_log: Mutex::new(Vec::new()),
        }
    }

    pub fn with_policy(mut self, policy: SandboxPolicy) -> Self {
        self.default_policy = policy;
        self
    }

    /// The runtime's base policy (code-level defaults, pre-merge). Used by
    /// the runtime bootstrap to derive the policy applied to non-script
    /// execution paths (shell tool, CLI executors) through the shared
    /// `cmd` gateway.
    pub fn default_policy(&self) -> &SandboxPolicy {
        &self.default_policy
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
            workdir: config.workdir.clone().or(profile.workdir.clone()),
            env: config.env.clone().or(profile.env.clone()),
            legacy_type: config.legacy_type.clone(),
            resource_limits: config.resource_limits.clone(),
            skip_gate_check: config.skip_gate_check,
        }
    }

    /// Resolve the effective config: explicit `config` wins, then the
    /// profile selected by the first matching global rule, then the global
    /// `default_profile` fills the remaining gaps.
    ///
    /// Infallible: the profile resolver is precompiled and validated when
    /// the runtime is constructed.
    fn resolve_config(
        &self,
        config: &SandboxConfig,
        language: &str,
        script_name: &str,
    ) -> SandboxConfig {
        let mut resolved = config.clone();
        if let Some(resolver) = &self.profile_resolver {
            if let Some(profile) = resolver.resolve(language, script_name) {
                resolved = Self::merge_profile_into_config(profile, &resolved);
            } else if let Some(default_profile) = resolver.default_profile() {
                resolved = Self::merge_profile_into_config(default_profile, &resolved);
            }
        }
        self.apply_legacy_mappings(&mut resolved);
        resolved
    }

    fn apply_legacy_mappings(&self, config: &mut SandboxConfig) {
        match config.legacy_type.as_deref() {
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
            .profile_resolver
            .as_ref()
            .map(|r| r.audit_logging())
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
        self.execute_named(language, "", command, config).await
    }

    /// Execute with a script name so global `rules` can route to a profile by
    /// `script_name` (or `language`). Callers without a script name should
    /// use [`SandboxRuntime::execute`].
    pub async fn execute_named(
        &self,
        language: &str,
        script_name: &str,
        command: &str,
        config: &SandboxConfig,
    ) -> ScriptExecutionResult {
        let resolved_config = self.resolve_config(config, language, script_name);

        // Mode resolution happens AFTER profile merge so a profile-selected
        // mode is honored: config → profile → global config → default → Strict.
        let mode = self.resolve_mode(&resolved_config);

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
                // Associate the event with the first strategy id of the
                // intended chain (explicit config, else the language default).
                let chain_context = preferred_ids.first().cloned().or_else(|| {
                    crate::resolver::default_chain(language)
                        .first()
                        .map(|s| s.to_string())
                });
                self.record_audit(
                    AuditEventType::StrategyFallback,
                    language,
                    &format!("sandbox-{language}"),
                    Some(e.clone()),
                    chain_context,
                    false,
                );
                return self.failed_result(language, &mode, e, None);
            }
        };

        // Gate guarantee: languages with an analysis gate by default must keep
        // at least one Analysis strategy in the chain. Failing closed prevents
        // custom chains (e.g. ["os-hook"]) from silently dropping the
        // pre-execution checks. Explicit skip_gate_check opts out and is
        // always audited so the exemption stays traceable.
        let has_analysis_gate = chain.iter().any(|s| s.kind() == StrategyKind::Analysis);
        let skip_gate = resolved_config.skip_gate_check.unwrap_or(false);
        let gate_warning: Option<String> = if analysis_gate_required(language) && !has_analysis_gate
        {
            if !skip_gate {
                let e = format!(
                    "Strategy chain for language '{language}' has no analysis gate \
                     (Analysis strategies); refusing to run without it. Configure \
                     an analysis strategy (e.g. vfs-gate for shell) or set \
                     skip_gate_check to true to opt out."
                );
                self.record_audit(
                    AuditEventType::ExecutionDenied,
                    language,
                    &format!("sandbox-{language}"),
                    Some(e.clone()),
                    None,
                    false,
                );
                return self.failed_result(language, &mode, e, None);
            }
            let chain_ids: Vec<String> = chain.iter().map(|s| s.id().to_string()).collect();
            let warning = format!(
                "WARNING: analysis gate skipped via skip_gate_check=true; chain {chain_ids:?} \
                 has no Analysis strategy, command-level policy is NOT enforced"
            );
            self.record_audit(
                AuditEventType::StrategyFallback,
                language,
                &format!("sandbox-{language}"),
                Some(warning.clone()),
                None,
                true,
            );
            Some(warning)
        } else {
            None
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

        // VFS path enforcement is the single responsibility of `vfs-gate`
        // (a shell analysis strategy). Whenever VFS is enabled and the chain
        // lacks it, inject it at the head of the chain: this only adds
        // checks, never removes any, so it cannot weaken security.
        let mut chain = chain;
        if vfs.is_some() && !chain.iter().any(|s| s.id() == "vfs-gate") {
            match self.resolver.resolve_shell_strategy("vfs-gate") {
                Some(gate) => {
                    chain.insert(0, gate);
                    self.record_audit(
                        AuditEventType::StrategyFallback,
                        language,
                        &format!("sandbox-{language}"),
                        Some(format!(
                            "VFS enabled: auto-injected 'vfs-gate' into chain {:?}",
                            chain.iter().map(|s| s.id().to_string()).collect::<Vec<_>>()
                        )),
                        Some("vfs-gate".to_string()),
                        true,
                    );
                }
                None => {
                    let e = format!(
                        "VFS is enabled but the 'vfs-gate' strategy is not registered \
                         for language '{language}'; refusing to run without path checks"
                    );
                    self.record_audit(
                        AuditEventType::ExecutionDenied,
                        language,
                        &format!("sandbox-{language}"),
                        Some(e.clone()),
                        None,
                        false,
                    );
                    return self.failed_result(language, &mode, e, None);
                }
            }
        }

        let options = StrategyExecuteOptions {
            command: command.to_string(),
            shell_type: None,
            runtime: None,
            workdir: resolved_config.workdir.clone(),
            env_vars: resolved_config.env.clone(),
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

        // Phase 2: run execution strategies in chain order. The first one
        // that actually produces a result wins. In Strict mode a sandbox
        // layer failure fails fast; in Lenient mode the next available
        // execution strategy is tried instead (no silent downgrade of an
        // executed-but-failed command).
        let mut unavailable: Vec<String> = Vec::new();
        let mut last_strategy_error: Option<(String, String)> = None;
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
            match result {
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
                    return self.finalize_result(res, gate_warning);
                }
                Err(e) => {
                    if mode == SandboxMode::Strict {
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
                            format!("Execution strategy '{}' failed: {e}", s.id()),
                            Some(s.id().to_string()),
                        );
                    }
                    // Lenient: try the next execution strategy.
                    self.record_audit(
                        AuditEventType::StrategyFallback,
                        language,
                        &format!("sandbox-{language}"),
                        Some(format!(
                            "Execution strategy '{}' failed, trying next: {e}",
                            s.id()
                        )),
                        Some(s.id().to_string()),
                        true,
                    );
                    last_strategy_error = Some((s.id().to_string(), e.to_string()));
                }
            }
        }

        let error = if let Some((sid, msg)) = last_strategy_error {
            format!("All execution strategies failed; last error from '{sid}': {msg}")
        } else if unavailable.is_empty() {
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
            Some(error.clone()),
            None,
            false,
        );
        let mut result = self.failed_result(language, &mode, error, None);
        // Lenient: keep the analysis violations on the failed result so no
        // collected information is dropped.
        if mode == SandboxMode::Lenient && !analysis_violations.is_empty() {
            result.violations = Some(analysis_violations.clone());
        }
        self.finalize_result(result, gate_warning)
    }

    /// Resolve the effective mode: config → profile (already merged above) →
    /// global config → default policy → Strict.
    fn resolve_mode(&self, config: &SandboxConfig) -> SandboxMode {
        config
            .mode
            .clone()
            .or_else(|| {
                self.profile_resolver
                    .as_ref()
                    .and_then(|r| r.mode().cloned())
            })
            .or(self.default_policy.mode.clone())
            .unwrap_or(SandboxMode::Strict)
    }

    /// Attach the skip-gate warning (if any) to the returned result so the
    /// exemption is visible to the caller, not just in the audit log.
    fn finalize_result(
        &self,
        mut res: ScriptExecutionResult,
        gate_warning: Option<String>,
    ) -> ScriptExecutionResult {
        if let Some(warning) = gate_warning {
            res.stderr = Some(match res.stderr.take() {
                Some(prev) if !prev.is_empty() => format!("{warning}\n{prev}"),
                _ => warning,
            });
        }
        res
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
            workdir: None,
            env: None,
            legacy_type: None,
            resource_limits: None,
            skip_gate_check: None,
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
            skip_gate_check: Some(true),
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

    #[tokio::test]
    async fn test_gate_guarantee_denies_gateless_shell_chain() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            shell_strategy: Some(vec!["os-hook".to_string()]),
            ..make_config(Some(SandboxMode::Strict))
        };

        let result = runtime.execute("shell", "echo hello", &config).await;
        assert!(
            !result.success,
            "chain without analysis gate must fail closed: {:?}",
            result.error
        );
        let error = result.error.unwrap_or_default();
        assert!(error.contains("no analysis gate"), "error: {error}");
    }

    #[tokio::test]
    async fn test_gate_guarantee_skipped_when_explicit() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            shell_strategy: Some(vec!["os-hook".to_string()]),
            skip_gate_check: Some(true),
            ..make_config(Some(SandboxMode::Strict))
        };

        let result = runtime.execute("shell", "echo hello", &config).await;
        assert!(
            result.success,
            "skip_gate_check must allow gateless chain: {:?}",
            result.stderr
        );
        assert_eq!(result.strategy_id.as_deref(), Some("os-hook"));
    }

    #[tokio::test]
    async fn test_gate_guarantee_js_exempt() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            javascript_strategy: Some(vec!["vm-context".to_string()]),
            ..make_config(Some(SandboxMode::Strict))
        };

        let result = runtime.execute("js", "console.log('hi')", &config).await;
        let reached_execution = result.strategy_id.as_deref() == Some("vm-context")
            || result
                .error
                .as_deref()
                .is_some_and(|e| e.contains("vm-context"));
        assert!(
            reached_execution,
            "gate guarantee must not reject js; it must reach vm-context: {:?}",
            result.error
        );
    }

    #[tokio::test]
    async fn test_global_config_mode_applied_when_unset() {
        let runtime = SandboxRuntime::with_global_config(SandboxGlobalConfig {
            mode: Some(SandboxMode::Disabled),
            ..SandboxGlobalConfig::default()
        })
        .expect("default global config must compile");
        let config = make_config(None);

        let result = runtime.execute("shell", "echo hello", &config).await;
        assert!(
            result.success,
            "global Disabled mode must apply when config/profile unset: {:?}",
            result.error
        );
        assert_eq!(result.sandbox_mode, Some("Disabled".to_string()));
    }

    #[tokio::test]
    async fn test_global_config_mode_does_not_override_explicit() {
        let runtime = SandboxRuntime::with_global_config(SandboxGlobalConfig {
            mode: Some(SandboxMode::Disabled),
            ..SandboxGlobalConfig::default()
        })
        .expect("default global config must compile");
        let config = make_config(Some(SandboxMode::Strict));

        let result = runtime.execute("shell", "echo hello", &config).await;
        assert_eq!(result.sandbox_mode, Some("Strict".to_string()));
    }

    #[tokio::test]
    async fn test_audit_logging_enabled_by_default() {
        let runtime = SandboxRuntime::new();
        let config = make_config(Some(SandboxMode::Strict));

        let result = runtime.execute("shell", "echo hello", &config).await;
        assert!(result.success);
        let log = runtime.get_audit_log();
        assert!(
            !log.is_empty(),
            "audit must be recorded by default (global config audit_logging=true)"
        );
        assert!(log
            .iter()
            .any(|e| e.event_type == AuditEventType::ExecutionAllowed));
    }

    #[tokio::test]
    async fn test_audit_disabled_mode_records_complete_event() {
        let runtime = SandboxRuntime::new();
        let config = make_config(Some(SandboxMode::Disabled));

        let result = runtime.execute("shell", "echo hello", &config).await;
        assert!(result.success);
        let log = runtime.get_audit_log();
        let event = log
            .iter()
            .find(|e| e.event_type == AuditEventType::ExecutionAllowed)
            .expect("disabled direct execution must be audited");
        assert_eq!(event.language, "shell");
        assert_eq!(event.script_name, "direct-exec");
        assert_eq!(event.strategy_id, None);
        assert_eq!(event.violation, None);
        assert!(event.allowed);
    }

    #[tokio::test]
    async fn test_audit_chain_resolution_failure_links_strategy_id() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            python_strategy: Some(vec!["nonexistent-strategy".to_string()]),
            ..make_config(None)
        };

        let result = runtime.execute("python", "print(1)", &config).await;
        assert!(!result.success);
        let log = runtime.get_audit_log();
        let event = log
            .iter()
            .find(|e| e.event_type == AuditEventType::StrategyFallback)
            .expect("chain resolution failure must be audited");
        assert_eq!(
            event.strategy_id.as_deref(),
            Some("nonexistent-strategy"),
            "audit must associate the failing chain's first strategy id"
        );
        assert!(event
            .violation
            .as_deref()
            .unwrap_or_default()
            .contains("unregistered"));
        assert!(!event.allowed);
    }

    fn vfs_config() -> wf_types::script::sandbox::VfsConfig {
        wf_types::script::sandbox::VfsConfig {
            enabled: true,
            storage: None,
            db_path: None,
            workspace_root: None,
            path_policy: Some(wf_types::script::sandbox::PathPolicy {
                allowed_read: vec!["/tmp".to_string()],
                allowed_write: vec!["/tmp".to_string()],
            }),
        }
    }

    #[tokio::test]
    async fn test_vfs_gate_chain_denies_write_outside_policy() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            shell_strategy: Some(vec!["vfs-gate".to_string(), "os-hook".to_string()]),
            vfs: Some(vfs_config()),
            ..make_config(Some(SandboxMode::Strict))
        };

        let result = runtime
            .execute("shell", "echo hi > /etc/shadow", &config)
            .await;
        assert!(
            !result.success,
            "vfs-gate must deny write outside VFS policy: {:?}",
            result.error
        );
        assert_eq!(
            result.strategy_id.as_deref(),
            Some("vfs-gate"),
            "denial must be attributed to vfs-gate"
        );
        assert!(result.error.unwrap().contains("path violation"));
    }

    #[tokio::test]
    async fn test_vfs_gate_chain_allows_write_inside_policy() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            shell_strategy: Some(vec!["vfs-gate".to_string(), "os-hook".to_string()]),
            vfs: Some(vfs_config()),
            ..make_config(Some(SandboxMode::Strict))
        };

        let result = runtime
            .execute("shell", "echo hi > /tmp/ok.txt", &config)
            .await;
        assert!(
            result.success,
            "write under /tmp must be allowed: {:?}",
            result.stderr
        );
        assert_eq!(result.strategy_id.as_deref(), Some("os-hook"));
        let _ = std::fs::remove_file("/tmp/ok.txt");
    }

    #[tokio::test]
    async fn test_vfs_auto_injects_gate_without_vfs_gate() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            shell_strategy: Some(vec!["static-analyzer".to_string(), "os-hook".to_string()]),
            vfs: Some(vfs_config()),
            ..make_config(Some(SandboxMode::Strict))
        };

        let result = runtime
            .execute("shell", "echo hi > /etc/shadow", &config)
            .await;
        assert!(
            !result.success,
            "vfs-gate must be auto-injected when VFS is enabled: {:?}",
            result.error
        );
        assert_eq!(
            result.strategy_id.as_deref(),
            Some("vfs-gate"),
            "denial must be attributed to the injected vfs-gate"
        );
        assert!(result.error.unwrap().contains("path violation"));

        // The injection must be recorded in the audit log.
        let log = runtime.get_audit_log();
        assert!(
            log.iter().any(|e| e
                .violation
                .as_deref()
                .is_some_and(|v| v.contains("auto-injected 'vfs-gate'"))),
            "auto-injection must be audited: {log:?}"
        );
    }

    #[tokio::test]
    async fn test_runtime_rejects_mixed_order_chain() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            shell_strategy: Some(vec!["os-hook".to_string(), "static-analyzer".to_string()]),
            ..make_config(Some(SandboxMode::Strict))
        };

        let result = runtime.execute("shell", "echo hello", &config).await;
        assert!(
            !result.success,
            "execution-before-analysis chain must fail closed: {:?}",
            result.error
        );
        assert!(
            result
                .error
                .unwrap_or_default()
                .contains("after an Execution"),
            "error should explain the shape violation"
        );
    }

    #[tokio::test]
    async fn test_skip_gate_check_is_audited_and_warned() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            shell_strategy: Some(vec!["os-hook".to_string()]),
            skip_gate_check: Some(true),
            ..make_config(Some(SandboxMode::Strict))
        };

        let result = runtime.execute("shell", "echo hello", &config).await;
        assert!(
            result.success,
            "skip_gate_check must allow the gateless chain: {:?}",
            result.stderr
        );
        assert_eq!(result.strategy_id.as_deref(), Some("os-hook"));
        let stderr = result.stderr.unwrap_or_default();
        assert!(
            stderr.contains("skip_gate_check"),
            "exemption warning must be attached to the result: {stderr}"
        );

        let log = runtime.get_audit_log();
        assert!(
            log.iter()
                .any(|e| e.event_type == AuditEventType::StrategyFallback
                    && e.violation
                        .as_deref()
                        .is_some_and(|v| v.contains("skip_gate_check"))),
            "exemption must be audited: {log:?}"
        );
    }

    #[tokio::test]
    async fn test_profile_rule_routes_mode() {
        use wf_types::script::sandbox::{
            SandboxProfile, SandboxProfileRule, SandboxRuleMatchField,
        };

        let global = SandboxGlobalConfig {
            profiles: vec![SandboxProfile {
                name: "lenient".to_string(),
                description: None,
                mode: Some(SandboxMode::Lenient),
                shell_strategy: None,
                python_strategy: None,
                javascript_strategy: None,
                lua_strategy: None,
                policy: None,
                vfs: None,
                workdir: None,
                env: None,
            }],
            rules: vec![SandboxProfileRule {
                match_field: SandboxRuleMatchField::Language,
                match_pattern: "python".to_string(),
                profile: "lenient".to_string(),
            }],
            ..SandboxGlobalConfig::default()
        };
        let runtime = SandboxRuntime::with_global_config(global)
            .expect("rule referencing an existing profile must compile");

        // python matches the rule -> Lenient mode from the profile.
        let config = make_config(None);
        let result = runtime.execute("python", "print(1)", &config).await;
        assert!(
            result.success,
            "lenient profile must allow execution: {:?}",
            result.stderr
        );
        assert_eq!(result.sandbox_mode, Some("Lenient".to_string()));

        // shell does not match -> falls back to the global Strict default.
        let result = runtime.execute("shell", "echo hello", &config).await;
        assert!(result.success);
        assert_eq!(result.sandbox_mode, Some("Strict".to_string()));
    }

    #[test]
    fn test_profile_rule_config_error_fails_fast() {
        use wf_types::script::sandbox::{SandboxProfileRule, SandboxRuleMatchField};

        let global = SandboxGlobalConfig {
            rules: vec![SandboxProfileRule {
                match_field: SandboxRuleMatchField::Language,
                match_pattern: "python".to_string(),
                profile: "does-not-exist".to_string(),
            }],
            ..SandboxGlobalConfig::default()
        };
        let err = match SandboxRuntime::with_global_config(global) {
            Ok(_) => panic!("rule referencing an unknown profile must fail at construction"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("unknown profile"), "error: {err}");
    }

    #[tokio::test]
    async fn test_workdir_and_env_propagate_to_execution() {
        use std::collections::HashMap;

        let runtime = SandboxRuntime::new();
        let tmp = std::env::temp_dir();
        let mut env = HashMap::new();
        env.insert("WF_SANDBOX_TEST".to_string(), "hello-from-env".to_string());
        let config = SandboxConfig {
            python_strategy: Some(vec!["direct".to_string()]),
            skip_gate_check: Some(true),
            workdir: Some(tmp.to_string_lossy().to_string()),
            env: Some(env),
            ..make_config(Some(SandboxMode::Strict))
        };

        let result = runtime
            .execute(
                "python",
                "import os; print(os.environ['WF_SANDBOX_TEST']); print(os.getcwd())",
                &config,
            )
            .await;
        assert!(
            result.success,
            "python direct must receive workdir/env: {:?}",
            result.stderr
        );
        let stdout = result.stdout.unwrap_or_default();
        assert!(stdout.contains("hello-from-env"), "stdout: {stdout}");
        assert!(
            stdout.contains(&tmp.to_string_lossy().to_string()),
            "workdir must be applied: {stdout}"
        );
    }

    #[tokio::test]
    async fn test_workdir_propagates_to_os_hook() {
        let runtime = SandboxRuntime::new();
        let tmp = std::env::temp_dir();
        let config = SandboxConfig {
            shell_strategy: Some(vec!["static-analyzer".to_string(), "os-hook".to_string()]),
            workdir: Some(tmp.to_string_lossy().to_string()),
            ..make_config(Some(SandboxMode::Strict))
        };

        let result = runtime.execute("shell", "pwd", &config).await;
        assert!(
            result.success,
            "os-hook must honor workdir: {:?}",
            result.stderr
        );
        let stdout = result.stdout.unwrap_or_default();
        assert!(
            stdout.contains(&tmp.to_string_lossy().to_string()),
            "workdir must be applied: {stdout}"
        );
    }

    #[tokio::test]
    async fn test_lenient_execution_falls_back_on_strategy_failure() {
        use crate::StrategyImplementation;
        use async_trait::async_trait;

        struct FailingStrategy;

        #[async_trait]
        impl StrategyImplementation for FailingStrategy {
            fn id(&self) -> &str {
                "failing"
            }
            fn name(&self) -> &str {
                "Failing"
            }
            fn description(&self) -> &str {
                "mock strategy that fails with an error"
            }
            fn kind(&self) -> StrategyKind {
                StrategyKind::Execution
            }
            fn is_available(&self) -> bool {
                true
            }
            async fn execute(
                &self,
                _options: StrategyExecuteOptions,
                _policy: &SandboxPolicy,
            ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>>
            {
                Err("mock sandbox failure".into())
            }
        }

        struct PassingGate;

        #[async_trait]
        impl StrategyImplementation for PassingGate {
            fn id(&self) -> &str {
                "pass-gate"
            }
            fn name(&self) -> &str {
                "Passing Gate"
            }
            fn description(&self) -> &str {
                "mock analysis gate that always allows"
            }
            fn kind(&self) -> StrategyKind {
                StrategyKind::Analysis
            }
            fn is_available(&self) -> bool {
                true
            }
            async fn execute(
                &self,
                _options: StrategyExecuteOptions,
                _policy: &SandboxPolicy,
            ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>>
            {
                Ok(ScriptExecutionResult {
                    success: true,
                    script_name: "sandbox-python".to_string(),
                    stdout: None,
                    stderr: None,
                    exit_code: Some(0),
                    execution_time: 0,
                    error: None,
                    sandbox_mode: None,
                    strategy_id: Some("pass-gate".to_string()),
                    violations: None,
                })
            }
        }

        let mut resolver = DefaultStrategyResolver::with_defaults();
        resolver.register_strategy("python", std::sync::Arc::new(PassingGate));
        resolver.register_strategy("python", std::sync::Arc::new(FailingStrategy));
        let runtime = SandboxRuntime::with_resolver(std::sync::Arc::new(resolver));

        // Lenient: the failing execution strategy is skipped, `direct` runs.
        let config = SandboxConfig {
            python_strategy: Some(vec![
                "pass-gate".to_string(),
                "failing".to_string(),
                "direct".to_string(),
            ]),
            ..make_config(Some(SandboxMode::Lenient))
        };
        let result = runtime
            .execute("python", "print('fallback-ok')", &config)
            .await;
        assert!(
            result.success,
            "lenient must fall back to the next execution strategy: {:?}",
            result.error
        );
        assert_eq!(result.strategy_id.as_deref(), Some("direct"));

        // Strict: the first execution strategy failure fails fast.
        let config = SandboxConfig {
            python_strategy: Some(vec![
                "pass-gate".to_string(),
                "failing".to_string(),
                "direct".to_string(),
            ]),
            ..make_config(Some(SandboxMode::Strict))
        };
        let result = runtime
            .execute("python", "print('should-not-run')", &config)
            .await;
        assert!(!result.success, "strict must fail fast");
        assert_eq!(result.strategy_id.as_deref(), Some("failing"));
    }
}
