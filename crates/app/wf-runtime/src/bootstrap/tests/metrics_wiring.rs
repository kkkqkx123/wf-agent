use super::*;

#[tokio::test]
async fn test_runtime_metrics_wiring() {
    clear_env_vars();

    let config = RuntimeConfig {
        metrics: Some(MetricsConfig {
            workflow_metrics: Some(wf_types::config::metrics::MetricCollectorConfig {
                flush_interval: Some(100),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..default_test_config()
    };

    let runtime = Runtime::bootstrap(config).await.unwrap();

    let metrics = runtime
        .metrics()
        .expect("metrics system should be initialized");
    metrics.registry().workflow().record_execution_start("wf-1");
    assert_eq!(metrics.registry().workflow().usage_stats().total, 1);

    // Background flush task persists buffered metrics into storage.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    use wf_storage::adapter::metrics::MetricsStorageAdapter;
    let loaded = runtime
        .storage()
        .context()
        .unwrap()
        .metrics
        .query("workflow.execution.count", 0, wf_common::now() + 1000)
        .await
        .unwrap();
    assert_eq!(loaded.len(), 1);

    runtime.shutdown().await.unwrap();
    clear_env_vars();
}

#[tokio::test]
async fn test_runtime_metrics_disabled() {
    clear_env_vars();

    let config = RuntimeConfig {
        metrics: Some(MetricsConfig {
            enabled: Some(false),
            ..Default::default()
        }),
        ..default_test_config()
    };

    let runtime = Runtime::bootstrap(config).await.unwrap();
    assert!(runtime.metrics().is_none());

    runtime.shutdown().await.unwrap();
    clear_env_vars();
}
