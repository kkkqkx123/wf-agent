use std::sync::Arc;
use std::sync::Mutex;

use wf_common::lock::lock_ok;

use async_trait::async_trait;
use serde_json::Value;

use wf_types::MiddlewarePhase;

use crate::context::PluginContext;
use crate::contributions::registrar::ContributionRegistrar;
use crate::contributions::types::*;
use crate::contributions::NextFn;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;

pub struct LuaPlugin {
    manifest: PluginManifest,
    lua: Arc<Mutex<mlua::Lua>>,
}

impl LuaPlugin {
    pub fn new(manifest: PluginManifest, lua: mlua::Lua) -> Self {
        Self {
            manifest,
            lua: Arc::new(Mutex::new(lua)),
        }
    }

    /// Run a named plugin lifecycle hook on the blocking pool so a slow or
    /// infinite Lua hook never pins a tokio worker (and the `PluginGuard`
    /// timeout can actually fire).
    async fn call_hook(&self, hook_name: &'static str, ctx: PluginContext) -> PluginResult<()> {
        let lua = self.lua.clone();
        let manifest_id = self.manifest.id.clone();
        run_lua_blocking(move || {
            let lua = lua
                .lock()
                .map_err(|e| PluginError::LuaError(e.to_string()))?;

            let plugin_table: mlua::Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::LuaError(e.to_string()))?;

            let hook: mlua::Function = match plugin_table.get(hook_name) {
                Ok(f) => f,
                Err(_) => {
                    tracing::debug!("lua plugin '{}' has no hook '{}'", manifest_id, hook_name);
                    return Ok(());
                }
            };

            let ctx_tbl = build_lua_context(&lua, &ctx)?;
            hook.call::<_, ()>(ctx_tbl)
                .map_err(|e| PluginError::LuaError(e.to_string()))
        })
        .await
    }
}

/// Run a blocking Lua interaction on the blocking pool. `mlua::Lua` is built
/// with the `send` feature, so the state can be moved off the async worker.
async fn run_lua_blocking<F, T>(f: F) -> PluginResult<T>
where
    F: FnOnce() -> PluginResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| PluginError::LuaError(format!("lua task panicked: {e}")))?
}

fn build_lua_context<'lua>(
    lua: &'lua mlua::Lua,
    ctx: &PluginContext,
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

fn to_lua_value<'lua>(lua: &'lua mlua::Lua, value: &Value) -> mlua::Value<'lua> {
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

fn from_lua_value(value: mlua::Value) -> Value {
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

fn create_lua_handler_table<'lua>(lua: &'lua mlua::Lua) -> PluginResult<mlua::Table<'lua>> {
    lua.create_table()
        .map_err(|e| PluginError::LuaError(format!("create table: {}", e)))
}

fn set_table_str<'lua>(t: &mlua::Table<'lua>, k: &str, v: &str) -> PluginResult<()> {
    t.set(k, v)
        .map_err(|e| PluginError::LuaError(format!("set {k}: {e}")))
}

fn set_table_value<'lua>(t: &mlua::Table<'lua>, k: &str, v: mlua::Value<'lua>) -> PluginResult<()> {
    t.set(k, v)
        .map_err(|e| PluginError::LuaError(format!("set {k}: {e}")))
}

// Handler structs

struct LuaNodeHandler {
    lua: Arc<Mutex<mlua::Lua>>,
    func_key: Arc<mlua::RegistryKey>,
}

struct LuaToolExecutor {
    lua: Arc<Mutex<mlua::Lua>>,
    func_key: Arc<mlua::RegistryKey>,
}

struct LuaLlmCodec {
    lua: Arc<Mutex<mlua::Lua>>,
    table_key: Arc<mlua::RegistryKey>,
}

/// Call one codec function of a Lua codec table synchronously and return
/// the host JSON value.
///
/// The codec table holds the wire-protocol functions (`build_request`,
/// `parse_response`, `parse_stream_chunk`, `convert_tools`,
/// `parse_tool_calls`, ...); the script only returns the request
/// description and the host constructs the HTTP request. When called
/// inside a tokio runtime the call runs on the blocking pool so a slow
/// script never pins an async worker.
fn call_lua_codec_fn(
    lua: &Arc<Mutex<mlua::Lua>>,
    table_key: &Arc<mlua::RegistryKey>,
    func_name: &'static str,
    args: Vec<Value>,
) -> PluginResult<Value> {
    let lua = lua.clone();
    let table_key = table_key.clone();
    let invoke = move || -> PluginResult<Value> {
        let locked = lua
            .lock()
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        let table: mlua::Table = locked
            .registry_value(&table_key)
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        let func: mlua::Function = table
            .get(func_name)
            .map_err(|e| PluginError::LuaError(format!("codec missing '{func_name}': {e}")))?;
        let lua_args = args
            .iter()
            .map(|v| to_lua_value(&locked, v))
            .collect::<Vec<_>>();
        let result: mlua::Value = func
            .call(mlua::Variadic::from_iter(lua_args))
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        Ok(from_lua_value(result))
    };
    // Inside a tokio runtime, hop to the blocking pool so a slow script
    // never pins an async worker; outside a runtime, call inline.
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(run_lua_blocking(invoke))),
        Err(_) => invoke(),
    }
}

impl PluginLlmCodec for LuaLlmCodec {
    fn build_request(&self, request: Value, profile: Value) -> PluginResult<CodecHttpRequest> {
        let value = call_lua_codec_fn(
            &self.lua,
            &self.table_key,
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
            "parse_response",
            vec![Value::String(body.to_owned()), request],
        )
    }

    fn parse_stream_chunk(&self, chunk: &str) -> PluginResult<Option<Value>> {
        let value = call_lua_codec_fn(
            &self.lua,
            &self.table_key,
            "parse_stream_chunk",
            vec![Value::String(chunk.to_owned())],
        )?;
        Ok(if value.is_null() { None } else { Some(value) })
    }

    fn convert_tools(&self, tools: Value) -> PluginResult<Value> {
        call_lua_codec_fn(&self.lua, &self.table_key, "convert_tools", vec![tools])
    }

    fn parse_tool_calls(&self, result: Value) -> PluginResult<Value> {
        call_lua_codec_fn(&self.lua, &self.table_key, "parse_tool_calls", vec![result])
    }

    fn build_count_tokens_request(
        &self,
        request: Value,
        profile: Value,
    ) -> PluginResult<Option<CodecHttpRequest>> {
        let value = call_lua_codec_fn(
            &self.lua,
            &self.table_key,
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
            "parse_count_tokens_response",
            vec![body],
        )?;
        value.as_u64().map(|n| n as u32).ok_or_else(|| {
            PluginError::LuaError("codec parse_count_tokens_response must return a number".into())
        })
    }
}

struct LuaEventHandler {
    lua: Arc<Mutex<mlua::Lua>>,
    func_key: Arc<mlua::RegistryKey>,
}

struct LuaMiddlewareHandler {
    lua: Arc<Mutex<mlua::Lua>>,
    func_key: Arc<mlua::RegistryKey>,
}

// Async handler implementations

#[async_trait]
impl PluginNodeHandler for LuaNodeHandler {
    async fn execute(&self, ctx: PluginExecutionContext) -> PluginResult<PluginNodeResult> {
        let lua = self.lua.clone();
        let func_key = self.func_key.clone();
        run_lua_blocking(move || {
            let lua = lua
                .lock()
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let func: mlua::Function = lua
                .registry_value(&func_key)
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let ctx_tbl = create_lua_handler_table(&lua)?;
            set_table_str(&ctx_tbl, "node_id", &ctx.node_id)?;
            set_table_value(&ctx_tbl, "inputs", to_lua_value(&lua, &ctx.inputs))?;
            set_table_value(&ctx_tbl, "config", to_lua_value(&lua, &ctx.config))?;
            let result: mlua::Value = func
                .call(ctx_tbl)
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let o = result
                .as_table()
                .and_then(|t| t.get::<&str, mlua::Value>("outputs").ok())
                .unwrap_or(mlua::Value::Nil);
            Ok(PluginNodeResult {
                outputs: from_lua_value(o),
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
        run_lua_blocking(move || {
            let lua = lua
                .lock()
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let func: mlua::Function = lua
                .registry_value(&func_key)
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let ctx_tbl = create_lua_handler_table(&lua)?;
            set_table_value(&ctx_tbl, "args", to_lua_value(&lua, &ctx.args))?;
            let result: mlua::Value = func
                .call(ctx_tbl)
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let r = result
                .as_table()
                .and_then(|t| t.get::<&str, mlua::Value>("result").ok())
                .unwrap_or(mlua::Value::Nil);
            Ok(PluginToolResult {
                result: from_lua_value(r),
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
        run_lua_blocking(move || {
            let lua = lua
                .lock()
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let func: mlua::Function = lua
                .registry_value(&func_key)
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let evt_tbl = create_lua_handler_table(&lua)?;
            set_table_str(&evt_tbl, "event_type", &event.event_type)?;
            set_table_value(&evt_tbl, "data", to_lua_value(&lua, &event.data))?;
            func.call(evt_tbl)
                .map_err(|e| PluginError::LuaError(e.to_string()))
        })
        .await
    }
}

#[async_trait]
impl PluginMiddlewareHandler for LuaMiddlewareHandler {
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value> {
        let lua = self.lua.clone();
        let func_key = self.func_key.clone();
        // Middleware `next` may drive arbitrary async handlers, so the Lua
        // call must stay on the worker thread; `block_in_place` releases the
        // worker so other tasks keep running while Lua executes. The `next`
        // future is driven by the *current tokio runtime handle* (multi-thread
        // only) rather than a foreign executor, so tokio-dependent handlers do
        // not panic on a missing reactor. Without a suitable runtime we fall
        // back to a lightweight executor, which supports non-tokio futures.
        let handle = tokio::runtime::Handle::try_current().ok();
        let is_multithread = handle
            .as_ref()
            .is_some_and(|h| h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread);

        let run = move || {
            let lua = lua
                .lock()
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let func: mlua::Function = lua
                .registry_value(&func_key)
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let ctx_val = to_lua_value(&lua, &context);
            // Request rewrite: `next(new_ctx)` continues with the replacement
            // (`next()`, `next(nil)`, and `next(tbl)` all work); without an
            // argument the incoming context travels downstream.
            let incoming = context.clone();
            let next_cell: Arc<std::sync::Mutex<Option<NextFn>>> =
                Arc::new(std::sync::Mutex::new(Some(next)));
            let downstream: Arc<std::sync::Mutex<Option<Value>>> =
                Arc::new(std::sync::Mutex::new(None));
            let wrapper_incoming = incoming.clone();
            let wrapper_next = next_cell.clone();
            let wrapper_downstream = downstream.clone();
            let wrapper_handle = handle.clone();
            let next_wrapper = lua
                .create_function(move |lua_ctx, args: mlua::Variadic<mlua::Value>| {
            let next = lock_ok(wrapper_next.lock())
                .take()
                .ok_or_else(|| mlua::Error::external("next already called"))?;
                    let replacement = match args.into_iter().next().map(from_lua_value) {
                        None | Some(Value::Null) => wrapper_incoming.clone(),
                        Some(value) => value,
                    };
                    let out = match &wrapper_handle {
                        Some(wrapper_handle) => wrapper_handle.block_on(next(replacement)),
                        None => futures::executor::block_on(next(replacement)),
                    }
                    .map_err(|e| mlua::Error::external(e.to_string()))?;
                    *lock_ok(wrapper_downstream.lock()) = Some(out.clone());
                    Ok(to_lua_value(lua_ctx, &out))
                })
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            let ret: mlua::Value = func
                .call((ctx_val, next_wrapper))
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            // Response rules: the function return value is converted and run
            // through the shared envelope parser. An envelope object
            // (`{proceed, context}`) rewrites; a plain returned table becomes
            // the short-circuit context when `next` was never called, while
            // any other value keeps the incoming context and stops the chain
            // (legacy behavior: not calling `next` halts).
            let ret_val = from_lua_value(ret);
            let is_envelope = ret_val
                .as_object()
                .is_some_and(|o| o.contains_key("proceed") || o.contains_key("context"));
            let downstream = lock_ok(downstream.lock()).take();
            match downstream {
                Some(down) => {
                    if is_envelope {
                        Ok(parse_middleware_outcome(&ret_val, &down).context)
                    } else {
                        Ok(down)
                    }
                }
                None => {
                    if is_envelope {
                        let outcome = parse_middleware_outcome(&ret_val, &incoming);
                        if outcome.proceed {
                            let next = lock_ok(next_cell.lock()).take().expect("next never taken");
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
        };

        // `block_in_place` panics on a current-thread runtime; only use it when
        // the current runtime is multi-thread.
        if is_multithread {
            tokio::task::block_in_place(run)
        } else {
            run()
        }
    }
}

// Plugin trait implementation

#[async_trait]
impl Plugin for LuaPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn on_load(&self, ctx: &PluginContext) -> PluginResult<()> {
        self.call_hook("on_load", ctx.clone()).await
    }

    async fn on_unload(&self, ctx: &PluginContext) -> PluginResult<()> {
        self.call_hook("on_unload", ctx.clone()).await
    }

    async fn on_activate(&self, ctx: &PluginContext) -> PluginResult<()> {
        self.call_hook("on_activate", ctx.clone()).await
    }

    async fn on_deactivate(&self, ctx: &PluginContext) -> PluginResult<()> {
        self.call_hook("on_deactivate", ctx.clone()).await
    }

    async fn on_config_change(&self, config: &serde_json::Value) -> PluginResult<()> {
        let lua = self.lua.clone();
        let config = config.clone();
        run_lua_blocking(move || {
            let lua = match lua.lock() {
                Ok(l) => l,
                Err(e) => return Err(PluginError::LuaError(e.to_string())),
            };
            let plugin_table: mlua::Table = match lua.globals().get("plugin") {
                Ok(t) => t,
                Err(_) => return Ok(()),
            };
            let hook: mlua::Function = match plugin_table.get("on_config_change") {
                Ok(f) => f,
                Err(_) => return Ok(()),
            };
            let cfg_val = to_lua_value(&lua, &config);
            hook.call::<_, ()>(cfg_val)
                .map_err(|e| PluginError::LuaError(e.to_string()))
        })
        .await
    }

    fn register_contributions(
        &self,
        registrar: &mut dyn ContributionRegistrar,
    ) -> PluginResult<()> {
        // Step 1: extract keys from Lua state (Mutex held)
        struct RegEntry {
            name: String,
            key: mlua::RegistryKey,
            kind: u8,
            phase: String,
            priority: i32,
        }
        let entries: Vec<RegEntry> = {
            let locked = match self.lua.lock() {
                Ok(l) => l,
                Err(_) => {
                    return Err(PluginError::Internal("lua state lock poisoned".into()));
                }
            };

            let plugin_table: mlua::Table = match locked.globals().get("plugin") {
                Ok(t) => t,
                Err(_) => return Ok(()),
            };
            let register_fn: mlua::Function = match plugin_table.get("register_contributions") {
                Ok(f) => f,
                Err(_) => return Ok(()),
            };
            let contribs: mlua::Value = match register_fn.call(()) {
                Ok(v) => v,
                Err(e) => return Err(PluginError::LuaError(e.to_string())),
            };
            let contribs_table: mlua::Table = match contribs {
                mlua::Value::Table(t) => t,
                _ => return Ok(()),
            };

            let mut out: Vec<RegEntry> = Vec::new();

            {
                let mut extract = |type_key: &str, handler_key: &str, kind: u8| {
                    if let Ok(Some(t)) = contribs_table.get::<_, Option<mlua::Table>>(type_key) {
                        for (name, handler_tbl) in t.pairs::<String, mlua::Table>().flatten() {
                            if let Ok(func) = handler_tbl.get::<_, mlua::Function>(handler_key) {
                                if let Ok(key) = locked.create_registry_value(&func) {
                                    out.push(RegEntry {
                                        name,
                                        key,
                                        kind,
                                        phase: String::new(),
                                        priority: 0,
                                    });
                                }
                            }
                        }
                    }
                };

                extract("node_types", "execute", 0);
                extract("tool_types", "execute", 1);
            }
            // Low-level wire-protocol codecs: each entry is a table of
            // codec functions (`build_request`, `parse_response`,
            // `parse_stream_chunk`, `convert_tools`, ...). The script
            // only returns the request description; the host constructs
            // the HTTP request.
            if let Ok(Some(t)) = contribs_table.get::<_, Option<mlua::Table>>("llm_codecs") {
                for (name, codec_tbl) in t.pairs::<String, mlua::Table>().flatten() {
                    if let Ok(key) = locked.create_registry_value(&codec_tbl) {
                        out.push(RegEntry {
                            name,
                            key,
                            kind: 2,
                            phase: String::new(),
                            priority: 0,
                        });
                    }
                }
            }
            {
                let mut extract = |type_key: &str, handler_key: &str, kind: u8| {
                    if let Ok(Some(t)) = contribs_table.get::<_, Option<mlua::Table>>(type_key) {
                        for (name, handler_tbl) in t.pairs::<String, mlua::Table>().flatten() {
                            if let Ok(func) = handler_tbl.get::<_, mlua::Function>(handler_key) {
                                if let Ok(key) = locked.create_registry_value(&func) {
                                    out.push(RegEntry {
                                        name,
                                        key,
                                        kind,
                                        phase: String::new(),
                                        priority: 0,
                                    });
                                }
                            }
                        }
                    }
                };

                extract("event_handlers", "handle", 4);
            }

            if let Ok(Some(t)) = contribs_table.get::<_, Option<mlua::Table>>("middleware") {
                for (_, mw_tbl) in t.pairs::<i32, mlua::Table>().flatten() {
                    let phase: String = mw_tbl.get("phase").unwrap_or_default();
                    let priority: i32 = mw_tbl.get("priority").unwrap_or(0);
                    if let Ok(func) = mw_tbl.get::<_, mlua::Function>("handle") {
                        if let Ok(key) = locked.create_registry_value(&func) {
                            out.push(RegEntry {
                                name: String::new(),
                                key,
                                kind: 6,
                                phase,
                                priority,
                            });
                        }
                    }
                }
            }

            out
        };

        // Step 2: register (no Lua access)
        for e in entries {
            match e.kind {
                0 => registrar.register_node_type(
                    &e.name,
                    Arc::new(LuaNodeHandler {
                        lua: self.lua.clone(),
                        func_key: Arc::new(e.key),
                    }),
                )?,
                1 => registrar.register_tool_type(
                    &e.name,
                    Arc::new(LuaToolExecutor {
                        lua: self.lua.clone(),
                        func_key: Arc::new(e.key),
                    }),
                )?,
                2 => registrar.register_llm_provider(
                    &e.name,
                    Arc::new(LuaLlmCodec {
                        lua: self.lua.clone(),
                        table_key: Arc::new(e.key),
                    }),
                )?,
                4 => registrar.register_event_handler(
                    &e.name,
                    Arc::new(LuaEventHandler {
                        lua: self.lua.clone(),
                        func_key: Arc::new(e.key),
                    }),
                )?,
                6 => registrar.register_middleware(
                    MiddlewarePhase::from(e.phase.as_str()),
                    e.priority,
                    Arc::new(LuaMiddlewareHandler {
                        lua: self.lua.clone(),
                        func_key: Arc::new(e.key),
                    }),
                )?,
                _ => {}
            }
        }
        Ok(())
    }
}
