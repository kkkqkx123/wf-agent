use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::Value;

use crate::context::PluginContext;
use crate::contributions::types::*;
use crate::contributions::registrar::ContributionRegistrar;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;
use crate::contributions::NextFn;

pub struct LuaPlugin {
    manifest: PluginManifest,
    lua: Arc<Mutex<mlua::Lua>>,
}

impl LuaPlugin {
    pub fn new(manifest: PluginManifest, lua: mlua::Lua) -> Self {
        Self { manifest, lua: Arc::new(Mutex::new(lua)) }
    }

    fn call_hook(&self, hook_name: &str, ctx: &PluginContext) -> PluginResult<()> {
        let lua = self.lua.lock().map_err(|e| PluginError::LuaError(e.to_string()))?;

        let plugin_table: mlua::Table = lua.globals().get("plugin")
            .map_err(|e| PluginError::LuaError(e.to_string()))?;

        let hook: mlua::Function = match plugin_table.get(hook_name) {
            Ok(f) => f,
            Err(_) => {
                tracing::debug!("lua plugin '{}' has no hook '{}'", self.manifest.id, hook_name);
                return Ok(());
            }
        };

        let ctx_tbl = build_lua_context(&lua, ctx)?;
        hook.call::<_, ()>(ctx_tbl).map_err(|e| PluginError::LuaError(e.to_string()))
    }
}

fn build_lua_context<'lua>(lua: &'lua mlua::Lua, ctx: &PluginContext) -> PluginResult<mlua::Table<'lua>> {
    let t = lua.create_table()
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
            Value::Number(n) => {
                Ok(n.as_i64().map(mlua::Value::Integer)
                    .unwrap_or_else(|| mlua::Value::Number(n.as_f64().unwrap_or(0.0))))
            }
            Value::String(s) => {
                Ok(mlua::Value::String(l.create_string(s.as_bytes())?))
            }
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
                    Some(Value::Number(serde_json::Number::from_f64(n).unwrap_or(0.into())))
                }
            }
            mlua::Value::String(s) => {
                match s.to_str() {
                    Ok(s) => Some(Value::String(s.to_owned())),
                    Err(_) => Some(Value::Null),
                }
            }
            mlua::Value::Table(t) => {
                let mut is_array = true;
                let mut map = serde_json::Map::new();
                let mut arr: Vec<Value> = Vec::new();
                for pair in t.pairs::<mlua::Value, mlua::Value>() {
                    let (k, v) = match pair { Ok(p) => p, Err(_) => continue };
                    let v = try_convert(v).unwrap_or(Value::Null);
                    match k {
                        mlua::Value::Integer(i) if i >= 1 => {
                            let idx = (i - 1) as usize;
                            while arr.len() <= idx { arr.push(Value::Null); }
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

struct LuaLLMFormatter {
    lua: Arc<Mutex<mlua::Lua>>,
    func_key: Arc<mlua::RegistryKey>,
}

struct LuaEventHandler {
    lua: Arc<Mutex<mlua::Lua>>,
    func_key: Arc<mlua::RegistryKey>,
}

struct LuaHookHandler {
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
        let lua = self.lua.lock().map_err(|e| PluginError::LuaError(e.to_string()))?;
        let func: mlua::Function = lua.registry_value(&self.func_key)
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        let ctx_tbl = create_lua_handler_table(&lua)?;
        set_table_str(&ctx_tbl, "node_id", &ctx.node_id)?;
        set_table_value(&ctx_tbl, "inputs", to_lua_value(&lua, &ctx.inputs))?;
        set_table_value(&ctx_tbl, "config", to_lua_value(&lua, &ctx.config))?;
        let result: mlua::Value = func.call(ctx_tbl).map_err(|e| PluginError::LuaError(e.to_string()))?;
        let o = result.as_table()
            .and_then(|t| t.get::<&str, mlua::Value>("outputs").ok())
            .unwrap_or(mlua::Value::Nil);
        Ok(PluginNodeResult { outputs: from_lua_value(o) })
    }
}

#[async_trait]
impl PluginToolExecutor for LuaToolExecutor {
    async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
        let lua = self.lua.lock().map_err(|e| PluginError::LuaError(e.to_string()))?;
        let func: mlua::Function = lua.registry_value(&self.func_key)
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        let ctx_tbl = create_lua_handler_table(&lua)?;
        set_table_value(&ctx_tbl, "args", to_lua_value(&lua, &ctx.args))?;
        let result: mlua::Value = func.call(ctx_tbl).map_err(|e| PluginError::LuaError(e.to_string()))?;
        let r = result.as_table()
            .and_then(|t| t.get::<&str, mlua::Value>("result").ok())
            .unwrap_or(mlua::Value::Nil);
        Ok(PluginToolResult { result: from_lua_value(r) })
    }
}

#[async_trait]
impl PluginLLMFormatter for LuaLLMFormatter {
    async fn format(&self, request: PluginLLMRequest) -> PluginResult<PluginLLMResponse> {
        let lua = self.lua.lock().map_err(|e| PluginError::LuaError(e.to_string()))?;
        let func: mlua::Function = lua.registry_value(&self.func_key)
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        let req_tbl = create_lua_handler_table(&lua)?;
        let msgs_arr = create_lua_handler_table(&lua)?;
        for (i, msg) in request.messages.iter().enumerate() {
            let msg_tbl = create_lua_handler_table(&lua)?;
            set_table_str(&msg_tbl, "role", &msg.role)?;
            set_table_str(&msg_tbl, "content", &msg.content)?;
            msgs_arr.set(i + 1, msg_tbl)
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
        }
        set_table_value(&req_tbl, "messages", mlua::Value::Table(msgs_arr))?;
        if let Some(ref config) = request.config {
            let cfg_tbl = create_lua_handler_table(&lua)?;
            set_table_str(&cfg_tbl, "model", &config.model)?;
            set_table_str(&cfg_tbl, "provider", &config.provider)?;
            if let Some(t) = config.temperature {
                cfg_tbl.set("temperature", t)
                    .map_err(|e| PluginError::LuaError(e.to_string()))?;
            }
            if let Some(m) = config.max_tokens {
                cfg_tbl.set("max_tokens", m)
                    .map_err(|e| PluginError::LuaError(e.to_string()))?;
            }
            set_table_value(&req_tbl, "config", mlua::Value::Table(cfg_tbl))?;
        }
        let result: mlua::Value = func.call(req_tbl).map_err(|e| PluginError::LuaError(e.to_string()))?;
        let t = result.as_table().ok_or_else(|| PluginError::LuaError("result must be a table".into()))?;
        let content: String = t.get("content")
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        let usage = t.get::<&str, mlua::Table>("usage").ok().map(|ut| PluginLLMUsage {
            prompt_tokens: ut.get("prompt_tokens").unwrap_or(0),
            completion_tokens: ut.get("completion_tokens").unwrap_or(0),
            total_tokens: ut.get("total_tokens").unwrap_or(0),
        });
        Ok(PluginLLMResponse { content, usage })
    }
}

#[async_trait]
impl PluginEventHandler for LuaEventHandler {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()> {
        let lua = self.lua.lock().map_err(|e| PluginError::LuaError(e.to_string()))?;
        let func: mlua::Function = lua.registry_value(&self.func_key)
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        let evt_tbl = create_lua_handler_table(&lua)?;
        set_table_str(&evt_tbl, "event_type", &event.event_type)?;
        set_table_value(&evt_tbl, "data", to_lua_value(&lua, &event.data))?;
        func.call(evt_tbl).map_err(|e| PluginError::LuaError(e.to_string()))
    }
}

#[async_trait]
impl PluginHookHandler for LuaHookHandler {
    async fn handle(&self, context: Value) -> PluginResult<()> {
        let lua = self.lua.lock().map_err(|e| PluginError::LuaError(e.to_string()))?;
        let func: mlua::Function = lua.registry_value(&self.func_key)
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        let val = to_lua_value(&lua, &context);
        func.call(val).map_err(|e| PluginError::LuaError(e.to_string()))
    }
}

#[async_trait]
impl PluginMiddlewareHandler for LuaMiddlewareHandler {
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<()> {
        let lua = self.lua.lock().map_err(|e| PluginError::LuaError(e.to_string()))?;
        let func: mlua::Function = lua.registry_value(&self.func_key)
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        let ctx_val = to_lua_value(&lua, &context);
        let next = std::sync::Mutex::new(Some(next));
        let next_wrapper = lua.create_function(move |_lua, _: ()| {
            let f = next.lock().unwrap().take()
                .ok_or_else(|| mlua::Error::external("next already called"))?;
            let fut = f();
            tokio::task::block_in_place(|| {
                futures::executor::block_on(fut)
            }).map_err(|e| mlua::Error::external(e.to_string()))
        }).map_err(|e| PluginError::LuaError(e.to_string()))?;
        func.call::<_, ()>((ctx_val, next_wrapper)).map_err(|e| PluginError::LuaError(e.to_string()))
    }
}

// Plugin trait implementation

#[async_trait]
impl Plugin for LuaPlugin {
    fn manifest(&self) -> &PluginManifest { &self.manifest }

    async fn on_load(&self, ctx: &PluginContext) -> PluginResult<()> {
        self.call_hook("on_load", ctx)
    }

    async fn on_unload(&self, ctx: &PluginContext) -> PluginResult<()> {
        self.call_hook("on_unload", ctx)
    }

    async fn on_activate(&self, ctx: &PluginContext) -> PluginResult<()> {
        self.call_hook("on_activate", ctx)
    }

    async fn on_deactivate(&self, ctx: &PluginContext) -> PluginResult<()> {
        self.call_hook("on_deactivate", ctx)
    }

    async fn on_config_change(&self, config: &serde_json::Value) -> PluginResult<()> {
        let lua = match self.lua.lock() {
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
        let cfg_val = to_lua_value(&lua, config);
        hook.call::<_, ()>(cfg_val).map_err(|e| PluginError::LuaError(e.to_string()))
    }

    fn register_contributions(&self, registrar: &mut dyn ContributionRegistrar) {
        // Phase 1: extract keys from Lua state (Mutex held)
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
                Err(_) => return,
            };

            let plugin_table: mlua::Table = match locked.globals().get("plugin") {
                Ok(t) => t,
                Err(_) => return,
            };
            let register_fn: mlua::Function = match plugin_table.get("register_contributions") {
                Ok(f) => f,
                Err(_) => return,
            };
            let contribs: mlua::Value = match register_fn.call(()) {
                Ok(v) => v,
                Err(_) => return,
            };
            let contribs_table: mlua::Table = match contribs {
                mlua::Value::Table(t) => t,
                _ => return,
            };

            let mut out: Vec<RegEntry> = Vec::new();

            let mut extract = |type_key: &str, handler_key: &str, kind: u8| {
                if let Ok(Some(t)) = contribs_table.get::<_, Option<mlua::Table>>(type_key) {
                    for (name, handler_tbl) in t.pairs::<String, mlua::Table>().flatten() {
                        if let Ok(func) = handler_tbl.get::<_, mlua::Function>(handler_key) {
                            if let Ok(key) = locked.create_registry_value(&func) {
                                out.push(RegEntry { name, key, kind, phase: String::new(), priority: 0 });
                            }
                        }
                    }
                }
            };

            extract("node_types", "execute", 0);
            extract("tool_types", "execute", 1);
            extract("llm_providers", "format", 2);
            extract("formatters", "format", 3);
            extract("event_handlers", "handle", 4);
            extract("hook_handlers", "handle", 5);

            if let Ok(Some(t)) = contribs_table.get::<_, Option<mlua::Table>>("middleware") {
                for (_, mw_tbl) in t.pairs::<i32, mlua::Table>().flatten() {
                    let phase: String = mw_tbl.get("phase").unwrap_or_default();
                    let priority: i32 = mw_tbl.get("priority").unwrap_or(0);
                    if let Ok(func) = mw_tbl.get::<_, mlua::Function>("handle") {
                        if let Ok(key) = locked.create_registry_value(&func) {
                            out.push(RegEntry { name: String::new(), key, kind: 6, phase, priority });
                        }
                    }
                }
            }

            out
        };

        // Phase 2: register (no Lua access)
        for e in entries {
            match e.kind {
                0 => registrar.register_node_type(&e.name, Arc::new(LuaNodeHandler { lua: self.lua.clone(), func_key: Arc::new(e.key) })),
                1 => registrar.register_tool_type(&e.name, Arc::new(LuaToolExecutor { lua: self.lua.clone(), func_key: Arc::new(e.key) })),
                2 => registrar.register_llm_provider(&e.name, Arc::new(LuaLLMFormatter { lua: self.lua.clone(), func_key: Arc::new(e.key) })),
                3 => registrar.register_formatter(&e.name, Arc::new(LuaLLMFormatter { lua: self.lua.clone(), func_key: Arc::new(e.key) })),
                4 => registrar.register_event_handler(&e.name, Arc::new(LuaEventHandler { lua: self.lua.clone(), func_key: Arc::new(e.key) })),
                5 => registrar.register_hook_handler(&e.name, Arc::new(LuaHookHandler { lua: self.lua.clone(), func_key: Arc::new(e.key) })),
                6 => registrar.register_middleware(&e.phase, e.priority, Arc::new(LuaMiddlewareHandler { lua: self.lua.clone(), func_key: Arc::new(e.key) })),
                _ => {}
            }
        }
    }
}


