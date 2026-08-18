use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::config_metrics;
use crate::labels;

/// Usage statistics aggregated from config processing records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ConfigStats {
    pub access_count: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub validation_errors: u64,
    pub avg_load_duration_ms: f64,
    pub total_load_count: u64,
}

/// Domain collector for configuration access/load/validation metrics.
///
/// Wired into the `wf-config` merge/parse/validation paths through an
/// optional collector parameter; absent collectors add zero overhead.
#[derive(Clone)]
pub struct ConfigMetricsCollector {
    inner: BaseMetricCollector,
}

impl ConfigMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record a configuration read/merge access.
    pub fn record_access(&self) {
        self.inner
            .increment_counter(config_metrics::ACCESS_COUNT, labels(&[]));
    }

    /// Record a completed config load/parse cycle with its duration.
    pub fn record_load_complete(&self, duration_ms: f64) {
        self.inner.observe_histogram(
            config_metrics::LOAD_DURATION,
            duration_ms,
            std::collections::HashMap::new(),
        );
    }

    pub fn record_cache_hit(&self) {
        self.inner
            .increment_counter(config_metrics::CACHE_HIT_COUNT, labels(&[]));
    }

    pub fn record_cache_miss(&self) {
        self.inner
            .increment_counter(config_metrics::CACHE_MISS_COUNT, labels(&[]));
    }

    pub fn record_validation_error(&self) {
        self.inner
            .increment_counter(config_metrics::VALIDATION_ERROR_COUNT, labels(&[]));
    }

    pub fn stats(&self) -> ConfigStats {
        let load = crate::collectors::latest(&self.inner, config_metrics::LOAD_DURATION);
        ConfigStats {
            access_count: crate::collectors::counter_total(
                &self.inner,
                config_metrics::ACCESS_COUNT,
            ) as u64,
            cache_hits: crate::collectors::counter_total(
                &self.inner,
                config_metrics::CACHE_HIT_COUNT,
            ) as u64,
            cache_misses: crate::collectors::counter_total(
                &self.inner,
                config_metrics::CACHE_MISS_COUNT,
            ) as u64,
            validation_errors: crate::collectors::counter_total(
                &self.inner,
                config_metrics::VALIDATION_ERROR_COUNT,
            ) as u64,
            avg_load_duration_ms: load
                .as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
            total_load_count: load.as_ref().map(|d| d.count).unwrap_or(0),
        }
    }

    pub fn to_prometheus(&self) -> String {
        crate::formatter::format_collector_prometheus(&self.inner)
    }

    pub fn to_json(&self) -> serde_json::Value {
        crate::formatter::format_collector_json(&self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collector() -> ConfigMetricsCollector {
        ConfigMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_access_and_load() {
        let c = collector();
        c.record_access();
        c.record_access();
        c.record_load_complete(12.0);
        c.record_load_complete(20.0);
        let stats = c.stats();
        assert_eq!(stats.access_count, 2);
        assert_eq!(stats.total_load_count, 2);
        assert_eq!(stats.avg_load_duration_ms, 16.0);
    }

    #[test]
    fn records_cache_and_validation() {
        let c = collector();
        c.record_cache_hit();
        c.record_cache_miss();
        c.record_validation_error();
        let stats = c.stats();
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
        assert_eq!(stats.validation_errors, 1);
    }

    #[test]
    fn exports_prometheus() {
        let c = collector();
        c.record_access();
        let text = c.to_prometheus();
        assert!(text.contains(config_metrics::ACCESS_COUNT));
    }
}
