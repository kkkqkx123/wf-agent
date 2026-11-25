//! Point, query-result and collection types shared by every backend.
//!
//! Payloads are plain JSON objects. Two keys carry reserved retrieval
//! meaning: `namespace` scopes a point to one knowledge base, `source`
//! identifies the document the point was chunked from. Backends must treat
//! those keys like any other payload field for storage and expose them for
//! filtering; the filter helpers in [`crate::filter`] implement the matching
//! semantics both backends agree on.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Reserved payload key scoping a point to one knowledge base.
pub const NAMESPACE_KEY: &str = "namespace";
/// Reserved payload key identifying the source document of a point.
pub const SOURCE_KEY: &str = "source";
/// Reserved payload key carrying the contracts-level point id on backends
/// whose native ids cannot hold arbitrary strings.
pub const PID_KEY: &str = "pid";

/// Distance function of a collection. Scores are always higher-is-better:
/// cosine and dot return similarity directly, euclid returns the negated
/// distance so ordering stays uniform across metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    /// Cosine similarity.
    #[default]
    Cosine,
    /// Dot product.
    Dot,
    /// Negated euclidean distance.
    Euclid,
}

/// Product quantization compression level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressionRatio {
    /// Fourfold compression.
    X4,
    /// Eightfold compression.
    X8,
    /// Sixteenfold compression.
    X16,
    /// Thirty-twofold compression.
    X32,
    /// Sixty-fourfold compression.
    X64,
}

impl CompressionRatio {
    /// Server-facing name used by remote backends.
    pub fn as_name(self) -> &'static str {
        match self {
            CompressionRatio::X4 => "x4",
            CompressionRatio::X8 => "x8",
            CompressionRatio::X16 => "x16",
            CompressionRatio::X32 => "x32",
            CompressionRatio::X64 => "x64",
        }
    }
}

/// Backend-neutral quantization declaration for a collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantizationConfig {
    /// Scalar int8 quantization with an optional quantile in `(0, 1]`.
    Scalar { quantile: Option<f32> },
    /// Product quantization with the requested compression level.
    Product { compression: CompressionRatio },
    /// Binary quantization for maximum compression.
    Binary,
}

impl QuantizationConfig {
    /// Scalar quantization with an explicit quantile.
    pub fn scalar(quantile: f32) -> Self {
        Self::Scalar {
            quantile: Some(quantile),
        }
    }

    /// Product quantization with the requested compression level.
    pub fn product(compression: CompressionRatio) -> Self {
        Self::Product { compression }
    }

    /// Binary quantization.
    pub fn binary() -> Self {
        Self::Binary
    }
}

/// Collection-level graph tuning shared by local and remote backends.
/// Every field is optional; unset fields keep the backend default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HnswConfig {
    /// Graph connectivity per layer.
    pub m: Option<usize>,
    /// Build-time candidate list width.
    pub ef_construct: Option<usize>,
    /// Keep the graph on disk instead of memory when the backend supports it.
    pub on_disk: Option<bool>,
}

impl HnswConfig {
    /// Empty tuning that keeps every backend default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Overrides the graph connectivity.
    pub fn with_m(mut self, m: usize) -> Self {
        self.m = Some(m);
        self
    }

    /// Overrides the build-time candidate width.
    pub fn with_ef_construct(mut self, ef_construct: usize) -> Self {
        self.ef_construct = Some(ef_construct);
        self
    }

    /// Toggles disk-backed graph storage.
    pub fn with_on_disk(mut self, on_disk: bool) -> Self {
        self.on_disk = Some(on_disk);
        self
    }
}

/// Per-query quantized-scan controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct QuantizationSearchParams {
    /// Skip the quantized scan and use the exact path.
    pub ignore: Option<bool>,
    /// Rescore quantized candidates with exact vectors before returning.
    pub rescore: Option<bool>,
    /// Candidate multiplier for the quantized coarse scan (`>= 1.0`).
    pub oversampling: Option<f64>,
}

impl QuantizationSearchParams {
    /// Empty controls that keep backend defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggles skipping the quantized scan.
    pub fn with_ignore(mut self, ignore: bool) -> Self {
        self.ignore = Some(ignore);
        self
    }

    /// Toggles exact rescoring of quantized candidates.
    pub fn with_rescore(mut self, rescore: bool) -> Self {
        self.rescore = Some(rescore);
        self
    }

    /// Overrides the candidate multiplier.
    pub fn with_oversampling(mut self, oversampling: f64) -> Self {
        self.oversampling = Some(oversampling);
        self
    }
}

/// One point to store: an id, a dense vector and a JSON payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorPoint {
    /// Caller-assigned stable id; upserts with the same id overwrite.
    pub id: String,
    /// Dense embedding vector; must match the collection dimension.
    pub vector: Vec<f32>,
    /// JSON metadata, conventionally holding `namespace` and `source`.
    #[serde(default)]
    pub payload: HashMap<String, serde_json::Value>,
}

impl VectorPoint {
    /// Builds a point without payload.
    pub fn new(id: impl Into<String>, vector: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            vector,
            payload: HashMap::new(),
        }
    }

    /// Attaches a full payload.
    pub fn with_payload(mut self, payload: HashMap<String, serde_json::Value>) -> Self {
        self.payload = payload;
        self
    }

    /// Inserts one payload field.
    pub fn with_field(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.payload.insert(key.into(), value);
        self
    }

    /// Validates dimension and finiteness without touching any backend.
    pub fn validate(&self, expected_dim: usize) -> crate::error::Result<()> {
        if self.id.is_empty() {
            return Err(crate::error::VectorError::InvalidInput(
                "point id must not be empty".to_string(),
            ));
        }
        if self.vector.len() != expected_dim {
            return Err(crate::error::VectorError::DimensionMismatch {
                expected: expected_dim,
                actual: self.vector.len(),
            });
        }
        if self.vector.iter().any(|v| !v.is_finite()) {
            return Err(crate::error::VectorError::InvalidInput(format!(
                "point '{}' contains non-finite vector elements",
                self.id
            )));
        }
        Ok(())
    }
}

/// One query hit: the point id, its score and its payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredPoint {
    /// Id of the matched point.
    pub id: String,
    /// Higher is better, regardless of the collection distance metric.
    pub score: f32,
    /// Stored payload of the matched point.
    #[serde(default)]
    pub payload: HashMap<String, serde_json::Value>,
}

/// Text representation of a payload value for exact-match filtering.
///
/// Numbers render in canonical decimal form and booleans as `true`/`false`,
/// mirroring the storage-layer convention so filters behave identically on
/// every backend. Complex values (arrays, objects, null) have no text form
/// and never match.
pub fn payload_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_empty_id_and_bad_vectors() {
        let point = VectorPoint::new("", vec![1.0, 0.0]);
        assert!(point.validate(2).is_err());

        let point = VectorPoint::new("a", vec![1.0]);
        assert!(matches!(
            point.validate(2),
            Err(crate::error::VectorError::DimensionMismatch {
                expected: 2,
                actual: 1
            })
        ));

        let point = VectorPoint::new("a", vec![f32::NAN, 0.0]);
        assert!(point.validate(2).is_err());
    }

    #[test]
    fn payload_text_renders_scalars_only() {
        assert_eq!(
            payload_text(&serde_json::json!("hi")).expect("string"),
            "hi"
        );
        assert_eq!(payload_text(&serde_json::json!(3)).expect("number"), "3");
        assert_eq!(
            payload_text(&serde_json::json!(true)).expect("bool"),
            "true"
        );
        assert!(payload_text(&serde_json::json!([1])).is_none());
        assert!(payload_text(&serde_json::Value::Null).is_none());
    }
}
