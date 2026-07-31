use std::sync::Arc;
use std::time::Duration;

use wf_core::event::EventBus;
use wf_core::EventMetricsBridge;
use wf_metrics::{MetricPoint, MetricsError, MetricsRegistry, MetricsSink};
use wf_storage::adapter::metrics::{MetricsDataPoint, MetricsStorageAdapter};
use wf_types::config::metrics::MetricsConfig;

use crate::error::RuntimeResult;
use crate::storage_manager::StorageManager;
/// Persistence sink backed by the storage `MetricsStorageAdapter`.
///
/// `wf-metrics` stays decoupled from `wf-storage` through the `MetricsSink`
/// trait; this adapter wires the two at the runtime layer.
pub struct StorageMetricsSink<A: MetricsStorageAdapter> {
    adapter: A,
}

impl<A: MetricsStorageAdapter> StorageMetricsSink<A> {
    pub fn new(adapter: A) -> Self {
        Self { adapter }
    }
}

#[async_trait::async_trait]
impl<A: MetricsStorageAdapter> MetricsSink for StorageMetricsSink<A> {
    async fn save_batch(&self, points: &[MetricPoint]) -> Result<(), MetricsError> {
        let data: Vec<MetricsDataPoint> = points
            .iter()
            .map(|p| MetricsDataPoint {
                name: p.name.clone(),
                metric_type: p.metric_type.as_str().to_string(),
                value: p.value,
                timestamp: p.timestamp,
                tags: if p.labels.is_empty() { None } else { Some(p.labels.clone()) },
            })
            .collect();
        self.adapter.save_batch(&data).await.map_err(map_err)
    }

    async fn query(
        &self,
        name: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<MetricPoint>, MetricsError> {
        self.adapter
            .query(name, start_time, end_time)
            .await
            .map(|points| {
                points
                    .into_iter()
                    .map(|p| MetricPoint {
                        name: p.name,
                        metric_type: parse_metric_type(&p.metric_type),
                        value: p.value,
                        timestamp: p.timestamp,
                        labels: p.tags.unwrap_or_default(),
                        source: String::new(),
                    })
                    .collect()
            })
            .map_err(map_err)
    }

    async fn delete_old(&self, older_than: i64) -> Result<u64, MetricsError> {
        self.adapter.delete_old(older_than).await.map_err(map_err)
    }
}

fn map_err(e: wf_storage::error::StorageError) -> MetricsError {
    MetricsError::Sink(e.to_string())
}

fn parse_metric_type(value: &str) -> wf_metrics::MetricType {
    match value {
        "counter" => wf_metrics::MetricType::Counter,
        "gauge" => wf_metrics::MetricType::Gauge,
        "histogram" => wf_metrics::MetricType::Histogram,
        "summary" => wf_metrics::MetricType::Summary,
        _ => wf_metrics::MetricType::Counter,
    }
}

/// Runtime-owned metrics system: registry + persistence sink + background
/// flush/cleanup tasks and the event bridge subscription.
///
/// Created from `SdkOptions`-style metrics config; returns `None` when
/// metrics are disabled.
pub struct MetricsContext {
    registry: Arc<MetricsRegistry>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    event_bridge_task: Option<tokio::task::JoinHandle<()>>,
}

impl MetricsContext {
    /// Initialize metrics from a merged config. The registry is created only
    /// when `enabled` (default true for a present config) so disabled or
    /// absent metrics configs add no overhead.
    pub async fn start(
        config: &MetricsConfig,
        storage: &StorageManager,
        event_bus: Option<Arc<EventBus>>,
    ) -> RuntimeResult<Option<Arc<Self>>> {
        if !config.enabled.unwrap_or(false) {
            return Ok(None);
        }

        let mut registry = MetricsRegistry::with_config(config);

        if let Ok(ctx) = storage.context() {
            let sink: Arc<dyn MetricsSink> = Arc::new(StorageMetricsSink::new(ctx.metrics.clone()));
            registry = registry.with_sink(sink);
        } else {
            tracing::warn!(target: "wf_metrics", "storage not initialized, metrics persistence disabled");
        }
        let registry = Arc::new(registry);

        let mut tasks = Vec::new();

        let flush_interval = flush_interval(config);
        let cleanup_interval = flush_interval.saturating_mul(12).max(Duration::from_secs(30));
        {
            let registry = registry.clone();
            tasks.push(tokio::spawn(async move {
                let mut ticker = tokio::time::interval(flush_interval);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    ticker.tick().await;
                    registry.flush_all().await;
                }
            }));
        }
        {
            let registry = registry.clone();
            tasks.push(tokio::spawn(async move {
                let mut ticker = tokio::time::interval(cleanup_interval);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    ticker.tick().await;
                    registry.cleanup_all();
                }
            }));
        }

        let event_bridge_task = event_bus.map(|bus| EventMetricsBridge::new(registry.clone()).spawn(bus));

        Ok(Some(Arc::new(Self {
            registry,
            tasks,
            event_bridge_task,
        })))
    }

    pub fn registry(&self) -> &Arc<MetricsRegistry> {
        &self.registry
    }

    /// Abort background tasks and stop the event bridge.
    pub async fn shutdown(&self) {
        if let Some(task) = self.event_bridge_task.as_ref() {
            task.abort();
        }
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// Periodic flush cadence: the smallest configured collector flush interval
/// (bounded below by 100ms), defaulting to 5000ms.
fn flush_interval(config: &MetricsConfig) -> Duration {
    let mut ms = 5000_i64;
    let collectors = [
        config.workflow_metrics.as_ref(),
        config.node_metrics.as_ref(),
        config.agent_metrics.as_ref(),
        config.agent_loop_metrics.as_ref(),
        config.event_metrics.as_ref(),
        config.tool_metrics.as_ref(),
        config.token_metrics.as_ref(),
        config.error_metrics.as_ref(),
        config.template_metrics.as_ref(),
        config.config_metrics.as_ref(),
        config.resource_metrics.as_ref(),
    ];
    for collector in collectors.into_iter().flatten() {
        if let Some(interval) = collector.flush_interval {
            ms = ms.min(interval);
        }
    }
    Duration::from_millis(ms.max(100) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_manager::{StorageBackendType, StorageConfig};
    use wf_metrics::MetricType;

    #[test]
    fn flush_interval_uses_smallest_collector_interval() {
        let config = MetricsConfig {
            workflow_metrics: Some(wf_types::config::metrics::MetricCollectorConfig {
                flush_interval: Some(1000),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(flush_interval(&config), Duration::from_millis(1000));
        assert_eq!(flush_interval(&MetricsConfig::default()), Duration::from_millis(5000));
        assert_eq!(
            flush_interval(&MetricsConfig {
                workflow_metrics: Some(wf_types::config::metrics::MetricCollectorConfig {
                    flush_interval: Some(10),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            Duration::from_millis(100)
        );
    }

    #[test]
    fn parses_metric_types() {
        assert_eq!(parse_metric_type("counter"), MetricType::Counter);
        assert_eq!(parse_metric_type("gauge"), MetricType::Gauge);
        assert_eq!(parse_metric_type("histogram"), MetricType::Histogram);
        assert_eq!(parse_metric_type("summary"), MetricType::Summary);
        assert_eq!(parse_metric_type("unknown"), MetricType::Counter);
    }

    #[tokio::test]
    async fn start_returns_none_when_disabled() {
        let storage = StorageManager::new(StorageConfig {
            backend_type: StorageBackendType::Memory,
            ..Default::default()
        });
        let config = MetricsConfig {
            enabled: Some(false),
            ..Default::default()
        };
        let ctx = MetricsContext::start(&config, &storage, None).await.unwrap();
        assert!(ctx.is_none());
    }

    #[tokio::test]
    async fn sink_roundtrips_through_storage_adapter() {
        let mut storage = StorageManager::new(StorageConfig {
            backend_type: StorageBackendType::Memory,
            ..Default::default()
        });
        storage.initialize().await.unwrap();
        let ctx = storage.context().unwrap();

        let sink = StorageMetricsSink::new(ctx.metrics.clone());
        let point = MetricPoint {
            name: "test.metric".into(),
            metric_type: MetricType::Counter,
            value: 1.0,
            timestamp: 1000,
            labels: wf_metrics::labels(&[("env", "prod")]),
            source: "test".into(),
        };
        sink.save_batch(&[point]).await.unwrap();
        let loaded = sink.query("test.metric", 0, 2000).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "test.metric");
        assert_eq!(loaded[0].value, 1.0);
        assert_eq!(loaded[0].metric_type, MetricType::Counter);
        assert_eq!(loaded[0].labels.get("env").map(String::as_str), Some("prod"));
    }
}
