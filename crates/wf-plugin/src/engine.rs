use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use tokio::fs;

use crate::context::{PluginContext, PluginLogger};
use crate::contributions::{ContributionBridge, ContributionManager, OverridePolicy};
use crate::dependency::{resolve_dependencies, ResolvedGraph};
use crate::error::{PluginError, PluginResult};
use crate::events::PluginEvent;
use crate::guard::PluginGuard;
use crate::manifest::PluginManifest;
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
    initialized: bool,
}

impl PluginEngine {
    pub fn new(
        registry: Arc<PluginRegistry>,
        contribution_manager: Arc<ContributionManager>,
        bridge: Option<Arc<dyn ContributionBridge>>,
        options: PluginSystemConfig,
    ) -> Self {
        let guard = PluginGuard::new(options.guard_timeout_ms);
        contribution_manager.set_override_policy(options.override_policy);
        Self { registry, guard, contribution_manager, bridge, options, event_bus: None, initialized: false }
    }

    pub fn with_event_bus(mut self, event_bus: wf_core::EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    fn publish(&self, event: PluginEvent) {
        let base_event = plugin_event_to_base(&event);
        if let Some(ref bus) = self.event_bus {
            let _ = bus.publish(base_event);
        }
    }

    pub async fn discover(&self) -> PluginResult<Vec<PluginInfo>> {
        let manifests = scan_plugin_manifests(&self.options.paths).await?;
        let mut loaded = Vec::new();

        for manifest in manifests {
            let plugin_id = manifest.id.clone();

            if !self.is_allowed(&plugin_id) {
                continue;
            }

            if let Some(result) = validate_manifest(&manifest) {
                tracing::warn!("plugin '{}' manifest invalid: {:?}", plugin_id, result);
                continue;
            }

            let plugin = load_plugin_module(manifest.clone()).await?;
            self.registry.register(manifest, plugin)?;
            self.registry.update_status(&plugin_id, PluginStatus::Loaded);

            self.publish(PluginEvent::Discovered { plugin_id: plugin_id.clone() });

            tracing::info!("discovered plugin: {}", plugin_id);
            loaded.push(self.registry.get(&plugin_id).unwrap());
        }

        Ok(loaded)
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

        let loaded = self.discover().await?;

        let manifests: Vec<PluginManifest> = loaded.iter().map(|i| i.manifest.clone()).collect();
        let resolved = resolve_dependencies(&manifests);
        if let Ok(ref graph) = resolved {
            if !graph.cycles.is_empty() {
                tracing::warn!("plugin dependency cycles detected: {:?}", graph.cycles);
            }
        }

        tracing::info!("discovered {} plugin(s)", loaded.len());

        if self.options.auto_activate {
            for info in &loaded {
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
        let record = self.registry.get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_owned()))?;

        if record.status != PluginStatus::Loaded {
            return Err(PluginError::InvalidState {
                plugin_id: plugin_id.to_owned(),
                state: format!("{:?}", record.status),
            });
        }

        self.publish(PluginEvent::Activating { plugin_id: plugin_id.to_owned() });
        self.registry.update_status(plugin_id, PluginStatus::Activating);

        let instance = self.registry.instance(plugin_id).unwrap();
        let plugin_config = self.options.config.get(plugin_id).cloned().unwrap_or_default();
        let ctx = PluginContext {
            plugin_id: plugin_id.to_owned(),
            config: plugin_config,
            logger: PluginLogger,
            contribution_manager: self.contribution_manager.clone(),
        };

        match self.guard.execute(plugin_id, instance.on_load(&ctx)).await {
            Ok(_) => {}
            Err(e) => {
                self.registry.set_error(plugin_id, e.to_string());
                self.publish(PluginEvent::Error { plugin_id: plugin_id.to_owned(), error: e.to_string() });
                return Err(e);
            }
        }

        self.contribution_manager.start_registration(plugin_id);
        let mut registrar = self.contribution_manager.as_registrar();

        match self.guard.execute(plugin_id, async {
            instance.register_contributions(&mut registrar);
            PluginResult::Ok(())
        }).await {
            Ok(_) => {}
            Err(e) => {
                self.registry.set_error(plugin_id, e.to_string());
                self.publish(PluginEvent::Error { plugin_id: plugin_id.to_owned(), error: e.to_string() });
                return Err(e);
            }
        }

        if let Some(ref bridge) = self.bridge {
            bridge.sync_all(plugin_id, &self.contribution_manager).await?;
        }

        match self.guard.execute(plugin_id, instance.on_activate(&ctx)).await {
            Ok(_) => {}
            Err(e) => {
                self.registry.set_error(plugin_id, e.to_string());
                self.publish(PluginEvent::Error { plugin_id: plugin_id.to_owned(), error: e.to_string() });
                return Err(e);
            }
        }

        self.registry.update_status(plugin_id, PluginStatus::Active);
        self.publish(PluginEvent::Activated { plugin_id: plugin_id.to_owned() });
        Ok(())
    }

    pub async fn deactivate(&self, plugin_id: &str) -> PluginResult<()> {
        let record = self.registry.get(plugin_id);
        if record.is_none() {
            return Ok(());
        }

        self.publish(PluginEvent::Deactivating { plugin_id: plugin_id.to_owned() });
        self.registry.update_status(plugin_id, PluginStatus::Deactivating);

        if let Some(ref bridge) = self.bridge {
            let _ = bridge.unsync_all(plugin_id, &self.contribution_manager).await;
        }

        self.contribution_manager.unregister_all(plugin_id);

        if let Some(instance) = self.registry.instance(plugin_id) {
            let ctx = PluginContext {
                plugin_id: plugin_id.to_owned(),
                config: Value::Null,
                logger: PluginLogger,
                contribution_manager: self.contribution_manager.clone(),
            };
            let _ = instance.on_deactivate(&ctx).await;
            let _ = instance.on_unload(&ctx).await;
        }

        self.registry.update_status(plugin_id, PluginStatus::Deactivated);
        self.publish(PluginEvent::Deactivated { plugin_id: plugin_id.to_owned() });
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
        let content = fs::read_to_string(&manifest_path).await
            .map_err(PluginError::Io)?;
        let manifest: PluginManifest = toml::from_str(&content)
            .map_err(|e| PluginError::InvalidManifest(e.to_string()))?;

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
        let current_ids: Vec<String> = self.registry.all().into_iter().map(|i| i.manifest.id.clone()).collect();
        let manifests = scan_plugin_manifests(&self.options.paths).await?;

        let new_ids: Vec<String> = manifests.iter().map(|m| m.id.clone()).collect();
        let removed: Vec<String> = current_ids.into_iter().filter(|id| !new_ids.contains(id)).collect();
        let added: Vec<String> = new_ids.into_iter().filter(|id| !self.registry.has(id)).collect();

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
                if let Some(result) = validate_manifest(&manifest) {
                    tracing::warn!("plugin '{}' manifest invalid: {:?}", plugin_id, result);
                    continue;
                }
                let plugin = load_plugin_module(manifest.clone()).await?;
                self.registry.register(manifest, plugin)?;
                self.registry.update_status(&plugin_id, PluginStatus::Loaded);

                if self.options.auto_activate {
                    let _ = self.activate(&plugin_id).await;
                }
            }
        }

        Ok(added)
    }

    pub fn registry(&self) -> &PluginRegistry { &self.registry }
    pub fn contribution_manager(&self) -> &Arc<ContributionManager> { &self.contribution_manager }
    pub fn is_initialized(&self) -> bool { self.initialized }
    pub fn event_bus(&self) -> Option<&wf_core::EventBus> { self.event_bus.as_ref() }
    pub fn resolved_graph(&self) -> PluginResult<ResolvedGraph> {
        let manifests: Vec<PluginManifest> = self.registry.all().into_iter().map(|i| i.manifest).collect();
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
    use wf_types::events::{BaseEvent, EventType};
    use std::collections::HashMap;
    let (etype, meta): (EventType, Option<HashMap<String, serde_json::Value>>) = match event {
        PluginEvent::Discovered { plugin_id } => (EventType::Heartbeat, Some(HashMap::from([("plugin:discovered".into(), serde_json::Value::String(plugin_id.clone()))]))),
        PluginEvent::Loading { plugin_id } => (EventType::Heartbeat, Some(HashMap::from([("plugin:loading".into(), serde_json::Value::String(plugin_id.clone()))]))),
        PluginEvent::Loaded { plugin_id, version } => (EventType::Heartbeat, Some(HashMap::from([
            ("plugin:loaded".into(), serde_json::Value::String(plugin_id.clone())),
            ("version".into(), serde_json::Value::String(version.clone())),
        ]))),
        PluginEvent::Activating { plugin_id } => (EventType::Heartbeat, Some(HashMap::from([("plugin:activating".into(), serde_json::Value::String(plugin_id.clone()))]))),
        PluginEvent::Activated { plugin_id } => (EventType::Heartbeat, Some(HashMap::from([("plugin:activated".into(), serde_json::Value::String(plugin_id.clone()))]))),
        PluginEvent::Deactivating { plugin_id } => (EventType::Heartbeat, Some(HashMap::from([("plugin:deactivating".into(), serde_json::Value::String(plugin_id.clone()))]))),
        PluginEvent::Deactivated { plugin_id } => (EventType::Heartbeat, Some(HashMap::from([("plugin:deactivated".into(), serde_json::Value::String(plugin_id.clone()))]))),
        PluginEvent::Error { plugin_id, error } => (EventType::Error, Some(HashMap::from([
            ("plugin:error".into(), serde_json::Value::String(plugin_id.clone())),
            ("error".into(), serde_json::Value::String(error.clone())),
        ]))),
    };
    BaseEvent {
        id: uuid_or_fallback(),
        r#type: etype,
        timestamp: chrono::Utc::now().timestamp_millis(),
        workflow_id: None,
        execution_id: None,
        agent_loop_id: None,
        metadata: meta,
    }
}

fn uuid_or_fallback() -> String {
    format!("plugin-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0))
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
            let content = fs::read_to_string(&manifest_path).await.map_err(PluginError::Io)?;
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
    if manifest.id.is_empty() { errors.push("id is required".into()); }
    if manifest.version.is_empty() { errors.push("version is required".into()); }
    if manifest.entry_point.is_empty() { errors.push("entry_point is required".into()); }
    if errors.is_empty() { None } else { Some(errors) }
}

async fn load_plugin_module(manifest: PluginManifest) -> PluginResult<Arc<dyn Plugin>> {
    let entry = manifest.entry_point.as_str();

    if entry.ends_with(".lua") {
        return load_lua_plugin(&manifest).await;
    }

    if entry.ends_with(".so") || entry.ends_with(".dylib") || entry.ends_with(".dll") {
        return load_native_plugin(&manifest);
    }

    Err(PluginError::LoadFailed(format!("unsupported entry point: {}", entry)))
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
