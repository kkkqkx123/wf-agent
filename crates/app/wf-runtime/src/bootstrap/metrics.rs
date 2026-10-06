use std::sync::Arc;

use crate::error::RuntimeResult;

pub async fn init_metrics_context(
    config: &Option<wf_types::config::metrics::MetricsConfig>,
    storage_manager: &crate::storage_manager::StorageManager,
    event_bus: &Arc<wf_core::event::EventBus>,
    agent_registry: &Arc<wf_agent::registry::AgentLoopRegistry>,
    config_metrics: Option<Arc<wf_metrics::ConfigMetricsCollector>>,
) -> RuntimeResult<Option<Arc<crate::metrics::MetricsContext>>> {
    use wf_config::processor::infrastructure::merge_metrics_with_defaults;

    let Some(cfg) = config.as_ref() else {
        return Ok(None);
    };
    let config_metrics = config_metrics.unwrap_or_else(|| {
        Arc::new(wf_metrics::ConfigMetricsCollector::new(
            wf_metrics::CollectorConfig::default(),
        ))
    });
    let merged = merge_metrics_with_defaults(cfg);
    let ctx = crate::metrics::MetricsContext::start(
        &merged,
        storage_manager,
        Some(event_bus.clone()),
        Some(config_metrics),
        Some(agent_registry.capacity_gate()),
    )
    .await?;
    Ok(ctx)
}
