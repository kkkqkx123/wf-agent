//! `ExecuteScript` action execution (legacy sandbox runner + routed transport).

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_types::script::sandbox::ScriptExecutionResult;
use wf_types::trigger::TriggerAction;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::trigger::context::{ScriptRun, TriggerContext};
use crate::handler::trigger::events::emit;
use crate::registry::lookup_script;
use crate::trigger::internal;
use wf_execution_shared::script_router::ScriptRouter;

pub(crate) async fn handle_execute_script(
    action: &TriggerAction,
    ctx: &TriggerContext,
) -> WorkflowResult<Value> {
    let (script_name, parameters, timeout, ignore_error) = match action {
        TriggerAction::ExecuteScript {
            script_name,
            parameters,
            timeout,
            ignore_error,
        } => (
            script_name.clone(),
            parameters.clone(),
            timeout.unwrap_or(0),
            ignore_error.unwrap_or(false),
        ),
        _ => return Err(WorkflowError::Internal("Invalid action type".to_string())),
    };

    let script = match &ctx.script_registry {
        Some(registry) => registry.get(&script_name),
        None => lookup_script(&script_name),
    }
    .ok_or_else(|| {
        WorkflowError::TriggerError(format!(
            "Script '{}' not found in script registry",
            script_name
        ))
    })?;

    emit(
        ctx,
        wf_types::events::EventType::ScriptStarted,
        &format!("trigger_script:{}", script_name),
    )
    .await;

    let language = script
        .language
        .clone()
        .unwrap_or_else(|| "javascript".to_string());
    let context_variables = ctx
        .variables
        .iter()
        .map(|entry| (entry.key().clone(), entry.value().clone()))
        .collect::<HashMap<String, Value>>();
    let provided: HashMap<String, Value> = match &parameters {
        Some(Value::Object(map)) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => HashMap::new(),
    };

    let invocation = ScriptRun {
        script: &script,
        script_name: &script_name,
        language: &language,
        parameters: parameters.as_ref(),
        provided: &provided,
        context_variables: &context_variables,
        timeout,
    };
    if let Some(runner) = ctx.script_runner.clone() {
        let execution_result = execute_legacy(ctx, &invocation, &runner).await?;
        return finish_script_execution(ctx, &script_name, execution_result, ignore_error).await;
    }

    let router = ctx
        .script_router
        .clone()
        .unwrap_or_else(|| Arc::new(ScriptRouter::new()));
    let execution_result = execute_routed(&router, &invocation).await?;
    finish_script_execution(ctx, &script_name, execution_result, ignore_error).await
}

pub(crate) async fn finish_script_execution(
    ctx: &TriggerContext,
    script_name: &str,
    execution_result: ScriptExecutionResult,
    ignore_error: bool,
) -> WorkflowResult<Value> {
    use wf_types::events::EventType;

    if !execution_result.success {
        let stderr = execution_result
            .stderr
            .as_deref()
            .unwrap_or("unknown error")
            .to_string();
        if ignore_error {
            let result = serde_json::json!({
                "success": false,
                "error": stderr,
                "script_name": script_name,
                "execution_time": execution_result.execution_time,
            });
            ctx.variables
                .insert(internal::SCRIPT_RESULT.to_string(), result.clone());
            emit(
                ctx,
                EventType::ScriptCompleted,
                &format!("trigger_script_completed:{}", script_name),
            )
            .await;
            return Ok(result);
        }
        emit(
            ctx,
            EventType::ScriptFailed,
            &format!("trigger_script_failed:{}", script_name),
        )
        .await;
        return Err(WorkflowError::TriggerError(format!(
            "Script '{}' execution failed: {}",
            script_name, stderr
        )));
    }

    let output = execution_result
        .stdout
        .clone()
        .map(Value::String)
        .unwrap_or(Value::Null);
    let parsed = execution_result
        .stdout
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or(output);

    ctx.variables
        .insert(internal::SCRIPT_RESULT.to_string(), parsed.clone());
    if let Some(bus) = &ctx.signal_bus {
        internal::publish_script_result(
            bus,
            ctx.execution_id.clone(),
            ctx.execution_id.clone(),
            parsed.clone(),
        );
    }

    emit(
        ctx,
        EventType::ScriptCompleted,
        &format!("trigger_script_completed:{}", script_name),
    )
    .await;

    Ok(serde_json::json!({
        "success": true,
        "result": parsed,
        "script_name": script_name,
        "execution_time": execution_result.execution_time,
    }))
}

fn is_js_language(language: &str) -> bool {
    matches!(language.to_lowercase().as_str(), "javascript" | "js")
}

fn trigger_sandbox_config() -> wf_types::script::sandbox::SandboxConfig {
    wf_types::script::sandbox::SandboxConfig {
        mode: Some(wf_types::script::sandbox::SandboxMode::Strict),
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
    }
}

fn forced_trigger_mode(
    language: &str,
    declared: Option<wf_script::ExecutorMode>,
) -> WorkflowResult<wf_script::ExecutorMode> {
    match declared {
        Some(
            mode @ (wf_script::ExecutorMode::SandboxShell
            | wf_script::ExecutorMode::SandboxPython
            | wf_script::ExecutorMode::SandboxJavaScript),
        ) => Ok(mode),
        Some(other) => Err(WorkflowError::TriggerError(format!(
            "Script trigger only runs sandboxed modes, got '{other:?}'; declare a sandbox mode instead"
        ))),
        None => Ok(match language.to_lowercase().as_str() {
            "python" => wf_script::ExecutorMode::SandboxPython,
            "javascript" | "js" => wf_script::ExecutorMode::SandboxJavaScript,
            _ => wf_script::ExecutorMode::SandboxShell,
        }),
    }
}

fn trigger_effective_content(
    script: &wf_script::ScriptDefinition,
    language: &str,
    parameters: Option<&Value>,
) -> String {
    let mut code = String::new();
    if is_js_language(language) {
        if let Some(params) = parameters {
            let serialized = serde_json::to_string(params).unwrap_or_else(|_| "null".to_string());
            code.push_str(&format!("const parameters = {};\n", serialized));
        }
    }
    code.push_str(script.content.as_deref().unwrap_or_default());
    code
}

async fn execute_legacy(
    ctx: &TriggerContext,
    run: &ScriptRun<'_>,
    runner: &Arc<dyn crate::handler::trigger::runner::ScriptRunner>,
) -> WorkflowResult<ScriptExecutionResult> {
    use wf_types::events::EventType;

    let ScriptRun {
        script,
        script_name,
        language,
        parameters,
        provided,
        context_variables,
        timeout,
    } = run;
    wf_script::ScriptEngine::validate_definition_shape(script)
        .map_err(|e| WorkflowError::TriggerError(e.to_string()))?;
    if script.interactive.is_some() {
        return Err(WorkflowError::TriggerError(format!(
            "Script '{script_name}' declares interactive input: run it through the interactive session driver instead of the trigger path"
        )));
    }
    if let Some(policy) = script.security_policy.as_ref() {
        wf_script::ScriptEngine::check_security_policy(script, policy)
            .map_err(|e| WorkflowError::TriggerError(e.to_string()))?;
    }
    let mut code = String::new();
    if let Some(template) = script.template.clone() {
        let declarations = script.arguments.clone().unwrap_or_default();
        let rendered = wf_script::ScriptTemplateEngine::render_command_braced_only(
            &template,
            &declarations,
            provided,
            context_variables,
            None,
        )
        .map_err(|e| WorkflowError::TriggerError(e.to_string()))?;
        code.push_str(&rendered);
    } else {
        code.push_str(&trigger_effective_content(script, language, *parameters));
    }
    if let Some(policy) = script.security_policy.as_ref() {
        wf_script::ScriptEngine::check_final_command(script_name, &code, policy)
            .map_err(|e| WorkflowError::TriggerError(e.to_string()))?;
    }
    let sandbox_config = trigger_sandbox_config();
    let execution = runner.execute(language, &code, &sandbox_config);
    if *timeout > 0 {
        match tokio::time::timeout(std::time::Duration::from_millis(*timeout), execution).await {
            Ok(result) => Ok(result),
            Err(_) => {
                emit(
                    ctx,
                    EventType::ScriptFailed,
                    &format!("trigger_script_failed:{script_name}"),
                )
                .await;
                Err(WorkflowError::NodeFailure {
                    node_id: ctx.node_id.clone(),
                    category: wf_types::workflow::error_branch::NodeErrorCategory::TransportTimeout,
                    detail: format!("Script '{script_name}' timed out after {timeout}ms"),
                })
            }
        }
    } else {
        Ok(execution.await)
    }
}

async fn execute_routed(
    router: &Arc<ScriptRouter>,
    run: &ScriptRun<'_>,
) -> WorkflowResult<ScriptExecutionResult> {
    let ScriptRun {
        script,
        script_name,
        language,
        parameters,
        provided,
        context_variables,
        timeout,
    } = run;
    let mut definition = (*script).clone();
    definition.name = script_name.to_string();
    if definition.template.is_none() {
        definition.content = Some(trigger_effective_content(
            &definition,
            language,
            *parameters,
        ));
        definition.arguments = None;
    }
    if definition.language.is_none() {
        definition.language = Some(language.to_string());
    }
    let mode = forced_trigger_mode(language, definition.executor_mode.clone())?;
    definition.executor_mode = Some(mode);
    let options = wf_script::ScriptExecutionOptions {
        executor_mode: definition.executor_mode.clone(),
        working_directory: None,
        environment: None,
        timeout_ms: if *timeout > 0 { Some(*timeout) } else { None },
        retries: None,
        retry_delay_ms: None,
        exponential_backoff: None,
        interactive: None,
        security_policy: None,
        input_files: None,
        stdin: None,
        stdin_file: None,
        max_output_bytes: None,
        output_spill_dir: None,
    };
    let engine_options = wf_script::ScriptEngineOptions {
        args: (*provided).clone(),
        context_variables: (*context_variables).clone(),
    };
    let routed = router
        .run(
            &definition,
            Some(&options),
            &engine_options,
            Some(trigger_sandbox_config()),
        )
        .await;
    if routed.result.requires_review {
        return Err(WorkflowError::TriggerError(
            routed
                .result
                .error
                .unwrap_or_else(|| format!("Script '{script_name}' requires human review")),
        ));
    }
    Ok(ScriptExecutionResult {
        success: routed.result.success,
        script_name: routed.result.script_name,
        stdout: routed.result.stdout,
        stderr: routed.result.stderr,
        exit_code: routed.result.exit_code,
        execution_time: routed.result.execution_time_ms,
        error: routed.result.error,
        sandbox_mode: routed.sandbox_mode,
        strategy_id: routed.strategy_id,
        violations: None,
    })
}
