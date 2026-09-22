use std::collections::HashMap;

use regex::Regex;
use serde_json::Value;

use super::risk::RiskEvaluator;
use super::template::ScriptTemplateEngine;
use super::types::{
    ExecutorMode, ScriptDefinition, ScriptExecutionOptions, ScriptExecutionResult,
    ScriptSecurityPolicy,
};
use crate::error::{ScriptError, ScriptResult};

pub struct ScriptEngine;

/// Uppercase `name` for `WF_INPUT_<NAME>` export, mapping every
/// non-alphanumeric byte to an underscore so the result is a valid env key.
fn sanitize_env_name(name: &str) -> String {
    name.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                b.to_ascii_uppercase() as char
            } else {
                '_'
            }
        })
        .collect()
}

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

    /// Require exactly one of `content` or `template` to be non-blank.
    /// Public so configuration validation shares the same rule instead of
    /// restating the shape check.
    pub fn validate_definition_shape(script: &ScriptDefinition) -> Result<(), ScriptError> {
        let has_content = script
            .content
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());
        let has_template = script
            .template
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());
        match (has_content, has_template) {
            (true, true) => Err(ScriptError::InvalidDefinition(format!(
                "Script '{}' defines both 'content' and 'template': pass exactly one",
                script.name
            ))),
            (false, false) => Err(ScriptError::InvalidDefinition(format!(
                "Script '{}' defines neither 'content' nor 'template': pass exactly one",
                script.name
            ))),
            _ => Ok(()),
        }
    }

    /// Check script against the security policy before execution.
    /// Public so session-based paths gate on the same policy without
    /// duplicating the rule set. This is the early gate on the declared
    /// text; the rendered command is re-checked by `check_final_command`.
    pub fn check_security_policy(
        script: &ScriptDefinition,
        policy: &ScriptSecurityPolicy,
    ) -> Result<(), ScriptError> {
        if let Some(max_size) = policy.max_script_size {
            let content_len = script.content.as_deref().map(str::len).unwrap_or(0);
            let template_len = script.template.as_deref().map(str::len).unwrap_or(0);
            if content_len + template_len > max_size {
                return Err(ScriptError::PolicyDenied(format!(
                    "Script '{}' declared size ({} bytes) exceeds maximum allowed ({} bytes)",
                    script.name,
                    content_len + template_len,
                    max_size
                )));
            }
        }

        if let Some(ref allowed) = policy.allowed_languages {
            if let Some(ref lang) = script.language {
                if !allowed.iter().any(|a| a.eq_ignore_ascii_case(lang)) {
                    return Err(ScriptError::PolicyDenied(format!(
                        "Script '{}' language '{}' is not in allowed languages: [{}]",
                        script.name,
                        lang,
                        allowed.join(", ")
                    )));
                }
            }
        }

        let declared = script.content.as_deref().unwrap_or("").to_owned()
            + script.template.as_deref().unwrap_or("");
        Self::deny_text_patterns(&script.name, &declared, policy)?;

        if policy.allow_dynamic_scripts == Some(false)
            && script.content.is_some()
            && script.template.is_none()
        {
            return Err(ScriptError::PolicyDenied(format!(
                    "Script '{}' is a dynamic script (runtime-generated) and dynamic scripts are not allowed",
                    script.name
                )));
        }

        if policy.require_review == Some(true) {
            return Err(ScriptError::ReviewRequired(format!(
                "Script '{}' requires human review before execution",
                script.name
            )));
        }

        Ok(())
    }

    /// Mandatory gate on the rendered command. Argument interpolation can
    /// introduce blocked content that is invisible in the declared text,
    /// so the final command is checked even when the early gate passed.
    /// Risk scoring stays advisory; isolation is owned by the transport.
    pub fn check_final_command(
        script_name: &str,
        command: &str,
        policy: &ScriptSecurityPolicy,
    ) -> Result<(), ScriptError> {
        Self::deny_text_patterns(script_name, command, policy)?;
        if policy.require_review == Some(true) {
            return Err(ScriptError::ReviewRequired(format!(
                "Script '{script_name}' requires human review before execution"
            )));
        }
        Ok(())
    }

    fn deny_text_patterns(
        script_name: &str,
        text: &str,
        policy: &ScriptSecurityPolicy,
    ) -> Result<(), ScriptError> {
        if let Some(ref blocked) = policy.blocked_patterns {
            for pattern in blocked {
                match Regex::new(pattern) {
                    Ok(re) => {
                        if re.is_match(text) {
                            return Err(ScriptError::PolicyDenied(format!(
                                "Script '{script_name}' contains blocked pattern '{pattern}'"
                            )));
                        }
                    }
                    Err(e) => {
                        return Err(ScriptError::InvalidDefinition(format!(
                            "Invalid blocked pattern '{pattern}' in security policy: {e}"
                        )));
                    }
                }
            }
        }

        if let Some(ref forbidden) = policy.forbidden_commands {
            for cmd in forbidden {
                if Self::contains_command_token(text, cmd) {
                    return Err(ScriptError::PolicyDenied(format!(
                        "Script '{script_name}' contains forbidden command '{cmd}'"
                    )));
                }
            }
        }

        if let Some(ref path_patterns) = policy.forbidden_path_patterns {
            for pattern in path_patterns {
                match Regex::new(pattern) {
                    Ok(re) => {
                        if re.is_match(text) {
                            return Err(ScriptError::PolicyDenied(format!(
                                "Script '{script_name}' contains forbidden path pattern '{pattern}'"
                            )));
                        }
                    }
                    Err(e) => {
                        return Err(ScriptError::InvalidDefinition(format!(
                            "Invalid forbidden path pattern '{pattern}' in security policy: {e}"
                        )));
                    }
                }
            }
        }

        if let Some(ref max_risk) = policy.max_risk_level {
            let actual = RiskEvaluator::evaluate(text);
            if actual.rank() > max_risk.rank() {
                return Err(ScriptError::ReviewRequired(format!(
                    "Script '{script_name}' risk level {actual:?} exceeds maximum allowed {max_risk:?}: heuristic match gates to review, isolation stays with the transport"
                )));
            }
        }

        Ok(())
    }

    /// Token-bounded match so `rm` does not fire on `farm` while `rm -rf`
    /// still fires inside a longer command line.
    fn contains_command_token(haystack: &str, needle: &str) -> bool {
        if needle.is_empty() {
            return false;
        }
        let is_boundary = |b: u8| !(b.is_ascii_alphanumeric() || b == b'_');
        haystack
            .as_bytes()
            .windows(needle.len())
            .enumerate()
            .any(|(i, window)| {
                window == needle.as_bytes()
                    && (i == 0 || is_boundary(haystack.as_bytes()[i - 1]))
                    && (i + needle.len() == haystack.len()
                        || is_boundary(haystack.as_bytes()[i + needle.len()]))
            })
    }

    fn prepare_command(
        &self,
        script: &ScriptDefinition,
        engine_options: &ScriptEngineOptions,
        workdir: Option<&str>,
    ) -> ScriptResult<String> {
        if let Some(ref template) = script.template {
            let args = script.arguments.as_deref().unwrap_or_default();
            ScriptTemplateEngine::render_command_braced_only(
                template,
                args,
                &engine_options.args,
                &engine_options.context_variables,
                workdir,
            )
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

    /// Materialize and bound the payload channels before transport selection:
    /// `stdin_file` is read into `stdin` (capped), inline `stdin` is capped,
    /// the total environment size is capped, and every declared input file is
    /// validated and exported as `WF_INPUT_<NAME>` for the command to read.
    async fn prepare_payload(
        script: &ScriptDefinition,
        merged: &mut ScriptExecutionOptions,
    ) -> Result<(), ScriptError> {
        if let Err(reason) = merged.validate_output_cap() {
            return Err(ScriptError::Payload(format!(
                "Script '{}': {reason}",
                script.name
            )));
        }
        if merged.stdin.is_some() && merged.stdin_file.is_some() {
            return Err(ScriptError::Payload(format!(
                "Script '{}' sets both 'stdin' and 'stdin_file': pass exactly one",
                script.name
            )));
        }
        if let Some(path) = merged.stdin_file.clone() {
            let content = tokio::fs::read(&path).await.map_err(|e| {
                ScriptError::Payload(format!(
                    "Script '{}' cannot read stdin file '{path}': {e}",
                    script.name
                ))
            })?;
            if content.len() > super::payload::MAX_STDIN_BYTES {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' stdin file '{path}' ({} bytes) exceeds the {} byte limit: pass it as an input file instead",
                    script.name,
                    content.len(),
                    super::payload::MAX_STDIN_BYTES
                )));
            }
            merged.stdin = Some(String::from_utf8(content).map_err(|_| {
                ScriptError::Payload(format!(
                    "Script '{}' stdin file '{path}' is not valid UTF-8",
                    script.name
                ))
            })?);
            merged.stdin_file = None;
        }
        if let Some(ref stdin) = merged.stdin {
            if stdin.len() > super::payload::MAX_STDIN_BYTES {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' stdin ({} bytes) exceeds the {} byte limit: pass it as an input file instead",
                    script.name,
                    stdin.len(),
                    super::payload::MAX_STDIN_BYTES
                )));
            }
        }
        if let Some(ref env) = merged.environment {
            let total = super::payload::total_env_bytes(env);
            if total > super::payload::MAX_ENV_BYTES {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' environment ({} bytes) exceeds the {} byte limit: pass large values as input files instead",
                    script.name,
                    total,
                    super::payload::MAX_ENV_BYTES
                )));
            }
        }
        let Some(files) = merged.input_files.clone() else {
            return Ok(());
        };
        let workdir = merged.working_directory.as_deref();
        let env = merged.environment.get_or_insert_with(HashMap::new);
        let mut names: Vec<&String> = files.keys().collect();
        names.sort();
        let mut claimed: std::collections::HashSet<String> = std::collections::HashSet::new();
        for name in names {
            let path = &files[name];
            if let Err(reason) = super::payload::validate_file_arg(path, workdir) {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' input file '{name}': {reason}",
                    script.name
                )));
            }
            // `WF_INPUT_*` is the script data plane (input file paths handed
            // to the child process), not application configuration: it is
            // never read by config resolution and must stay out of any
            // `WF_*` env-override unification.
            let var = format!("WF_INPUT_{}", sanitize_env_name(name));
            if !claimed.insert(var.clone()) {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' input file '{name}' collides after env sanitization: rename one of the colliding inputs",
                    script.name
                )));
            }
            if env.contains_key(&var) {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' input file '{name}' overwrites existing environment '{var}': rename the input or the env key",
                    script.name
                )));
            }
            env.insert(var, path.clone());
        }
        Ok(())
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
