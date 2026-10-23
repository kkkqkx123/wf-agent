use std::collections::HashMap;

use serde_json::Value;

use super::types::{ScriptDefinition, ScriptExecutionOptions, ScriptExecutionResult};
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
#[path = "engine/tests.rs"]
mod tests;
