use super::*;
use crate::resolver::StrategyImplementation;
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
    use wf_types::script::sandbox::{SandboxProfile, SandboxProfileRule, SandboxRuleMatchField};

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
    use crate::resolver::StrategyImplementation;
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
        ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
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
        ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
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

/// Mock executor that records which VFS instance it received and writes
/// one file through it.
struct VfsCaptureWriter {
    captured_vfs: Arc<Mutex<Option<Arc<dyn VfsProvider>>>>,
}

#[async_trait::async_trait]
impl StrategyImplementation for VfsCaptureWriter {
    fn id(&self) -> &str {
        "vfs-capture-writer"
    }
    fn name(&self) -> &str {
        "VFS Capture Writer"
    }
    fn description(&self) -> &str {
        "records the injected VFS and writes one file through it"
    }
    fn kind(&self) -> StrategyKind {
        StrategyKind::Execution
    }
    fn is_available(&self) -> bool {
        true
    }
    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        _policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        *self.captured_vfs.lock().expect("capture lock") = options.vfs.clone();
        if let Some(vfs) = &options.vfs {
            vfs.write_file("f6-delta.txt", b"injected-write".to_vec())
                .await?;
        }
        Ok(ScriptExecutionResult {
            success: true,
            script_name: "sandbox-test".to_string(),
            stdout: None,
            stderr: None,
            exit_code: Some(0),
            execution_time: 0,
            error: None,
            sandbox_mode: None,
            strategy_id: Some("vfs-capture-writer".to_string()),
            violations: None,
        })
    }
}

fn injection_config() -> SandboxConfig {
    // Config enables its own VFS too: external injection must win over
    // the internally created overlay.
    SandboxConfig {
        python_strategy: Some(vec!["vfs-capture-writer".to_string()]),
        vfs: Some(vfs_config()),
        skip_gate_check: Some(true),
        ..make_config(Some(SandboxMode::Strict))
    }
}

fn injected_overlay(dir: &std::path::Path) -> (Arc<OverlayVfs>, Arc<dyn VfsProvider>) {
    let overlay = Arc::new(OverlayVfs::new(
        dir.to_path_buf(),
        wf_types::script::sandbox::PathPolicy {
            allowed_read: vec!["f6".to_string()],
            allowed_write: vec!["f6".to_string()],
        },
    ));
    let provider: Arc<dyn VfsProvider> = overlay.clone();
    (overlay, provider)
}

#[tokio::test]
async fn test_execute_named_with_vfs_uses_injected_instance_and_returns_delta() {
    let captured: Arc<Mutex<Option<Arc<dyn VfsProvider>>>> = Arc::new(Mutex::new(None));
    let mut resolver = DefaultStrategyResolver::with_defaults();
    resolver.register_strategy(
        "python",
        Arc::new(VfsCaptureWriter {
            captured_vfs: captured.clone(),
        }),
    );
    let runtime = SandboxRuntime::with_resolver(Arc::new(resolver));

    let dir = std::env::temp_dir().join("vfs-runtime-inject");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let (_overlay, injected) = injected_overlay(&dir);

    let outcome = runtime
        .execute_named_with_vfs(
            "python",
            "vfs-inject",
            "write",
            &injection_config(),
            Some(injected.clone()),
        )
        .await;

    assert!(outcome.result.success, "{:?}", outcome.result.error);
    let received = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("strategy must receive a VFS");
    assert!(
        Arc::ptr_eq(&received, &injected),
        "strategies must run against the externally injected instance"
    );

    // Delta-based consumption: drain the writes from the active VFS.
    let used_vfs = outcome.vfs.expect("an active VFS must be reported");
    assert!(
        Arc::ptr_eq(&used_vfs, &injected),
        "outcome must report the injected instance"
    );
    let delta = used_vfs.take_delta();
    assert_eq!(
        delta
            .get(PathBuf::from("f6-delta.txt").as_path())
            .map(|v| v.as_slice()),
        Some(b"injected-write".as_slice())
    );
    assert!(
        used_vfs.take_delta().is_empty(),
        "drain must clear pending writes so repeated calls do not re-report"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_injected_vfs_flush_commits_writes_onto_base() {
    let captured: Arc<Mutex<Option<Arc<dyn VfsProvider>>>> = Arc::new(Mutex::new(None));
    let mut resolver = DefaultStrategyResolver::with_defaults();
    resolver.register_strategy(
        "python",
        Arc::new(VfsCaptureWriter {
            captured_vfs: captured.clone(),
        }),
    );
    let runtime = SandboxRuntime::with_resolver(Arc::new(resolver));

    let dir = std::env::temp_dir().join("vfs-runtime-flush");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let (overlay, injected) = injected_overlay(&dir);

    // Flush-based consumption: the caller keeps its typed handle and
    // commits the sandbox view onto the base directory after execution.
    let outcome = runtime
        .execute_named_with_vfs(
            "python",
            "vfs-flush",
            "write",
            &injection_config(),
            Some(injected.clone()),
        )
        .await;
    assert!(outcome.result.success, "{:?}", outcome.result.error);

    overlay.flush().await.expect("flush must commit");
    let committed = tokio::fs::read(dir.join("f6-delta.txt")).await.unwrap();
    assert_eq!(committed, b"injected-write");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_execute_named_with_vfs_none_keeps_legacy_no_vfs_path() {
    let runtime = SandboxRuntime::new();
    let config = make_config(Some(SandboxMode::Strict));

    let outcome = runtime
        .execute_named_with_vfs("shell", "", "echo hello", &config, None)
        .await;
    assert!(
        outcome.result.success,
        "no-vfs execution must be unchanged: {:?}",
        outcome.result.error
    );
    assert!(
        outcome.vfs.is_none(),
        "no VFS configured or injected must report no active VFS"
    );
}
