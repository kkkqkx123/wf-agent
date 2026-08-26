use std::path::Path;
use std::sync::Arc;

use super::plugin::NativePlugin;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;

pub fn load_native_plugin(manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    let base_path = determine_native_base_path(manifest)?;
    load_native_plugin_at(manifest, &base_path)
}

pub fn load_native_plugin_with_base(
    manifest: &PluginManifest,
    base: &Path,
) -> PluginResult<Arc<dyn Plugin>> {
    load_native_plugin_at(manifest, base)
}

fn load_native_plugin_at(
    manifest: &PluginManifest,
    base_path: &Path,
) -> PluginResult<Arc<dyn Plugin>> {
    let lib_path = base_path.join(&manifest.entry_point);

    let lib = unsafe {
        libloading::Library::new(&lib_path)
            .map_err(|e| PluginError::LoadFailed(format!("cannot load {:?}: {}", lib_path, e)))?
    };

    let plugin = NativePlugin::new(manifest.clone(), lib)?;
    Ok(Arc::new(plugin) as Arc<dyn Plugin>)
}

fn determine_native_base_path(manifest: &PluginManifest) -> PluginResult<std::path::PathBuf> {
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
