use super::*;

use crate::error::ScriptResult;
use crate::template::ScriptTemplateEngine;
use crate::types::{ExecutorMode, ScriptDefinition, ScriptExecutionOptions};

impl ScriptEngine {
    pub(super) fn prepare_command(
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
}
