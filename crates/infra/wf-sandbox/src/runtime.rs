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
    StrategyResolver, VfsProvider,
};
use crate::vfs::overlay::OverlayVfs;

/// Result of an execution that may involve an overlay VFS.
///
/// `vfs` is the VFS instance that participated in the execution (`None` when
/// no VFS was active). Consuming its delta ([`VfsProvider::take_delta`]) and
/// committing it onto the base directory ([`crate::vfs::overlay::OverlayVfs::flush`])
/// are alternative consumption paths — draining first leaves nothing for a
/// later flush — so the runtime hands out the provider instead of
/// pre-draining and lets hosts pick.
pub struct VfsExecutionOutcome {
    pub result: ScriptExecutionResult,
    pub vfs: Option<Arc<dyn VfsProvider>>,
}

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
        self.execute_named_with_vfs(language, script_name, command, config, None)
            .await
            .result
    }

    /// Execute like [`SandboxRuntime::execute_named`] but with an externally
    /// supplied VFS.
    ///
    /// When `vfs` is provided it takes precedence over the config-derived
    /// overlay (`config.vfs` is not used for provisioning), so a host that
    /// built its own overlay gets exactly that instance through the strategy
    /// options. Without injection the behavior of `execute_named` is preserved
    /// exactly. The outcome reports which VFS participated so hosts can drain
    /// its delta or flush it onto the base directory after execution.
    pub async fn execute_named_with_vfs(
        &self,
        language: &str,
        script_name: &str,
        command: &str,
        config: &SandboxConfig,
        vfs: Option<Arc<dyn VfsProvider>>,
    ) -> VfsExecutionOutcome {
        let (result, used_vfs) = self
            .execute_inner(language, script_name, command, config, vfs)
            .await;
        VfsExecutionOutcome {
            vfs: used_vfs,
            result,
        }
    }

    async fn execute_inner(
        &self,
        language: &str,
        script_name: &str,
        command: &str,
        config: &SandboxConfig,
        external_vfs: Option<Arc<dyn VfsProvider>>,
    ) -> (ScriptExecutionResult, Option<Arc<dyn VfsProvider>>) {
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
            return (result, None);
        }

        let policy = resolved_config
            .policy
            .as_ref()
            .map(|p| SandboxPolicyManager::merge(&self.default_policy, p))
            .unwrap_or_else(|| self.default_policy.clone());

        // External injection wins over config-derived creation; without it an
        // enabled `config.vfs` creates the overlay here as before. The value
        // is built before chain resolution so every early return still
        // reports which VFS (if any) would have participated.
        let vfs =
            external_vfs.or_else(|| {
                let vfs_config = resolved_config.vfs.as_ref()?;
                if !vfs_config.enabled {
                    return None;
                }
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
                Some(Arc::new(OverlayVfs::new(base, path_policy)) as Arc<dyn VfsProvider>)
            });

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
                return (self.failed_result(language, &mode, e, None), vfs.clone());
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
                return (self.failed_result(language, &mode, e, None), vfs.clone());
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
                    return (self.failed_result(language, &mode, e, None), vfs.clone());
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
            vfs: vfs.clone(),
        };

        // Step 1: run all analysis gates in chain order. Strict rejects on
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
                return (
                    self.failed_result(language, &mode, e, Some(s.id().to_string())),
                    vfs.clone(),
                );
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
                        return (res, vfs.clone());
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
                    return (
                        self.failed_result(
                            language,
                            &mode,
                            format!("Analysis strategy '{}' failed: {e}", s.id()),
                            Some(s.id().to_string()),
                        ),
                        vfs.clone(),
                    );
                }
            }
        }

        // Step 2: run execution strategies in chain order. The first one
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
                    return (self.finalize_result(res, gate_warning), vfs.clone());
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
                        return (
                            self.failed_result(
                                language,
                                &mode,
                                format!("Execution strategy '{}' failed: {e}", s.id()),
                                Some(s.id().to_string()),
                            ),
                            vfs.clone(),
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
        (self.finalize_result(result, gate_warning), vfs.clone())
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
#[path = "runtime_test.rs"]
mod runtime_test;
