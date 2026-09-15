use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use tokio::fs;

use crate::context::{PluginContext, PluginLogger};
use crate::contributions::{
    ContributionBridge, ContributionManager, ContributionRegistrar, OverridePolicy,
};
use crate::dependency::{resolve_dependencies, ResolvedGraph};
use crate::error::{PluginError, PluginResult};
use crate::event_bus::{PluginEventBus, PluginEventSubscription};
use crate::events::PluginEvent;
use crate::guard::PluginGuard;
use crate::manifest::{PluginManifest, PluginPermission, PluginType};
use crate::package::{InstalledPlugin, PluginPackageManager};
use crate::plugin::Plugin;
use crate::registry::{PluginInfo, PluginRegistry, PluginStatus};

pub struct PluginSystemConfig {
    pub enabled: bool,
    pub paths: Vec<PathBuf>,
    pub auto_activate: bool,
    pub guard_timeout_ms: u64,
    pub override_policy: OverridePolicy,
    pub allow_list: Vec<String>,
    pub block_list: Vec<String>,
    /// Plugins declaring any of these permissions are refused at load time.
    pub required_permissions_blocklist: Vec<PluginPermission>,
    pub config: std::collections::HashMap<String, Value>,
    /// How to treat an unparseable `sdk_version` requirement (or host
    /// version): `false` (default) keeps the historical fail-open skip,
    /// `true` rejects the plugin with `InvalidManifest` instead.
    pub strict_sdk_version: bool,
}

impl Default for PluginSystemConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            paths: vec![PathBuf::from("./plugins")],
            auto_activate: true,
            guard_timeout_ms: 10000,
            override_policy: OverridePolicy::Forbid,
            allow_list: vec![],
            block_list: vec![],
            required_permissions_blocklist: vec![],
            config: std::collections::HashMap::new(),
            strict_sdk_version: false,
        }
    }
}

pub struct PluginEngine {
    registry: Arc<PluginRegistry>,
    guard: PluginGuard,
    contribution_manager: Arc<ContributionManager>,
    bridge: Option<Arc<dyn ContributionBridge>>,
    options: PluginSystemConfig,
    event_bus: Option<wf_core::EventBus>,
    plugin_event_bus: PluginEventBus,
    package_manager: Arc<PluginPackageManager>,
    sdk_version: String,
    initialized: bool,
    /// Per-plugin event dispatch tasks (subscribe the plugin event bus and
    /// forward events to the plugin's event-handler contributions). Aborted
    /// on deactivation so the subscription teardown is symmetric.
    event_tasks: Arc<std::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl PluginEngine {
    pub fn new(
        registry: Arc<PluginRegistry>,
        contribution_manager: Arc<ContributionManager>,
        bridge: Option<Arc<dyn ContributionBridge>>,
        options: PluginSystemConfig,
        sdk_version: &str,
    ) -> Self {
        let guard = PluginGuard::new(options.guard_timeout_ms);
        contribution_manager.set_override_policy(options.override_policy);
        let state_dir = options.paths.first().cloned().unwrap_or_default();
        Self {
            registry,
            guard,
            contribution_manager,
            bridge,
            options,
            event_bus: None,
            plugin_event_bus: PluginEventBus::default(),
            package_manager: Arc::new(PluginPackageManager::new(&state_dir)),
            sdk_version: sdk_version.to_owned(),
            initialized: false,
            event_tasks: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn with_event_bus(mut self, event_bus: wf_core::EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn subscribe(&self) -> PluginEventSubscription {
        self.plugin_event_bus.subscribe()
    }

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
        Ok(self
            .package_manager
            .installed()
            .into_iter()
            .find(|p| p.id == manifest.id)
            .expect("just-installed entry present"))
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

    fn publish(&self, event: PluginEvent) {
        let _ = self.plugin_event_bus.publish(event.clone());
        let base_event = plugin_event_to_base(&event);
        if let Some(ref bus) = self.event_bus {
            let _ = bus.publish(base_event);
        }
    }

    async fn load_plugin(&self, manifest: PluginManifest) -> PluginResult<()> {
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

        let blocked = manifest
            .permissions
            .iter()
            .any(|p| self.options.required_permissions_blocklist.contains(p));
        if blocked {
            return Err(PluginError::PermissionDenied {
                plugin_id: plugin_id.clone(),
                reason: format!(
                    "plugin declares permissions blocked by host policy: {:?}",
                    manifest.permissions
                ),
            });
        }

        self.check_sdk_version(&manifest)?;

        let plugin = load_plugin_module(manifest.clone()).await?;
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

    /// Enforce the manifest's `sdk_version` requirement against the host
    /// version. A mismatch always rejects the plugin; unparseable
    /// requirements (or host version) reject only under
    /// `strict_sdk_version`, otherwise they are skipped with a warning
    /// (historical fail-open behavior).
    fn check_sdk_version(&self, manifest: &PluginManifest) -> PluginResult<()> {
        let sdk_req = match manifest.sdk_version.as_deref() {
            Some(req) => req,
            None => return Ok(()),
        };
        let req = match semver::VersionReq::parse(sdk_req) {
            Ok(req) => req,
            Err(e) => {
                let message = format!(
                    "plugin '{}' declares unparseable sdk_version '{}': {}",
                    manifest.id, sdk_req, e
                );
                if self.options.strict_sdk_version {
                    return Err(PluginError::InvalidManifest(message));
                }
                tracing::warn!("{message}; skipping sdk_version check (fail-open)");
                return Ok(());
            }
        };
        let host = match semver::Version::parse(&self.sdk_version) {
            Ok(host) => host,
            Err(e) => {
                let message = format!(
                    "host sdk_version '{}' is unparseable: {}",
                    self.sdk_version, e
                );
                if self.options.strict_sdk_version {
                    return Err(PluginError::InvalidManifest(message));
                }
                tracing::warn!("{message}; skipping sdk_version check (fail-open)");
                return Ok(());
            }
        };
        if !req.matches(&host) {
            return Err(PluginError::InvalidManifest(format!(
                "sdk version '{}' not satisfied by host '{}'",
                sdk_req, self.sdk_version
            )));
        }
        Ok(())
    }

    pub async fn discover(&self) -> PluginResult<Vec<PluginInfo>> {
        let manifests = scan_plugin_manifests(&self.options.paths).await?;
        for manifest in manifests {
            // Installed-but-disabled plugins stay unloaded; plugins absent
            // from the install registry keep loading (registry is an
            // enhancement, not a gate).
            if !self.package_manager.is_enabled(&manifest.id) {
                tracing::info!("plugin '{}' is disabled, skipping", manifest.id);
                continue;
            }
            let _ = self.load_plugin(manifest).await;
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

        Ok(self.registry.get(&manifest.id).unwrap())
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

        let instance = self.registry.instance(plugin_id).unwrap();
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

    /// Spawn the per-plugin event dispatch task: subscribes the plugin event
    /// bus and forwards every event to the plugin's event-handler
    /// contributions. The handle is stored so `deactivate` can abort it —
    /// the subscription teardown is symmetric with activation.
    fn start_event_dispatch(&self, plugin_id: &str) {
        if self.contribution_manager.all_event_handlers().is_empty() {
            return;
        }
        let subscription = self.plugin_event_bus.subscribe();
        let manager = self.contribution_manager.clone();
        let owned_plugin_id = plugin_id.to_owned();
        let handle = tokio::spawn(async move {
            let mut subscription = subscription;
            let plugin_id = owned_plugin_id;
            loop {
                match subscription.recv().await {
                    Ok(event) => {
                        let data = crate::contributions::PluginEventData {
                            event_type: event.event_type().to_string(),
                            data: event.payload(),
                        };
                        for handler in manager.get_event_handlers(data.event_type.as_str()) {
                            if let Err(e) = handler.handle(data.clone()).await {
                                tracing::warn!(
                                    plugin_id = %plugin_id,
                                    event_type = %data.event_type,
                                    "plugin event handler failed: {}",
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        // `PluginEventSubscription::recv` maps Lagged/Closed
                        // onto `PluginError`; a closed bus (engine dropped)
                        // ends the dispatch loop, other errors just log.
                        tracing::warn!(
                            plugin_id = %plugin_id,
                            "plugin event dispatch stopped: {}",
                            e
                        );
                        break;
                    }
                }
            }
        });
        self.event_tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(plugin_id.to_owned(), handle);
    }

    /// Abort the per-plugin event dispatch task (if any).
    fn stop_event_dispatch(&self, plugin_id: &str) {
        if let Some(handle) = self
            .event_tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(plugin_id)
        {
            handle.abort();
        }
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

    /// Current plugin-specific configuration, or `None` when the plugin has
    /// no configuration.
    pub fn get_plugin_config(&self, plugin_id: &str) -> Option<Value> {
        self.options.config.get(plugin_id).cloned()
    }

    pub async fn update_plugin_config(
        &mut self,
        plugin_id: &str,
        config: Value,
    ) -> PluginResult<()> {
        let instance = self
            .registry
            .instance(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_owned()))?;

        let schema = self
            .registry
            .get(plugin_id)
            .and_then(|info| info.manifest.config_schema);
        wf_plugin_sdk::validate_config_for(plugin_id, &config, schema.as_ref())?;

        self.options
            .config
            .insert(plugin_id.to_owned(), config.clone());

        match self
            .guard
            .execute(plugin_id, instance.on_config_change(&config))
            .await
        {
            Ok(_) => {}
            Err(e) => {
                return Err(PluginError::ConfigChangeFailed {
                    plugin_id: plugin_id.to_owned(),
                    message: e.to_string(),
                });
            }
        }

        self.publish(PluginEvent::ConfigChanged {
            plugin_id: plugin_id.to_owned(),
            config,
        });

        // A config change may alter a plugin's declared contributions (wasm
        // guests re-evaluate `register` on reload). Re-sync when the plugin
        // reports a changed declaration; backends with static declarations
        // report no change and skip this entirely.
        if let Err(e) = self.refresh_plugin_contributions(plugin_id).await {
            let message = format!("contribution refresh failed: {e}");
            self.registry.set_error(plugin_id, message.clone());
            self.publish(PluginEvent::Error {
                plugin_id: plugin_id.to_owned(),
                error: message.clone(),
            });
            return Err(PluginError::ConfigChangeFailed {
                plugin_id: plugin_id.to_owned(),
                message,
            });
        }

        Ok(())
    }

    /// Re-read one plugin's contribution declaration and re-sync the
    /// contribution manager when it changed. Returns true when a re-sync
    /// happened. Safe to call for any backend: plugins without dynamic
    /// declarations report no change.
    pub async fn refresh_plugin_contributions(&self, plugin_id: &str) -> PluginResult<bool> {
        let instance = self
            .registry
            .instance(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_owned()))?;
        if !self
            .guard
            .execute(plugin_id, instance.reload_declaration())
            .await?
        {
            return Ok(false);
        }
        if let Some(ref bridge) = self.bridge {
            if let Err(e) = bridge
                .unsync_all(plugin_id, &self.contribution_manager)
                .await
            {
                tracing::warn!(
                    plugin_id,
                    "plugin bridge unsync failed during refresh: {}",
                    e
                );
            }
        }
        self.contribution_manager.unregister_all(plugin_id);
        self.contribution_manager.start_registration(plugin_id);
        self.sync_manifest_llm_providers(plugin_id);
        let mut registrar = self.contribution_manager.as_registrar();
        self.guard
            .execute(plugin_id, async {
                instance.register_contributions(&mut registrar)
            })
            .await?;
        self.check_manifest_contributions(plugin_id);
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
        self.registry.replace_contributions(plugin_id, records);
        if let Some(ref bridge) = self.bridge {
            bridge
                .sync_all(plugin_id, &self.contribution_manager)
                .await?;
        }
        tracing::info!(plugin_id, "plugin contributions refreshed");
        Ok(true)
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

        let plugin = load_plugin_module(manifest.clone()).await?;
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
        let manifests = scan_plugin_manifests(&self.options.paths).await?;

        let new_ids: Vec<String> = manifests.iter().map(|m| m.id.clone()).collect();
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

        for manifest in manifests {
            if added.contains(&manifest.id) {
                let plugin_id = manifest.id.clone();
                if !self.is_allowed(&plugin_id) {
                    continue;
                }
                if let Some(errors) = validate_manifest(&manifest) {
                    tracing::warn!("plugin '{}' manifest invalid: {:?}", plugin_id, errors);
                    continue;
                }
                let plugin = load_plugin_module(manifest.clone()).await?;
                self.registry.register(manifest, plugin)?;
                self.registry
                    .update_status(&plugin_id, PluginStatus::Loaded);

                if self.options.auto_activate {
                    let _ = self.activate(&plugin_id).await;
                }
            }
        }

        Ok(added)
    }

    pub fn registry(&self) -> &PluginRegistry {
        &self.registry
    }
    pub fn contribution_manager(&self) -> &Arc<ContributionManager> {
        &self.contribution_manager
    }
    pub fn plugin_event_bus(&self) -> &PluginEventBus {
        &self.plugin_event_bus
    }
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
    pub fn central_event_bus(&self) -> Option<&wf_core::EventBus> {
        self.event_bus.as_ref()
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

    fn is_allowed(&self, plugin_id: &str) -> bool {
        if !self.options.allow_list.is_empty() {
            return self.options.allow_list.contains(&plugin_id.to_owned());
        }
        if !self.options.block_list.is_empty() {
            return !self.options.block_list.contains(&plugin_id.to_owned());
        }
        true
    }

    /// Cross-check the manifest's declared `contributions` / `hooks`
    /// against what the plugin actually registered. Declarations are
    /// advisory: mismatches warn (they usually mean a stale manifest or a
    /// silently skipped registration) but do not fail activation.
    /// Sync the manifest `llm_providers` segment into the contribution
    /// manager, linked with the `llm-provider` codec contributions.
    ///
    /// Codec registration requires the `llm_codec` permission (the coarse
    /// `llm` permission is accepted with a warning for existing manifests).
    /// Remote model discovery additionally requires `llm_discovery` (or the
    /// coarse `network` permission); without it discovery is downgraded to
    /// `Disabled` so the codec still registers but lists no models.
    fn sync_manifest_llm_providers(&self, plugin_id: &str) {
        let info = match self.registry.get(plugin_id) {
            Some(info) => info,
            None => return,
        };
        if info.manifest.llm_providers.is_empty() {
            return;
        }
        let permissions = &info.manifest.permissions;
        let may_register_codec = permissions.contains(&PluginPermission::LlmCodec)
            || permissions.contains(&PluginPermission::Llm);
        if !may_register_codec {
            tracing::warn!(
                plugin_id,
                "manifest declares llm_providers but lacks the llm_codec permission; skipping"
            );
            return;
        }
        if !permissions.contains(&PluginPermission::LlmCodec)
            && permissions.contains(&PluginPermission::Llm)
        {
            tracing::warn!(
                plugin_id,
                "manifest uses the coarse llm permission for llm_providers; declare llm_codec instead"
            );
        }
        let may_discover = permissions.contains(&PluginPermission::LlmDiscovery)
            || permissions.contains(&PluginPermission::Network);
        for provider in &info.manifest.llm_providers {
            let mut definition = convert_provider_definition(provider);
            match definition.model_discovery {
                Some(wf_types::llm::ModelDiscovery::ModelsEndpoint { .. })
                | Some(wf_types::llm::ModelDiscovery::CustomEndpoint { .. })
                    if !may_discover =>
                {
                    tracing::warn!(
                        plugin_id,
                        provider = %definition.id,
                        "provider discovery needs the llm_discovery permission; downgrading to disabled"
                    );
                    definition.model_discovery = Some(wf_types::llm::ModelDiscovery::Disabled);
                }
                _ => {}
            }
            let mut registrar = self.contribution_manager.as_registrar();
            if let Err(e) = registrar.register_llm_provider_definition(definition) {
                tracing::warn!(plugin_id, "manifest llm_providers sync failed: {e}");
            }
        }
    }

    fn check_manifest_contributions(&self, plugin_id: &str) {
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

    async fn find_plugin_dir(&self, plugin_id: &str) -> PluginResult<PathBuf> {
        for path in &self.options.paths {
            let candidate = path.join(plugin_id);
            if candidate.exists() && candidate.is_dir() {
                return Ok(candidate);
            }
        }
        Err(PluginError::NotFound(plugin_id.to_owned()))
    }
}

/// Convert a manifest provider entry into the host provider definition.
fn convert_provider_definition(
    provider: &wf_plugin_sdk::manifest::PluginLlmProviderDefinition,
) -> wf_types::llm::LlmProviderDefinition {
    wf_types::llm::LlmProviderDefinition {
        id: provider.id.clone(),
        name: provider.name.clone(),
        description: provider.description.clone(),
        base_url: provider.base_url.clone(),
        auth_type: provider.auth_type.clone(),
        default_headers: provider.default_headers.clone(),
        format: provider.format.clone(),
        model_discovery: provider.model_discovery.as_ref().map(convert_discovery),
        api_version: provider.api_version.clone(),
        metadata: provider.metadata.clone(),
    }
}

fn convert_discovery(
    discovery: &wf_plugin_sdk::manifest::PluginModelDiscovery,
) -> wf_types::llm::ModelDiscovery {
    use wf_plugin_sdk::manifest::PluginModelDiscovery as From;
    use wf_types::llm::ModelDiscovery as To;
    match discovery {
        From::ModelsEndpoint { path, json_path } => To::ModelsEndpoint {
            path: path.clone(),
            json_path: json_path.clone(),
        },
        From::CustomEndpoint {
            url,
            method,
            headers,
            json_path,
        } => To::CustomEndpoint {
            url: url.clone(),
            method: method.clone(),
            headers: headers.clone(),
            json_path: json_path.clone(),
        },
        From::StaticList { models } => To::StaticList {
            models: models
                .iter()
                .map(|m| wf_types::llm::ModelInfo {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    context_window_size: m.context_window_size,
                    metadata: m.metadata.clone(),
                })
                .collect(),
        },
        From::Disabled => To::Disabled,
    }
}

fn plugin_event_to_base(event: &PluginEvent) -> wf_types::events::BaseEvent {
    use std::collections::HashMap;
    use wf_types::events::{BaseEvent, EventType};

    let (etype, meta): (EventType, Option<HashMap<String, serde_json::Value>>) = match event {
        PluginEvent::Discovered { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:discovered".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Loading { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:loading".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Loaded { plugin_id, version } => (
            EventType::Heartbeat,
            Some(HashMap::from([
                (
                    "plugin:loaded".into(),
                    serde_json::Value::String(plugin_id.clone()),
                ),
                ("version".into(), serde_json::Value::String(version.clone())),
            ])),
        ),
        PluginEvent::Activating { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:activating".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Activated { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:activated".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Deactivating { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:deactivating".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Deactivated { plugin_id } => (
            EventType::Heartbeat,
            Some(HashMap::from([(
                "plugin:deactivated".into(),
                serde_json::Value::String(plugin_id.clone()),
            )])),
        ),
        PluginEvent::Error { plugin_id, error } => (
            EventType::Error,
            Some(HashMap::from([
                (
                    "plugin:error".into(),
                    serde_json::Value::String(plugin_id.clone()),
                ),
                ("error".into(), serde_json::Value::String(error.clone())),
            ])),
        ),
        PluginEvent::ConfigChanged { plugin_id, config } => (
            EventType::Heartbeat,
            Some(HashMap::from([
                (
                    "plugin:config-changed".into(),
                    serde_json::Value::String(plugin_id.clone()),
                ),
                ("config".into(), config.clone()),
            ])),
        ),
    };
    BaseEvent {
        id: uuid_or_fallback(),
        r#type: etype,
        timestamp: chrono::Utc::now().timestamp_millis(),
        workflow_id: None,
        execution_id: None,
        agent_loop_id: None,

        event_name: None,
        metadata: meta,
    }
}

fn uuid_or_fallback() -> String {
    format!(
        "plugin-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    )
}

// ============================================================
// Standalone loading helpers
// ============================================================

async fn scan_plugin_manifests(paths: &[PathBuf]) -> PluginResult<Vec<PluginManifest>> {
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
                Ok(m) => manifests.push(m),
                Err(e) => tracing::warn!("failed to parse {:?}: {}", manifest_path, e),
            }
        }
    }
    Ok(manifests)
}

fn validate_manifest(manifest: &PluginManifest) -> Option<Vec<String>> {
    let mut errors = Vec::new();
    if manifest.id.is_empty() {
        errors.push("id is required".into());
    }
    if manifest.version.is_empty() {
        errors.push("version is required".into());
    }
    if manifest.entry_point.is_empty() {
        errors.push("entry_point is required".into());
    }
    if manifest.sdk_version.is_some() && manifest.sdk_version.as_deref() == Some("") {
        errors.push("sdk_version must not be empty when present".into());
    }
    if errors.is_empty() {
        None
    } else {
        Some(errors)
    }
}

fn resolve_plugin_type(manifest: &PluginManifest) -> PluginResult<PluginType> {
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

async fn load_plugin_module(manifest: PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    match resolve_plugin_type(&manifest)? {
        #[cfg(feature = "lua")]
        PluginType::Lua => load_lua_plugin(&manifest).await,
        #[cfg(not(feature = "lua"))]
        PluginType::Lua => Err(PluginError::LoadFailed("lua feature not enabled".into())),
        #[cfg(feature = "native")]
        PluginType::Native => load_native_plugin(&manifest),
        #[cfg(not(feature = "native"))]
        PluginType::Native => Err(PluginError::LoadFailed("native feature not enabled".into())),
        #[cfg(feature = "wasm")]
        PluginType::Wasm => load_wasm_plugin(&manifest).await,
        #[cfg(not(feature = "wasm"))]
        PluginType::Wasm => Err(PluginError::LoadFailed("wasm feature not enabled".into())),
    }
}

#[cfg_attr(
    not(any(feature = "lua", feature = "native", feature = "wasm")),
    allow(unused_variables)
)]
async fn load_plugin_module_with_base(
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

#[cfg(feature = "lua")]
async fn load_lua_plugin(manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    crate::lua::loader::load_lua_plugin(manifest).await
}

#[cfg(feature = "wasm")]
async fn load_wasm_plugin(manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    crate::wasm::loader::load_wasm_plugin(manifest).await
}

#[cfg(feature = "native")]
fn load_native_plugin(manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    crate::native::loader::load_native_plugin(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contributions::types::PluginToolExecutor;
    use crate::contributions::PluginToolContext;
    use crate::contributions::PluginToolResult;
    use crate::ContributionRegistrar;
    use async_trait::async_trait;

    struct TestPlugin {
        manifest: PluginManifest,
    }

    #[async_trait]
    impl Plugin for TestPlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }
        fn register_contributions(
            &self,
            registrar: &mut dyn ContributionRegistrar,
        ) -> PluginResult<()> {
            registrar.register_tool_type("my_tool", Arc::new(NoopToolExecutor))?;
            Ok(())
        }
    }

    struct NoopToolExecutor;

    #[async_trait]
    impl PluginToolExecutor for NoopToolExecutor {
        async fn execute(&self, _ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
            Ok(PluginToolResult {
                result: serde_json::json!({}),
            })
        }
    }

    /// Plugin with a changing declaration: every reload bumps the
    /// generation, and registration exposes the current generation as a
    /// tool name.
    struct VersionedPlugin {
        manifest: PluginManifest,
        generation: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl Plugin for VersionedPlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }
        async fn reload_declaration(&self) -> PluginResult<bool> {
            self.generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(true)
        }
        fn register_contributions(
            &self,
            registrar: &mut dyn ContributionRegistrar,
        ) -> PluginResult<()> {
            let generation = self.generation.load(std::sync::atomic::Ordering::SeqCst);
            registrar
                .register_tool_type(&format!("tool_v{generation}"), Arc::new(NoopToolExecutor))?;
            Ok(())
        }
    }

    fn make_engine(enabled: bool) -> PluginEngine {
        let registry = Arc::new(PluginRegistry::new());
        let manager = Arc::new(ContributionManager::new());
        let options = PluginSystemConfig {
            enabled,
            ..PluginSystemConfig::default()
        };
        PluginEngine::new(registry, manager, None, options, "0.1.0")
    }

    fn make_manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.into(),
            version: "1.0.0".into(),
            name: None,
            description: None,
            plugin_type: None,
            sdk_version: None,
            entry_point: "entry.so".into(),
            dependencies: Default::default(),
            optional_dependencies: Default::default(),
            contributions: Default::default(),
            permissions: vec![],
            config_schema: None,
            config: None,
            hooks: None,
            llm_providers: vec![],
            wasm: None,
        }
    }

    async fn load_and_activate(engine: &PluginEngine, id: &str) {
        engine
            .registry
            .register(
                make_manifest(id),
                Arc::new(TestPlugin {
                    manifest: make_manifest(id),
                }),
            )
            .unwrap();
        engine.registry.update_status(id, PluginStatus::Loaded);
        engine.activate(id).await.unwrap();
    }

    #[tokio::test]
    async fn activate_populates_contribution_records() {
        let engine = make_engine(true);
        load_and_activate(&engine, "test-plugin").await;

        let info = engine.registry.get("test-plugin").unwrap();
        assert_eq!(info.status, PluginStatus::Active);
        assert!(!info.contributions.is_empty(), "contributions recorded");
        assert!(info
            .contributions
            .iter()
            .any(|c| c.contribution_type == "tool-type" && c.key == "my_tool"));

        let by_tool = engine.registry.list_by_contribution("tool-type");
        assert_eq!(by_tool.len(), 1);
        assert_eq!(by_tool[0].plugin_id, "test-plugin");

        // The contribution is queryable through the manager.
        assert!(engine
            .contribution_manager()
            .get_tool_executor("my_tool")
            .is_some());
    }

    #[tokio::test]
    async fn refresh_replaces_changed_contributions() {
        let engine = make_engine(true);
        let manifest = make_manifest("versioned");
        engine
            .registry
            .register(
                manifest.clone(),
                Arc::new(VersionedPlugin {
                    manifest,
                    generation: std::sync::atomic::AtomicUsize::new(0),
                }),
            )
            .unwrap();
        engine
            .registry
            .update_status("versioned", PluginStatus::Loaded);
        engine.activate("versioned").await.unwrap();
        assert!(engine
            .contribution_manager()
            .get_tool_executor("tool_v0")
            .is_some());

        assert!(engine
            .refresh_plugin_contributions("versioned")
            .await
            .unwrap());
        let manager = engine.contribution_manager();
        assert!(manager.get_tool_executor("tool_v1").is_some());
        assert!(manager.get_tool_executor("tool_v0").is_none());

        let tools = engine.registry.list_by_contribution("tool-type");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].key, "tool_v1");
    }

    #[tokio::test]
    async fn refresh_is_noop_for_static_declarations() {
        let engine = make_engine(true);
        load_and_activate(&engine, "test-plugin").await;
        assert!(!engine
            .refresh_plugin_contributions("test-plugin")
            .await
            .unwrap());
        assert!(engine
            .contribution_manager()
            .get_tool_executor("my_tool")
            .is_some());
    }

    #[tokio::test]
    async fn get_plugin_config_returns_current_config() {
        let mut engine = make_engine(true);
        assert!(engine.get_plugin_config("test-plugin").is_none());

        load_and_activate(&engine, "test-plugin").await;
        engine
            .update_plugin_config("test-plugin", serde_json::json!({"a": 1}))
            .await
            .unwrap();
        assert_eq!(
            engine.get_plugin_config("test-plugin"),
            Some(serde_json::json!({"a": 1}))
        );

        // Updating a missing plugin errors and stores no config.
        let err = engine
            .update_plugin_config("missing", serde_json::json!({"b": 2}))
            .await
            .unwrap_err();
        assert!(matches!(err, PluginError::NotFound(_)));
        assert!(engine.get_plugin_config("missing").is_none());
    }

    #[tokio::test]
    async fn unload_removes_plugin_and_config() {
        let mut engine = make_engine(true);
        load_and_activate(&engine, "test-unload").await;

        engine
            .update_plugin_config("test-unload", serde_json::json!({"x": 1}))
            .await
            .unwrap();
        assert!(engine.get_plugin_config("test-unload").is_some());

        engine.unload("test-unload").await.unwrap();

        // Registry entry is gone (must be re-discovered before re-activation)
        // and the config is cleared.
        assert!(!engine.registry.has("test-unload"));
        assert!(engine.get_plugin_config("test-unload").is_none());
        // Manager contributions were cleaned up by the deactivate path.
        assert!(engine
            .contribution_manager()
            .get_tool_executor("my_tool")
            .is_none());

        // Unloading an unknown plugin is an error.
        let err = engine.unload("test-unknown").await.unwrap_err();
        assert!(matches!(err, PluginError::NotFound(_)));
    }

    #[tokio::test]
    async fn deactivate_keeps_registry_entry_while_removing_contributions() {
        let engine = make_engine(true);
        load_and_activate(&engine, "test-deactivate").await;

        engine.deactivate("test-deactivate").await.unwrap();
        assert!(engine.registry.has("test-deactivate"));
        assert_eq!(
            engine.registry.get("test-deactivate").unwrap().status,
            PluginStatus::Deactivated
        );
        // Manager-level contributions are removed, registry records remain.
        assert!(engine
            .contribution_manager()
            .get_tool_executor("my_tool")
            .is_none());
        assert!(!engine
            .registry
            .get("test-deactivate")
            .unwrap()
            .contributions
            .is_empty());
    }

    #[tokio::test]
    async fn panicking_on_load_does_not_break_the_engine() {
        struct PanicPlugin {
            manifest: PluginManifest,
        }

        #[async_trait]
        impl Plugin for PanicPlugin {
            fn manifest(&self) -> &PluginManifest {
                &self.manifest
            }
            async fn on_load(&self, _ctx: &PluginContext) -> PluginResult<()> {
                panic!("on_load bug");
            }
        }

        let engine = make_engine(true);
        engine
            .registry
            .register(
                make_manifest("panic-plugin"),
                Arc::new(PanicPlugin {
                    manifest: make_manifest("panic-plugin"),
                }),
            )
            .unwrap();
        engine
            .registry
            .update_status("panic-plugin", PluginStatus::Loaded);

        let err = engine.activate("panic-plugin").await.unwrap_err();
        assert!(matches!(err, PluginError::PluginPanic { .. }));
        assert_eq!(
            engine.registry.get("panic-plugin").unwrap().status,
            PluginStatus::Error
        );

        // The engine still works for other plugins.
        load_and_activate(&engine, "after-panic").await;
        assert_eq!(
            engine.registry.get("after-panic").unwrap().status,
            PluginStatus::Active
        );
    }

    #[tokio::test]
    async fn invalid_contributions_are_rejected() {
        struct EmptyKeyPlugin {
            manifest: PluginManifest,
        }

        #[async_trait]
        impl Plugin for EmptyKeyPlugin {
            fn manifest(&self) -> &PluginManifest {
                &self.manifest
            }
            fn register_contributions(
                &self,
                registrar: &mut dyn ContributionRegistrar,
            ) -> PluginResult<()> {
                // Invalid keys report errors without registering; the
                // plugin skips them and continues with the valid one.
                let _ = registrar.register_tool_type("", Arc::new(NoopToolExecutor));
                let _ = registrar.register_tool_type("  ", Arc::new(NoopToolExecutor));
                registrar.register_tool_type("valid_tool", Arc::new(NoopToolExecutor))?;
                Ok(())
            }
        }

        let engine = make_engine(true);
        engine
            .registry
            .register(
                make_manifest("empty-key"),
                Arc::new(EmptyKeyPlugin {
                    manifest: make_manifest("empty-key"),
                }),
            )
            .unwrap();
        engine
            .registry
            .update_status("empty-key", PluginStatus::Loaded);
        engine.activate("empty-key").await.unwrap();

        // Only the valid key was registered.
        assert!(engine
            .contribution_manager()
            .get_tool_executor("valid_tool")
            .is_some());
        assert!(engine
            .contribution_manager()
            .get_tool_executor("")
            .is_none());
    }

    #[cfg(feature = "wasm")]
    #[tokio::test]
    async fn wasm_plugin_load_single_activate_deactivate() {
        let dir = std::env::temp_dir().join("wf-wasm-test-engine");
        let plugin_dir = dir.join("echo-wasm");
        let _ = std::fs::create_dir_all(&plugin_dir);
        let wat = crate::wasm::loader::wasm_test_echo_wat(r#"{"tool_types":["echo_tool"]}"#);
        let bytes = wat::parse_str(&wat).expect("valid wat");
        std::fs::write(plugin_dir.join("plugin.wasm"), &bytes).expect("write module");
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            "id = \"echo-wasm\"\nversion = \"1.0.0\"\nentry_point = \"plugin.wasm\"\n",
        )
        .expect("write manifest");

        let engine = make_engine(true);
        let info = engine
            .load_single(&plugin_dir.join("plugin.toml"))
            .await
            .expect("load_single resolves .wasm entry point");
        assert_eq!(info.status, PluginStatus::Loaded);

        engine.activate("echo-wasm").await.expect("activate");
        assert_eq!(
            engine.registry.get("echo-wasm").unwrap().status,
            PluginStatus::Active
        );

        let executor = engine
            .contribution_manager()
            .get_tool_executor("echo_tool")
            .expect("wasm tool contribution visible");
        let out = executor
            .execute(crate::contributions::PluginToolContext {
                args: serde_json::json!({}),
            })
            .await
            .expect("wasm tool executes");
        assert_eq!(out.result["echo"], true);

        engine.deactivate("echo-wasm").await.expect("deactivate");
        assert_eq!(
            engine.registry.get("echo-wasm").unwrap().status,
            PluginStatus::Deactivated
        );
        assert!(engine
            .contribution_manager()
            .get_tool_executor("echo_tool")
            .is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn forbid_conflict_aborts_activation() {
        struct ClaimingPlugin {
            manifest: PluginManifest,
        }

        #[async_trait]
        impl Plugin for ClaimingPlugin {
            fn manifest(&self) -> &PluginManifest {
                &self.manifest
            }
            fn register_contributions(
                &self,
                registrar: &mut dyn ContributionRegistrar,
            ) -> PluginResult<()> {
                registrar.register_tool_type("my_tool", Arc::new(NoopToolExecutor))?;
                Ok(())
            }
        }

        let engine = make_engine(true);
        load_and_activate(&engine, "owner").await;

        engine
            .registry
            .register(
                make_manifest("claimer"),
                Arc::new(ClaimingPlugin {
                    manifest: make_manifest("claimer"),
                }),
            )
            .unwrap();
        engine
            .registry
            .update_status("claimer", PluginStatus::Loaded);
        // Default policy is `Forbid`: activation fails and the error is
        // recorded on the registry instead of dropping the conflict.
        let err = engine.activate("claimer").await.unwrap_err();
        assert!(matches!(err, PluginError::ContributionConflict(_)));
        assert_eq!(
            engine.registry.get("claimer").unwrap().status,
            PluginStatus::Error
        );
        // The original owner's contribution is untouched.
        assert!(engine
            .contribution_manager()
            .get_tool_executor("my_tool")
            .is_some());
    }

    #[tokio::test]
    async fn deactivation_reports_lifecycle_failures() {
        struct FailingDeactivate {
            manifest: PluginManifest,
        }

        #[async_trait]
        impl Plugin for FailingDeactivate {
            fn manifest(&self) -> &PluginManifest {
                &self.manifest
            }
            async fn on_deactivate(&self, _ctx: &PluginContext) -> PluginResult<()> {
                Err(PluginError::Internal("cannot stop".into()))
            }
        }

        let engine = make_engine(true);
        engine
            .registry
            .register(
                make_manifest("flaky"),
                Arc::new(FailingDeactivate {
                    manifest: make_manifest("flaky"),
                }),
            )
            .unwrap();
        engine.registry.update_status("flaky", PluginStatus::Loaded);
        engine.activate("flaky").await.unwrap();

        let err = engine.deactivate("flaky").await.unwrap_err();
        assert!(matches!(err, PluginError::DeactivationFailed(_)));
        assert_eq!(
            engine.registry.get("flaky").unwrap().status,
            PluginStatus::Error
        );
    }

    #[tokio::test]
    async fn strict_sdk_version_rejects_unparseable_requirement() {
        let registry = Arc::new(PluginRegistry::new());
        let manager = Arc::new(ContributionManager::new());
        let options = PluginSystemConfig {
            strict_sdk_version: true,
            ..PluginSystemConfig::default()
        };
        let engine = PluginEngine::new(registry, manager, None, options, "0.1.0");

        let mut manifest = make_manifest("strict-sdk");
        manifest.sdk_version = Some("not-a-version".into());
        let err = engine.load_plugin(manifest).await.unwrap_err();
        assert!(matches!(err, PluginError::InvalidManifest(_)));

        // Fail-open (default) still skips the check with a warning.
        let engine = make_engine(true);
        let mut manifest = make_manifest("lenient-sdk");
        manifest.sdk_version = Some("not-a-version".into());
        // No loadable module exists for the fake entry point, so a
        // fail-open check proceeds past the sdk gate into module loading.
        let err = engine.load_plugin(manifest).await.unwrap_err();
        assert!(!matches!(err, PluginError::InvalidManifest(_)));
    }
}
