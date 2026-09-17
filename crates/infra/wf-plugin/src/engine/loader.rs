use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::fs;

use super::PluginEngine;
use crate::engine::config::{validate_manifest, PluginSystemConfig};
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

        let plugin = load_plugin_module(&manifest, base, &self.options).await?;
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
        let plugin = load_plugin_module(&manifest, &plugin_dir, &self.options).await?;
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

/// Load the backend module for a manifest, applying engine-global limit
/// defaults under the per-plugin manifest values. Native plugins take no
/// limits; only admission checks apply to them. A backend disabled via
/// `PluginSystemConfig` refuses its plugins here with a logged error.
pub(crate) async fn load_plugin_module(
    manifest: &PluginManifest,
    base: &Path,
    options: &PluginSystemConfig,
) -> PluginResult<Arc<dyn Plugin>> {
    let plugin_type = resolve_plugin_type(manifest)?;
    check_backend_gate(&manifest.id, &plugin_type, options)?;
    match plugin_type {
        #[cfg(feature = "lua")]
        PluginType::Lua => {
            crate::lua::loader::load_lua_plugin_with_base_and_defaults(
                manifest,
                base,
                options.lua_defaults.as_ref(),
            )
            .await
        }
        #[cfg(not(feature = "lua"))]
        PluginType::Lua => Err(PluginError::LoadFailed("lua feature not enabled".into())),
        #[cfg(feature = "native")]
        PluginType::Native => crate::native::loader::load_native_plugin_with_base(manifest, base),
        #[cfg(not(feature = "native"))]
        PluginType::Native => Err(PluginError::LoadFailed(
            "native plugins require the `native` feature and a trusted library; untrusted code should use wasm instead".into(),
        )),
        #[cfg(feature = "wasm")]
        PluginType::Wasm => {
            crate::wasm::loader::load_wasm_plugin_with_engine_config(
                manifest,
                base,
                options.wasm_defaults.as_ref(),
                options.guard_timeout_ms,
            )
            .await
        }
        #[cfg(not(feature = "wasm"))]
        PluginType::Wasm => Err(PluginError::LoadFailed("wasm feature not enabled".into())),
    }
}

/// Enforce the per-backend load gates. Disabled backends fail loudly with
/// the flag name so operators can tell "refused by policy" apart from
/// "failed to load". The warning keeps bulk discovery (which drops load
/// errors) observable.
fn check_backend_gate(
    plugin_id: &str,
    plugin_type: &PluginType,
    options: &PluginSystemConfig,
) -> PluginResult<()> {
    let (allowed, flag) = match plugin_type {
        PluginType::Lua => (options.lua_enabled, "lua_enabled"),
        PluginType::Native => (options.native_enabled, "native_enabled"),
        PluginType::Wasm => (options.wasm_enabled, "wasm_enabled"),
    };
    if allowed {
        return Ok(());
    }
    tracing::warn!(
        "plugin '{plugin_id}' refused: {:?} backend is disabled by host policy ({flag}=false)",
        plugin_type
    );
    Err(PluginError::LoadFailed(format!(
        "plugin '{plugin_id}' refused: {flag}=false disables this backend"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;

    fn gate_manifest(id: &str, entry_point: &str) -> PluginManifest {
        PluginManifest {
            id: id.into(),
            version: "1.0.0".into(),
            name: None,
            description: None,
            plugin_type: None,
            sdk_version: None,
            entry_point: entry_point.into(),
            dependencies: Default::default(),
            optional_dependencies: Default::default(),
            contributions: vec![],
            permissions: vec![],
            config_schema: None,
            config: None,
            hooks: None,
            llm_providers: vec![],
            wasm: None,
            lua: None,
        }
    }

    #[test]
    fn backend_gates_default_to_enabled() {
        let options = PluginSystemConfig::default();
        assert!(options.lua_enabled && options.native_enabled && options.wasm_enabled);
    }

    #[test]
    fn disabled_backend_is_refused_with_flag_name() {
        let options = PluginSystemConfig {
            lua_enabled: false,
            ..Default::default()
        };
        let manifest = gate_manifest("gated", "main.lua");
        let err = check_backend_gate(&manifest.id, &PluginType::Lua, &options)
            .expect_err("disabled backend must fail");
        assert!(err.to_string().contains("lua_enabled"), "got: {err}");
    }

    #[tokio::test]
    async fn load_module_honors_gates_without_touching_disk() {
        let options = PluginSystemConfig {
            wasm_enabled: false,
            ..Default::default()
        };
        let manifest = gate_manifest("gated-wasm", "plugin.wasm");
        let err = match load_plugin_module(&manifest, std::path::Path::new("."), &options).await {
            Ok(_) => panic!("gated load must fail"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("wasm_enabled"), "got: {err}");
    }
}
