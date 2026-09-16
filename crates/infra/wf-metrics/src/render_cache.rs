use std::sync::Mutex;

use wf_common::time::now;

use crate::formatter::{snapshots_json, snapshots_prometheus};
use crate::metric::MetricFilter;
use crate::MetricsRegistry;

/// Cached dual-format render of a registry snapshot.
///
/// One `refresh` collects per-collector snapshots a single time and renders
/// both Prometheus text and JSON from them, so frequent scrapes never pay
/// repeated snapshot passes. Rendering happens outside the swap lock.
#[derive(Debug, Default)]
pub struct CachedRender {
    pub prometheus: String,
    pub json: serde_json::Value,
    pub rendered_at: i64,
}

pub struct RenderCache {
    inner: Mutex<CachedRender>,
}

impl Default for RenderCache {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(CachedRender::default()),
        }
    }

    /// Re-render both formats from a single snapshot pass.
    pub fn refresh(&self, registry: &MetricsRegistry) {
        let filter = MetricFilter::default();
        let mut prometheus = String::new();
        let mut json_parts = Vec::new();
        for collector in registry.collectors() {
            let snapshots = collector.export_snapshots(&filter);
            prometheus.push_str(&snapshots_prometheus(&snapshots));
            json_parts.push(snapshots_json(&snapshots));
        }
        let rendered = CachedRender {
            prometheus,
            json: serde_json::Value::Array(json_parts),
            rendered_at: now(),
        };
        *wf_common::lock::lock_ok(self.inner.lock()) = rendered;
    }

    pub fn prometheus(&self) -> String {
        wf_common::lock::lock_ok(self.inner.lock())
            .prometheus
            .clone()
    }

    pub fn json(&self) -> serde_json::Value {
        wf_common::lock::lock_ok(self.inner.lock()).json.clone()
    }

    pub fn rendered_at(&self) -> i64 {
        wf_common::lock::lock_ok(self.inner.lock()).rendered_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with_data() -> MetricsRegistry {
        let registry = MetricsRegistry::new();
        registry.workflow().record_execution_start("wf-1");
        registry
    }

    #[test]
    fn refresh_renders_both_formats() {
        let registry = registry_with_data();
        let cache = RenderCache::new();
        cache.refresh(&registry);
        let text = cache.prometheus();
        assert!(text.contains("workflow.execution.count_total"));
        assert!(cache.json().is_array());
        assert!(cache.rendered_at() > 0);
    }

    #[test]
    fn cached_reads_are_stable_without_refresh() {
        let registry = registry_with_data();
        let cache = RenderCache::new();
        cache.refresh(&registry);
        let first = cache.prometheus();
        registry.workflow().record_execution_start("wf-3");
        assert_eq!(cache.prometheus(), first);
        cache.refresh(&registry);
        assert!(cache.prometheus().contains("wf-3"));
    }
}
