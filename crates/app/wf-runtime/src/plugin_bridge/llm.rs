use wf_plugin::{ContributionManager, PluginError, PluginResult};

pub(crate) fn sync_llm(
    gateway: &wf_llm::LlmGateway,
    plugin_id: &str,
    manager: &ContributionManager,
) -> PluginResult<()> {
    for (name, owner) in manager.all_llm_providers() {
        if owner != plugin_id {
            continue;
        }
        let Some(codec) = manager.get_llm_codec(&name) else {
            continue;
        };
        let adapter = std::sync::Arc::new(wf_llm::PluginCodecAdapter::new(codec));
        if let Err(e) = gateway.codec_registry().register(&name, adapter) {
            tracing::warn!(
                plugin_id,
                format = %name,
                "plugin llm codec registration failed: {e}"
            );
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' codec '{name}': {e}"
            )));
        }
        tracing::debug!("  llm-codec (gateway-registered): {}", name);
    }
    for (id, owner) in manager.all_llm_provider_definitions() {
        if owner != plugin_id {
            continue;
        }
        let Some(definition) = manager.get_llm_provider_definition(&id) else {
            continue;
        };
        gateway
            .register_provider_definition((*definition).clone())
            .map_err(|e| {
                PluginError::LoadFailed(format!("plugin '{plugin_id}' provider '{id}': {e}"))
            })?;
        tracing::debug!("  llm-provider-definition (gateway-registered): {}", id);
    }
    Ok(())
}

pub(crate) fn unsync_llm(
    gateway: &wf_llm::LlmGateway,
    plugin_id: &str,
    manager: &ContributionManager,
) {
    for (name, owner) in manager.all_llm_providers() {
        if owner == plugin_id {
            gateway.codec_registry().unregister(&name);
            tracing::debug!("  llm-codec (gateway-removed): {}", name);
        }
    }
    for (id, owner) in manager.all_llm_provider_definitions() {
        if owner == plugin_id {
            gateway.remove_provider_definition(&id);
            tracing::debug!("  llm-provider-definition (gateway-removed): {}", id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_plugin::ContributionBridge;
    use wf_plugin::ContributionRegistrar;
    use wf_resource::registry::ResourceRegistries;
    use wf_tools::registry::ToolRegistry;

    use crate::plugin_bridge::bridge::WfPluginBridge;

    struct NoopCodec;

    impl wf_plugin::PluginLlmCodec for NoopCodec {
        fn build_request(
            &self,
            _request: serde_json::Value,
            _profile: serde_json::Value,
        ) -> wf_plugin::PluginResult<wf_plugin::CodecHttpRequest> {
            Err(wf_plugin::PluginError::Internal("noop".to_string()))
        }

        fn parse_response(
            &self,
            _body: &str,
            _request: serde_json::Value,
        ) -> wf_plugin::PluginResult<serde_json::Value> {
            Err(wf_plugin::PluginError::Internal("noop".to_string()))
        }
    }

    #[tokio::test]
    async fn sync_registers_codec_and_provider_unsync_removes_both() {
        let registries = Arc::new(ResourceRegistries::new());
        let tool_registry = Arc::new(ToolRegistry::new());
        let gateway = Arc::new(wf_llm::LlmGateway::new());
        let bridge = WfPluginBridge::new(
            registries,
            tool_registry,
            gateway.clone(),
            Arc::new(wf_execution_shared::hooks::HookHandlerRegistry::new()),
            Arc::new(wf_plugin::PluginRegistry::new()),
        );

        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_llm_provider("acme-codec", Arc::new(NoopCodec))
            .unwrap();
        manager
            .as_registrar()
            .register_llm_provider_definition(wf_types::llm::LlmProviderDefinition {
                id: "acme".to_string(),
                name: None,
                description: None,
                base_url: Some("https://api.acme.test".to_string()),
                auth_type: Some("bearer".to_string()),
                default_headers: None,
                format: wf_types::llm::LlmFormat::Custom("ACME_CODEC".to_string()),
                model_discovery: None,
                api_version: None,
                metadata: None,
                proxy: None,
                no_proxy: None,
                rate_limit: None,
                auth: None,
                compat: None,
                identity: None,
                transport: None,
                env: None,
            })
            .unwrap();

        bridge.sync_all("p1", &manager).await.unwrap();
        assert!(
            gateway.codec_registry().contains("acme-codec"),
            "codec registered as custom format"
        );
        assert!(
            gateway.provider_registry().has("acme"),
            "provider definition written to provider registry"
        );

        bridge.unsync_all("p1", &manager).await.unwrap();
        assert!(
            !gateway.codec_registry().contains("acme-codec"),
            "codec removed on unsync"
        );
        assert!(
            !gateway.provider_registry().has("acme"),
            "provider definition removed on unsync"
        );
    }
}
