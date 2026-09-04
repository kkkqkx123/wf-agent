use std::path::{Component, Path};
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
    validate_plugin_id(&manifest.id)?;
    validate_entry_point(&manifest.entry_point)?;

    let lib_path = base_path.join(&manifest.entry_point);

    if let (Ok(canonical_base), Ok(canonical_lib)) =
        (base_path.canonicalize(), lib_path.canonicalize())
    {
        if !canonical_lib.starts_with(&canonical_base) {
            return Err(PluginError::LoadFailed(format!(
                "native plugin path traversal denied: {:?} escapes base {:?}",
                lib_path, base_path
            )));
        }
    }

    // libloading cannot provide sandbox isolation; this loader only enforces
    // basic path containment. Future isolation should use wasm, not emulation
    // of the lua sandbox.
    let lib = unsafe {
        libloading::Library::new(&lib_path)
            .map_err(|e| PluginError::LoadFailed(format!("cannot load {:?}: {}", lib_path, e)))?
    };

    let plugin = NativePlugin::new(manifest.clone(), lib)?;
    Ok(Arc::new(plugin) as Arc<dyn Plugin>)
}

fn validate_plugin_id(id: &str) -> PluginResult<()> {
    if id.is_empty() {
        return Err(PluginError::LoadFailed("plugin id is empty".into()));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(PluginError::LoadFailed(format!(
            "plugin id '{}' contains path traversal",
            id
        )));
    }
    Ok(())
}

fn validate_entry_point(entry: &str) -> PluginResult<()> {
    let path = Path::new(entry);
    if path.is_absolute() {
        return Err(PluginError::LoadFailed(format!(
            "native entry_point '{}' must be relative",
            entry
        )));
    }
    for comp in path.components() {
        if matches!(comp, Component::ParentDir) {
            return Err(PluginError::LoadFailed(format!(
                "native entry_point '{}' contains parent traversal",
                entry
            )));
        }
    }
    Ok(())
}

fn determine_native_base_path(manifest: &PluginManifest) -> PluginResult<std::path::PathBuf> {
    validate_plugin_id(&manifest.id)?;
    validate_entry_point(&manifest.entry_point)?;

    let candidate = std::path::PathBuf::from("plugins").join(&manifest.id);
    if candidate.join(&manifest.entry_point).exists() {
        return Ok(candidate);
    }
    Err(PluginError::LoadFailed(format!(
        "cannot find entry point '{}' for plugin '{}'",
        manifest.entry_point, manifest.id
    )))
}
