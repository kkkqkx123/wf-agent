use std::collections::HashMap;

use regex::Regex;
use serde_json::Value;

use super::resolver::{ArgumentResolver, DynamicResolver};
use super::template::ScriptTemplateEngine;
use super::types::{
    ExecutorMode, ScriptDefinition, ScriptExecutionOptions, ScriptExecutionResult,
    ScriptSecurityPolicy,
};
use crate::error::ScriptResult;

pub struct ScriptEngine;

#[derive(Default)]
pub struct ScriptEngineOptions {
    pub args: HashMap<String, Value>,
    pub context_variables: HashMap<String, Value>,
}

impl ScriptEngine {
    pub async fn execute<F, Fut>(
        &self,
        script: &ScriptDefinition,
        options: Option<&ScriptExecutionOptions>,
        engine_options: &ScriptEngineOptions,
        execute_command: F,
    ) -> ScriptExecutionResult
    where
        F: FnOnce(String, Option<&ScriptExecutionOptions>) -> Fut,
        Fut: std::future::Future<Output = ScriptExecutionResult>,
    {
        let start = std::time::Instant::now();

        // Check if the script is enabled
        if script.enabled == Some(false) {
            return ScriptExecutionResult {
                success: false,
                script_name: script.name.clone(),
                stdout: None,
                stderr: None,
                exit_code: None,
                execution_time_ms: start.elapsed().as_millis() as u64,
                error: Some(format!("Script '{}' is disabled", script.name)),
            };
        }

        // Security policy check
        let security_policy = options
            .and_then(|o| o.security_policy.as_ref())
            .or(script.security_policy.as_ref());
        if let Some(policy) = security_policy {
            if let Err(e) = Self::check_security_policy(script, policy) {
                return ScriptExecutionResult {
                    success: false,
                    script_name: script.name.clone(),
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    error: Some(e),
                };
            }
        }

        let command = match self.prepare_command(script, engine_options) {
            Ok(cmd) => cmd,
            Err(e) => {
                return ScriptExecutionResult {
                    success: false,
                    script_name: script.name.clone(),
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    error: Some(e.to_string()),
                };
            }
        };

        if command.is_empty() {
            return ScriptExecutionResult {
                success: false,
                script_name: script.name.clone(),
                stdout: None,
                stderr: None,
                exit_code: None,
                execution_time_ms: start.elapsed().as_millis() as u64,
                error: Some("No command to execute (empty template or content)".to_string()),
            };
        }

        // Merge interactive config from options over script definition
        let merged_options = options.map(|o| {
            let mut merged = o.clone();
            if merged.interactive.is_none() {
                merged.interactive = script.interactive.clone();
            }
            merged
        });

        let result = execute_command(command, merged_options.as_ref().or(options)).await;
        ScriptExecutionResult {
            execution_time_ms: start.elapsed().as_millis() as u64,
            ..result
        }
    }

    /// Check script against the security policy before execution.
    fn check_security_policy(
        script: &ScriptDefinition,
        policy: &ScriptSecurityPolicy,
    ) -> Result<(), String> {
        // Check max_script_size
        if let Some(max_size) = policy.max_script_size {
            if let Some(ref content) = script.content {
                if content.len() > max_size {
                    return Err(format!(
                        "Script '{}' content size ({} bytes) exceeds maximum allowed ({} bytes)",
                        script.name,
                        content.len(),
                        max_size
                    ));
                }
            }
        }

        // Check allowed_languages
        if let Some(ref allowed) = policy.allowed_languages {
            if let Some(ref lang) = script.language {
                if !allowed.iter().any(|a| a.eq_ignore_ascii_case(lang)) {
                    return Err(format!(
                        "Script '{}' language '{}' is not in allowed languages: [{}]",
                        script.name,
                        lang,
                        allowed.join(", ")
                    ));
                }
            }
        }

        // Check blocked_patterns in content and template
        if let Some(ref blocked) = policy.blocked_patterns {
            let content = script.content.as_deref().unwrap_or("").to_owned()
                + script.template.as_deref().unwrap_or("");
            for pattern in blocked {
                match Regex::new(pattern) {
                    Ok(re) => {
                        if re.is_match(&content) {
                            return Err(format!(
                                "Script '{}' contains blocked pattern '{}'",
                                script.name, pattern
                            ));
                        }
                    }
                    Err(e) => {
                        return Err(format!(
                            "Invalid blocked pattern '{}' in security policy: {}",
                            pattern, e
                        ));
                    }
                }
            }
        }

        // Check forbidden_commands in content and template
        if let Some(ref forbidden) = policy.forbidden_commands {
            let content = script.content.as_deref().unwrap_or("").to_owned()
                + " "
                + script.template.as_deref().unwrap_or("");
            for cmd in forbidden {
                if content.contains(cmd) {
                    return Err(format!(
                        "Script '{}' contains forbidden command '{}'",
                        script.name, cmd
                    ));
                }
            }
        }

        // Check forbidden_path_patterns (directory traversal, null bytes, etc.)
        if let Some(ref path_patterns) = policy.forbidden_path_patterns {
            let content = script.content.as_deref().unwrap_or("").to_owned()
                + script.template.as_deref().unwrap_or("");
            for pattern in path_patterns {
                match Regex::new(pattern) {
                    Ok(re) => {
                        if re.is_match(&content) {
                            return Err(format!(
                                "Script '{}' contains forbidden path pattern '{}'",
                                script.name, pattern
                            ));
                        }
                    }
                    Err(e) => {
                        return Err(format!(
                            "Invalid forbidden path pattern '{}' in security policy: {}",
                            pattern, e
                        ));
                    }
                }
            }
        }

        // Check allow_dynamic_scripts
        if policy.allow_dynamic_scripts == Some(false)
            && script.content.is_some()
            && script.template.is_none()
        {
            // Dynamic scripts have content but no template
            return Err(format!(
                    "Script '{}' is a dynamic script (runtime-generated) and dynamic scripts are not allowed",
                    script.name
                ));
        }

        Ok(())
    }

    fn prepare_command(
        &self,
        script: &ScriptDefinition,
        engine_options: &ScriptEngineOptions,
    ) -> ScriptResult<String> {
        if let Some(ref template) = script.template {
            let args = script.arguments.as_deref().unwrap_or_default();

            let resolved_args = ArgumentResolver::resolve(
                args,
                &engine_options.args,
                &engine_options.context_variables,
            )?;

            let dynamic_args =
                DynamicResolver::resolve_map(&resolved_args, &engine_options.context_variables);

            let render_result = ScriptTemplateEngine::render(template, &dynamic_args)?;

            Ok(render_result.command)
        } else {
            Ok(script.content.clone().unwrap_or_default())
        }
    }

    pub fn resolve_executor_mode(
        script: &ScriptDefinition,
        options: Option<&ScriptExecutionOptions>,
    ) -> ExecutorMode {
        script
            .executor_mode
            .clone()
            .or_else(|| options.and_then(|o| o.executor_mode.clone()))
            .unwrap_or(ExecutorMode::Direct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    }
                },
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("blocked pattern"));
    }
}
