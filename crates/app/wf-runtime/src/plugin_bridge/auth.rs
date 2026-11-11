use serde_json::Value;
use wf_plugin::{ContributionManager, PluginError, PluginResult};

pub(crate) fn sync_auth(
    gateway: &wf_llm::LlmGateway,
    plugin_id: &str,
    manager: &ContributionManager,
) -> PluginResult<()> {
    let registry = gateway.auth_registry();
    let mut registered: Vec<String> = Vec::new();
    for (name, owner) in manager.all_llm_auths() {
        if owner != plugin_id {
            continue;
        }
        let Some(auth) = manager.get_llm_auth(&name) else {
            continue;
        };
        let Some(registry) = registry else {
            tracing::warn!(
                plugin_id,
                auth = %name,
                "gateway has no auth registry; plugin llm auth not registered"
            );
            continue;
        };
        let params = auth_params_for(plugin_id, manager, &name);
        let adapter = std::sync::Arc::new(wf_llm::PluginAuthAdapter::new(auth, params));
        registry.register_provider(&name, wf_llm::ProviderAuth::oauth_only(adapter));
        gateway.evict_clients_for_auth(&name);
        registered.push(name.clone());
        tracing::debug!("  llm-auth (gateway-registered): {}", name);
    }
    for (id, owner) in manager.all_llm_provider_definitions() {
        if owner != plugin_id {
            continue;
        }
        let Some(definition) = manager.get_llm_provider_definition(&id) else {
            continue;
        };
        if let Some(auth_ref) = definition.auth.as_ref() {
            if !registered.contains(&auth_ref.provider) {
                return Err(PluginError::ActivationFailed(format!(
                    "plugin '{plugin_id}' provider '{id}' references llm auth '{}' \
                     which the plugin does not register",
                    auth_ref.provider
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn unsync_auth(
    gateway: &wf_llm::LlmGateway,
    plugin_id: &str,
    manager: &ContributionManager,
) {
    let Some(registry) = gateway.auth_registry() else {
        return;
    };
    for (name, owner) in manager.all_llm_auths() {
        if owner == plugin_id {
            registry.remove_provider(&name);
            gateway.evict_clients_for_auth(&name);
            tracing::debug!("  llm-auth (gateway-removed): {}", name);
        }
    }
}

fn auth_params_for(
    plugin_id: &str,
    manager: &ContributionManager,
    auth_name: &str,
) -> Option<Value> {
    let mut definitions: Vec<_> = manager.all_llm_provider_definitions();
    definitions.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, owner) in definitions {
        if owner != plugin_id {
            continue;
        }
        let Some(definition) = manager.get_llm_provider_definition(&id) else {
            continue;
        };
        if let Some(auth_ref) = definition.auth.as_ref() {
            if auth_ref.provider == auth_name {
                return auth_ref.params.as_ref().map(|params| {
                    Value::Object(params.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                });
            }
        }
    }
    None
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

    struct StubAuth;

    impl wf_plugin::PluginLlmAuth for StubAuth {
        fn describe(&self) -> wf_plugin::PluginResult<wf_plugin::AuthDescriptor> {
            Ok(wf_plugin::AuthDescriptor {
                name: "Example Sub".to_string(),
                is_subscription: true,
                login_label: Some("Sign in with Example Sub".to_string()),
            })
        }

        fn login_start(
            &self,
            _config: serde_json::Value,
        ) -> wf_plugin::PluginResult<wf_plugin::PluginLoginChallenge> {
            Err(wf_plugin::PluginError::Internal("stub".to_string()))
        }

        fn login_poll(
            &self,
            _session: serde_json::Value,
        ) -> wf_plugin::PluginResult<wf_plugin::PluginLoginStep> {
            Err(wf_plugin::PluginError::Internal("stub".to_string()))
        }

        fn login_submit(
            &self,
            _session: serde_json::Value,
            _input: String,
        ) -> wf_plugin::PluginResult<wf_plugin::PluginLoginStep> {
            Err(wf_plugin::PluginError::Internal("stub".to_string()))
        }

        fn refresh(
            &self,
            _credential: serde_json::Value,
        ) -> wf_plugin::PluginResult<serde_json::Value> {
            Err(wf_plugin::PluginError::Internal("stub".to_string()))
        }

        fn resolve(
            &self,
            _credential: serde_json::Value,
        ) -> wf_plugin::PluginResult<serde_json::Value> {
            Err(wf_plugin::PluginError::Internal("stub".to_string()))
        }
    }

    fn gateway_with_auth_registry() -> Arc<wf_llm::LlmGateway> {
        let store = Arc::new(wf_llm::llm_auth::MemoryCredentialStore::new());
        let registry =
            wf_llm::llm_auth::AuthRegistry::new(store, Arc::new(wf_llm::llm_auth::StdAuthContext));
        Arc::new(wf_llm::LlmGateway::new().with_auth(Arc::new(registry)))
    }

    fn provider_definition_with_auth(
        id: &str,
        auth_provider: &str,
    ) -> wf_types::llm::LlmProviderDefinition {
        wf_types::llm::LlmProviderDefinition {
            id: id.to_string(),
            name: None,
            description: None,
            base_url: Some("https://api.acme.test".to_string()),
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: wf_types::llm::LlmFormat::Anthropic,
            model_discovery: None,
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: Some(wf_types::llm::ProviderAuthRef {
                provider: auth_provider.to_string(),
                method: Some(wf_types::llm::AuthMethod::OAuth),
                is_subscription: Some(true),
                params: Some(
                    [("flow".to_string(), serde_json::json!("pkce_paste"))]
                        .into_iter()
                        .collect(),
                ),
            }),
            compat: None,
            identity: None,
            transport: None,
            env: None,
        }
    }

    #[tokio::test]
    async fn sync_registers_plugin_auth_unsync_removes_it() {
        let registries = Arc::new(ResourceRegistries::new());
        let tool_registry = Arc::new(ToolRegistry::new());
        let gateway = gateway_with_auth_registry();
        let bridge = WfPluginBridge::new(registries, tool_registry, gateway.clone());

        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_llm_auth("claude-subscription", Arc::new(StubAuth))
            .unwrap();
        manager
            .as_registrar()
            .register_llm_provider_definition(provider_definition_with_auth(
                "claude-sub",
                "claude-subscription",
            ))
            .unwrap();

        bridge.sync_all("p1", &manager).await.unwrap();
        let registry = gateway.auth_registry().expect("auth registry attached");
        assert!(
            registry.has_provider("claude-subscription"),
            "plugin auth registered under its contribution name"
        );
        let surfaces = registry.describe_providers();
        let surface = surfaces
            .iter()
            .find(|surface| surface.id == "claude-subscription")
            .expect("auth surface described");
        assert_eq!(surface.display_name, "Example Sub");
        assert!(surface.is_subscription);

        bridge.unsync_all("p1", &manager).await.unwrap();
        assert!(
            !registry.has_provider("claude-subscription"),
            "auth surface removed on unsync"
        );
    }

    #[tokio::test]
    async fn sync_fails_when_provider_references_unregistered_auth() {
        let registries = Arc::new(ResourceRegistries::new());
        let tool_registry = Arc::new(ToolRegistry::new());
        let gateway = gateway_with_auth_registry();
        let bridge = WfPluginBridge::new(registries, tool_registry, gateway.clone());

        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_llm_provider_definition(provider_definition_with_auth(
                "claude-sub",
                "claude-subscription",
            ))
            .unwrap();

        let err = bridge
            .sync_all("p1", &manager)
            .await
            .expect_err("dangling auth reference must fail activation");
        assert!(
            err.to_string().contains("claude-subscription"),
            "the missing auth name must stay in the message: {err}"
        );
    }
}
