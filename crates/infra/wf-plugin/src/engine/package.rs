use std::path::Path;
use std::sync::Arc;

use super::PluginEngine;
use crate::error::{PluginError, PluginResult};
use crate::events::PluginEvent;
use crate::manifest::PluginManifest;
use crate::package::{InstalledPlugin, PluginPackageManager};

impl PluginEngine {
    /// Package manager backing install/uninstall/enable/disable state.
    pub fn package_manager(&self) -> Arc<PluginPackageManager> {
        self.package_manager.clone()
    }

    /// Register a plugin directory in the install registry (files are not
    /// copied; the directory must already contain `plugin.toml`).
    pub fn install_from_path(&self, source_path: &Path) -> PluginResult<InstalledPlugin> {
        let manifest_path = source_path.join("plugin.toml");
        let content = std::fs::read_to_string(&manifest_path).map_err(PluginError::Io)?;
        let manifest: PluginManifest =
            toml::from_str(&content).map_err(|e| PluginError::InvalidManifest(e.to_string()))?;
        self.package_manager.install(&manifest, source_path)?;
        tracing::info!("installed plugin '{}' from {:?}", manifest.id, source_path);
        self.package_manager
            .installed()
            .into_iter()
            .find(|p| p.id == manifest.id)
            .ok_or_else(|| PluginError::NotFound(manifest.id.clone()))
    }

    /// Uninstall a plugin: deactivate + unload when loaded, then drop the
    /// registry entry. Plugin files are left in place for the caller.
    pub async fn uninstall(&mut self, plugin_id: &str) -> PluginResult<bool> {
        if self.registry.has(plugin_id) {
            self.unload(plugin_id).await?;
        }
        let removed = self.package_manager.uninstall(plugin_id)?;
        tracing::info!("uninstalled plugin '{}' (removed: {})", plugin_id, removed);
        Ok(removed)
    }

    /// Enable an installed plugin. Persisted; takes effect on next discover.
    pub fn enable(&self, plugin_id: &str) -> PluginResult<bool> {
        let changed = self.package_manager.set_enabled(plugin_id, true)?;
        self.publish(PluginEvent::ConfigChanged {
            plugin_id: plugin_id.to_owned(),
            config: serde_json::json!({ "enabled": true }),
        });
        Ok(changed)
    }

    /// Disable an installed plugin. Persisted; a loaded/active plugin is
    /// deactivated immediately.
    pub async fn disable(&mut self, plugin_id: &str) -> PluginResult<bool> {
        let changed = self.package_manager.set_enabled(plugin_id, false)?;
        if self.registry.has(plugin_id) {
            self.deactivate(plugin_id).await?;
        }
        self.publish(PluginEvent::ConfigChanged {
            plugin_id: plugin_id.to_owned(),
            config: serde_json::json!({ "enabled": false }),
        });
        tracing::info!("disabled plugin '{}'", plugin_id);
        Ok(changed)
    }
}
