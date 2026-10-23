use std::sync::Arc;
use std::sync::Mutex;

use serde_json::Value;

use crate::error::{PluginError, PluginResult};

pub fn arm_hook(lua: &mlua::Lua, limits: super::pool::LuaExecutionLimits) -> PluginResult<()> {
    let deadline = limits.timeout.map(|t| std::time::Instant::now() + t);
    let max_kb = limits.memory_limit_kb;
    super::pool::set_protection_hook(lua, deadline, max_kb)
        .map_err(|e| PluginError::LuaError(e.to_string()))
}

pub async fn run_lua_blocking<F, T>(f: F) -> PluginResult<T>
where
    F: FnOnce() -> PluginResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| PluginError::LuaError(format!("lua task panicked: {e}")))?
}

pub fn with_guarded_lua<T>(
    lua: &Arc<Mutex<mlua::Lua>>,
    limits: super::pool::LuaExecutionLimits,
    f: impl FnOnce(&mlua::Lua) -> PluginResult<T>,
) -> PluginResult<T> {
    let start = std::time::Instant::now();
    let locked = lua
        .lock()
        .map_err(|e| PluginError::LuaError(e.to_string()))?;
    arm_hook(&locked, limits)?;
    let outcome = f(&locked);
    locked.remove_hook();
    let elapsed_ms = start.elapsed().as_millis();
    match &outcome {
        Ok(_) => {
            if elapsed_ms > 1_000 {
                tracing::debug!("lua call completed slowly: {}ms", elapsed_ms);
            }
        }
        Err(e) => {
            tracing::warn!("lua call failed after {}ms: {}", elapsed_ms, e);
        }
    }
    outcome
}

pub fn build_lua_context<'lua>(
    lua: &'lua mlua::Lua,
    ctx: &crate::context::PluginContext,
) -> PluginResult<mlua::Table<'lua>> {
    let t = lua
        .create_table()
        .map_err(|e| PluginError::LuaError(format!("create context table: {}", e)))?;
    t.set("plugin_id", ctx.plugin_id.as_str())
        .map_err(|e| PluginError::LuaError(format!("set plugin_id: {}", e)))?;
    t.set("config", to_lua_value(lua, &ctx.config))
        .map_err(|e| PluginError::LuaError(format!("set config: {}", e)))?;
    Ok(t)
}

pub fn to_lua_value<'lua>(lua: &'lua mlua::Lua, value: &Value) -> mlua::Value<'lua> {
    fn try_to_table<'l>(l: &'l mlua::Lua, v: &Value) -> Result<mlua::Value<'l>, mlua::Error> {
        match v {
            Value::Null => Ok(mlua::Value::Nil),
            Value::Bool(b) => Ok(mlua::Value::Boolean(*b)),
            Value::Number(n) => Ok(n
                .as_i64()
                .map(mlua::Value::Integer)
                .unwrap_or_else(|| mlua::Value::Number(n.as_f64().unwrap_or(0.0)))),
            Value::String(s) => Ok(mlua::Value::String(l.create_string(s.as_bytes())?)),
            Value::Array(arr) => {
                let t = l.create_table()?;
                for (i, v) in arr.iter().enumerate() {
                    t.set(i + 1, try_to_table(l, v)?)?;
                }
                Ok(mlua::Value::Table(t))
            }
            Value::Object(map) => {
                let t = l.create_table()?;
                for (k, v) in map {
                    t.set(k.as_str(), try_to_table(l, v)?)?;
                }
                Ok(mlua::Value::Table(t))
            }
        }
    }
    try_to_table(lua, value).unwrap_or(mlua::Value::Nil)
}

pub fn from_lua_value(value: mlua::Value) -> Value {
    fn try_convert(v: mlua::Value) -> Option<Value> {
        match v {
            mlua::Value::Nil => Some(Value::Null),
            mlua::Value::Boolean(b) => Some(Value::Bool(b)),
            mlua::Value::Integer(i) => Some(Value::Number(i.into())),
            mlua::Value::Number(n) => {
                if n.is_nan() || n.is_infinite() {
                    Some(Value::Null)
                } else {
                    Some(Value::Number(
                        serde_json::Number::from_f64(n).unwrap_or(0.into()),
                    ))
                }
            }
            mlua::Value::String(s) => match s.to_str() {
                Ok(s) => Some(Value::String(s.to_owned())),
                Err(_) => Some(Value::Null),
            },
            mlua::Value::Table(t) => {
                let mut is_array = true;
                let mut map = serde_json::Map::new();
                let mut arr: Vec<Value> = Vec::new();
                for pair in t.pairs::<mlua::Value, mlua::Value>() {
                    let (k, v) = match pair {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    let v = try_convert(v).unwrap_or(Value::Null);
                    match k {
                        mlua::Value::Integer(i) if i >= 1 => {
                            let idx = (i - 1) as usize;
                            while arr.len() <= idx {
                                arr.push(Value::Null);
                            }
                            arr[idx] = v;
                        }
                        mlua::Value::String(s) => {
                            is_array = false;
                            if let Ok(s) = s.to_str() {
                                map.insert(s.to_owned(), v);
                            }
                        }
                        _ => is_array = false,
                    }
                }
                if is_array && !arr.is_empty() {
                    Some(Value::Array(arr))
                } else {
                    Some(Value::Object(map))
                }
            }
            _ => Some(Value::Null),
        }
    }
    try_convert(value).unwrap_or(Value::Null)
}

pub fn create_lua_handler_table<'lua>(lua: &'lua mlua::Lua) -> PluginResult<mlua::Table<'lua>> {
    lua.create_table()
        .map_err(|e| PluginError::LuaError(format!("create table: {}", e)))
}

pub fn set_table_str<'lua>(t: &mlua::Table<'lua>, k: &str, v: &str) -> PluginResult<()> {
    t.set(k, v)
        .map_err(|e| PluginError::LuaError(format!("set {k}: {e}")))
}

pub fn set_table_value<'lua>(
    t: &mlua::Table<'lua>,
    k: &str,
    v: mlua::Value<'lua>,
) -> PluginResult<()> {
    t.set(k, v)
        .map_err(|e| PluginError::LuaError(format!("set {k}: {e}")))
}
