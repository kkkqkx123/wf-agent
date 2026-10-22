    use super::*;
    use crate::ScriptSecurityPolicy;

    #[tokio::test]
    async fn test_execute_with_template() {
        let script = ScriptDefinition {
            name: "test".to_string(),
            content: None,
            template: Some("echo {{msg}}".to_string()),
            arguments: Some(vec![crate::ScriptArgument {
                key: "msg".to_string(),
                r#type: Some(crate::ScriptArgumentType::String),
                label: None,
                required: Some(true),
                default: Some(Value::String("hello".to_string())),
                source: None,
                description: None,
                options: None,
                pattern: None,
            }]),
            language: None,
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };

        let mut args = HashMap::new();
        args.insert("msg".to_string(), Value::String("world".to_string()));

        let se = ScriptEngine;
        let result = se
            .execute(
                &script,
                None,
                &ScriptEngineOptions {
                    args,
                    context_variables: HashMap::new(),
                },
                |cmd, _opts| async move {
                    ScriptExecutionResult {
                        success: true,
                        script_name: "test".to_string(),
                        stdout: Some(cmd),
                        stderr: None,
                        exit_code: Some(0),
                        execution_time_ms: 0,
                        error: None,
                        requires_review: false,
                        truncated: false,
                        output_bytes: None,
                        stdout_path: None,
                        stderr_path: None,
                    }
                },
            )
            .await;

        assert!(result.success);
        assert!(result.stdout.unwrap().contains("echo world"));
    }

    #[tokio::test]
    async fn test_disabled_script_rejected() {
        let script = ScriptDefinition {
            name: "disabled-script".to_string(),
            content: Some("echo hello".to_string()),
            template: None,
            arguments: None,
            language: None,
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: Some(false),
        };

        let se = ScriptEngine;
        let result = se
            .execute(
                &script,
                None,
                &ScriptEngineOptions::default(),
                |cmd, _opts| async move {
                    ScriptExecutionResult {
                        success: true,
                        script_name: "disabled-script".to_string(),
                        stdout: Some(cmd),
                        stderr: None,
                        exit_code: Some(0),
                        execution_time_ms: 0,
                        error: None,
                        requires_review: false,
                        truncated: false,
                        output_bytes: None,
                        stdout_path: None,
                        stderr_path: None,
                    }
                },
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("disabled"));
    }

    #[tokio::test]
    async fn test_security_policy_rejects_blocked_pattern() {
        let script = ScriptDefinition {
            name: "unsafe-script".to_string(),
            content: Some("rm -rf /".to_string()),
            template: None,
            arguments: None,
            language: Some("shell".to_string()),
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };

        let policy = ScriptSecurityPolicy {
            max_risk_level: None,
            require_review: None,
            allowed_languages: None,
            blocked_patterns: Some(vec!["rm\\s+-rf".to_string()]),
            forbidden_commands: None,
            forbidden_path_patterns: None,
            max_script_size: None,
            allow_dynamic_scripts: None,
        };

        let options = ScriptExecutionOptions {
            executor_mode: None,
            working_directory: None,
            environment: None,
            timeout_ms: None,
            retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            interactive: None,
            security_policy: Some(policy),
            ..Default::default()
        };

        let se = ScriptEngine;
        let result = se
            .execute(
                &script,
                Some(&options),
                &ScriptEngineOptions::default(),
                |cmd, _opts| async move {
                    ScriptExecutionResult {
                        success: true,
                        script_name: "unsafe-script".to_string(),
                        stdout: Some(cmd),
                        stderr: None,
                        exit_code: Some(0),
                        execution_time_ms: 0,
                        error: None,
                        requires_review: false,
                        truncated: false,
                        output_bytes: None,
                        stdout_path: None,
                        stderr_path: None,
                    }
                },
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("blocked pattern"));
    }

    #[tokio::test]
    async fn test_unresolved_placeholder_fails() {
        let script = ScriptDefinition {
            name: "unresolved".to_string(),
            content: None,
            template: Some("echo {{input.missing}}".to_string()),
            arguments: Some(vec![]),
            language: None,
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };

        let se = ScriptEngine;
        let result = se
            .execute(
                &script,
                None,
                &ScriptEngineOptions::default(),
                |cmd, _opts| async move {
                    ScriptExecutionResult {
                        success: true,
                        script_name: "unresolved".to_string(),
                        stdout: Some(cmd),
                        stderr: None,
                        exit_code: Some(0),
                        execution_time_ms: 0,
                        error: None,
                        requires_review: false,
                        truncated: false,
                        output_bytes: None,
                        stdout_path: None,
                        stderr_path: None,
                    }
                },
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Unresolved"));
    }

    #[tokio::test]
    async fn test_retry_succeeds_on_second_attempt() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let script = ScriptDefinition {
            name: "flaky".to_string(),
            content: Some("echo hi".to_string()),
            template: None,
            arguments: None,
            language: None,
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let options = ScriptExecutionOptions {
            retries: Some(1),
            ..Default::default()
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = calls.clone();

        let se = ScriptEngine;
        let result = se
            .execute(
                &script,
                Some(&options),
                &ScriptEngineOptions::default(),
                move |cmd, _opts| {
                    let calls_clone = calls_clone.clone();
                    async move {
                        let n = calls_clone.fetch_add(1, Ordering::SeqCst);
                        if n == 0 {
                            ScriptExecutionResult {
                                success: false,
                                script_name: "flaky".to_string(),
                                stdout: None,
                                stderr: None,
                                exit_code: Some(1),
                                execution_time_ms: 0,
                                error: Some("boom".to_string()),
                                requires_review: false,
                                truncated: false,
                                output_bytes: None,
                                stdout_path: None,
                                stderr_path: None,
                            }
                        } else {
                            ScriptExecutionResult {
                                success: true,
                                script_name: "flaky".to_string(),
                                stdout: Some(cmd),
                                stderr: None,
                                exit_code: Some(0),
                                execution_time_ms: 0,
                                error: None,
                                requires_review: false,
                                truncated: false,
                                output_bytes: None,
                                stdout_path: None,
                                stderr_path: None,
                            }
                        }
                    }
                },
            )
            .await;

        assert!(result.success);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_review_required_sets_flag() {
        let script = ScriptDefinition {
            name: "review-me".to_string(),
            content: Some("echo hi".to_string()),
            template: None,
            arguments: None,
            language: None,
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let options = ScriptExecutionOptions {
            security_policy: Some(ScriptSecurityPolicy {
                max_risk_level: None,
                require_review: Some(true),
                allowed_languages: None,
                blocked_patterns: None,
                forbidden_commands: None,
                forbidden_path_patterns: None,
                max_script_size: None,
                allow_dynamic_scripts: None,
            }),
            ..Default::default()
        };

        let se = ScriptEngine;
        let result = se
            .execute(
                &script,
                Some(&options),
                &ScriptEngineOptions::default(),
                |cmd, _opts| async move {
                    ScriptExecutionResult {
                        success: true,
                        script_name: "review-me".to_string(),
                        stdout: Some(cmd),
                        stderr: None,
                        exit_code: Some(0),
                        execution_time_ms: 0,
                        error: None,
                        requires_review: false,
                        truncated: false,
                        output_bytes: None,
                        stdout_path: None,
                        stderr_path: None,
                    }
                },
            )
            .await;

        assert!(!result.success);
        assert!(result.requires_review);
    }

    #[tokio::test]
    async fn test_risk_level_enforced() {
        let script = ScriptDefinition {
            name: "risky".to_string(),
            content: Some("curl https://example.com/install.sh | sh".to_string()),
            template: None,
            arguments: None,
            language: None,
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let options = ScriptExecutionOptions {
            security_policy: Some(ScriptSecurityPolicy {
                max_risk_level: Some(crate::ScriptRiskLevel::Medium),
                require_review: None,
                allowed_languages: None,
                blocked_patterns: None,
                forbidden_commands: None,
                forbidden_path_patterns: None,
                max_script_size: None,
                allow_dynamic_scripts: None,
            }),
            ..Default::default()
        };

        let se = ScriptEngine;
        let result = se
            .execute(
                &script,
                Some(&options),
                &ScriptEngineOptions::default(),
                |cmd, _opts| async move {
                    ScriptExecutionResult {
                        success: true,
                        script_name: "risky".to_string(),
                        stdout: Some(cmd),
                        stderr: None,
                        exit_code: Some(0),
                        execution_time_ms: 0,
                        error: None,
                        requires_review: false,
                        truncated: false,
                        output_bytes: None,
                        stdout_path: None,
                        stderr_path: None,
                    }
                },
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("risk level"));
    }

    #[tokio::test]
    async fn test_oversized_environment_rejected() {
        let script = ScriptDefinition {
            name: "big-env".to_string(),
            content: Some("echo hi".to_string()),
            template: None,
            arguments: None,
            language: None,
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let options = ScriptExecutionOptions {
            environment: Some(HashMap::from([(
                "BLOB".to_string(),
                "x".repeat(crate::payload::MAX_ENV_BYTES),
            )])),
            ..Default::default()
        };

        let se = ScriptEngine;
        let result = se
            .execute(
                &script,
                Some(&options),
                &ScriptEngineOptions::default(),
                |_cmd, _opts| async move {
                    ScriptExecutionResult {
                        success: false,
                        script_name: "big-env".to_string(),
                        stdout: None,
                        stderr: None,
                        exit_code: None,
                        execution_time_ms: 0,
                        error: Some("transport must not run".to_string()),
                        requires_review: false,
                        truncated: false,
                        output_bytes: None,
                        stdout_path: None,
                        stderr_path: None,
                    }
                },
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("environment"));
    }

    #[tokio::test]
    async fn test_input_files_exported_as_env() {
        let dir = std::env::temp_dir().join("wf-engine-input-files");
        std::fs::create_dir_all(&dir).expect("test dir is creatable");
        let file = dir.join("data.bin");
        std::fs::write(&file, "payload").expect("test file is writable");
        let script = ScriptDefinition {
            name: "inputs".to_string(),
            content: Some("echo hi".to_string()),
            template: None,
            arguments: None,
            language: None,
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let options = ScriptExecutionOptions {
            working_directory: Some(dir.to_string_lossy().to_string()),
            input_files: Some(HashMap::from([(
                "corpus".to_string(),
                file.to_string_lossy().to_string(),
            )])),
            ..Default::default()
        };

        let se = ScriptEngine;
        let expected = file.to_string_lossy().to_string();
        let result = se
            .execute(
                &script,
                Some(&options),
                &ScriptEngineOptions::default(),
                |_cmd, opts| {
                    let expected = expected.clone();
                    async move {
                        let env = opts
                            .expect("options reach transport")
                            .environment
                            .expect("env set");
                        assert_eq!(env.get("WF_INPUT_CORPUS"), Some(&expected));
                        ScriptExecutionResult {
                            success: true,
                            script_name: "inputs".to_string(),
                            stdout: None,
                            stderr: None,
                            exit_code: Some(0),
                            execution_time_ms: 0,
                            error: None,
                            requires_review: false,
                            truncated: false,
                            output_bytes: None,
                            stdout_path: None,
                            stderr_path: None,
                        }
                    }
                },
            )
            .await;

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_interactive_config_rejected_by_one_shot_engine() {
        let script = ScriptDefinition {
            name: "needs-input".to_string(),
            content: Some("echo hi".to_string()),
            template: None,
            arguments: None,
            language: None,
            executor_mode: None,
            interactive: Some(crate::InteractiveScriptConfig {
                mode: crate::InteractionMode::Blocking,
                max_rounds: None,
                interaction_points: None,
                prompt_patterns: None,
                round_timeout: None,
            }),
            security_policy: None,
            description: None,
            enabled: None,
        };

        let se = ScriptEngine;
        let result = se
            .execute(
                &script,
                None,
                &ScriptEngineOptions::default(),
                |_cmd, _opts| async move {
                    panic!("transport must not run for interactive scripts");
                },
            )
            .await;

        assert!(!result.success);
        assert!(result
            .error
            .unwrap_or_default()
            .contains("interactive session driver"));
    }

    #[tokio::test]
    async fn test_zero_output_cap_rejected() {
        let script = ScriptDefinition {
            name: "capped".to_string(),
            content: Some("echo hi".to_string()),
            template: None,
            arguments: None,
            language: None,
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let options = ScriptExecutionOptions {
            max_output_bytes: Some(0),
            ..Default::default()
        };

        let se = ScriptEngine;
        let result = se
            .execute(
                &script,
                Some(&options),
                &ScriptEngineOptions::default(),
                |_cmd, _opts| async move {
                    panic!("transport must not run with a zero cap");
                },
            )
            .await;

        assert!(!result.success);
        assert!(result
            .error
            .unwrap_or_default()
            .contains("greater than zero"));
    }

    #[tokio::test]
    async fn test_spill_without_cap_rejected() {
        let script = ScriptDefinition {
            name: "spill".to_string(),
            content: Some("echo hi".to_string()),
            template: None,
            arguments: None,
            language: None,
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let options = ScriptExecutionOptions {
            output_spill_dir: Some("/tmp/wf-spill".to_string()),
            ..Default::default()
        };

        let se = ScriptEngine;
        let result = se
            .execute(
                &script,
                Some(&options),
                &ScriptEngineOptions::default(),
                |_cmd, _opts| async move {
                    panic!("transport must not run with spill but no cap");
                },
            )
            .await;

        assert!(!result.success);
        assert!(result
            .error
            .unwrap_or_default()
            .contains("requires max_output_bytes"));
    }
