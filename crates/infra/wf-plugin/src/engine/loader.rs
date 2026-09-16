use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::fs;

use super::PluginEngine;
use crate::engine::config::validate_manifest;
use crate::error::{PluginError, PluginResult};
use crate::events::PluginEvent;
use crate::manifest::{PluginManifest, PluginType};
use crate::plugin::Plugin;
use crate::registry::{PluginInfo, PluginStatus};

impl PluginEngine {
    pub(crate) async fn load_plugin(
        &self,
        manifest: PluginManifest,
        base: &Path,
    ) -> PluginResult<()> {
        let plugin_id = manifest.id.clone();

        if !self.is_allowed(&plugin_id) {
            return Ok(());
        }

        self.publish(PluginEvent::Loading {
            plugin_id: plugin_id.clone(),
        });

        if let Some(errors) = validate_manifest(&manifest) {
            tracing::warn!("plugin '{}' manifest invalid: {:?}", plugin_id, errors);
            return Err(PluginError::InvalidManifest(errors.join(", ")));
        }

        self.check_permissions(&manifest)?;
        self.check_sdk_version(&manifest)?;

        if manifest.entry_point.ends_with(".wasm") {
            self.verify_wasm_signature(&base.join(&manifest.entry_point), &plugin_id)?;
        }

        let plugin = load_plugin_module_with_base(&manifest, base).await?;
        self.registry.register(manifest, plugin)?;
        self.registry
            .update_status(&plugin_id, PluginStatus::Loaded);

        let version = self
            .registry
            .get(&plugin_id)
            .map(|info| info.manifest.version)
            .unwrap_or_default();
        self.publish(PluginEvent::Loaded {
            plugin_id: plugin_id.clone(),
            version,
        });
        self.publish(PluginEvent::Discovered {
            plugin_id: plugin_id.clone(),
        });
        tracing::info!("discovered plugin: {}", plugin_id);

        Ok(())
    }

    pub async fn discover(&self) -> PluginResult<Vec<PluginInfo>> {
        let manifests = scan_plugin_manifests(&self.options.paths).await?;
        for (manifest, base) in manifests {
            // Installed-but-disabled plugins stay unloaded; plugins absent
            // from the install registry keep loading (registry is an
            // enhancement, not a gate).
            if !self.package_manager.is_enabled(&manifest.id) {
                tracing::info!("plugin '{}' is disabled, skipping", manifest.id);
                continue;
            }
            let _ = self.load_plugin(manifest, &base).await;
        }
        Ok(self.registry.all())
    }

    pub async fn load_single(&self, manifest_path: &Path) -> PluginResult<PluginInfo> {
        if !manifest_path.exists() {
            return Err(PluginError::NotFound(manifest_path.display().to_string()));
        }

        let content = fs::read_to_string(manifest_path)
            .await
            .map_err(PluginError::Io)?;
        let manifest: PluginManifest =
            toml::from_str(&content).map_err(|e| PluginError::InvalidManifest(e.to_string()))?;

        // Set base path from parent dir for loading relative entry points
        let plugin_dir = manifest_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        self.publish(PluginEvent::Loading {
            plugin_id: manifest.id.clone(),
        });
        if manifest.entry_point.ends_with(".wasm") {
            self.verify_wasm_signature(&plugin_dir.join(&manifest.entry_point), &manifest.id)?;
        }
        let plugin = load_plugin_module_with_base(&manifest, &plugin_dir).await?;
        self.registry.register(manifest.clone(), plugin)?;
        self.registry
            .update_status(&manifest.id, PluginStatus::Loaded);

        self.publish(PluginEvent::Loaded {
            plugin_id: manifest.id.clone(),
            version: manifest.version.clone(),
        });
        self.publish(PluginEvent::Discovered {
            plugin_id: manifest.id.clone(),
        });
        tracing::info!("loaded plugin '{}' from {:?}", manifest.id, manifest_path);

        self.registry
            .get(&manifest.id)
            .ok_or_else(|| PluginError::NotFound(manifest.id.clone()))
    }

    /// Register an in-memory plugin instance directly (no manifest file on
    /// disk). Used for programmatically-provided plugins such as the built-in
    /// resource-plugin adapters. The plugin is registered as `Loaded` and can
    /// be activated through [`Self::activate`] like any file-based plugin.
    pub fn register_plugin(
        &self,
        manifest: PluginManifest,
        instance: Arc<dyn Plugin>,
    ) -> PluginResult<()> {
        self.registry.register(manifest.clone(), instance)?;
        self.registry
            .update_status(&manifest.id, PluginStatus::Loaded);
        self.publish(PluginEvent::Discovered {
            plugin_id: manifest.id.clone(),
        });
        tracing::info!("registered in-memory plugin: {}", manifest.id);
        Ok(())
    }

    pub(crate) async fn find_plugin_dir(&self, plugin_id: &str) -> PluginResult<PathBuf> {
        for path in &self.options.paths {
            let candidate = path.join(plugin_id);
            if candidate.exists() && candidate.is_dir() {
                return Ok(candidate);
            }
        }
        Err(PluginError::NotFound(plugin_id.to_owned()))
    }
}

pub(crate) async fn scan_plugin_manifests(
    paths: &[PathBuf],
) -> PluginResult<Vec<(PluginManifest, PathBuf)>> {
    let mut manifests = Vec::new();
    for path in paths {
        if !path.exists() {
            continue;
        }
        let mut read_dir = fs::read_dir(path).await.map_err(PluginError::Io)?;
        while let Some(entry) = read_dir.next_entry().await.map_err(PluginError::Io)? {
            let dir_path = entry.path();
            if !dir_path.is_dir() {
                continue;
            }
            let manifest_path = dir_path.join("plugin.toml");
            if !manifest_path.exists() {
                continue;
            }
            let content = fs::read_to_string(&manifest_path)
                .await
                .map_err(PluginError::Io)?;
            match toml::from_str::<PluginManifest>(&content) {
                Ok(m) => manifests.push((m, dir_path)),
                Err(e) => tracing::warn!("failed to parse {:?}: {}", manifest_path, e),
            }
        }
    }
    Ok(manifests)
}

pub(crate) fn resolve_plugin_type(manifest: &PluginManifest) -> PluginResult<PluginType> {
    if let Some(ref t) = manifest.plugin_type {
        return Ok(t.clone());
    }
    let entry = manifest.entry_point.as_str();
    if entry.ends_with(".lua") {
        return Ok(PluginType::Lua);
    }
    if entry.ends_with(".so") || entry.ends_with(".dylib") || entry.ends_with(".dll") {
        return Ok(PluginType::Native);
    }
    if entry.ends_with(".wasm") {
        return Ok(PluginType::Wasm);
    }
    Err(PluginError::LoadFailed(format!(
        "cannot determine plugin type for '{}': set plugin_type in manifest or use .lua/.so/.dylib/.dll entry point",
        manifest.id
    )))
}

#[cfg_attr(
    not(any(feature = "lua", feature = "native", feature = "wasm")),
    allow(unused_variables)
)]
pub(crate) async fn load_plugin_module_with_base(
    manifest: &PluginManifest,
    base: &Path,
) -> PluginResult<Arc<dyn Plugin>> {
    match resolve_plugin_type(manifest)? {
        #[cfg(feature = "lua")]
        PluginType::Lua => crate::lua::loader::load_lua_plugin_with_base(manifest, base).await,
        #[cfg(not(feature = "lua"))]
        PluginType::Lua => Err(PluginError::LoadFailed("lua feature not enabled".into())),
        #[cfg(feature = "native")]
        PluginType::Native => crate::native::loader::load_native_plugin_with_base(manifest, base),
        #[cfg(not(feature = "native"))]
        PluginType::Native => Err(PluginError::LoadFailed("native feature not enabled".into())),
        #[cfg(feature = "wasm")]
        PluginType::Wasm => crate::wasm::loader::load_wasm_plugin_with_base(manifest, base).await,
        #[cfg(not(feature = "wasm"))]
        PluginType::Wasm => Err(PluginError::LoadFailed("wasm feature not enabled".into())),
    }
}
