use std::sync::{Arc, Mutex};

use wf_common::lock::lock_ok;

use async_trait::async_trait;
use serde_json::Value;

use crate::contributions::types::{
    PluginEventHandler, PluginExecutionContext, PluginMiddlewareHandler, PluginNodeHandler,
    PluginNodeResult, PluginToolContext, PluginToolExecutor, PluginToolResult,
};
use crate::contributions::NextFn;
use crate::contributions::types::PluginEventData;
use crate::error::{PluginError, PluginResult};

pub struct LuaNodeHandler {
    pub lua: Arc<Mutex<mlua::Lua>>,
    pub func_key: Arc<mlua::RegistryKey>,
    pub limits: super::pool::LuaExecutionLimits,
}

pub struct LuaToolExecutor {
    pub lua: Arc<Mutex<mlua::Lua>>,
    pub func_key: Arc<mlua::RegistryKey>,
    pub limits: super::pool::LuaExecutionLimits,
}

pub struct LuaEventHandler {
    pub lua: Arc<Mutex<mlua::Lua>>,
    pub func_key: Arc<mlua::RegistryKey>,
    pub limits: super::pool::LuaExecutionLimits,
}

pub struct LuaMiddlewareHandler {
    pub lua: Arc<Mutex<mlua::Lua>>,
    pub func_key: Arc<mlua::RegistryKey>,
    pub limits: super::pool::LuaExecutionLimits,
}

#[async_trait]
impl PluginNodeHandler for LuaNodeHandler {
    async fn execute(&self, ctx: PluginExecutionContext) -> PluginResult<PluginNodeResult> {
        let lua = self.lua.clone();
        let func_key = self.func_key.clone();
        let limits = self.limits;
        super::bridge::run_lua_blocking(move || {
            super::bridge::with_guarded_lua(&lua, limits, |locked| {
                let func: mlua::Function = locked
                    .registry_value(&func_key)
                    .map_err(|e| PluginError::LuaError(e.to_string()))?;
                let ctx_tbl = super::bridge::create_lua_handler_table(locked)?;
                super::bridge::set_table_str(&ctx_tbl, "node_id", &ctx.node_id)?;
                super::bridge::set_table_value(&ctx_tbl, "inputs", super::bridge::to_lua_value(locked, &ctx.inputs))?;
                super::bridge::set_table_value(&ctx_tbl, "config", super::bridge::to_lua_value(locked, &ctx.config))?;
                let result: mlua::Value = func
                    .call(ctx_tbl)
                    .map_err(|e| PluginError::LuaError(e.to_string()))?;
                let o = result
                    .as_table()
                    .and_then(|t| t.get::<&str, mlua::Value>("outputs").ok())
                    .unwrap_or(mlua::Value::Nil);
                Ok(PluginNodeResult {
                    outputs: super::bridge::from_lua_value(o),
                })
            })
        })
        .await
    }
}

#[async_trait]
impl PluginToolExecutor for LuaToolExecutor {
    async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
        let lua = self.lua.clone();
        let func_key = self.func_key.clone();
        let limits = self.limits;
        super::bridge::run_lua_blocking(move || {
            super::bridge::with_guarded_lua(&lua, limits, |locked| {
                let func: mlua::Function = locked
                    .registry_value(&func_key)
                    .map_err(|e| PluginError::LuaError(e.to_string()))?;
                let ctx_tbl = super::bridge::create_lua_handler_table(locked)?;
                super::bridge::set_table_value(&ctx_tbl, "args", super::bridge::to_lua_value(locked, &ctx.args))?;
                let result: mlua::Value = func
                    .call(ctx_tbl)
                    .map_err(|e| PluginError::LuaError(e.to_string()))?;
                let r = result
                    .as_table()
                    .and_then(|t| t.get::<&str, mlua::Value>("result").ok())
                    .unwrap_or(mlua::Value::Nil);
                Ok(PluginToolResult {
                    result: super::bridge::from_lua_value(r),
                })
            })
        })
        .await
    }
}

#[async_trait]
impl PluginEventHandler for LuaEventHandler {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()> {
        let lua = self.lua.clone();
        let func_key = self.func_key.clone();
        let limits = self.limits;
        super::bridge::run_lua_blocking(move || {
            super::bridge::with_guarded_lua(&lua, limits, |locked| {
                let func: mlua::Function = locked
                    .registry_value(&func_key)
                    .map_err(|e| PluginError::LuaError(e.to_string()))?;
                let evt_tbl = super::bridge::create_lua_handler_table(locked)?;
                super::bridge::set_table_str(&evt_tbl, "event_type", &event.event_type)?;
                super::bridge::set_table_value(&evt_tbl, "data", super::bridge::to_lua_value(locked, &event.data))?;
                func.call(evt_tbl)
                    .map_err(|e| PluginError::LuaError(e.to_string()))
            })
        })
        .await
    }
}

#[async_trait]
impl PluginMiddlewareHandler for LuaMiddlewareHandler {
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value> {
        let lua = self.lua.clone();
        let func_key = self.func_key.clone();
        let limits = self.limits;
        let handle = tokio::runtime::Handle::try_current().ok();
        let is_multithread = handle
            .as_ref()
            .is_some_and(|h| h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread);

        let run = move || {
            let locked = lua
                .lock()
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            super::bridge::arm_hook(&locked, limits)?;
            let lua = locked;
            let outcome = (|| -> PluginResult<Value> {
                let func: mlua::Function = lua
                    .registry_value(&func_key)
                    .map_err(|e| PluginError::LuaError(e.to_string()))?;
                let ctx_val = super::bridge::to_lua_value(&lua, &context);
                let incoming = context.clone();
                let next_cell: Arc<Mutex<Option<NextFn>>> =
                    Arc::new(Mutex::new(Some(next)));
                let downstream: Arc<Mutex<Option<Value>>> =
                    Arc::new(Mutex::new(None));
                let wrapper_incoming = incoming.clone();
                let wrapper_next = next_cell.clone();
                let wrapper_downstream = downstream.clone();
                let wrapper_handle = handle.clone();
                let next_wrapper = lua
                    .create_function(move |lua_ctx, args: mlua::Variadic<mlua::Value>| {
                        let next = lock_ok(wrapper_next.lock())
                            .take()
                            .ok_or_else(|| mlua::Error::external("next already called"))?;
                        let replacement = match args.into_iter().next().map(super::bridge::from_lua_value) {
                            None | Some(Value::Null) => wrapper_incoming.clone(),
                            Some(value) => value,
                        };
                        let out = match &wrapper_handle {
                            Some(wrapper_handle) => wrapper_handle.block_on(next(replacement)),
                            None => futures::executor::block_on(next(replacement)),
                        }
                        .map_err(|e| mlua::Error::external(e.to_string()))?;
                        *lock_ok(wrapper_downstream.lock()) = Some(out.clone());
                        Ok(super::bridge::to_lua_value(lua_ctx, &out))
                    })
                    .map_err(|e| PluginError::LuaError(e.to_string()))?;
                let ret: mlua::Value = func
                    .call((ctx_val, next_wrapper))
                    .map_err(|e| PluginError::LuaError(e.to_string()))?;
                let ret_val = super::bridge::from_lua_value(ret);
                let is_envelope = ret_val
                    .as_object()
                    .is_some_and(|o| o.contains_key("proceed") || o.contains_key("context"));
                let downstream = lock_ok(downstream.lock()).take();
                match downstream {
                    Some(down) => {
                        if is_envelope {
                            Ok(crate::contributions::parse_middleware_outcome(&ret_val, &down).context)
                        } else {
                            Ok(down)
                        }
                    }
                    None => {
                        if is_envelope {
                            let outcome = crate::contributions::parse_middleware_outcome(&ret_val, &incoming);
                            if outcome.proceed {
                                let next =
                                    lock_ok(next_cell.lock()).take().expect("next never taken");
                                match &handle {
                                    Some(handle) => handle.block_on(next(outcome.context)),
                                    None => futures::executor::block_on(next(outcome.context)),
                                }
                                .map_err(|e| PluginError::LuaError(e.to_string()))
                            } else {
                                Ok(outcome.context)
                            }
                        } else {
                            match ret_val {
                                Value::Object(_) => Ok(ret_val),
                                _ => Ok(incoming),
                            }
                        }
                    }
                }
            })();
            lua.remove_hook();
            outcome
        };

        if is_multithread {
            tokio::task::block_in_place(run)
        } else {
            run()
        }
    }
}
