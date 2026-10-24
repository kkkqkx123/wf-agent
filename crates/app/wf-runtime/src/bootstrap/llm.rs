use std::path::PathBuf;
use std::sync::Arc;

use wf_config::processor::llm_auth::{transform_auth_providers, validate_auth_config};
use wf_config::processor::llm_profile::{
    transform_llm_profile, validate_llm_profile, validate_provider_definition,
};
use wf_llm::auth::{DeclaredFlow, DeclaredOAuth, DeclaredOAuthConfig, EnvApiKeyAuth};
use wf_llm::LlmGateway;
use wf_pricing::PricingResolver;
use wf_types::config::llm_auth::{AuthMethodConfig, AuthProviderConfig};

use crate::error::RuntimeResult;

use super::config::LlmConfig;

/// Default credential store location: the user's home directory. Tokens
/// are a person's identity, not a project property, so the store follows
/// the user while endpoints and client ids stay in project config.
pub fn default_auth_store_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".wf").join("auth.json")
}

/// Resolve the credential store path from configuration. A relative path
/// resolves against the project root when one is known.
fn resolve_store_path(configured: Option<&str>, project_root: Option<&std::path::Path>) -> PathBuf {
    match configured {
        Some(path) => {
            let candidate = PathBuf::from(path);
            if candidate.is_absolute() {
                candidate
            } else {
                match project_root {
                    Some(root) => root.join(candidate),
                    None => candidate,
                }
            }
        }
        None => default_auth_store_path(),
    }
}

/// Build the auth registry from the declared configuration: a file-backed
/// credential store (0600, atomic replace) plus one built-in auth surface
/// per declared provider. Plugins register additional surfaces on top of
/// the same registry after activation.
pub fn create_auth_registry(
    config: &wf_types::config::llm_auth::AuthConfig,
    project_root: Option<&std::path::Path>,
) -> RuntimeResult<Arc<wf_llm::AuthRegistry>> {
    validate_auth_config(&config.providers).map_err(|e| {
        crate::error::RuntimeError::Config(format!("Invalid LLM auth declaration: {e}"))
    })?;
    let providers = transform_auth_providers(&config.providers, &std::collections::HashMap::new())
        .map_err(|e| {
            crate::error::RuntimeError::Config(format!("Invalid LLM auth declaration: {e}"))
        })?;

    let store_path = resolve_store_path(config.store_path.as_deref(), project_root);
    let store = Arc::new(wf_llm::FileCredentialStore::new(&store_path));
    let registry = wf_llm::AuthRegistry::new(store, Arc::new(wf_llm::StdAuthContext));
    for provider in &providers {
        registry.register_provider(&provider.id, provider_auth_surface(provider));
    }
    Ok(Arc::new(registry))
}

/// Turn one declaration into its provider auth surface.
fn provider_auth_surface(provider: &AuthProviderConfig) -> wf_llm::ProviderAuth {
    match provider.method {
        AuthMethodConfig::ApiKey => EnvApiKeyAuth::provider_auth(
            provider.env.clone().unwrap_or_default(),
            provider.display_name.clone(),
        ),
        AuthMethodConfig::OAuth => {
            let flow = match provider.flow {
                Some(wf_types::config::llm_auth::OAuthFlowConfig::Pkce) => DeclaredFlow::Pkce,
                _ => DeclaredFlow::Device,
            };
            DeclaredOAuth::provider_auth(DeclaredOAuthConfig {
                display_name: provider.display_name.clone(),
                flow,
                is_subscription: provider.is_subscription.unwrap_or(false),
                device_authorization_url: provider.device_authorization_url.clone(),
                token_url: provider.token_url.clone().unwrap_or_default(),
                authorize_url: provider.authorize_url.clone(),
                client_id: provider.client_id.clone().unwrap_or_default(),
                scope: provider.scope.clone(),
                redirect_uri: provider.redirect_uri.clone(),
                loopback: provider.loopback.unwrap_or(false),
                login_label: provider.login_label.clone(),
            })
        }
    }
}

/// Create an empty LLM gateway (no providers, no profiles). Plugins are
/// activated against this gateway first so their codecs and provider
/// definitions land before file-layer profiles register.
///
/// `pricing` optionally attaches a pricing resolver so recorded token usage is
/// enriched with a computed cost before reaching the metrics sink.
/// `auth` optionally attaches the credential registry; profiles carrying an
/// auth reference then resolve stored keys and OAuth tokens per request.
pub fn create_llm_gateway(
    metrics: Option<&wf_metrics::MetricsRegistry>,
    pricing: Option<Arc<dyn PricingResolver>>,
    auth: Option<Arc<wf_llm::AuthRegistry>>,
) -> Arc<LlmGateway> {
    let gateway = LlmGateway::new();
    let gateway = match metrics {
        Some(registry) => gateway.with_token_metrics(Arc::new(
            wf_llm::token_stream_adapter::MetricsSinkAdapter::new(registry.token()),
        )),
        None => gateway,
    };
    let gateway = match pricing {
        Some(resolver) => gateway.with_pricing_resolver(resolver),
        None => gateway,
    };
    let gateway = match auth {
        Some(registry) => gateway.with_auth(registry),
        None => gateway,
    };
    Arc::new(gateway)
}

/// Register file-layer provider definitions and profiles on a gateway.
///
/// Provider definitions register first, profiles second: profiles with
/// `provider_id` merge connection defaults once at registration time, so
/// the request hot path never merges again. Plugin codecs and providers
/// must already be synced (fixed startup order, no lazy backfill).
pub fn register_llm_config(gateway: &LlmGateway, config: &LlmConfig) -> RuntimeResult<()> {
    for definition in &config.provider_definitions {
        validate_provider_definition(definition).map_err(|e| {
            crate::error::RuntimeError::Config(format!("Invalid LLM provider: {e}"))
        })?;
        gateway
            .register_provider_definition(definition.clone())
            .map_err(|e| {
                crate::error::RuntimeError::Config(format!("Failed to register LLM provider: {e}"))
            })?;
    }

    for profile in &config.profiles {
        validate_llm_profile(profile)
            .map_err(|e| crate::error::RuntimeError::Config(format!("Invalid LLM profile: {e}")))?;
        let transformed = transform_llm_profile(profile, &std::collections::HashMap::new())
            .map_err(|e| crate::error::RuntimeError::Config(format!("Invalid LLM profile: {e}")))?;
        gateway.register_profile(transformed).map_err(|e| {
            crate::error::RuntimeError::Config(format!("Failed to register LLM profile: {e}"))
        })?;
    }
    Ok(())
}

pub fn init_llm_gateway(
    config: &LlmConfig,
    metrics: Option<&wf_metrics::MetricsRegistry>,
    pricing: Option<Arc<dyn PricingResolver>>,
    auth: Option<Arc<wf_llm::AuthRegistry>>,
) -> RuntimeResult<Arc<LlmGateway>> {
    let gateway = create_llm_gateway(metrics, pricing, auth);
    register_llm_config(&gateway, config)?;
    Ok(gateway)
}
