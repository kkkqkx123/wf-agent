use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::metric::MetricType;

#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("metrics sink error: {0}")]
    Sink(String),
}

/// Persistable metric point produced by a collector flush.
///
/// Summary metrics are never persisted (aligned with the TS SDK).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricPoint {
    pub name: String,
    pub metric_type: MetricType,
    pub value: f64,
    /// Epoch milliseconds.
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,
    /// Collector/source identifier.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

/// Persistence abstraction decoupling `wf-metrics` from `wf-storage`.
///
/// `wf-runtime` wires an implementation backed by the `MetricsStorageAdapter`
/// (memory/SQLite/PostgreSQL) so the crate dependency DAG stays acyclic.
#[async_trait::async_trait]
pub trait MetricsSink: Send + Sync {
    async fn save_batch(&self, points: &[MetricPoint]) -> Result<(), MetricsError>;
    async fn query(
        &self,
        name: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<MetricPoint>, MetricsError>;
    async fn delete_old(&self, older_than: i64) -> Result<u64, MetricsError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_point_serde_roundtrip() {
        let point = MetricPoint {
            name: "workflow.execution.count".into(),
            metric_type: MetricType::Counter,
            value: 1.0,
            timestamp: 123,
            labels: crate::labels(&[("env", "prod")]),
            source: "workflow".into(),
        };
        let json = serde_json::to_string(&point).unwrap();
        let decoded: MetricPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, point);
    }
}
