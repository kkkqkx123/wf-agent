use std::collections::HashMap;

use serde_json::Value;

use super::resolver::{ArgumentResolver, DynamicResolver};
use super::template::ScriptTemplateEngine;
use super::types::{ExecutorMode, ScriptDefinition, ScriptExecutionOptions, ScriptExecutionResult};
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

        let result = execute_command(command, options).await;
        ScriptExecutionResult {
            execution_time_ms: start.elapsed().as_millis() as u64,
            ..result
        }
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
                required: Some(true),
                default: Some(Value::String("hello".to_string())),
                source: None,
                description: None,
            }]),
            language: None,
            executor_mode: None,
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
}
