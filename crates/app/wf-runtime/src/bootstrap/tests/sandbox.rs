use super::*;

#[tokio::test]
async fn test_runtime_sandbox_config_valid() {
    clear_env_vars();

    use wf_types::script::sandbox::{
        SandboxConfig, SandboxGlobalConfig, SandboxMode, SandboxProfile, SandboxProfileRule,
        SandboxRuleMatchField,
    };

    // Valid global config: bootstrap succeeds and exposes the compiled
    // shared sandbox runtime; the rule routes shell executions to the
    // Lenient profile.
    let global = SandboxGlobalConfig {
        mode: Some(SandboxMode::Strict),
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
            match_pattern: "shell".to_string(),
            profile: "lenient".to_string(),
        }],
        default_profile: None,
        audit_logging: true,
    };
    let config = RuntimeConfig {
        sandbox: Some(global),
        ..Default::default()
    };
    let runtime = Runtime::bootstrap(config).await.unwrap();
    let result = runtime
        .sandbox_runtime()
        .execute(
            "shell",
            "echo hello",
            &SandboxConfig {
                mode: None,
                policy: None,
                shell_strategy: None,
                python_strategy: None,
                javascript_strategy: None,
                lua_strategy: None,
                vfs: None,
                workdir: None,
                env: None,
                resource_limits: None,
                skip_gate_check: None,
            },
        )
        .await;
    assert!(
        result.success,
        "shared sandbox runtime must execute shell: {:?}",
        result.error
    );
    assert_eq!(
        result.sandbox_mode,
        Some("Lenient".to_string()),
        "rule must route shell to the lenient profile"
    );
    runtime.shutdown().await.unwrap();
    clear_env_vars();
}

#[tokio::test]
async fn test_runtime_sandbox_config_invalid_fails_fast() {
    clear_env_vars();

    use wf_types::script::sandbox::{
        SandboxGlobalConfig, SandboxProfileRule, SandboxRuleMatchField,
    };

    // Invalid config (rule references unknown profile): bootstrap must
    // fail fast instead of deferring the error to script execution.
    let bad = SandboxGlobalConfig {
        rules: vec![SandboxProfileRule {
            match_field: SandboxRuleMatchField::Language,
            match_pattern: "shell".to_string(),
            profile: "does-not-exist".to_string(),
        }],
        ..Default::default()
    };
    let config = RuntimeConfig {
        sandbox: Some(bad),
        ..Default::default()
    };
    let err = match Runtime::bootstrap(config).await {
        Ok(_) => panic!("invalid sandbox global config must fail bootstrap"),
        Err(e) => e,
    };
    assert!(
        err.to_string().contains("Invalid sandbox global config"),
        "error: {err}"
    );
    assert!(err.to_string().contains("unknown profile"), "error: {err}");

    clear_env_vars();
}
