use async_trait::async_trait;
use wf_types::script::sandbox::{LuaPolicy, SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation};

pub struct LuaMluaSandboxStrategy;

impl LuaMluaSandboxStrategy {
    fn create_safe_environment(lua: &mlua::Lua, policy: &LuaPolicy) -> Result<(), mlua::Error> {
        let globals = lua.globals();

        let denied_by_default = ["os", "io", "package", "debug", "ffi"];
        for module in &denied_by_default {
            if policy.denied_modules.contains(&(*module).to_string())
                || policy.denied_modules.is_empty()
            {
                globals.set(*module, mlua::Value::Nil)?;
            }
        }

        let safe_print = lua.create_function(|_, s: String| {
            println!("{}", s);
            Ok(())
        })?;
        globals.set("print", safe_print)?;

        let allowed = policy.allowed_modules.clone();
        let denied = policy.denied_modules.clone();

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
    fn priority(&self) -> i32 {
        100
    }
    fn is_available(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let lua_policy = policy.lua.clone().unwrap_or(LuaPolicy {
            allowed_modules: vec![],
            denied_modules: vec![],
            allow_os_execute: false,
            restrict_io_open: true,
            allow_dynamic_load: false,
        });

        let code = options.command.clone();

        tokio::task::spawn_blocking(move || Self::execute_sync(&code, &lua_policy))
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            .and_then(|r| r)
    }
}
