use std::path::Path;
use std::sync::Arc;

use tokio::fs;

use super::plugin::LuaPlugin;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;

pub async fn load_lua_plugin(manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    let base_path = determine_base_path(manifest)?;
    load_lua_plugin_at(manifest, &base_path).await
}

pub async fn load_lua_plugin_with_base(
    manifest: &PluginManifest,
    base: &Path,
) -> PluginResult<Arc<dyn Plugin>> {
    load_lua_plugin_at(manifest, base).await
}

async fn load_lua_plugin_at(
    manifest: &PluginManifest,
    base_path: &Path,
) -> PluginResult<Arc<dyn Plugin>> {
    let entry_path = base_path.join(&manifest.entry_point);
    let script = fs::read_to_string(&entry_path)
        .await
        .map_err(|e| PluginError::LoadFailed(format!("cannot read {:?}: {}", entry_path, e)))?;

    let lua = mlua::Lua::new();

    apply_sandbox(&lua)?;

    let result: mlua::Value = lua
        .load(&script)
        .eval()
        .map_err(|e| PluginError::LoadFailed(format!("lua eval error: {}", e)))?;

    if let mlua::Value::Table(tbl) = &result {
        let globals = lua.globals();
        globals
            .set("plugin", tbl.clone())
            .map_err(|e| PluginError::LoadFailed(format!("lua set globals error: {}", e)))?;
        drop(globals);
        drop(result);
    } else {
        return Err(PluginError::LoadFailed(
            "lua plugin must return a table".into(),
        ));
    }

    Ok(Arc::new(LuaPlugin::new(manifest.clone(), lua)))
}

fn apply_sandbox(lua: &mlua::Lua) -> PluginResult<()> {
    let denied = ["os", "io", "package", "debug", "ffi"];
    let globals = lua.globals();

    for module in &denied {
        let _ = globals.set(*module, mlua::Value::Nil);
    }

    let safe_print = lua
        .create_function(|_, s: String| {
            tracing::info!("[lua:print] {}", s);
            Ok(())
        })
        .map_err(|e| PluginError::LuaError(e.to_string()))?;
    globals
        .set("print", safe_print)
        .map_err(|e| PluginError::LuaError(e.to_string()))?;

    let safe_require = lua
        .create_function(|_, module_name: String| -> mlua::Result<mlua::Value> {
            Err(mlua::Error::RuntimeError(format!(
                "module '{}' not allowed in plugin sandbox",
                module_name
            )))
        })
        .map_err(|e| PluginError::LuaError(e.to_string()))?;
    globals
        .set("require", safe_require)
        .map_err(|e| PluginError::LuaError(e.to_string()))?;

    Ok(())
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
