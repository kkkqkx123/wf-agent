use std::sync::Arc;

use wf_config::processor::llm_profile::{
    transform_llm_profile, validate_llm_profile, validate_provider_definition,
};
use wf_llm::LlmGateway;

use crate::error::RuntimeResult;

use super::config::LlmConfig;

/// Create an empty LLM gateway (no providers, no profiles). Plugins are
/// activated against this gateway first so their codecs and provider
/// definitions land before file-layer profiles register.
pub fn create_llm_gateway(metrics: Option<&wf_metrics::MetricsRegistry>) -> Arc<LlmGateway> {
    let gateway = LlmGateway::new();
    let gateway = match metrics {
        Some(registry) => gateway.with_token_metrics(Arc::new(
            wf_llm::token_stream_adapter::MetricsSinkAdapter::new(registry.token()),
        )),
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
) -> RuntimeResult<Arc<LlmGateway>> {
    let gateway = create_llm_gateway(metrics);
    register_llm_config(&gateway, config)?;
    Ok(gateway)
}
