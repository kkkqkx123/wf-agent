use std::sync::Arc;
use std::time::Duration;

use wf_core::event::EventBus;
use wf_core::EventMetricsBridge;
use wf_metrics::{
    generate_report, labels, storage_metrics, ConfigMetricsCollector, MetricPoint, MetricsError,
    MetricsRegistry, MetricsSink, ReportOptions, ResourceSample,
};
use wf_storage::adapter::metrics::{MetricsDataPoint, MetricsStorageAdapter};
use wf_storage::context::StorageContext;
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
                tags: if p.labels.is_empty() {
                    None
                } else {
                    Some(p.labels.clone())
                },
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
/// flush/cleanup/report/sampling tasks, the event bridge subscription and
/// the optional HTTP export server.
///
/// Created from `SdkOptions`-style metrics config; returns `None` when
/// metrics are disabled.
pub struct MetricsContext {
    registry: Arc<MetricsRegistry>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    event_bridge_task: Option<tokio::task::JoinHandle<()>>,
    /// Guarded so `shutdown(&self)` can consume it; the HTTP task itself is
    /// not part of `tasks`.
    http: std::sync::Mutex<Option<wf_server::ServerHandle>>,
}

impl MetricsContext {
    /// Initialize metrics from a merged config. The registry is created only
    /// when `enabled` (default true for a present config) so disabled or
    /// absent metrics configs add no overhead.
    ///
    /// `config_metrics` optionally supplies the config collector instance
    /// already wired into the config merge path, keeping a single counter.
    pub async fn start(
        config: &MetricsConfig,
        storage: &StorageManager,
        event_bus: Option<Arc<EventBus>>,
        config_metrics: Option<Arc<ConfigMetricsCollector>>,
    ) -> RuntimeResult<Option<Arc<Self>>> {
        if !config.enabled.unwrap_or(false) {
            return Ok(None);
        }

        let mut registry = MetricsRegistry::with_config(config);
        if let Some(metrics) = config_metrics {
            registry = registry.with_config_collector(metrics);
        }

        if let Ok(ctx) = storage.context() {
            let sink: Arc<dyn MetricsSink> = Arc::new(StorageMetricsSink::new(ctx.metrics.clone()));
            registry = registry.with_sink(sink);
        } else {
            tracing::warn!(target: "wf_metrics", "storage not initialized, metrics persistence disabled");
        }
        let registry = Arc::new(registry);

        let mut tasks = Vec::new();

        let flush_interval = flush_interval(config);
        let cleanup_interval = flush_interval
            .saturating_mul(12)
            .max(Duration::from_secs(30));
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
            let retention_ms = config
                .workflow_metrics
                .as_ref()
                .and_then(|c| c.max_age)
                .unwrap_or(3_600_000);
            tasks.push(tokio::spawn(async move {
                let mut ticker = tokio::time::interval(cleanup_interval);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    ticker.tick().await;
                    registry.cleanup_all();
                    // Prune persisted metric points older than the retention
                    // window (in-memory cleanup handles buffered data).
                    if let Some(Ok(_)) = registry
                        .delete_old_persisted(wf_common::now() - retention_ms)
                        .await
                    {
                        tracing::debug!(target: "wf_metrics", "pruned persisted metrics older than {retention_ms}ms");
                    }
                }
            }));
        }

        if config.enable_periodic_reporting.unwrap_or(false) {
            let report_interval =
                Duration::from_millis(config.reporting_interval.unwrap_or(60_000) as u64);
            let registry = registry.clone();
            tasks.push(tokio::spawn(async move {
                let mut ticker = tokio::time::interval(report_interval);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    ticker.tick().await;
                    // Skip generation entirely when nobody listens.
                    if registry.subscriber_count() == 0 {
                        continue;
                    }
                    let report = generate_report(&registry, &ReportOptions::default()).await;
                    registry.publish_report(&report);
                }
            }));
        }

        let sampler_interval = config
            .resource_metrics
            .as_ref()
            .and_then(|c| c.flush_interval)
            .map(|ms| Duration::from_millis(ms.max(100) as u64))
            .unwrap_or(Duration::from_millis(5000));
        let storage_ctx = storage.shared_context();
        let sampler = ResourceSampler::new(registry.clone(), sampler_interval)
            .with_event_bus(event_bus.clone())
            .with_storage(storage_ctx);
        tasks.push(sampler.spawn());

        let event_bridge_task =
            event_bus.map(|bus| EventMetricsBridge::new(registry.clone()).spawn(bus));

        let http = if let Some(ref addr) = config.http_addr {
            match addr.parse::<std::net::SocketAddr>() {
                Ok(socket_addr) => match wf_server::serve(registry.clone(), socket_addr).await {
                    Ok(handle) => {
                        tracing::info!(
                            target: "wf_metrics",
                            addr = %handle.addr(),
                            "metrics HTTP server listening"
                        );
                        Some(handle)
                    }
                    Err(err) => {
                        tracing::error!(
                            target: "wf_metrics",
                            error = %err,
                            addr = %addr,
                            "metrics HTTP server failed to start"
                        );
                        None
                    }
                },
                Err(err) => {
                    tracing::warn!(
                        target: "wf_metrics",
                        error = %err,
                        addr = %addr,
                        "invalid metrics http_addr, server not started"
                    );
                    None
                }
            }
        } else {
            None
        };

        Ok(Some(Arc::new(Self {
            registry,
            tasks,
            event_bridge_task,
            http: std::sync::Mutex::new(http),
        })))
    }

    pub fn registry(&self) -> &Arc<MetricsRegistry> {
        &self.registry
    }

    /// Abort background tasks, stop the event bridge and gracefully shut
    /// down the HTTP server (draining in-flight requests).
    pub async fn shutdown(&self) {
        if let Some(task) = self.event_bridge_task.as_ref() {
            task.abort();
        }
        for task in &self.tasks {
            task.abort();
        }
        let handle = self.http.lock().expect("metrics http lock").take();
        if let Some(handle) = handle {
            handle.shutdown().await;
        }
    }
}

/// Periodic process/entity resource sampler.
///
/// Records process RSS, the event bus backlog depth and storage I/O counters
/// each tick. Entity gauges without a data source yet (active executions,
/// queued tasks) record 0; `wf_core::scheduler::TaskScheduler::stats()` is
/// the future source once a scheduler instance exists.
pub struct ResourceSampler {
    registry: Arc<MetricsRegistry>,
    interval: Duration,
    event_bus: Option<Arc<EventBus>>,
    storage: Option<Arc<StorageContext>>,
}

impl ResourceSampler {
    pub fn new(registry: Arc<MetricsRegistry>, interval: Duration) -> Self {
        Self {
            registry,
            interval,
            event_bus: None,
            storage: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: Option<Arc<EventBus>>) -> Self {
        self.event_bus = event_bus;
        self
    }

    pub fn with_storage(mut self, storage: Option<Arc<StorageContext>>) -> Self {
        self.storage = storage;
        self
    }

    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(self.interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                self.sample();
            }
        })
    }

    pub fn sample(&self) {
        let sample = ResourceSample {
            memory_bytes: process_rss_bytes(),
            active_executions: Some(0),
            queued_tasks: Some(0),
            event_queue_length: self.event_bus.as_ref().map(|bus| bus.queue_len() as u64),
        };
        self.registry.resource().record_sample(&sample);

        if let Some(storage) = &self.storage {
            let snapshot = storage.ops_snapshot();
            let resource = self.registry.resource();
            for (op, metrics) in [
                ("save", &snapshot.save),
                ("load", &snapshot.load),
                ("delete", &snapshot.delete),
                ("list", &snapshot.list),
                ("exists", &snapshot.exists),
                ("clear", &snapshot.clear),
                ("batch", &snapshot.batch),
            ] {
                resource.collector().set_gauge(
                    storage_metrics::OP_COUNT,
                    metrics.count() as f64,
                    labels(&[("op", op)]),
                );
                resource.collector().set_gauge(
                    storage_metrics::OP_AVG_TIME_MS,
                    metrics.avg_time_ms(),
                    labels(&[("op", op)]),
                );
                resource.collector().set_gauge(
                    storage_metrics::OP_TOTAL_BYTES,
                    metrics.total_bytes() as f64,
                    labels(&[("op", op)]),
                );
            }
        }
    }
}

/// Resident set size of the current process in bytes, parsed from
/// `/proc/self/status` (Linux; zero dependencies).
fn process_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
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
        assert_eq!(
            flush_interval(&MetricsConfig::default()),
            Duration::from_millis(5000)
        );
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
        let ctx = MetricsContext::start(&config, &storage, None, None)
            .await
            .unwrap();
        assert!(ctx.is_none());
    }

    #[tokio::test]
    async fn start_runs_resource_sampler_and_report_task() {
        let storage = StorageManager::new(StorageConfig {
            backend_type: StorageBackendType::Memory,
            ..Default::default()
        });
        let config = MetricsConfig {
            enabled: Some(true),
            enable_periodic_reporting: Some(true),
            reporting_interval: Some(50),
            resource_metrics: Some(wf_types::config::metrics::MetricCollectorConfig {
                flush_interval: Some(50),
                ..Default::default()
            }),
            ..Default::default()
        };
        let ctx = MetricsContext::start(&config, &storage, None, None)
            .await
            .unwrap()
            .expect("metrics should be enabled");

        let delivered = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = delivered.clone();
        ctx.registry().on_report(Arc::new(move |_| {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }));

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            delivered.load(std::sync::atomic::Ordering::Relaxed) >= 1,
            "periodic report should be delivered to subscribers"
        );

        let memory = ctx
            .registry()
            .resource()
            .collector()
            .query(&wf_metrics::MetricFilter {
                name: Some(wf_metrics::resource_metrics::MEMORY_USAGE.to_string()),
                ..Default::default()
            });
        assert!(
            memory.metrics.iter().any(|m| m.value > 0.0),
            "sampler should record process RSS"
        );
        ctx.shutdown().await;
    }

    #[tokio::test]
    async fn start_serves_http_export_when_addr_configured() {
        let storage = StorageManager::new(StorageConfig {
            backend_type: StorageBackendType::Memory,
            ..Default::default()
        });
        let config = MetricsConfig {
            enabled: Some(true),
            http_addr: Some("127.0.0.1:0".to_string()),
            ..Default::default()
        };
        let ctx = MetricsContext::start(&config, &storage, None, None)
            .await
            .unwrap()
            .expect("metrics should be enabled");

        let _ = ctx; // server task aborts on shutdown; bind is exercised
        ctx.shutdown().await;
    }

    #[tokio::test]
    async fn sampler_records_event_queue_and_storage_ops() {
        let storage = StorageManager::new(StorageConfig {
            backend_type: StorageBackendType::Memory,
            ..Default::default()
        });
        let mut storage = storage;
        storage.initialize().await.unwrap();
        let bus = Arc::new(EventBus::new(16));
        let _subscription = bus.subscribe(); // keep a receiver so publish succeeds

        let registry = Arc::new(MetricsRegistry::new());
        let sampler = ResourceSampler::new(registry.clone(), Duration::from_millis(1000))
            .with_event_bus(Some(bus.clone()))
            .with_storage(storage.shared_context());
        bus.publish(wf_types::events::BaseEvent {
            id: "e".into(),
            r#type: wf_types::events::EventType::Heartbeat,
            timestamp: wf_common::now(),
            workflow_id: None,
            execution_id: None,
            agent_loop_id: None,
            metadata: None,
        })
        .unwrap();
        sampler.sample();

        let event_len = registry
            .resource()
            .collector()
            .query(&wf_metrics::MetricFilter {
                name: Some(wf_metrics::resource_metrics::EVENT_QUEUE_LENGTH.to_string()),
                ..Default::default()
            })
            .metrics
            .into_iter()
            .find(|m| m.name == wf_metrics::resource_metrics::EVENT_QUEUE_LENGTH)
            .map(|m| m.value)
            .unwrap_or(0.0);
        assert_eq!(event_len, 1.0);

        let op_count = registry
            .resource()
            .collector()
            .query(&wf_metrics::MetricFilter {
                name: Some(wf_metrics::storage_metrics::OP_COUNT.to_string()),
                ..Default::default()
            })
            .metrics
            .into_iter()
            .find(|m| m.name == wf_metrics::storage_metrics::OP_COUNT);
        assert!(op_count.is_some());
    }

    #[test]
    fn parses_process_rss() {
        if let Some(bytes) = process_rss_bytes() {
            assert!(bytes > 0);
        }
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
        assert_eq!(
            loaded[0].labels.get("env").map(String::as_str),
            Some("prod")
        );
    }
}
