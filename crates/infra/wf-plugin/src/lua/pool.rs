use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::{PluginError, PluginResult};

pub const DEFAULT_LUA_TIMEOUT: Duration = Duration::from_millis(5_000);
pub const DEFAULT_LUA_MEMORY_LIMIT_KB: usize = 64 * 1024;
pub const LUA_POOL_MAX_STATES: usize = 16;
pub const LUA_HOOK_INTERVAL: u32 = 10_000;

#[derive(Debug, Clone, Copy)]
pub struct LuaExecutionLimits {
    pub timeout: Duration,
    pub memory_limit_kb: usize,
}

impl Default for LuaExecutionLimits {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_LUA_TIMEOUT,
            memory_limit_kb: DEFAULT_LUA_MEMORY_LIMIT_KB,
        }
    }
}

pub fn create_state(script: &str) -> PluginResult<mlua::Lua> {
    let lua = wf_sandbox::strategy::lua::mlua_sandbox::create_restricted_lua()
        .map_err(|e| PluginError::LoadFailed(format!("lua state init failed: {e}")))?;
    wf_sandbox::strategy::lua::mlua_sandbox::apply_plugin_sandbox(&lua)
        .map_err(|e| PluginError::LoadFailed(format!("lua sandbox init failed: {e}")))?;
    lua.load(script)
        .eval::<mlua::Value>()
        .map_err(|e| PluginError::LoadFailed(format!("lua eval error: {e}")))?;
    {
        let value: mlua::Value = lua.globals().get("plugin").map_err(|e| {
            PluginError::LoadFailed(format!("lua plugin must set global 'plugin': {e}"))
        })?;
        if !matches!(value, mlua::Value::Table(_)) {
            return Err(PluginError::LoadFailed(
                "lua plugin must return a table".into(),
            ));
        }
    }
    Ok(lua)
}

pub fn set_protection_hook(
    lua: &mlua::Lua,
    deadline: Instant,
    max_kb: usize,
) -> Result<(), mlua::Error> {
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(LUA_HOOK_INTERVAL),
        move |state, _| {
            if Instant::now() >= deadline {
                return Err(mlua::Error::RuntimeError(
                    "lua execution timed out".to_string(),
                ));
            }
            if state.used_memory() / 1024 > max_kb {
                return Err(mlua::Error::RuntimeError(format!(
                    "lua memory limit exceeded: {} KB",
                    max_kb
                )));
            }
            Ok(())
        },
    );
    Ok(())
}

/// Reusable Lua states for stateless script execution.
/// Keyed handlers keep single-state affinity because registry keys are
/// state-bound; pooling applies to stateless calls and future adoption.
#[allow(dead_code)]
pub struct LuaVmPool {
    script: Arc<String>,
    limits: LuaExecutionLimits,
    states: Mutex<Vec<mlua::Lua>>,
}

impl LuaVmPool {
    pub fn new(script: Arc<String>, limits: LuaExecutionLimits) -> Self {
        Self {
            script,
            limits,
            states: Mutex::new(Vec::new()),
        }
    }

    pub fn acquire(&self) -> PluginResult<mlua::Lua> {
        if let Ok(mut states) = self.states.lock() {
            if let Some(lua) = states.pop() {
                return Ok(lua);
            }
        }
        create_state(&self.script)
    }

    pub fn release(&self, lua: mlua::Lua) {
        lua.remove_hook();
        if let Ok(mut states) = self.states.lock() {
            if states.len() < LUA_POOL_MAX_STATES {
                states.push(lua);
            }
        }
    }

    pub fn run<R, F>(&self, f: F) -> PluginResult<R>
    where
        F: FnOnce(&mlua::Lua) -> PluginResult<R>,
    {
        let lua = self.acquire()?;
        let deadline = Instant::now() + self.limits.timeout;
        set_protection_hook(&lua, deadline, self.limits.memory_limit_kb)
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        let result = f(&lua);
        self.release(lua);
        result
    }
}
