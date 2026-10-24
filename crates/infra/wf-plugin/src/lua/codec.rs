use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::contributions::types::AuthDescriptor;
use crate::contributions::types::CodecHttpRequest;
use crate::contributions::types::PluginLlmAuth;
use crate::contributions::types::PluginLlmCodec;
use crate::contributions::types::PluginLoginChallenge;
use crate::contributions::types::PluginLoginStep;
use crate::error::{PluginError, PluginResult};

pub struct LuaLlmCodec {
    pub lua: Arc<Mutex<mlua::Lua>>,
    pub table_key: Arc<mlua::RegistryKey>,
    pub limits: super::pool::LuaExecutionLimits,
}

fn call_lua_codec_fn(
    lua: &Arc<Mutex<mlua::Lua>>,
    table_key: &Arc<mlua::RegistryKey>,
    limits: super::pool::LuaExecutionLimits,
    func_name: &'static str,
    args: Vec<Value>,
) -> PluginResult<Value> {
    let lua = lua.clone();
    let table_key = table_key.clone();
    let invoke = move || -> PluginResult<Value> {
        super::bridge::with_guarded_lua(&lua, limits, |locked| {
            let table: mlua::Table = locked
                .registry_value(&table_key)
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let func: mlua::Function = table
                .get(func_name)
                .map_err(|e| PluginError::LuaError(format!("codec missing '{func_name}': {e}")))?;
            let lua_args = args
                .iter()
                .map(|v| super::bridge::to_lua_value(locked, v))
                .collect::<Vec<_>>();
            let result: mlua::Value = func
                .call(mlua::Variadic::from_iter(lua_args))
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            Ok(super::bridge::from_lua_value(result))
        })
    };
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            tokio::task::block_in_place(|| handle.block_on(super::bridge::run_lua_blocking(invoke)))
        }
        Err(_) => invoke(),
    }
}

impl PluginLlmCodec for LuaLlmCodec {
    fn build_request(&self, request: Value, profile: Value) -> PluginResult<CodecHttpRequest> {
        let value = call_lua_codec_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "build_request",
            vec![request, profile],
        )?;
        serde_json::from_value(value)
            .map_err(|e| PluginError::LuaError(format!("codec build_request: {e}")))
    }

    fn parse_response(&self, body: &str, request: Value) -> PluginResult<Value> {
        call_lua_codec_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "parse_response",
            vec![Value::String(body.to_owned()), request],
        )
    }

    fn parse_stream_chunk(&self, chunk: &str) -> PluginResult<Option<Value>> {
        let value = call_lua_codec_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "parse_stream_chunk",
            vec![Value::String(chunk.to_owned())],
        )?;
        Ok(if value.is_null() { None } else { Some(value) })
    }

    fn convert_tools(&self, tools: Value) -> PluginResult<Value> {
        call_lua_codec_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "convert_tools",
            vec![tools],
        )
    }

    fn parse_tool_calls(&self, result: Value) -> PluginResult<Value> {
        call_lua_codec_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "parse_tool_calls",
            vec![result],
        )
    }

    fn build_count_tokens_request(
        &self,
        request: Value,
        profile: Value,
    ) -> PluginResult<Option<CodecHttpRequest>> {
        let value = call_lua_codec_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "build_count_tokens_request",
            vec![request, profile],
        )?;
        if value.is_null() {
            return Ok(None);
        }
        serde_json::from_value(value)
            .map(Some)
            .map_err(|e| PluginError::LuaError(format!("codec build_count_tokens_request: {e}")))
    }

    fn parse_count_tokens_response(&self, body: Value) -> PluginResult<u32> {
        let value = call_lua_codec_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "parse_count_tokens_response",
            vec![body],
        )?;
        value.as_u64().map(|n| n as u32).ok_or_else(|| {
            PluginError::LuaError("codec parse_count_tokens_response must return a number".into())
        })
    }
}

pub struct LuaLlmAuth {
    pub lua: Arc<Mutex<mlua::Lua>>,
    pub table_key: Arc<mlua::RegistryKey>,
    pub limits: super::pool::LuaExecutionLimits,
}

fn call_lua_auth_fn(
    lua: &Arc<Mutex<mlua::Lua>>,
    table_key: &Arc<mlua::RegistryKey>,
    limits: super::pool::LuaExecutionLimits,
    func_name: &'static str,
    args: Vec<Value>,
) -> PluginResult<Value> {
    let lua = lua.clone();
    let table_key = table_key.clone();
    let invoke = move || -> PluginResult<Value> {
        super::bridge::with_guarded_lua(&lua, limits, |locked| {
            let table: mlua::Table = locked
                .registry_value(&table_key)
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let func: mlua::Function = table
                .get(func_name)
                .map_err(|e| PluginError::LuaError(format!("auth missing '{func_name}': {e}")))?;
            let lua_args = args
                .iter()
                .map(|v| super::bridge::to_lua_value(locked, v))
                .collect::<Vec<_>>();
            let result: mlua::Value = func
                .call(mlua::Variadic::from_iter(lua_args))
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            Ok(super::bridge::from_lua_value(result))
        })
    };
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            tokio::task::block_in_place(|| handle.block_on(super::bridge::run_lua_blocking(invoke)))
        }
        Err(_) => invoke(),
    }
}

impl PluginLlmAuth for LuaLlmAuth {
    fn describe(&self) -> PluginResult<AuthDescriptor> {
        let value = call_lua_auth_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "describe",
            Vec::new(),
        )?;
        serde_json::from_value(value)
            .map_err(|e| PluginError::LuaError(format!("auth describe: {e}")))
    }

    fn login_start(&self, config: Value) -> PluginResult<PluginLoginChallenge> {
        let value = call_lua_auth_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "login_start",
            vec![config],
        )?;
        serde_json::from_value(value)
            .map_err(|e| PluginError::LuaError(format!("auth login_start: {e}")))
    }

    fn login_poll(&self, session: Value) -> PluginResult<PluginLoginStep> {
        let value = call_lua_auth_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "login_poll",
            vec![session],
        )?;
        serde_json::from_value(value)
            .map_err(|e| PluginError::LuaError(format!("auth login_poll: {e}")))
    }

    fn login_submit(&self, session: Value, input: String) -> PluginResult<PluginLoginStep> {
        let value = call_lua_auth_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "login_submit",
            vec![session, Value::String(input)],
        )?;
        serde_json::from_value(value)
            .map_err(|e| PluginError::LuaError(format!("auth login_submit: {e}")))
    }

    fn refresh(&self, credential: Value) -> PluginResult<Value> {
        call_lua_auth_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "refresh",
            vec![credential],
        )
    }

    fn resolve(&self, credential: Value) -> PluginResult<Value> {
        call_lua_auth_fn(
            &self.lua,
            &self.table_key,
            self.limits,
            "resolve",
            vec![credential],
        )
    }
}
