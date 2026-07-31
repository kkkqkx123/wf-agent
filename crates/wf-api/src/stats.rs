//! Statistics aggregation over the metrics registry.
//!
//! Pure service-layer wrappers around `wf-metrics` domain stats, kept out
//! of the HTTP layer so any transport (server/CLI) can consume them.

use wf_metrics::collectors::{AgentUsageStats, NodeUsageStats, WorkflowUsageStats};
use wf_metrics::{MetricReport, MetricsRegistry, ReportOptions};

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

/// Generate the full registry report (summary/top/anomalies/trends).
pub async fn generate_report(registry: &MetricsRegistry) -> MetricReport {
    wf_metrics::generate_report(registry, &ReportOptions::default()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_wrap_registry_collectors() {
        let registry = MetricsRegistry::new();
        registry.workflow().record_execution_start("e1", "wf-1");
        registry
            .workflow()
            .record_execution_complete("e1", "wf-1", None, true, 10.0, None);
        registry
            .node()
            .record_execution(wf_metrics::collectors::node::NodeExecutionRecord {
                node_id: "n1",
                node_type: "Llm",
                execution_id: "e1",
                success: true,
                duration_ms: 5.0,
                input_size: 1,
                output_size: 1,
                error_type: None,
            });
        registry.agent().record_execution_start("default", "e1");
        registry
            .agent()
            .record_execution_complete("default", "e1", true, 10.0);

        assert_eq!(workflow_stats(&registry).total, 1);
        assert_eq!(node_stats(&registry).total, 1);
        assert_eq!(agent_stats(&registry).total, 1);
    }

    #[tokio::test]
    async fn report_wraps_registry_generation() {
        let registry = MetricsRegistry::new();
        registry.workflow().record_execution_start("e1", "wf-1");
        let report = generate_report(&registry).await;
        assert!(report.timestamp > 0);
        assert!(report.summary.total_metrics > 0);
    }
}
