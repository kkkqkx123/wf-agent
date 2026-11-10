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
use crate::manifest::{PluginManifest, PluginPermission};
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
    pub(crate) initialized: bool,
    pub(crate) event_tasks: Arc<std::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl PluginEngine {
    pub fn new(
        registry: Arc<PluginRegistry>,
        contribution_manager: Arc<ContributionManager>,
        bridge: Option<Arc<dyn ContributionBridge>>,
        options: PluginSystemConfig,
    ) -> Self {
        let guard = PluginGuard::new(options.guard_timeout_ms);
        if options.guard_timeout_ms == 0 {
            tracing::error!(
                "plugin guard timeout is disabled; backend per-call limits are the only time backstop"
            );
        }
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

    pub(crate) fn verify_artifact_signature(
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

    /// Verify the signature of every artifact a manifest points at.
    /// All backends share the load-time check when trust is configured;
    /// without trust this stays a no-op so directory-discovered plugins
    /// keep loading unchanged.
    ///
    /// Signature checking is plain file hashing and does not require any
    /// backend to be compiled in, so it stays available in trimmed builds.
    pub(crate) fn verify_artifacts(
        &self,
        base: &std::path::Path,
        manifest: &PluginManifest,
    ) -> PluginResult<()> {
        for entry in crate::manifest::all_entry_points(manifest) {
            self.verify_artifact_signature(&base.join(&entry), &manifest.id)?;
        }
        Ok(())
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
        let may_register_auth = permissions.contains(&PluginPermission::LlmAuth);
        if !may_register_codec
            && !(may_register_auth
                && info
                    .manifest
                    .llm_providers
                    .iter()
                    .any(|provider| provider.auth.is_some()))
        {
            tracing::warn!(
                plugin_id,
                "manifest declares llm_providers but lacks the llm_codec permission, or the llm_auth permission for auth-declaring entries; skipping"
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
            if definition.auth.is_some() && !may_register_auth {
                tracing::warn!(
                    plugin_id,
                    provider = %definition.id,
                    "provider declares an auth section but lacks the llm_auth permission; dropping the auth reference"
                );
                definition.auth = None;
            } else if definition.auth.is_some() && !may_register_codec {
                // Auth-only entry: the plugin contributes the login
                // implementation, the host owns the format codec.
                tracing::debug!(
                    plugin_id,
                    provider = %definition.id,
                    "manifest llm_provider rides on a contributed auth implementation"
                );
            }
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
    /// Register manifest-inline declarative CLI contributions with the
    /// same validation and conflict policy as programmatic ones. The
    /// `cli_ui` permission gates all five kinds: without it the inline
    /// declarations are skipped with a warning, mirroring the
    /// `llm_providers` permission gate. Runs after the plugin's own
    /// `register_contributions` so programmatic errors keep surfacing
    /// first; an inline conflict fails activation instead of silently
    /// winning.
    pub(crate) fn sync_manifest_cli_contributions(&self, plugin_id: &str) -> PluginResult<()> {
        let info = match self.registry.get(plugin_id) {
            Some(info) => info,
            None => return Ok(()),
        };
        let manifest = &info.manifest;
        if manifest.cli_commands.is_empty()
            && manifest.completion_sources.is_empty()
            && manifest.setting_fields.is_empty()
            && manifest.tool_summaries.is_empty()
            && manifest.markdown_rules.is_empty()
        {
            return Ok(());
        }
        if !manifest.permissions.contains(&PluginPermission::CliUi) {
            tracing::warn!(
                plugin_id,
                "manifest declares inline CLI contributions but lacks the cli_ui permission; skipping"
            );
            return Ok(());
        }
        // Dialog requests are runtime-gated by `check_cli_dialog`; the
        // declarative surface stays behind `cli_ui` only.
        let mut registrar = self.contribution_manager.as_registrar();
        for command in &manifest.cli_commands {
            registrar.register_cli_command(command.clone())?;
        }
        for source in &manifest.completion_sources {
            registrar.register_completion_provider(source.clone())?;
        }
        for field in &manifest.setting_fields {
            registrar.register_setting_field(field.clone())?;
        }
        for summary in &manifest.tool_summaries {
            registrar.register_tool_summary(summary.clone())?;
        }
        for rule in &manifest.markdown_rules {
            registrar.register_markdown_transformer(rule.clone())?;
        }
        Ok(())
    }

    /// Admit one host-mediated dialog request. Wasm and Lua callers must
    /// hold the `cli_dialog` permission; native callers pass the same
    /// check for audit. A denial names the plugin so the shell can log
    /// which plugin was refused without presenting anything.
    pub fn check_cli_dialog(&self, plugin_id: &str) -> PluginResult<()> {
        let info = match self.registry.get(plugin_id) {
            Some(info) => info,
            None => {
                return Err(crate::error::PluginError::PermissionDenied {
                    plugin_id: plugin_id.to_string(),
                    reason: "unknown plugin; dialog request denied".to_string(),
                });
            }
        };
        if info.manifest.permissions.contains(&PluginPermission::CliDialog) {
            return Ok(());
        }
        tracing::warn!(
            plugin_id,
            "plugin requested a host dialog without the cli_dialog permission; denying"
        );
        Err(crate::error::PluginError::PermissionDenied {
            plugin_id: plugin_id.to_string(),
            reason: "missing cli_dialog permission".to_string(),
        })
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
        proxy: provider.proxy.clone(),
        no_proxy: provider.no_proxy.clone(),
        rate_limit: provider
            .rate_limit
            .as_ref()
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        auth: provider.auth.as_ref().map(convert_auth_ref),
        compat: provider
            .compat
            .as_ref()
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        identity: provider
            .identity
            .as_ref()
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        transport: provider
            .transport
            .as_ref()
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        env: provider.env.clone(),
    }
}

/// Convert the manifest auth reference into the host type. The `method`
/// string maps onto the host `AuthMethod`; an unrecognized value keeps
/// `None` so the named auth implementation decides.
pub(crate) fn convert_auth_ref(
    auth: &wf_plugin_sdk::manifest::PluginProviderAuthRef,
) -> wf_types::llm::ProviderAuthRef {
    wf_types::llm::ProviderAuthRef {
        provider: auth.provider.clone(),
        method: auth
            .method
            .as_deref()
            .and_then(|m| m.parse::<wf_types::llm::AuthMethod>().ok()),
        is_subscription: auth.is_subscription,
        params: auth.params.clone(),
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
                    max_output_tokens: None,
                    metadata: m.metadata.clone(),
                })
                .collect(),
        },
        From::Disabled => To::Disabled,
    }
}
