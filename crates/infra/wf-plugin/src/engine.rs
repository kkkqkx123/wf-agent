use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use tokio::fs;

use crate::context::{PluginContext, PluginLogger};
use crate::contributions::{ContributionBridge, ContributionManager, OverridePolicy};
use crate::dependency::{resolve_dependencies, ResolvedGraph};
use crate::error::{PluginError, PluginResult};
use crate::event_bus::{PluginEventBus, PluginEventSubscription};
use crate::events::PluginEvent;
use crate::guard::PluginGuard;
use crate::manifest::{PluginManifest, PluginType};
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
    pub config: std::collections::HashMap<String, Value>,
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
            config: std::collections::HashMap::new(),
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
    sdk_version: String,
    initialized: bool,
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
        Self {
            registry,
            guard,
            contribution_manager,
            bridge,
            options,
            event_bus: None,
            plugin_event_bus: PluginEventBus::default(),
            sdk_version: sdk_version.to_owned(),
            initialized: false,
        }
    }

    pub fn with_event_bus(mut self, event_bus: wf_core::EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn subscribe(&self) -> PluginEventSubscription {
        self.plugin_event_bus.subscribe()
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

        if let Some(errors) = validate_manifest(&manifest) {
            tracing::warn!("plugin '{}' manifest invalid: {:?}", plugin_id, errors);
            return Err(PluginError::InvalidManifest(errors.join(", ")));
        }

        if let Some(ref sdk_req) = manifest.sdk_version {
            if let Ok(req) = semver::VersionReq::parse(sdk_req) {
                if let Ok(ver) = semver::Version::parse(&self.sdk_version) {
                    if !req.matches(&ver) {
                        return Err(PluginError::InvalidManifest(format!(
                            "sdk version '{}' not satisfied by host '{}'",
                            sdk_req, self.sdk_version
                        )));
                    }
                }
            }
        }

        let plugin = load_plugin_module(manifest.clone()).await?;
        self.registry.register(manifest, plugin)?;
        self.registry
            .update_status(&plugin_id, PluginStatus::Loaded);

        self.publish(PluginEvent::Discovered {
            plugin_id: plugin_id.clone(),
        });
        tracing::info!("discovered plugin: {}", plugin_id);

        Ok(())
    }

    pub async fn discover(&self) -> PluginResult<Vec<PluginInfo>> {
        let manifests = scan_plugin_manifests(&self.options.paths).await?;
        for manifest in manifests {
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
        let plugin = load_plugin_module_with_base(&manifest, &plugin_dir).await?;
        self.registry.register(manifest.clone(), plugin)?;
        self.registry
            .update_status(&manifest.id, PluginStatus::Loaded);

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
        let mut registrar = self.contribution_manager.as_registrar();

        match self
            .guard
            .execute(plugin_id, async {
                instance.register_contributions(&mut registrar);
                PluginResult::Ok(())
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

        if let Some(ref bridge) = self.bridge {
            let _ = bridge
                .unsync_all(plugin_id, &self.contribution_manager)
                .await;
        }

        self.contribution_manager.unregister_all(plugin_id);

        if let Some(instance) = self.registry.instance(plugin_id) {
            let ctx = PluginContext {
                plugin_id: plugin_id.to_owned(),
                sdk_version: self.sdk_version.clone(),
                config: Value::Null,
                logger: PluginLogger,
                contribution_manager: self.contribution_manager.clone(),
            };
            let _ = instance.on_deactivate(&ctx).await;
            let _ = instance.on_unload(&ctx).await;
        }

        self.registry
            .update_status(plugin_id, PluginStatus::Deactivated);
        self.publish(PluginEvent::Deactivated {
            plugin_id: plugin_id.to_owned(),
        });
        Ok(())
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
    Err(PluginError::LoadFailed(format!(
        "cannot determine plugin type for '{}': set plugin_type in manifest or use .lua/.so/.dylib/.dll entry point",
        manifest.id
    )))
}

async fn load_plugin_module(manifest: PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    match resolve_plugin_type(&manifest)? {
        PluginType::Lua => load_lua_plugin(&manifest).await,
        PluginType::Native => load_native_plugin(&manifest),
    }
}

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
    }
}

#[cfg(feature = "lua")]
async fn load_lua_plugin(manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    crate::lua::loader::load_lua_plugin(manifest).await
}

#[cfg(not(feature = "lua"))]
async fn load_lua_plugin(_manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    Err(PluginError::LoadFailed("lua feature not enabled".into()))
}

#[cfg(feature = "native")]
fn load_native_plugin(manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    crate::native::loader::load_native_plugin(manifest)
}

#[cfg(not(feature = "native"))]
fn load_native_plugin(_manifest: &PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    Err(PluginError::LoadFailed("native feature not enabled".into()))
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
        fn register_contributions(&self, registrar: &mut dyn ContributionRegistrar) {
            registrar.register_tool_type("my_tool", Arc::new(NoopToolExecutor));
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
            config: None,
            hooks: None,
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
            fn register_contributions(&self, registrar: &mut dyn ContributionRegistrar) {
                registrar.register_tool_type("", Arc::new(NoopToolExecutor));
                registrar.register_tool_type("  ", Arc::new(NoopToolExecutor));
                registrar.register_tool_type("valid_tool", Arc::new(NoopToolExecutor));
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
}
