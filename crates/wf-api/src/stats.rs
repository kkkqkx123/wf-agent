//! Statistics aggregation and export over the metrics registry.
//!
//! Pure service-layer wrappers around `wf-metrics` domain stats and report
//! capabilities (top-N, JSON/Prometheus export, report subscriptions and
//! report options), kept out of the HTTP layer so any transport
//! (server/CLI) can consume them.

use serde_json::Value;

use wf_metrics::collectors::tool::ToolUsageStats;
use wf_metrics::collectors::{
    AgentUsageStats, ErrorStats, EventStats, NodeUsageStats, WorkflowUsageStats,
};
use wf_metrics::formatter::{format_registry_json, format_registry_prometheus};
use wf_metrics::metric::LabelGroup;
use wf_metrics::{MetricReport, MetricsRegistry, ReportCallback, ReportOptions};

use crate::{ApiContext, ApiError, ApiResult};

/// Aggregate workflow usage statistics from the registry.
pub fn workflow_stats(registry: &MetricsRegistry) -> WorkflowUsageStats {
    registry.workflow().usage_stats()
}

/// Aggregate node usage statistics from the registry.
pub fn node_stats(registry: &MetricsRegistry) -> NodeUsageStats {
    registry.node().usage_stats()
}

/// Aggregate agent usage statistics from the registry.
pub fn agent_stats(registry: &MetricsRegistry) -> AgentUsageStats {
    registry.agent().usage_stats()
}

/// Aggregate tool call statistics from the registry.
pub fn tool_stats(registry: &MetricsRegistry) -> ToolUsageStats {
    registry.tool().usage_stats()
}

/// Aggregate error statistics from the registry.
pub fn error_stats(registry: &MetricsRegistry) -> ErrorStats {
    registry.error().stats()
}

/// Aggregate event statistics by event type.
pub fn event_stats(registry: &MetricsRegistry) -> EventStats {
    registry.event().stats()
}

/// Top workflows by execution count (needs the known workflow ids so the
/// collector can be queried per workflow). Sorted descending, truncated to
/// `limit`. Workflows without recorded executions are omitted.
pub fn top_workflows(
    registry: &MetricsRegistry,
    workflow_ids: &[String],
    limit: usize,
) -> Vec<(String, WorkflowUsageStats)> {
    let mut ranked: Vec<(String, WorkflowUsageStats)> = workflow_ids
        .iter()
        .map(|id| (id.clone(), registry.workflow().usage_stats_for(id)))
        .filter(|(_, stats)| stats.total > 0)
        .collect();
    ranked.sort_by(|a, b| b.1.total.cmp(&a.1.total).then_with(|| a.0.cmp(&b.0)));
    ranked.truncate(limit);
    ranked
}

/// Top node types by execution count, derived from the node collector's
/// `by_node_type` groups. Sorted descending, truncated to `limit`.
pub fn top_node_types(registry: &MetricsRegistry, limit: usize) -> Vec<LabelGroup> {
    let mut by_type = registry.node().usage_stats().by_node_type;
    by_type.sort_by(|a, b| {
        b.value
            .partial_cmp(&a.value)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| label_key(&a.labels, "node_type").cmp(&label_key(&b.labels, "node_type")))
    });
    by_type.truncate(limit);
    by_type
}

/// Per-profile agent usage statistics, sorted by execution count descending
/// and truncated to `limit`. Profiles without recorded executions are omitted.
pub fn agent_stats_by_profile(
    registry: &MetricsRegistry,
    profile_ids: &[String],
    limit: usize,
) -> Vec<(String, AgentUsageStats)> {
    let mut ranked: Vec<(String, AgentUsageStats)> = profile_ids
        .iter()
        .map(|id| (id.clone(), registry.agent().usage_stats_for(id)))
        .filter(|(_, stats)| stats.total > 0)
        .collect();
    ranked.sort_by(|a, b| b.1.total.cmp(&a.1.total).then_with(|| a.0.cmp(&b.0)));
    ranked.truncate(limit);
    ranked
}

/// Export the full registry state as a JSON document.
pub fn export_json(registry: &MetricsRegistry) -> Value {
    format_registry_json(registry)
}

/// Export the full registry state in Prometheus text format.
pub fn export_prometheus(registry: &MetricsRegistry) -> String {
    format_registry_prometheus(registry)
}

/// Subscribe to periodic reports; returns a subscription id for `unsubscribe`.
pub fn subscribe(registry: &MetricsRegistry, callback: ReportCallback) -> usize {
    registry.on_report(callback)
}

/// Cancel a report subscription previously created with `subscribe`.
pub fn unsubscribe(registry: &MetricsRegistry, subscription_id: usize) {
    registry.unsubscribe(subscription_id);
}

/// Generate the full registry report with the given options (time range and
/// trend inclusion).
pub async fn generate_report(registry: &MetricsRegistry, options: &ReportOptions) -> MetricReport {
    wf_metrics::generate_report(registry, options).await
}

/// Resolve the metrics registry carried by an `ApiContext`, if configured.
pub fn registry(ctx: &ApiContext) -> ApiResult<&MetricsRegistry> {
    ctx.metrics.as_deref().ok_or_else(|| {
        ApiError::Execution("metrics registry is not configured for this context".to_string())
    })
}

fn label_key(labels: &std::collections::HashMap<String, String>, key: &str) -> String {
    labels.get(key).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use wf_metrics::collectors::node::NodeExecutionRecord;
    use wf_metrics::metric::TimeRange;

    fn seeded_registry() -> MetricsRegistry {
        let registry = MetricsRegistry::new();
        registry.workflow().record_execution_start("e1", "wf-1");
        registry
            .workflow()
            .record_execution_complete("e1", "wf-1", None, true, 10.0, None);
        registry.workflow().record_execution_start("e2", "wf-2");
        registry
            .workflow()
            .record_execution_complete("e2", "wf-2", None, false, 20.0, None);
        registry.node().record_execution(NodeExecutionRecord {
            node_id: "n1",
            node_type: "Llm",
            execution_id: "e1",
            success: true,
            duration_ms: 5.0,
            input_size: 1,
            output_size: 1,
            error_type: None,
        });
        registry.node().record_execution(NodeExecutionRecord {
            node_id: "n2",
            node_type: "Script",
            execution_id: "e2",
            success: true,
            duration_ms: 3.0,
            input_size: 1,
            output_size: 1,
            error_type: None,
        });
        registry.agent().record_execution_start("profile-a", "e1");
        registry
            .agent()
            .record_execution_complete("profile-a", "e1", true, 10.0);
        registry.agent().record_execution_start("profile-b", "e2");
        registry
            .agent()
            .record_execution_complete("profile-b", "e2", true, 10.0);
        registry.tool().record_tool_call_start("http", "e1");
        registry
            .tool()
            .record_tool_call_complete("http", "e1", true, 2.0, 10, 20);
        registry.error().record_error("llm", "agent", Some("e1"));
        registry
            .event()
            .record_event("NodeStarted", Some("e1"), Some("wf-1"));
        registry
            .event()
            .record_event("NodeCompleted", Some("e1"), Some("wf-1"));
        registry
    }

    #[test]
    fn stats_wrap_registry_collectors() {
        let registry = seeded_registry();
        assert_eq!(workflow_stats(&registry).total, 2);
        assert_eq!(node_stats(&registry).total, 2);
        assert_eq!(agent_stats(&registry).total, 2);
        assert_eq!(tool_stats(&registry).total, 1);
        assert_eq!(error_stats(&registry).total, 1);
        assert_eq!(event_stats(&registry).total, 2);
    }

    #[test]
    fn top_workflows_ranks_by_execution_count() {
        let registry = seeded_registry();
        let ranked = top_workflows(
            &registry,
            &[
                "wf-1".to_string(),
                "wf-2".to_string(),
                "wf-none".to_string(),
            ],
            10,
        );
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].0, "wf-1");
        assert_eq!(ranked[0].1.total, 1);
        assert_eq!(ranked[0].1.success, 1);
        assert_eq!(ranked[1].0, "wf-2");
        assert_eq!(ranked[1].1.failure, 1);
    }

    #[test]
    fn top_node_types_ranks_descending() {
        let registry = seeded_registry();
        let top = top_node_types(&registry, 10);
        assert_eq!(top.len(), 2);
        assert!(top[0].value >= top[1].value);
    }

    #[test]
    fn agent_stats_by_profile_ranks_by_execution_count() {
        let registry = seeded_registry();
        let ranked = agent_stats_by_profile(
            &registry,
            &[
                "profile-a".to_string(),
                "profile-b".to_string(),
                "none".to_string(),
            ],
            10,
        );
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].0, "profile-a");
        assert_eq!(ranked[0].1.total, 1);
    }

    #[test]
    fn export_json_and_prometheus() {
        let registry = seeded_registry();
        let json = export_json(&registry);
        assert!(json.is_array());
        let prometheus = export_prometheus(&registry);
        assert!(prometheus.contains("workflow.execution.count"));
        assert!(prometheus.contains("# HELP"));
    }

    #[tokio::test]
    async fn subscribe_delivers_reports() {
        let registry = seeded_registry();
        let delivered = Arc::new(AtomicUsize::new(0));
        let counter = delivered.clone();
        let id = subscribe(
            &registry,
            Arc::new(move |_| {
                counter.fetch_add(1, Ordering::Relaxed);
            }),
        );
        let report = wf_metrics::generate_report(&registry, &ReportOptions::default()).await;
        registry.publish_report(&report);
        assert_eq!(delivered.load(Ordering::Relaxed), 1);

        unsubscribe(&registry, id);
        registry.publish_report(&report);
        assert_eq!(delivered.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn report_respects_time_range_and_trends() {
        let registry = seeded_registry();
        let report = generate_report(
            &registry,
            &ReportOptions {
                time_range: Some(TimeRange {
                    from: wf_common::now() - 10_000,
                    to: wf_common::now() + 10_000,
                }),
                include_trends: true,
            },
        )
        .await;
        assert!(report.timestamp > 0);
        assert!(report.summary.total_metrics > 0);
    }
}
