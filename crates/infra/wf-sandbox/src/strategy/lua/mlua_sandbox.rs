use async_trait::async_trait;
use wf_types::script::sandbox::{LuaPolicy, SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, StrategyKind};
use wf_common::exec::execute_with_timeout;

pub fn configure_lua_sandbox(lua: &mlua::Lua, policy: &LuaPolicy) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    let denied_by_default = ["os", "io", "package", "debug", "ffi"];
    let denied_modules = policy.denied_modules.as_deref().unwrap_or_default();
    for module in &denied_by_default {
        if denied_modules.contains(&(*module).to_string()) || denied_modules.is_empty() {
            globals.set(*module, mlua::Value::Nil)?;
        }
    }

    let safe_print = lua.create_function(|_, s: String| {
        println!("{}", s);
        Ok(())
    })?;
    globals.set("print", safe_print)?;

    let allowed = policy.allowed_modules.clone().unwrap_or_default();
    let denied = policy.denied_modules.clone().unwrap_or_default();

    let safe_require = lua.create_function(
        move |lua, module_name: String| -> mlua::Result<mlua::Value> {
            if !allowed.is_empty() && !allowed.contains(&module_name) {
                return Err(mlua::Error::RuntimeError(format!(
                    "Module not allowed: {module_name}"
                )));
            }

            if denied.contains(&module_name) {
                return Err(mlua::Error::RuntimeError(format!(
                    "Module denied: {module_name}"
                )));
            }

            match module_name.as_str() {
                "table" | "string" | "math" | "utf8" | "coroutine" => {
                    lua.load(format!("return require('{module_name}')")).eval()
                }
                _ => Err(mlua::Error::RuntimeError(
                    "Module not supported in sandbox".to_string(),
                )),
            }
        },
    )?;
    globals.set("require", safe_require)?;

    Ok(())
}

pub fn apply_plugin_sandbox(lua: &mlua::Lua) -> Result<(), mlua::Error> {
    let denied = ["os", "io", "package", "debug", "ffi"];
    let globals = lua.globals();

    for module in &denied {
        let _ = globals.set(*module, mlua::Value::Nil);
    }

    let safe_print = lua.create_function(|_, s: String| {
        tracing::info!("[lua:print] {}", s);
        Ok(())
    })?;
    globals.set("print", safe_print)?;

    let safe_require = lua.create_function(|_, module_name: String| -> mlua::Result<mlua::Value> {
        Err(mlua::Error::RuntimeError(format!(
            "module '{}' not allowed in plugin sandbox",
            module_name
        )))
    })?;
    globals.set("require", safe_require)?;

    Ok(())
}

pub struct LuaMluaSandboxStrategy;

impl LuaMluaSandboxStrategy {
    fn create_safe_environment(lua: &mlua::Lua, policy: &LuaPolicy) -> Result<(), mlua::Error> {
        configure_lua_sandbox(lua, policy)
    }

    fn execute_sync(
        code: &str,
        policy: &LuaPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();

        if code.is_empty() {
            return Ok(ScriptExecutionResult {
                success: false,
                script_name: "sandbox-lua".to_string(),
                stdout: None,
                stderr: Some("Empty Lua code".to_string()),
                exit_code: Some(1),
                execution_time: 0,
                error: Some("Empty Lua code".to_string()),
                sandbox_mode: None,
                strategy_id: Some("mlua-sandbox".to_string()),
                violations: None,
            });
        }

        let lua = mlua::Lua::new();

        if let Err(e) = Self::create_safe_environment(&lua, policy) {
            return Ok(ScriptExecutionResult {
                success: false,
                script_name: "sandbox-lua".to_string(),
                stdout: None,
                stderr: Some(e.to_string()),
                exit_code: Some(1),
                execution_time: start.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
                sandbox_mode: None,
                strategy_id: Some("mlua-sandbox".to_string()),
                violations: None,
            });
        }

        let eval_result = lua.load(code).eval::<mlua::Value>();
        match eval_result {
            Ok(_) => Ok(ScriptExecutionResult {
                success: true,
                script_name: "sandbox-lua".to_string(),
                stdout: Some(String::new()),
                stderr: None,
                exit_code: Some(0),
                execution_time: start.elapsed().as_millis() as u64,
                error: None,
                sandbox_mode: None,
                strategy_id: Some("mlua-sandbox".to_string()),
                violations: None,
            }),
            Err(e) => Ok(ScriptExecutionResult {
                success: false,
                script_name: "sandbox-lua".to_string(),
                stdout: None,
                stderr: Some(e.to_string()),
                exit_code: Some(1),
                execution_time: start.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
                sandbox_mode: None,
                strategy_id: Some("mlua-sandbox".to_string()),
                violations: None,
            }),
        }
    }
}

#[async_trait]
impl StrategyImplementation for LuaMluaSandboxStrategy {
    fn id(&self) -> &str {
        "mlua-sandbox"
    }
    fn name(&self) -> &str {
        "Lua MLua VM"
    }
    fn description(&self) -> &str {
        "Lua script sandboxing using mlua VM with API-level isolation"
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
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let lua_policy = policy.lua.clone().unwrap_or_default();

        let code = options.command.clone();

        execute_with_timeout(
            async move {
                tokio::task::spawn_blocking(move || Self::execute_sync(&code, &lua_policy))
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                        format!("Task join error: {e}").into()
                    })?
            },
            options.timeout_ms,
        )
        .await
        .map_err(|e| e.into_boxed())
    }
}
