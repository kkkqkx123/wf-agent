use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::compaction_metrics;
use crate::metric::labels;

/// Domain collector for budget-driven context compaction. It only counts
/// the silent transitions: emissions, handoff entries, KEEP-fact rescue
/// appends and plan capping. Success/failure main paths already surface
/// through the audit events and the trigger ledger, so they are not
/// duplicated here.
#[derive(Clone)]
pub struct CompactionMetricsCollector {
    inner: BaseMetricCollector,
}

impl CompactionMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record one emitted compaction signal (`mode` is `plan` or `legacy`).
    pub fn record_plan_emitted(&self, mode: &str, aggressive: bool) {
        self.inner.increment_counter_by(
            compaction_metrics::PLANS_EMITTED,
            1.0,
            labels(&[
                ("mode", mode),
                ("aggressive", if aggressive { "true" } else { "false" }),
            ]),
        );
    }

    /// Record one handoff entry (`reason` is the handoff reason code).
    pub fn record_handoff_entered(&self, reason: &str) {
        self.inner.increment_counter_by(
            compaction_metrics::HANDOFFS_ENTERED,
            1.0,
            labels(&[("reason", reason)]),
        );
    }

    /// Record KEEP facts the model omitted and the flow appended itself.
    pub fn record_keep_facts_appended(&self, count: u64) {
        if count > 0 {
            self.inner.increment_counter_by(
                compaction_metrics::KEEP_FACTS_APPENDED,
                count as f64,
                labels(&[]),
            );
        }
    }

    /// Record one plan round truncated by the per-round item cap while the
    /// reclaim target was still unmet (the remainder waits for the next
    /// settlement round).
    pub fn record_plan_capped(&self) {
        self.inner
            .increment_counter_by(compaction_metrics::PLAN_CAPPED, 1.0, labels(&[]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::CollectorConfig;
    use crate::collectors::{counter_total, counter_total_labeled};

    fn collector() -> CompactionMetricsCollector {
        CompactionMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn silent_transitions_count() {
        let c = collector();
        c.record_plan_emitted("plan", false);
        c.record_plan_emitted("legacy", false);
        c.record_handoff_entered("NO_COMPRESSIBLE_TARGET");
        c.record_keep_facts_appended(3);
        c.record_keep_facts_appended(0);
        c.record_plan_capped();
        assert_eq!(
            counter_total(c.collector(), compaction_metrics::PLANS_EMITTED),
            2.0
        );
        assert_eq!(
            counter_total(c.collector(), compaction_metrics::HANDOFFS_ENTERED),
            1.0
        );
        assert_eq!(
            counter_total(c.collector(), compaction_metrics::KEEP_FACTS_APPENDED),
            3.0
        );
        assert_eq!(
            counter_total(c.collector(), compaction_metrics::PLAN_CAPPED),
            1.0
        );
        // Labels distinguish the emission modes.
        let mut plan_labels = std::collections::HashMap::new();
        plan_labels.insert("mode".to_string(), "plan".to_string());
        assert_eq!(
            counter_total_labeled(
                c.collector(),
                compaction_metrics::PLANS_EMITTED,
                &plan_labels
            ),
            1.0
        );
    }
}
