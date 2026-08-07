use crate::domain::entity::Entity;
use crate::error::StorageError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsDataPoint {
    pub name: String,
    /// One of `counter`/`gauge`/`histogram`/`summary`.
    pub metric_type: String,
    pub value: f64,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<HashMap<String, String>>,
}

/// Persistence wrapper that derives a stable id from the point itself.
/// Keeps the storage id (`metric:{name}:{timestamp}`) out of the
/// transport model `MetricsDataPoint`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MetricRecord {
    pub id: String,
    pub point: MetricsDataPoint,
}

impl MetricRecord {
    pub fn from_point(point: MetricsDataPoint) -> Self {
        let id = format!("metric:{}:{}", point.name, point.timestamp);
        Self { id, point }
    }
}

impl Entity for MetricRecord {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "metric"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "metricName": self.point.name,
            "timestamp": self.point.timestamp,
        })
    }
}

pub trait MetricsStorageAdapter: Send + Sync {
    fn save_batch(
        &self,
        points: &[MetricsDataPoint],
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + Send;
    fn query(
        &self,
        name: &str,
        start_time: i64,
        end_time: i64,
    ) -> impl std::future::Future<Output = Result<Vec<MetricsDataPoint>, StorageError>> + Send;
    fn delete_old(
        &self,
        older_than: i64,
    ) -> impl std::future::Future<Output = Result<u64, StorageError>> + Send;
}
