use std::time::{Duration, Instant};

use wf_plugin_sdk::manifest::{LuaConfig, LUA_DEFAULT_MEMORY_LIMIT_KB, LUA_DEFAULT_TIMEOUT_MS};

use crate::error::{PluginError, PluginResult};

pub const DEFAULT_LUA_TIMEOUT: Duration = Duration::from_millis(LUA_DEFAULT_TIMEOUT_MS);
pub const DEFAULT_LUA_MEMORY_LIMIT_KB: usize = LUA_DEFAULT_MEMORY_LIMIT_KB;
pub const LUA_HOOK_INTERVAL: u32 = 10_000;

#[derive(Debug, Clone, Copy)]
pub struct LuaExecutionLimits {
    /// Per-call wall-clock budget; `None` disables the hook deadline (the
    /// outer `PluginGuard` timeout still applies).
    pub timeout: Option<Duration>,
    pub memory_limit_kb: usize,
}

impl Default for LuaExecutionLimits {
    fn default() -> Self {
        Self {
            timeout: Some(DEFAULT_LUA_TIMEOUT),
            memory_limit_kb: DEFAULT_LUA_MEMORY_LIMIT_KB,
        }
    }
}

/// Resolve lua execution limits with built-in < engine-global < manifest
/// priority. Limits never fail loading: `timeout_ms: Some(0)` disables the
/// hook deadline with a warning, and `memory_limit_kb: Some(0)` falls back
/// to the built-in default with a warning (memory has no outer backstop,
/// so "unlimited" is not expressible).
pub fn resolve_limits(
    manifest: Option<&LuaConfig>,
    engine_defaults: Option<&LuaConfig>,
) -> LuaExecutionLimits {
    let timeout = manifest
        .and_then(|c| c.timeout_ms)
        .or_else(|| engine_defaults.and_then(|c| c.timeout_ms))
        .unwrap_or(LUA_DEFAULT_TIMEOUT_MS);
    let timeout = match timeout {
        0 => {
            tracing::warn!("lua timeout_ms=0 disables the hook deadline; the outer guard timeout still applies");
            None
        }
        ms => Some(Duration::from_millis(ms)),
    };
    let memory_limit_kb = manifest
        .and_then(|c| c.memory_limit_kb)
        .or_else(|| engine_defaults.and_then(|c| c.memory_limit_kb))
        .unwrap_or(LUA_DEFAULT_MEMORY_LIMIT_KB);
    let memory_limit_kb = match memory_limit_kb {
        0 => {
            tracing::warn!(
                "lua memory_limit_kb=0 falls back to the built-in default of {LUA_DEFAULT_MEMORY_LIMIT_KB}KB"
            );
            LUA_DEFAULT_MEMORY_LIMIT_KB
        }
        kb => kb,
    };
    LuaExecutionLimits {
        timeout,
        memory_limit_kb,
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
    deadline: Option<Instant>,
    max_kb: usize,
) -> Result<(), mlua::Error> {
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(LUA_HOOK_INTERVAL),
        move |state, _| {
            if deadline.is_some_and(|d| Instant::now() >= d) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_fall_back_to_builtins() {
        let limits = resolve_limits(None, None);
        assert_eq!(limits.timeout, Some(DEFAULT_LUA_TIMEOUT));
        assert_eq!(limits.memory_limit_kb, DEFAULT_LUA_MEMORY_LIMIT_KB);
    }

    #[test]
    fn manifest_overrides_engine_defaults() {
        let engine = LuaConfig {
            timeout_ms: Some(2000),
            memory_limit_kb: Some(8192),
        };
        let manifest = LuaConfig {
            timeout_ms: Some(500),
            memory_limit_kb: None,
        };
        let limits = resolve_limits(Some(&manifest), Some(&engine));
        assert_eq!(limits.timeout, Some(Duration::from_millis(500)));
        assert_eq!(limits.memory_limit_kb, 8192);
    }

    #[test]
    fn zero_timeout_disables_deadline_and_zero_memory_falls_back() {
        let off = LuaConfig {
            timeout_ms: Some(0),
            memory_limit_kb: None,
        };
        let limits = resolve_limits(Some(&off), None);
        assert_eq!(limits.timeout, None);

        let fallback = LuaConfig {
            timeout_ms: None,
            memory_limit_kb: Some(0),
        };
        let limits = resolve_limits(Some(&fallback), None);
        assert_eq!(limits.timeout, Some(DEFAULT_LUA_TIMEOUT));
        assert_eq!(limits.memory_limit_kb, DEFAULT_LUA_MEMORY_LIMIT_KB);
    }
}
