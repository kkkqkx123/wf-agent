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

        let globals = lua.globals();
        let plugin_table: mlua::Table = globals.get("plugin")
            .map_err(|e| PluginError::LuaError(e.to_string()))?;

        let hook: mlua::Function = match plugin_table.get(hook_name) {
            Ok(f) => f,
            Err(_) => return Ok(()),
        };

        let ctx_tbl = build_lua_context(&lua, ctx);
        hook.call::<_, ()>(ctx_tbl).map_err(|e| PluginError::LuaError(e.to_string()))
    }
}

fn build_lua_context<'lua>(lua: &'lua mlua::Lua, ctx: &PluginContext) -> mlua::Table<'lua> {
    let t = lua.create_table().unwrap();
    let _ = t.set("plugin_id", ctx.plugin_id.as_str());
    let _ = t.set("config", to_lua_value(lua, &ctx.config));
    t
}

fn to_lua_value<'lua>(lua: &'lua mlua::Lua, value: &Value) -> mlua::Value<'lua> {
    match value {
        Value::Null => mlua::Value::Nil,
        Value::Bool(b) => mlua::Value::Boolean(*b),
        Value::Number(n) => {
            n.as_i64().map(mlua::Value::Integer)
                .unwrap_or_else(|| mlua::Value::Number(n.as_f64().unwrap_or(0.0)))
        }
        Value::String(s) => mlua::Value::String(lua.create_string(s.as_bytes()).unwrap()),
        Value::Array(arr) => {
            let t = lua.create_table().unwrap();
            for (i, v) in arr.iter().enumerate() {
                let _ = t.set(i + 1, to_lua_value(lua, v));
            }
            mlua::Value::Table(t)
        }
        Value::Object(map) => {
            let t = lua.create_table().unwrap();
            for (k, v) in map {
                let _ = t.set(k.as_str(), to_lua_value(lua, v));
            }
            mlua::Value::Table(t)
        }
    }
}

fn from_lua_value(value: mlua::Value) -> Value {
    match value {
        mlua::Value::Nil => Value::Null,
        mlua::Value::Boolean(b) => Value::Bool(b),
        mlua::Value::Integer(i) => Value::Number(i.into()),
        mlua::Value::Number(n) => {
            Value::Number(serde_json::Number::from_f64(n).unwrap_or(0.into()))
        }
        mlua::Value::String(s) => Value::String(s.to_str().unwrap_or("").to_owned()),
        mlua::Value::Table(t) => {
            let mut is_array = true;
            let mut map = serde_json::Map::new();
            let mut arr: Vec<Value> = Vec::new();
            for pair in t.pairs::<mlua::Value, mlua::Value>() {
                let (k, v) = match pair { Ok(p) => p, _ => continue };
                match k {
                    mlua::Value::Integer(i) if i >= 1 => {
                        let idx = (i - 1) as usize;
                        while arr.len() <= idx { arr.push(Value::Null); }
                        arr[idx] = from_lua_value(v);
                    }
                    mlua::Value::String(s) => {
                        is_array = false;
                        map.insert(s.to_str().unwrap_or("").to_owned(), from_lua_value(v));
                    }
                    _ => is_array = false,
                }
            }
            if is_array && !arr.is_empty() { Value::Array(arr) } else { Value::Object(map) }
        }
        _ => Value::Null,
    }
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
        let ctx_tbl = lua.create_table().unwrap();
        let _ = ctx_tbl.set("node_id", ctx.node_id.as_str());
        let _ = ctx_tbl.set("inputs", to_lua_value(&lua, &ctx.inputs));
        let _ = ctx_tbl.set("config", to_lua_value(&lua, &ctx.config));
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
        let ctx_tbl = lua.create_table().unwrap();
        let _ = ctx_tbl.set("args", to_lua_value(&lua, &ctx.args));
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
        let req_tbl = lua.create_table().unwrap();
        let msgs_arr = lua.create_table().unwrap();
        for (i, msg) in request.messages.iter().enumerate() {
            let msg_tbl = lua.create_table().unwrap();
            let _ = msg_tbl.set("role", msg.role.as_str());
            let _ = msg_tbl.set("content", msg.content.as_str());
            let _ = msgs_arr.set(i + 1, msg_tbl);
        }
        let _ = req_tbl.set("messages", msgs_arr);
        if let Some(ref config) = request.config {
            let cfg_tbl = lua.create_table().unwrap();
            let _ = cfg_tbl.set("model", config.model.as_str());
            let _ = cfg_tbl.set("provider", config.provider.as_str());
            if let Some(t) = config.temperature { let _ = cfg_tbl.set("temperature", t); }
            if let Some(m) = config.max_tokens { let _ = cfg_tbl.set("max_tokens", m); }
            let _ = req_tbl.set("config", cfg_tbl);
        }
        let result: mlua::Value = func.call(req_tbl).map_err(|e| PluginError::LuaError(e.to_string()))?;
        let t = result.as_table().ok_or_else(|| PluginError::LuaError("result must be a table".into()))?;
        let content: String = t.get("content").map_err(|e| PluginError::LuaError(e.to_string()))?;
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
        let evt_tbl = lua.create_table().unwrap();
        let _ = evt_tbl.set("event_type", event.event_type.as_str());
        let _ = evt_tbl.set("data", to_lua_value(&lua, &event.data));
        func.call::<_, ()>(evt_tbl).map_err(|e| PluginError::LuaError(e.to_string()))
    }
}

#[async_trait]
impl PluginHookHandler for LuaHookHandler {
    async fn handle(&self, context: Value) -> PluginResult<()> {
        let lua = self.lua.lock().map_err(|e| PluginError::LuaError(e.to_string()))?;
        let func: mlua::Function = lua.registry_value(&self.func_key)
            .map_err(|e| PluginError::LuaError(e.to_string()))?;
        let val = to_lua_value(&lua, &context);
        func.call::<_, ()>(val).map_err(|e| PluginError::LuaError(e.to_string()))
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
            match futures::executor::block_on(fut) {
                Ok(()) => Ok(()),
                Err(e) => Err(mlua::Error::external(e.to_string())),
            }
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

    fn register_contributions(&self, registrar: &mut dyn ContributionRegistrar) {
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

            let helper = r#"
                return function(plugin_table)
                    local contribs = plugin_table.register_contributions()
                    if not contribs then return {} end
                    local out = {}
                    local function add(typ, name, fn, phase, priority)
                        out[#out+1] = {type=typ, name=name, phase=phase or "", priority=priority or 0, fn=fn}
                    end
                    if contribs.node_types then
                        for name, h in pairs(contribs.node_types) do
                            if type(h.execute)=="function" then add("node", name, h.execute) end
                        end
                    end
                    if contribs.tool_types then
                        for name, h in pairs(contribs.tool_types) do
                            if type(h.execute)=="function" then add("tool", name, h.execute) end
                        end
                    end
                    if contribs.llm_providers then
                        for name, h in pairs(contribs.llm_providers) do
                            if type(h.format)=="function" then add("llm", name, h.format) end
                        end
                    end
                    if contribs.formatters then
                        for name, h in pairs(contribs.formatters) do
                            if type(h.format)=="function" then add("fmt", name, h.format) end
                        end
                    end
                    if contribs.event_handlers then
                        for name, h in pairs(contribs.event_handlers) do
                            if type(h.handle)=="function" then add("event", name, h.handle) end
                        end
                    end
                    if contribs.hook_handlers then
                        for name, h in pairs(contribs.hook_handlers) do
                            if type(h.handle)=="function" then add("hook", name, h.handle) end
                        end
                    end
                    if contribs.middleware then
                        for _, mw in ipairs(contribs.middleware) do
                            if type(mw.handle)=="function" then add("mw", "", mw.handle, mw.phase, mw.priority) end
                        end
                    end
                    return out
                end
            "#;

            let result: std::result::Result<Vec<RegEntry>, ()> = (|| -> std::result::Result<Vec<RegEntry>, ()> {
                let extractor: mlua::Function = locked.load(helper).eval().map_err(|_| ())?;
                let globals = locked.globals();
                let plugin_table: mlua::Table = globals.get("plugin").map_err(|_| ())?;
                let entries_tbl: mlua::Table = extractor.call::<_, mlua::Table>(plugin_table).map_err(|_| ())?;

                let mut out = Vec::new();
                for i in 1..=entries_tbl.raw_len() {
                    let entry_val: mlua::Value = entries_tbl.raw_get(i).map_err(|_| ())?;
                    let entry_tbl = match entry_val.as_table() { Some(t) => t, _ => continue };
                    let typ: String = match entry_tbl.get("type") { Ok(t) => t, _ => continue };
                    let name: String = match entry_tbl.get("name") { Ok(n) => n, _ => continue };
                    let raw_kind: u8 = match typ.as_str() {
                        "node" => 0, "tool" => 1, "llm" => 2, "fmt" => 3,
                        "event" => 4, "hook" => 5, "mw" => 6, _ => continue,
                    };
                    let phase: String = entry_tbl.get("phase").unwrap_or_default();
                    let priority: i32 = entry_tbl.get("priority").unwrap_or(0);
                    let func: mlua::Function = match entry_tbl.get("fn") { Ok(f) => f, _ => continue };
                    let key = locked.create_registry_value(&func).map_err(|_| ())?;
                    out.push(RegEntry { name, key, kind: raw_kind, phase, priority });
                }
                Ok(out)
            })();

            result.unwrap_or_default()
        };

        // Phase 2: register (no Lua access)
        for entry in entries {
            match entry.kind {
                0 => registrar.register_node_type(&entry.name, Arc::new(LuaNodeHandler { lua: self.lua.clone(), func_key: Arc::new(entry.key) })),
                1 => registrar.register_tool_type(&entry.name, Arc::new(LuaToolExecutor { lua: self.lua.clone(), func_key: Arc::new(entry.key) })),
                2 => registrar.register_llm_provider(&entry.name, Arc::new(LuaLLMFormatter { lua: self.lua.clone(), func_key: Arc::new(entry.key) })),
                3 => registrar.register_formatter(&entry.name, Arc::new(LuaLLMFormatter { lua: self.lua.clone(), func_key: Arc::new(entry.key) })),
                4 => registrar.register_event_handler(&entry.name, Arc::new(LuaEventHandler { lua: self.lua.clone(), func_key: Arc::new(entry.key) })),
                5 => registrar.register_hook_handler(&entry.name, Arc::new(LuaHookHandler { lua: self.lua.clone(), func_key: Arc::new(entry.key) })),
                6 => registrar.register_middleware(&entry.phase, entry.priority, Arc::new(LuaMiddlewareHandler { lua: self.lua.clone(), func_key: Arc::new(entry.key) })),
                _ => {}
            }
        }
    }
}


