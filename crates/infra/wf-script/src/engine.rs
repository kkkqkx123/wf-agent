use std::collections::HashMap;

use serde_json::Value;

use super::types::{
    ScriptDefinition, ScriptExecutionOptions, ScriptExecutionResult,
};
use crate::error::ScriptError;

pub struct ScriptEngine;

#[derive(Default)]
pub struct ScriptEngineOptions {
    pub args: HashMap<String, Value>,
    pub context_variables: HashMap<String, Value>,
}

mod command;
mod payload;
mod security;
mod validate;

impl ScriptEngine {
    pub async fn execute<F, Fut>(
        &self,
        script: &ScriptDefinition,
        options: Option<&ScriptExecutionOptions>,
        engine_options: &ScriptEngineOptions,
        execute_command: F,
    ) -> ScriptExecutionResult
    where
        F: Fn(String, Option<ScriptExecutionOptions>) -> Fut,
        Fut: std::future::Future<Output = ScriptExecutionResult>,
    {
        let start = std::time::Instant::now();
        let fail = |error: String, requires_review: bool| ScriptExecutionResult {
            success: false,
            script_name: script.name.clone(),
            stdout: None,
            stderr: None,
            exit_code: None,
            execution_time_ms: start.elapsed().as_millis() as u64,
            error: Some(error),
            requires_review,
            truncated: false,
            output_bytes: None,
            stdout_path: None,
            stderr_path: None,
        };

        if script.enabled == Some(false) {
            return fail(format!("Script '{}' is disabled", script.name), false);
        }

        if let Err(e) = Self::validate_definition_shape(script) {
            return fail(e.to_string(), false);
        }

        let security_policy = options
            .and_then(|o| o.security_policy.as_ref())
            .or(script.security_policy.as_ref());
        if let Some(policy) = security_policy {
            match Self::check_security_policy(script, policy) {
                Ok(()) => {}
                Err(ScriptError::ReviewRequired(msg)) => return fail(msg, true),
                Err(e) => return fail(e.to_string(), false),
            }
        }

        let mut merged_owned = options.cloned();
        if let Some(ref mut merged) = merged_owned {
            if merged.interactive.is_none() {
                merged.interactive = script.interactive.clone();
            }
            if merged.interactive.is_some() {
                return fail(
                    format!(
                        "Script '{}' declares interactive input: run it through the interactive session driver instead of the one-shot engine",
                        script.name
                    ),
                    false,
                );
            }
            if let Err(e) = Self::prepare_payload(script, merged).await {
                return fail(e.to_string(), false);
            }
        } else if script.interactive.is_some() {
            return fail(
                format!(
                    "Script '{}' declares interactive input: run it through the interactive session driver instead of the one-shot engine",
                    script.name
                ),
                false,
            );
        }

        let workdir = merged_owned
            .as_ref()
            .and_then(|o| o.working_directory.clone());
        let command = match self.prepare_command(script, engine_options, workdir.as_deref()) {
            Ok(cmd) => cmd,
            Err(e) => return fail(e.to_string(), false),
        };

        if command.is_empty() {
            return fail(
                "No command to execute (empty template or content)".to_string(),
                false,
            );
        }

        if let Some(policy) = security_policy {
            match Self::check_final_command(&script.name, &command, policy) {
                Ok(()) => {}
                Err(ScriptError::ReviewRequired(msg)) => return fail(msg, true),
                Err(e) => return fail(e.to_string(), false),
            }
        }

        let retries = merged_owned.as_ref().and_then(|o| o.retries).unwrap_or(0);
        let timeout_ms = merged_owned.as_ref().and_then(|o| o.timeout_ms);
        let base_delay_ms = merged_owned
            .as_ref()
            .and_then(|o| o.retry_delay_ms)
            .unwrap_or(0);
        let exponential = merged_owned
            .as_ref()
            .and_then(|o| o.exponential_backoff)
            .unwrap_or(false);
        let attempts = retries.saturating_add(1);

        // Outer timeout is authoritative when set; transports keep their own
        // floor (direct defaults to 120s) so an unset outer timeout still
        // bounds execution.
        let mut last_result: Option<ScriptExecutionResult> = None;
        for attempt in 0..attempts {
            let attempt_result = match timeout_ms {
                Some(limit) => {
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(limit),
                        execute_command(command.clone(), merged_owned.clone()),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => {
                            let mut timed_out = fail(
                                format!("Script '{}' timed out after {} ms", script.name, limit),
                                false,
                            );
                            timed_out.execution_time_ms = start.elapsed().as_millis() as u64;
                            timed_out
                        }
                    }
                }
                None => execute_command(command.clone(), merged_owned.clone()).await,
            };
            let succeeded = attempt_result.success;
            last_result = Some(attempt_result);
            if succeeded || attempt + 1 >= attempts {
                break;
            }
            let delay = if exponential {
                base_delay_ms.saturating_mul(1u64 << attempt.min(10))
            } else {
                base_delay_ms
            };
            if delay > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
        }

        let mut result = last_result.expect("retry loop runs at least once");
        result.execution_time_ms = start.elapsed().as_millis() as u64;
        result
    }
}

#[cfg(test)]
mod tests {
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
}