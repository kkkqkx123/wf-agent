use crate::domain::entity::Entity;
use crate::error::StorageError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsDataPoint {
    pub name: String,
    /// One of `counter`/`gauge`/`histogram`/`summary`.
    pub metric_type: String,
    pub value: f64,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<HashMap<String, String>>,
    /// Cumulative histogram bucket counts (empty for non-histograms),
    /// enabling distribution rebuild after a restart (M5).
    pub buckets: Vec<HistogramBucket>,
    /// Histogram sum of all observed samples.
    pub sum: f64,
    /// Histogram sample count.
    pub count: u64,
}

/// Histogram bucket upper bound and cumulative count, mirrored from
/// `wf-metrics` so the storage adapter stays self-contained.
///
/// JSON cannot represent `f64::INFINITY`, so the upper bound is serialized as
/// a string (`"+Inf"`/`"-Inf"` or a plain number) and parsed back on read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistogramBucket {
    pub upper_bound: f64,
    pub count: u64,
}

impl Serialize for HistogramBucket {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("HistogramBucket", 2)?;
        s.serialize_field("upperBound", &bound_to_string(self.upper_bound))?;
        s.serialize_field("count", &self.count)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for HistogramBucket {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(rename = "upperBound")]
            upper_bound: String,
            count: u64,
        }
        let raw = Raw::deserialize(deserializer)?;
        parse_bound(&raw.upper_bound)
            .map(|upper_bound| HistogramBucket {
                upper_bound,
                count: raw.count,
            })
            .map_err(serde::de::Error::custom)
    }
}

fn bound_to_string(bound: f64) -> String {
    if bound.is_infinite() {
        if bound > 0.0 {
            "+Inf".to_string()
        } else {
            "-Inf".to_string()
        }
    } else if bound.fract() == 0.0 {
        format!("{bound:.0}")
    } else {
        bound.to_string()
    }
}

fn parse_bound(value: &str) -> Result<f64, String> {
    match value {
        "+Inf" => Ok(f64::INFINITY),
        "-Inf" => Ok(f64::NEG_INFINITY),
        other => other.parse::<f64>().map_err(|e| e.to_string()),
    }
}

/// Persistence wrapper that derives a stable id from the point itself.
/// Keeps the storage id (`metric:{name}:{label_fingerprint}:{timestamp}`)
/// out of the transport model `MetricsDataPoint`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MetricRecord {
    pub id: String,
    pub point: MetricsDataPoint,
}

impl MetricRecord {
    pub fn from_point(point: MetricsDataPoint) -> Self {
        // The id includes a fingerprint of the label set so points sharing a
        // name and timestamp but differing in labels do not collide in the
        // storage backend (which keys records by id).
        let fingerprint = label_fingerprint(point.tags.as_ref());
        let id = format!("metric:{}:{}:{}", point.name, fingerprint, point.timestamp);
        Self { id, point }
    }
}

/// Stable 64-bit FNV-1a fingerprint of a sorted label set, rendered as hex.
/// Deterministic across builds and platforms so persisted ids stay stable.
fn label_fingerprint(tags: Option<&HashMap<String, String>>) -> String {
    let mut pairs: Vec<(&String, &String)> = match tags {
        Some(tags) => tags.iter().collect(),
        None => Vec::new(),
    };
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (key, value) in &pairs {
        for chunk in [key.as_bytes(), value.as_bytes()] {
            hash ^= chunk.len() as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            for byte in chunk {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_name_and_timestamp_with_different_labels_do_not_collide() {
        fn tag(kv: &[(&str, &str)]) -> Option<HashMap<String, String>> {
            Some(
                kv.iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            )
        }
        let base = MetricsDataPoint {
            name: "event.count".into(),
            metric_type: "counter".into(),
            value: 1.0,
            timestamp: 1000,
            tags: None,
            buckets: Vec::new(),
            sum: 0.0,
            count: 0,
        };
        let a = MetricRecord::from_point(MetricsDataPoint {
            tags: tag(&[("event_type", "NodeStarted")]),
            ..base.clone()
        });
        let b = MetricRecord::from_point(MetricsDataPoint {
            tags: tag(&[("event_type", "NodeCompleted")]),
            ..base.clone()
        });
        let c = MetricRecord::from_point(MetricsDataPoint {
            tags: None,
            ..base.clone()
        });
        assert_ne!(a.id, b.id, "different label sets must not share an id");
        assert_ne!(a.id, c.id, "labeled and unlabeled points must differ");
        assert_ne!(b.id, c.id);
        assert!(a.id.starts_with("metric:event.count:"));
        assert!(a.id.ends_with(":1000"));
    }

    #[test]
    fn fingerprint_is_order_and_hash_map_independent() {
        let mut tags_a = HashMap::new();
        tags_a.insert("env".to_string(), "prod".to_string());
        tags_a.insert("region".to_string(), "us".to_string());
        let mut tags_b = HashMap::new();
        tags_b.insert("region".to_string(), "us".to_string());
        tags_b.insert("env".to_string(), "prod".to_string());
        assert_eq!(
            label_fingerprint(Some(&tags_a)),
            label_fingerprint(Some(&tags_b))
        );
    }

    #[test]
    fn point_serde_roundtrip_keeps_id_external() {
        let mut tags = HashMap::new();
        tags.insert("env".to_string(), "prod".to_string());
        let point = MetricsDataPoint {
            name: "workflow.execution.duration".into(),
            metric_type: "histogram".into(),
            value: 1.5,
            timestamp: 123,
            tags: Some(tags),
            buckets: vec![
                HistogramBucket {
                    upper_bound: 0.5,
                    count: 0,
                },
                HistogramBucket {
                    upper_bound: 1.0,
                    count: 1,
                },
            ],
            sum: 1.5,
            count: 1,
        };
        let json = serde_json::to_string(&point).unwrap();
        let decoded: MetricsDataPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, point);
        assert_eq!(decoded.buckets.len(), 2);
        assert_eq!(decoded.sum, 1.5);
        assert_eq!(decoded.count, 1);
        let record = MetricRecord::from_point(decoded.clone());
        assert!(record
            .id
            .contains(&label_fingerprint(decoded.tags.as_ref())));
    }
}
