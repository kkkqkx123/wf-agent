use std::path::Path;
use std::sync::Arc;

use tokio::fs;

use super::plugin::LuaPlugin;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;
use wf_plugin_sdk::manifest::LuaConfig;

pub async fn load_lua_plugin(manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    let base_path = determine_base_path(manifest)?;
    load_lua_plugin_at(manifest, &base_path, None).await
}

pub async fn load_lua_plugin_with_base(
    manifest: &PluginManifest,
    base: &Path,
) -> PluginResult<Arc<dyn Plugin>> {
    load_lua_plugin_at(manifest, base, None).await
}

pub async fn load_lua_plugin_with_base_and_defaults(
    manifest: &PluginManifest,
    base: &Path,
    engine_defaults: Option<&LuaConfig>,
) -> PluginResult<Arc<dyn Plugin>> {
    load_lua_plugin_at(manifest, base, engine_defaults).await
}

async fn load_lua_plugin_at(
    manifest: &PluginManifest,
    base_path: &Path,
    engine_defaults: Option<&LuaConfig>,
) -> PluginResult<Arc<dyn Plugin>> {
    let entry_path = base_path.join(&manifest.entry_point);
    let script = fs::read_to_string(&entry_path)
        .await
        .map_err(|e| PluginError::LoadFailed(format!("cannot read {:?}: {}", entry_path, e)))?;
    if script.trim().is_empty() {
        return Err(PluginError::LoadFailed("lua plugin script is empty".into()));
    }

    let lua = super::pool::create_state(&script)?;
    let limits = super::pool::resolve_limits(manifest.lua.as_ref(), engine_defaults);
    tracing::info!(
        "lua plugin '{}' limits: timeout={} memory={}KB",
        manifest.id,
        limits
            .timeout
            .map(|t| format!("{}ms", t.as_millis()))
            .unwrap_or_else(|| "off".into()),
        limits.memory_limit_kb,
    );

    {
        let plugin_value: mlua::Value = lua.globals().get("plugin").map_err(|e| {
            PluginError::LoadFailed(format!("lua plugin must set global 'plugin': {}", e))
        })?;
        let plugin_table = match plugin_value {
            mlua::Value::Table(table) => table,
            _ => {
                return Err(PluginError::LoadFailed(
                    "lua plugin must return a table".into(),
                ));
            }
        };
        validate_priority(&plugin_table)?;
    }

    Ok(Arc::new(
        LuaPlugin::new(manifest.clone(), lua, Arc::new(script)).with_limits(limits),
    ))
}

fn validate_priority(plugin_table: &mlua::Table) -> PluginResult<()> {
    let raw: mlua::Value = plugin_table.get("priority").unwrap_or(mlua::Value::Nil);
    match raw {
        mlua::Value::Nil => Ok(()),
        mlua::Value::Integer(_) => Ok(()),
        mlua::Value::Number(n) if n.is_finite() && n.fract() == 0.0 => Ok(()),
        _ => Err(PluginError::LoadFailed(
            "lua plugin 'priority' must be an integer".into(),
        )),
    }
}

fn determine_base_path(manifest: &PluginManifest) -> PluginResult<std::path::PathBuf> {
    let candidate = std::path::PathBuf::from("plugins").join(&manifest.id);
    if candidate.join(&manifest.entry_point).exists() {
        return Ok(candidate);
    }
    if std::path::Path::new(&manifest.entry_point).exists() {
        return Ok(std::path::PathBuf::from("."));
    }
    Err(PluginError::LoadFailed(format!(
        "cannot find entry point '{}' for plugin '{}'",
        manifest.entry_point, manifest.id
    )))
}
