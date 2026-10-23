use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;

use wf_types::MiddlewarePhase;

use crate::context::PluginContext;
use crate::contributions::registrar::ContributionRegistrar;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;

use super::bridge;
use super::codec;
use super::handlers;
use super::pool;

pub struct LuaPlugin {
    manifest: PluginManifest,
    lua: Arc<Mutex<mlua::Lua>>,
    script: Arc<String>,
    limits: pool::LuaExecutionLimits,
}

impl LuaPlugin {
    pub fn new(manifest: PluginManifest, lua: mlua::Lua, script: Arc<String>) -> Self {
        Self {
            manifest,
            lua: Arc::new(Mutex::new(lua)),
            script,
            limits: pool::LuaExecutionLimits::default(),
        }
    }

    pub fn with_limits(mut self, limits: pool::LuaExecutionLimits) -> Self {
        self.limits = limits;
        self
    }

    fn is_limit_error(error: &PluginError) -> bool {
        match error {
            PluginError::LuaError(message) => {
                message.contains("timed out") || message.contains("memory limit")
            }
            PluginError::Timeout { .. } => true,
            _ => false,
        }
    }

    fn reload_state(lua: &Arc<Mutex<mlua::Lua>>, script: &Arc<String>) {
        let fresh = (|| -> PluginResult<mlua::Lua> {
            let state = wf_sandbox::strategy::lua::mlua_sandbox::create_restricted_lua()
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            wf_sandbox::strategy::lua::mlua_sandbox::apply_plugin_sandbox(&state)
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            state
                .load(script.as_str())
                .eval::<mlua::Value>()
                .map_err(|e| PluginError::LuaError(e.to_string()))?;
            Ok(state)
        })();
        if let Ok(state) = fresh {
            if let Ok(mut locked) = lua.lock() {
                *locked = state;
            }
        }
    }

    /// Run a named plugin lifecycle hook on the blocking pool so a slow or
    /// infinite Lua hook never pins a tokio worker (and the `PluginGuard`
    /// timeout can actually fire).
    async fn call_hook(&self, hook_name: &'static str, ctx: PluginContext) -> PluginResult<()> {
        let lua = self.lua.clone();
        let script = self.script.clone();
        let limits = self.limits;
        let manifest_id = self.manifest.id.clone();
        let result = bridge::run_lua_blocking(move || {
            let locked = lua
                .lock()
                .map_err(|e| PluginError::LuaError(e.to_string()))?;

            bridge::arm_hook(&locked, limits)?;
            let outcome = (|| -> PluginResult<()> {
                let plugin_table: mlua::Table = locked
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

                let ctx_tbl = bridge::build_lua_context(&locked, &ctx)?;
                hook.call::<_, ()>(ctx_tbl)
                    .map_err(|e| PluginError::LuaError(e.to_string()))
            })();
            locked.remove_hook();
            if outcome.as_ref().is_err_and(Self::is_limit_error) {
                drop(locked);
                Self::reload_state(&lua, &script);
            }
            outcome
        })
        .await;
        result
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
        let script = self.script.clone();
        let limits = self.limits;
        let config = config.clone();
        bridge::run_lua_blocking(move || {
            let locked = match lua.lock() {
                Ok(l) => l,
                Err(e) => return Err(PluginError::LuaError(e.to_string())),
            };
            if bridge::arm_hook(&locked, limits).is_err() {
                return Err(PluginError::LuaError("lua hook setup failed".into()));
            }
            let outcome = (|| -> PluginResult<()> {
                let plugin_table: mlua::Table = match locked.globals().get("plugin") {
                    Ok(t) => t,
                    Err(_) => return Ok(()),
                };
                let hook: mlua::Function = match plugin_table.get("on_config_change") {
                    Ok(f) => f,
                    Err(_) => return Ok(()),
                };
                let cfg_val = bridge::to_lua_value(&locked, &config);
                hook.call::<_, ()>(cfg_val)
                    .map_err(|e| PluginError::LuaError(e.to_string()))
            })();
            locked.remove_hook();
            if outcome.as_ref().is_err_and(LuaPlugin::is_limit_error) {
                drop(locked);
                LuaPlugin::reload_state(&lua, &script);
            }
            outcome
        })
        .await
    }

    fn register_contributions(
        &self,
        registrar: &mut dyn ContributionRegistrar,
    ) -> PluginResult<()> {
        struct RegEntry {
            name: String,
            key: mlua::RegistryKey,
            kind: u8,
            phase: String,
            priority: i32,
        }
        let (entries, lua_tools, lua_tool_descriptions): (
            Vec<RegEntry>,
            Vec<wf_types::tool::Tool>,
            Vec<wf_types::tool_description::ToolDescriptionData>,
        ) = {
            let locked = match self.lua.lock() {
                Ok(l) => l,
                Err(_) => {
                    return Err(PluginError::Internal("lua state lock poisoned".into()));
                }
            };
            if bridge::arm_hook(&locked, self.limits).is_err() {
                return Err(PluginError::LuaError("lua hook setup failed".into()));
            }

            let plugin_table: mlua::Table = match locked.globals().get("plugin") {
                Ok(t) => t,
                Err(_) => {
                    locked.remove_hook();
                    return Ok(());
                }
            };
            let register_fn: mlua::Function = match plugin_table.get("register_contributions") {
                Ok(f) => f,
                Err(_) => {
                    locked.remove_hook();
                    return Ok(());
                }
            };
            let contribs: mlua::Value = match register_fn.call(()) {
                Ok(v) => v,
                Err(e) => {
                    locked.remove_hook();
                    return Err(PluginError::LuaError(e.to_string()));
                }
            };
            let contribs_table: mlua::Table = match contribs {
                mlua::Value::Table(t) => t,
                _ => {
                    locked.remove_hook();
                    return Ok(());
                }
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

            let mut tools: Vec<wf_types::tool::Tool> = Vec::new();
            if let Ok(Some(tbl)) = contribs_table.get::<_, Option<mlua::Table>>("tools") {
                for tool_tbl in tbl.sequence_values::<mlua::Table>().flatten() {
                    let id: String = tool_tbl.get("id").unwrap_or_default();
                    if id.trim().is_empty() {
                        continue;
                    }
                    let name: String = tool_tbl
                        .get::<_, Option<String>>("name")
                        .unwrap_or_default()
                        .unwrap_or_default();
                    let description: String = tool_tbl
                        .get::<_, Option<String>>("description")
                        .unwrap_or_default()
                        .unwrap_or_default();
                    tools.push(wf_types::tool::Tool {
                        id: id.clone(),
                        name: if name.is_empty() { id } else { name },
                        description,
                        tool_type: wf_types::tool::ToolType::Stateless,
                        parameters: None,
                        metadata: None,
                        config: None,
                        enabled: None,
                        strict: None,
                        default_timeout_ms: None,
                    });
                }
            }
            let mut tool_descriptions: Vec<wf_types::tool_description::ToolDescriptionData> =
                Vec::new();
            if let Ok(Some(tbl)) = contribs_table.get::<_, Option<mlua::Table>>("tool_descriptions")
            {
                for desc_tbl in tbl.sequence_values::<mlua::Table>().flatten() {
                    let id: String = desc_tbl.get("id").unwrap_or_default();
                    if id.trim().is_empty() {
                        continue;
                    }
                    let description: String = desc_tbl
                        .get::<_, Option<String>>("description")
                        .unwrap_or_default()
                        .unwrap_or_default();
                    tool_descriptions.push(wf_types::tool_description::ToolDescriptionData {
                        id,
                        r#type: "tool".to_string(),
                        category: None,
                        description,
                        parameters: Vec::new(),
                        tips: None,
                        examples: None,
                    });
                }
            }

            locked.remove_hook();
            (out, tools, tool_descriptions)
        };

        for tool in lua_tools {
            let id = tool.id.clone();
            registrar.register_tool(&id, tool)?;
        }
        for desc in lua_tool_descriptions {
            let id = desc.id.clone();
            registrar.register_tool_description(&id, desc)?;
        }
        for e in entries {
            match e.kind {
                0 => registrar.register_node_type(
                    &e.name,
                    Arc::new(handlers::LuaNodeHandler {
                        lua: self.lua.clone(),
                        func_key: Arc::new(e.key),
                        limits: self.limits,
                    }),
                )?,
                1 => registrar.register_tool_type(
                    &e.name,
                    Arc::new(handlers::LuaToolExecutor {
                        lua: self.lua.clone(),
                        func_key: Arc::new(e.key),
                        limits: self.limits,
                    }),
                )?,
                2 => registrar.register_llm_provider(
                    &e.name,
                    Arc::new(codec::LuaLlmCodec {
                        lua: self.lua.clone(),
                        table_key: Arc::new(e.key),
                        limits: self.limits,
                    }),
                )?,
                4 => registrar.register_event_handler(
                    &e.name,
                    Arc::new(handlers::LuaEventHandler {
                        lua: self.lua.clone(),
                        func_key: Arc::new(e.key),
                        limits: self.limits,
                    }),
                )?,
                6 => registrar.register_middleware(
                    MiddlewarePhase::from(e.phase.as_str()),
                    e.priority,
                    Arc::new(handlers::LuaMiddlewareHandler {
                        lua: self.lua.clone(),
                        func_key: Arc::new(e.key),
                        limits: self.limits,
                    }),
                )?,
                _ => {}
            }
        }
        crate::manifest::register_manifest_tools(&self.manifest, registrar)?;
        Ok(())
    }
}
