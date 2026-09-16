pub mod config;
pub mod events;
pub mod lifecycle;
pub mod loader;
pub mod package;
pub mod plugin_config;

pub use config::PluginSystemConfig;

use std::collections::HashMap;
use std::sync::Arc;

use crate::contributions::{ContributionBridge, ContributionManager, ContributionRegistrar};
use crate::error::PluginResult;
use crate::event_bus::PluginEventBus;
use crate::guard::PluginGuard;
use crate::manifest::PluginPermission;
use crate::package::PluginPackageManager;
use crate::registry::PluginRegistry;

pub struct PluginEngine {
    pub(crate) registry: Arc<PluginRegistry>,
    pub(crate) guard: PluginGuard,
    pub(crate) contribution_manager: Arc<ContributionManager>,
    pub(crate) bridge: Option<Arc<dyn ContributionBridge>>,
    pub(crate) options: PluginSystemConfig,
    pub(crate) event_bus: Option<wf_core::EventBus>,
    pub(crate) plugin_event_bus: PluginEventBus,
    pub(crate) package_manager: Arc<PluginPackageManager>,
    pub(crate) sdk_version: String,
    pub(crate) initialized: bool,
    pub(crate) event_tasks: Arc<std::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
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

    pub(crate) fn verify_wasm_signature(
        &self,
        artifact: &std::path::Path,
        plugin_id: &str,
    ) -> PluginResult<()> {
        let trust = &self.options.signing;
        if trust.is_empty() {
            return Ok(());
        }
        let status = crate::signing::verify_file(artifact, trust);
        crate::signing::enforce_signature(plugin_id, artifact, &status, trust, "loading")
    }

    pub(crate) fn sync_manifest_llm_providers(&self, plugin_id: &str) {
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
}

pub(crate) fn convert_provider_definition(
    provider: &wf_plugin_sdk::manifest::PluginLlmProviderDefinition,
) -> wf_types::llm::LlmProviderDefinition {
    wf_types::llm::LlmProviderDefinition {
        id: provider.id.clone(),
        name: provider.name.clone(),
        description: provider.description.clone(),
        base_url: provider.base_url.clone(),
        auth_type: provider.auth_type.clone(),
        default_headers: provider.default_headers.clone(),
        format: provider
            .format
            .parse::<wf_types::llm::LlmFormat>()
            .unwrap_or_else(|_| wf_types::llm::LlmFormat::Custom(provider.format.clone())),
        model_discovery: provider.model_discovery.as_ref().map(convert_discovery),
        api_version: provider.api_version.clone(),
        metadata: provider.metadata.clone(),
    }
}

pub(crate) fn convert_discovery(
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
