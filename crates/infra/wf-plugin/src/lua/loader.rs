use std::path::{Component, Path};
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
    validate_plugin_id(&manifest.id)?;
    validate_entry_point(&manifest.entry_point)?;
    let entry_path = base_path.join(&manifest.entry_point);
    if let (Ok(canonical_base), Ok(canonical_entry)) =
        (base_path.canonicalize(), entry_path.canonicalize())
    {
        if !canonical_entry.starts_with(&canonical_base) {
            return Err(PluginError::LoadFailed(format!(
                "lua plugin path traversal denied: {:?} escapes base {:?}",
                entry_path, base_path
            )));
        }
    }
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

fn validate_plugin_id(id: &str) -> PluginResult<()> {
    if id.is_empty() {
        return Err(PluginError::LoadFailed("plugin id is empty".into()));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(PluginError::LoadFailed(format!(
            "plugin id '{id}' contains path traversal"
        )));
    }
    Ok(())
}

fn validate_entry_point(entry: &str) -> PluginResult<()> {
    let path = Path::new(entry);
    if path.is_absolute() {
        return Err(PluginError::LoadFailed(format!(
            "lua entry_point '{entry}' must be relative"
        )));
    }
    for comp in path.components() {
        if matches!(comp, Component::ParentDir) {
            return Err(PluginError::LoadFailed(format!(
                "lua entry_point '{entry}' contains parent traversal"
            )));
        }
    }
    if !entry.ends_with(".lua") {
        return Err(PluginError::LoadFailed(format!(
            "lua entry_point '{entry}' must end with .lua"
        )));
    }
    Ok(())
}

fn determine_base_path(manifest: &PluginManifest) -> PluginResult<std::path::PathBuf> {
    validate_plugin_id(&manifest.id)?;
    validate_entry_point(&manifest.entry_point)?;
    if let Some(override_path) = std::env::var_os("WF_PLUGIN_ARTIFACT_PATH")
        .or_else(|| std::env::var_os("WF_PLUGIN_BASE_DIR"))
        .or_else(|| std::env::var_os("WF_PLUGIN_DIR"))
    {
        let override_path = std::path::PathBuf::from(override_path);
        if override_path.is_file() {
            if let Some(parent) = override_path.parent() {
                return Ok(parent.to_path_buf());
            }
        }
        let candidate = override_path.join(&manifest.id);
        if candidate.join(&manifest.entry_point).exists() {
            return Ok(candidate);
        }
        if override_path.join(&manifest.entry_point).exists() {
            return Ok(override_path);
        }
    }
    let candidate = std::path::PathBuf::from("plugins").join(&manifest.id);
    if candidate.join(&manifest.entry_point).exists() {
        return Ok(candidate);
    }
    for base in crate::engine::config::default_plugin_scan_paths() {
        let candidate = base.join(&manifest.id);
        if candidate.join(&manifest.entry_point).exists() {
            return Ok(candidate);
        }
    }
    if std::path::Path::new(&manifest.entry_point).exists() {
        return Ok(std::path::PathBuf::from("."));
    }
    Err(PluginError::LoadFailed(format!(
        "cannot find entry point '{}' for plugin '{}'",
        manifest.entry_point, manifest.id
    )))
}
