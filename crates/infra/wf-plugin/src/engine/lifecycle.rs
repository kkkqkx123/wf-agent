use serde_json::Value;
use tokio::fs;

use super::PluginEngine;
use crate::context::{PluginContext, PluginLogger};
use crate::dependency::{resolve_dependencies, ResolvedGraph};
use crate::engine::config::validate_manifest;
use crate::engine::loader::{load_plugin_module_with_base, scan_plugin_manifests};
use crate::error::{PluginError, PluginResult};
use crate::events::PluginEvent;
use crate::manifest::PluginManifest;
use crate::registry::{PluginInfo, PluginStatus};

impl PluginEngine {
    pub async fn initialize(&mut self) -> PluginResult<()> {
        if self.initialized {
            tracing::warn!("PluginEngine already initialized");
            return Ok(());
        }

        if !self.options.enabled {
            tracing::info!("plugin system is disabled");
            self.initialized = true;
            return Ok(());
        }

        tracing::info!("initializing plugin engine...");

        self.discover().await?;

        let manifests: Vec<PluginManifest> = self
            .registry
            .all()
            .into_iter()
            .map(|i| i.manifest)
            .collect();
        let resolved = resolve_dependencies(&manifests);
        if let Ok(ref graph) = resolved {
            if !graph.cycles.is_empty() {
                tracing::warn!("plugin dependency cycles detected: {:?}", graph.cycles);
            }
            if !graph.version_mismatches.is_empty() {
                for m in &graph.version_mismatches {
                    tracing::warn!("plugin version mismatch: {}", m);
                }
            }
        }

        let count = self.registry.len();
        tracing::info!("discovered {} plugin(s)", count);

        if self.options.auto_activate {
            for info in self.registry.all() {
                match self.activate(&info.manifest.id).await {
                    Ok(_) => tracing::info!("activated plugin: {}", info.manifest.id),
                    Err(e) => tracing::error!("failed to activate '{}': {}", info.manifest.id, e),
                }
            }
            let active = self.registry.list_by_status(PluginStatus::Active).len();
            tracing::info!("activated {} plugin(s)", active);
        }

        self.initialized = true;
        Ok(())
    }

    pub async fn activate(&self, plugin_id: &str) -> PluginResult<()> {
        let record = self
            .registry
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_owned()))?;

        if record.status != PluginStatus::Loaded {
            return Err(PluginError::InvalidState {
                plugin_id: plugin_id.to_owned(),
                state: format!("{:?}", record.status),
            });
        }

        self.publish(PluginEvent::Activating {
            plugin_id: plugin_id.to_owned(),
        });
        self.registry
            .update_status(plugin_id, PluginStatus::Activating);

        let instance = self
            .registry
            .instance(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_owned()))?;
        let plugin_config = self
            .options
            .config
            .get(plugin_id)
            .cloned()
            .unwrap_or_default();
        let ctx = PluginContext {
            plugin_id: plugin_id.to_owned(),
            sdk_version: self.sdk_version.clone(),
            config: plugin_config,
            logger: PluginLogger,
            contribution_manager: self.contribution_manager.clone(),
        };

        match self.guard.execute(plugin_id, instance.on_load(&ctx)).await {
            Ok(_) => {}
            Err(e) => {
                self.registry.set_error(plugin_id, e.to_string());
                self.publish(PluginEvent::Error {
                    plugin_id: plugin_id.to_owned(),
                    error: e.to_string(),
                });
                return Err(e);
            }
        }

        self.contribution_manager.start_registration(plugin_id);
        self.sync_manifest_llm_providers(plugin_id);
        let mut registrar = self.contribution_manager.as_registrar();

        match self
            .guard
            .execute(plugin_id, async {
                instance.register_contributions(&mut registrar)
            })
            .await
        {
            Ok(_) => {}
            Err(e) => {
                self.registry.set_error(plugin_id, e.to_string());
                self.publish(PluginEvent::Error {
                    plugin_id: plugin_id.to_owned(),
                    error: e.to_string(),
                });
                return Err(e);
            }
        }

        self.check_manifest_contributions(plugin_id);

        // Record the registered contributions on the registry record so
        // `list_by_contribution` / plugin info expose them.
        let records: Vec<crate::registry::ContributionRecord> = self
            .contribution_manager
            .contributions_for(plugin_id)
            .into_iter()
            .map(
                |(contribution_type, key)| crate::registry::ContributionRecord {
                    contribution_type,
                    key,
                    plugin_id: plugin_id.to_owned(),
                },
            )
            .collect();
        self.registry.add_contributions(plugin_id, records);

        if let Some(ref bridge) = self.bridge {
            bridge
                .sync_all(plugin_id, &self.contribution_manager)
                .await?;
        }

        match self
            .guard
            .execute(plugin_id, instance.on_activate(&ctx))
            .await
        {
            Ok(_) => {}
            Err(e) => {
                self.registry.set_error(plugin_id, e.to_string());
                self.publish(PluginEvent::Error {
                    plugin_id: plugin_id.to_owned(),
                    error: e.to_string(),
                });
                return Err(e);
            }
        }

        self.registry.update_status(plugin_id, PluginStatus::Active);
        self.start_event_dispatch(plugin_id);
        self.publish(PluginEvent::Activated {
            plugin_id: plugin_id.to_owned(),
        });
        Ok(())
    }

    pub async fn deactivate(&self, plugin_id: &str) -> PluginResult<()> {
        let record = self.registry.get(plugin_id);
        if record.is_none() {
            return Ok(());
        }

        self.publish(PluginEvent::Deactivating {
            plugin_id: plugin_id.to_owned(),
        });
        self.registry
            .update_status(plugin_id, PluginStatus::Deactivating);

        // Stop forwarding lifecycle events to the plugin's event handlers
        // before removing the contributions themselves.
        self.stop_event_dispatch(plugin_id);

        // Best-effort teardown: every step runs even when an earlier one
        // fails, failures warn and are accumulated into the returned error
        // instead of being silently dropped.
        let mut failures: Vec<String> = Vec::new();

        if let Some(ref bridge) = self.bridge {
            if let Err(e) = bridge
                .unsync_all(plugin_id, &self.contribution_manager)
                .await
            {
                tracing::warn!(
                    plugin_id,
                    "plugin bridge unsync failed during deactivation: {}",
                    e
                );
                failures.push(format!("unsync: {e}"));
            }
        }

        self.contribution_manager.unregister_all(plugin_id);

        // The plugin sees the same config it ran with, not `Value::Null`.
        let plugin_config = self
            .options
            .config
            .get(plugin_id)
            .cloned()
            .unwrap_or(Value::Null);
        if let Some(instance) = self.registry.instance(plugin_id) {
            let ctx = PluginContext {
                plugin_id: plugin_id.to_owned(),
                sdk_version: self.sdk_version.clone(),
                config: plugin_config,
                logger: PluginLogger,
                contribution_manager: self.contribution_manager.clone(),
            };
            if let Err(e) = instance.on_deactivate(&ctx).await {
                tracing::warn!(plugin_id, "on_deactivate failed: {}", e);
                failures.push(format!("on_deactivate: {e}"));
            }
            if let Err(e) = instance.on_unload(&ctx).await {
                tracing::warn!(plugin_id, "on_unload failed: {}", e);
                failures.push(format!("on_unload: {e}"));
            }
        }

        if failures.is_empty() {
            self.registry
                .update_status(plugin_id, PluginStatus::Deactivated);
            self.publish(PluginEvent::Deactivated {
                plugin_id: plugin_id.to_owned(),
            });
            Ok(())
        } else {
            let message = failures.join("; ");
            self.registry.set_error(plugin_id, message.clone());
            self.publish(PluginEvent::Error {
                plugin_id: plugin_id.to_owned(),
                error: message.clone(),
            });
            Err(PluginError::DeactivationFailed(message))
        }
    }

    /// Fully remove a plugin: deactivate it, then remove it from the
    /// registry. Unlike `deactivate`, unloading clears the registry entry so
    /// the plugin must be re-discovered and re-loaded before it can be
    /// activated again.
    pub async fn unload(&mut self, plugin_id: &str) -> PluginResult<()> {
        if !self.registry.has(plugin_id) {
            return Err(PluginError::NotFound(plugin_id.to_owned()));
        }
        let _ = self.deactivate(plugin_id).await;
        self.registry.remove(plugin_id);
        self.options.config.remove(plugin_id);
        Ok(())
    }

    pub async fn shutdown(&self) {
        if !self.initialized {
            return;
        }
        tracing::info!("shutting down plugin engine...");
        let all: Vec<PluginInfo> = self.registry.all();
        for info in all {
            if info.status == PluginStatus::Active || info.status == PluginStatus::Loaded {
                let _ = self.deactivate(&info.manifest.id).await;
            }
        }
        self.registry.clear();
    }

    pub async fn reload(&self, plugin_id: &str) -> PluginResult<()> {
        let plugin_dir = self.find_plugin_dir(plugin_id).await?;
        let manifest_path = plugin_dir.join("plugin.toml");
        let content = fs::read_to_string(&manifest_path)
            .await
            .map_err(PluginError::Io)?;
        let manifest: PluginManifest =
            toml::from_str(&content).map_err(|e| PluginError::InvalidManifest(e.to_string()))?;

        let _ = self.deactivate(plugin_id).await;
        self.registry.remove(plugin_id);

        if manifest.entry_point.ends_with(".wasm") {
            let artifact = plugin_dir.join(&manifest.entry_point);
            self.verify_wasm_signature(&artifact, plugin_id)?;
        }
        let plugin = load_plugin_module_with_base(&manifest, &plugin_dir).await?;
        self.registry.register(manifest, plugin)?;
        self.registry.update_status(plugin_id, PluginStatus::Loaded);

        if self.options.auto_activate {
            self.activate(plugin_id).await?;
        }

        Ok(())
    }

    pub async fn refresh(&self) -> PluginResult<Vec<String>> {
        let current_ids: Vec<String> = self
            .registry
            .all()
            .into_iter()
            .map(|i| i.manifest.id.clone())
            .collect();
        let scanned = scan_plugin_manifests(&self.options.paths).await?;
        let manifests: Vec<(PluginManifest, std::path::PathBuf)> = scanned;

        let new_ids: Vec<String> = manifests.iter().map(|(m, _)| m.id.clone()).collect();
        let removed: Vec<String> = current_ids
            .into_iter()
            .filter(|id| !new_ids.contains(id))
            .collect();
        let added: Vec<String> = new_ids
            .into_iter()
            .filter(|id| !self.registry.has(id))
            .collect();

        for id in &removed {
            let _ = self.deactivate(id).await;
            self.registry.remove(id);
        }

        for (manifest, base) in manifests {
            if added.contains(&manifest.id) {
                let plugin_id = manifest.id.clone();
                if !self.is_allowed(&plugin_id) {
                    continue;
                }
                if let Some(errors) = validate_manifest(&manifest) {
                    tracing::warn!("plugin '{}' manifest invalid: {:?}", plugin_id, errors);
                    continue;
                }
                if let Err(e) = self.check_permissions(&manifest) {
                    tracing::warn!("plugin '{}' refused: {}", plugin_id, e);
                    continue;
                }
                if let Err(e) = self.check_sdk_version(&manifest) {
                    tracing::warn!("plugin '{}' rejected: {}", plugin_id, e);
                    continue;
                }
                if manifest.entry_point.ends_with(".wasm") {
                    if let Err(e) =
                        self.verify_wasm_signature(&base.join(&manifest.entry_point), &plugin_id)
                    {
                        tracing::warn!("plugin '{}' signature rejected: {}", plugin_id, e);
                        continue;
                    }
                }
                let plugin = match load_plugin_module_with_base(&manifest, &base).await {
                    Ok(plugin) => plugin,
                    Err(e) => {
                        tracing::warn!("plugin '{}' load failed: {}", plugin_id, e);
                        continue;
                    }
                };
                if let Err(e) = self.registry.register(manifest, plugin) {
                    tracing::warn!("plugin '{}' registration failed: {}", plugin_id, e);
                    continue;
                }
                self.registry
                    .update_status(&plugin_id, PluginStatus::Loaded);

                if self.options.auto_activate {
                    let _ = self.activate(&plugin_id).await;
                }
            }
        }

        Ok(added)
    }

    pub fn resolved_graph(&self) -> PluginResult<ResolvedGraph> {
        let manifests: Vec<PluginManifest> = self
            .registry
            .all()
            .into_iter()
            .map(|i| i.manifest)
            .collect();
        resolve_dependencies(&manifests)
    }

    /// Cross-check the manifest's declared `contributions` / `hooks`
    /// against what the plugin actually registered. Declarations are
    /// advisory: mismatches warn (they usually mean a stale manifest or a
    /// silently skipped registration) but do not fail activation.
    pub(crate) fn check_manifest_contributions(&self, plugin_id: &str) {
        let manifest = match self.registry.get(plugin_id) {
            Some(info) => info.manifest,
            None => return,
        };
        if manifest.contributions.is_empty()
            && manifest.hooks.is_none()
            && manifest.llm_providers.is_empty()
        {
            return;
        }
        let registered = self.contribution_manager.contributions_for(plugin_id);
        for declared in &manifest.contributions {
            match declared.parse::<crate::contributions::ContributionType>() {
                Err(_) => tracing::warn!(
                    plugin_id,
                    "manifest declares unrecognized contribution type '{declared}'"
                ),
                Ok(kind) => {
                    if !registered.iter().any(|(t, _)| t == kind.as_str()) {
                        tracing::warn!(
                            plugin_id,
                            "manifest declares '{declared}' contributions but none were registered"
                        );
                    }
                }
            }
        }
        if let Some(hooks) = manifest.hooks.as_ref() {
            for key in hooks.keys() {
                if key.trim().is_empty() {
                    tracing::warn!(plugin_id, "manifest declares a hook with an empty name");
                }
            }
        }
        if !manifest.llm_providers.is_empty() {
            let codecs = self.contribution_manager.all_llm_providers();
            for provider in &manifest.llm_providers {
                if !codecs.iter().any(|(name, _)| name == &provider.id)
                    && !codecs
                        .iter()
                        .any(|(name, _)| name.eq_ignore_ascii_case(&provider.format))
                {
                    tracing::warn!(
                        plugin_id,
                        provider = %provider.id,
                        "manifest declares an llm_providers entry with no matching llm-provider codec"
                    );
                }
            }
        }
    }
}
