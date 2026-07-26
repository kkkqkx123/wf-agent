use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use crate::error::StorageError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsDataPoint {
    pub name: String,
    pub value: f64,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<HashMap<String, String>>,
}

pub trait MetricsStorageAdapter: Send + Sync {
    async fn save_batch(&self, points: &[MetricsDataPoint]) -> Result<(), StorageError>;
    async fn query(
        &self,
        name: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<MetricsDataPoint>, StorageError>;
    async fn delete_old(&self, older_than: i64) -> Result<u64, StorageError>;
}
