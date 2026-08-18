use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::template_metrics;
use crate::labels;

/// Usage statistics aggregated from template rendering records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TemplateUsageStats {
    pub instantiation_count: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub errors: u64,
    pub render_count: u64,
    pub avg_render_duration_ms: f64,
    pub p95_render_duration_ms: f64,
    pub p99_render_duration_ms: f64,
    pub cache_hit_rate: f64,
}

/// Domain collector for template rendering/usage metrics
/// (instantiation, render duration, cache hit/miss, errors).
#[derive(Clone)]
pub struct TemplateMetricsCollector {
    inner: BaseMetricCollector,
}

impl TemplateMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record a template instantiation (template pumped into a node plot).
    pub fn record_instantiation(&self, template_id: &str, context: &[(&str, &str)]) {
        let mut pairs = vec![("template_id", template_id)];
        pairs.extend_from_slice(context);
        self.inner
            .increment_counter(template_metrics::INSTANTIATION_COUNT, labels(&pairs));
    }

    /// Record a completed template render with its duration.
    pub fn record_render_complete(
        &self,
        template_id: &str,
        duration_ms: f64,
        success: bool,
        context: &[(&str, &str)],
    ) {
        let mut pairs = vec![
            ("template_id", template_id),
            ("success", if success { "true" } else { "false" }),
        ];
        pairs.extend_from_slice(context);
        let label_map = labels(&pairs);
        self.inner
            .observe_histogram(template_metrics::RENDER_DURATION, duration_ms, label_map);
        if !success {
            self.inner.increment_counter(
                template_metrics::ERROR_COUNT,
                labels(&[
                    ("template_id", template_id),
                    ("error_type", "render_failed"),
                ]),
            );
        }
    }

    pub fn record_cache_hit(&self, template_id: &str, context: &[(&str, &str)]) {
        let mut pairs = vec![("template_id", template_id)];
        pairs.extend_from_slice(context);
        self.inner
            .increment_counter(template_metrics::CACHE_HIT_COUNT, labels(&pairs));
    }

    pub fn record_cache_miss(&self, template_id: &str, context: &[(&str, &str)]) {
        let mut pairs = vec![("template_id", template_id)];
        pairs.extend_from_slice(context);
        self.inner
            .increment_counter(template_metrics::CACHE_MISS_COUNT, labels(&pairs));
    }

    /// Record a template render error.
    pub fn record_error(&self, template_id: &str, error_type: &str, context: &[(&str, &str)]) {
        let mut pairs = vec![("template_id", template_id), ("error_type", error_type)];
        pairs.extend_from_slice(context);
        self.inner
            .increment_counter(template_metrics::ERROR_COUNT, labels(&pairs));
    }

    /// Raw query result filtered by an optional template id.
    pub fn template_stats(&self, template_id: Option<&str>) -> crate::metric::MetricQueryResult {
        let filter = match template_id {
            Some(id) => crate::metric::MetricFilter {
                labels: Some(labels(&[("template_id", id)])),
                ..Default::default()
            },
            None => crate::metric::MetricFilter::default(),
        };
        self.inner.query(&filter)
    }

    /// Cache hit rate (0..=1) for a template; `None` when no usage exists.
    pub fn cache_hit_rate(&self, template_id: &str) -> Option<f64> {
        let hits = crate::collectors::counter_total_labeled(
            &self.inner,
            template_metrics::CACHE_HIT_COUNT,
            &labels(&[("template_id", template_id)]),
        );
        let misses = crate::collectors::counter_total_labeled(
            &self.inner,
            template_metrics::CACHE_MISS_COUNT,
            &labels(&[("template_id", template_id)]),
        );
        let total = hits + misses;
        if total <= 0.0 {
            return None;
        }
        Some(hits / total)
    }

    pub fn usage_stats(&self) -> TemplateUsageStats {
        let render = crate::collectors::latest(&self.inner, template_metrics::RENDER_DURATION);
        let hits = crate::collectors::counter_total(&self.inner, template_metrics::CACHE_HIT_COUNT);
        let misses =
            crate::collectors::counter_total(&self.inner, template_metrics::CACHE_MISS_COUNT);
        let percentile = |p: f64| {
            render
                .as_ref()
                .and_then(|d| {
                    d.percentiles
                        .iter()
                        .find(|q| (q.percentile - p).abs() < f64::EPSILON)
                })
                .map(|q| q.value)
                .unwrap_or(0.0)
        };

        TemplateUsageStats {
            instantiation_count: crate::collectors::counter_total(
                &self.inner,
                template_metrics::INSTANTIATION_COUNT,
            ) as u64,
            cache_hits: hits as u64,
            cache_misses: misses as u64,
            errors: crate::collectors::counter_total(&self.inner, template_metrics::ERROR_COUNT)
                as u64,
            render_count: render.as_ref().map(|d| d.count).unwrap_or(0),
            avg_render_duration_ms: render
                .as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
            p95_render_duration_ms: percentile(0.95),
            p99_render_duration_ms: percentile(0.99),
            cache_hit_rate: {
                let total = hits + misses;
                if total > 0.0 {
                    hits / total
                } else {
                    0.0
                }
            },
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

    fn collector() -> TemplateMetricsCollector {
        TemplateMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_usage_render_and_errors() {
        let c = collector();
        c.record_instantiation("system.main", &[("workflow_id", "wf-1")]);
        c.record_render_complete("system.main", 10.0, true, &[]);
        c.record_render_complete("system.main", 30.0, true, &[]);
        c.record_cache_hit("system.main", &[]);
        c.record_cache_hit("system.main", &[]);
        c.record_cache_miss("system.main", &[]);

        let stats = c.usage_stats();
        assert_eq!(stats.instantiation_count, 1);
        assert_eq!(stats.render_count, 2);
        assert_eq!(stats.errors, 0);
        assert_eq!(stats.cache_hits, 2);
        assert_eq!(stats.cache_misses, 1);
        assert_eq!(stats.avg_render_duration_ms, 20.0);
        assert!((stats.cache_hit_rate - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(
            c.cache_hit_rate("system.main"),
            Some(2.0 / 3.0),
            "per-template hit rate via label filter"
        );
        assert_eq!(c.cache_hit_rate("no-such-template"), None);
    }

    #[test]
    fn records_errors_by_type() {
        let c = collector();
        c.record_error("system.main", "cache_miss", &[]);
        c.record_error("system.main", "parse", &[]);
        assert_eq!(c.usage_stats().errors, 2);
    }

    #[test]
    fn template_stats_filters_by_template_id() {
        let c = collector();
        c.record_instantiation("system.main", &[]);
        c.record_instantiation("readme.sidebar", &[]);
        let all = c.template_stats(None).total_count;
        let filtered = c.template_stats(Some("system.main")).total_count;
        assert_eq!(all, 2);
        assert_eq!(filtered, 1);
    }

    #[test]
    fn exports_prometheus() {
        let c = collector();
        c.record_cache_miss("system.main", &[]);
        let text = c.to_prometheus();
        assert!(text.contains(template_metrics::CACHE_MISS_COUNT));
    }
}
