use std::sync::Arc;

use tokio::fs;

use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;
use super::plugin::LuaPlugin;

pub async fn load_lua_plugin(manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    let base_path = determine_base_path(manifest)?;
    let entry_path = base_path.join(&manifest.entry_point);
    let script = fs::read_to_string(&entry_path).await
        .map_err(|e| PluginError::LoadFailed(format!("cannot read {:?}: {}", entry_path, e)))?;

    let lua = mlua::Lua::new();

    let result: mlua::Value = lua.load(&script).eval()
        .map_err(|e| PluginError::LoadFailed(format!("lua eval error: {}", e)))?;

    if let mlua::Value::Table(tbl) = &result {
        let globals = lua.globals();
        globals.set("plugin", tbl.clone())
            .map_err(|e| PluginError::LoadFailed(format!("lua set globals error: {}", e)))?;
        drop(globals);
        drop(result);
    } else {
        return Err(PluginError::LoadFailed(
            "lua plugin must return a table".into()
        ));
    }

    Ok(Arc::new(LuaPlugin::new(manifest.clone(), lua)))
}

fn determine_base_path(manifest: &PluginManifest) -> PluginResult<std::path::PathBuf> {
    let candidate = std::path::PathBuf::from("plugins").join(&manifest.id);
    if candidate.join(&manifest.entry_point).exists() {
        return Ok(candidate);
    }
    if std::path::Path::new(&manifest.entry_point).exists() {
        return Ok(std::path::PathBuf::from("."));
    }
    Err(PluginError::LoadFailed(
        format!("cannot find entry point '{}' for plugin '{}'", manifest.entry_point, manifest.id)
    ))
}
