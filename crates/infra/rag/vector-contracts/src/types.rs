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

/// Retrieval kind declared by a collection and carried by each vector payload.
///
/// A collection declares exactly one kind at creation and rejects writes or
/// queries of another kind. The default is dense so existing callers and
/// collections keep their behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VectorKind {
    /// A single dense embedding vector.
    #[default]
    Dense,
    /// Sparse (index, weight) pairs used for exact term scoring.
    Sparse,
    /// A fixed-upper-bound sequence of dense vectors scored with MaxSim.
    Multi,
}

impl VectorKind {
    /// Human-readable name used in diagnostics and unsupported errors.
    pub fn as_name(self) -> &'static str {
        match self {
            VectorKind::Dense => "dense",
            VectorKind::Sparse => "sparse",
            VectorKind::Multi => "multi",
        }
    }
}

/// Payload of a point or query, shaped by its [`VectorKind`].
///
/// The untagged representation keeps dense vectors serialized as plain float
/// arrays, so dense JSON on every backend stays byte-for-byte compatible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VectorData {
    /// A single dense embedding vector.
    Dense(Vec<f32>),
    /// Sparse vector: strictly increasing indices paired with finite weights.
    Sparse { indices: Vec<u32>, values: Vec<f32> },
    /// Multiple dense vectors of identical dimension scored with MaxSim.
    Multi(Vec<Vec<f32>>),
}

impl VectorData {
    /// Builds a dense payload.
    pub fn dense(vector: Vec<f32>) -> Self {
        Self::Dense(vector)
    }

    /// Builds a sparse payload from index and weight arrays.
    pub fn sparse(indices: Vec<u32>, values: Vec<f32>) -> Self {
        Self::Sparse { indices, values }
    }

    /// Builds a multi-vector payload.
    pub fn multi(vectors: Vec<Vec<f32>>) -> Self {
        Self::Multi(vectors)
    }

    /// Kind described by this payload.
    pub fn kind(&self) -> VectorKind {
        match self {
            VectorData::Dense(_) => VectorKind::Dense,
            VectorData::Sparse { .. } => VectorKind::Sparse,
            VectorData::Multi(_) => VectorKind::Multi,
        }
    }

    /// Number of stored values: dense dimensions, sparse non-zero entries,
    /// or the number of sub-vectors.
    pub fn len(&self) -> usize {
        match self {
            VectorData::Dense(vector) => vector.len(),
            VectorData::Sparse { indices, .. } => indices.len(),
            VectorData::Multi(vectors) => vectors.len(),
        }
    }

    /// Whether this payload carries no values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrows the dense vector, or returns a readable invalid-input error.
    pub fn as_dense(&self) -> crate::error::Result<&[f32]> {
        match self {
            VectorData::Dense(vector) => Ok(vector),
            other => Err(crate::error::VectorError::InvalidInput(format!(
                "expected a dense vector, got {}",
                other.kind().as_name()
            ))),
        }
    }

    /// Consumes the payload into a dense vector, or returns an error.
    pub fn into_dense(self) -> crate::error::Result<Vec<f32>> {
        match self {
            VectorData::Dense(vector) => Ok(vector),
            other => Err(crate::error::VectorError::InvalidInput(format!(
                "expected a dense vector, got {}",
                other.kind().as_name()
            ))),
        }
    }

    /// Validates shape and numeric content without collection parameters.
    ///
    /// Sparse indices must be non-negative, strictly increasing and paired
    /// with finite weights; dense and multi elements must be finite. This is
    /// the check used by encoders before a collection is created or opened.
    pub fn validate_shape(&self) -> crate::error::Result<()> {
        match self {
            VectorData::Dense(vector) => {
                if vector.is_empty() {
                    return Err(crate::error::VectorError::InvalidInput(
                        "dense vector must not be empty".to_string(),
                    ));
                }
                if vector.iter().any(|v| !v.is_finite()) {
                    return Err(crate::error::VectorError::InvalidInput(
                        "dense vector contains non-finite elements".to_string(),
                    ));
                }
            }
            VectorData::Sparse { indices, values } => {
                if indices.is_empty() || indices.len() != values.len() {
                    return Err(crate::error::VectorError::InvalidInput(
                        "sparse vector needs equal, non-empty indices and values".to_string(),
                    ));
                }
                if !indices.windows(2).all(|w| w[0] < w[1]) {
                    return Err(crate::error::VectorError::InvalidInput(
                        "sparse indices must be strictly increasing".to_string(),
                    ));
                }
                if values.iter().any(|v| !v.is_finite()) {
                    return Err(crate::error::VectorError::InvalidInput(
                        "sparse vector contains non-finite weights".to_string(),
                    ));
                }
            }
            VectorData::Multi(vectors) => {
                if vectors.is_empty() {
                    return Err(crate::error::VectorError::InvalidInput(
                        "multi vector needs at least one sub-vector".to_string(),
                    ));
                }
                for vector in vectors {
                    if vector.is_empty() {
                        return Err(crate::error::VectorError::InvalidInput(
                            "multi sub-vector must not be empty".to_string(),
                        ));
                    }
                    if vector.iter().any(|v| !v.is_finite()) {
                        return Err(crate::error::VectorError::InvalidInput(
                            "multi vector contains non-finite elements".to_string(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Validates the payload against a collection kind and dimension.
    ///
    /// Dense must match `expected_dim`; multi sub-vectors must share that
    /// dimension; sparse payloads use their own vocabulary bound instead.
    pub fn validate(
        &self,
        kind: VectorKind,
        expected_dim: usize,
        max_vectors: Option<usize>,
    ) -> crate::error::Result<()> {
        if self.kind() != kind {
            return Err(crate::error::VectorError::InvalidInput(format!(
                "collection expects {} vectors, got {}",
                kind.as_name(),
                self.kind().as_name()
            )));
        }
        self.validate_shape()?;
        match self {
            VectorData::Dense(vector) => {
                if vector.len() != expected_dim {
                    return Err(crate::error::VectorError::DimensionMismatch {
                        expected: expected_dim,
                        actual: vector.len(),
                    });
                }
            }
            VectorData::Sparse { .. } => {}
            VectorData::Multi(vectors) => {
                if let Some(max) = max_vectors {
                    if vectors.len() > max {
                        return Err(crate::error::VectorError::InvalidInput(format!(
                            "multi vector has {} sub-vectors, limit is {}",
                            vectors.len(),
                            max
                        )));
                    }
                }
                for vector in vectors {
                    if vector.len() != expected_dim {
                        return Err(crate::error::VectorError::DimensionMismatch {
                            expected: expected_dim,
                            actual: vector.len(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

impl From<Vec<f32>> for VectorData {
    fn from(vector: Vec<f32>) -> Self {
        Self::Dense(vector)
    }
}

impl PartialEq<Vec<f32>> for VectorData {
    fn eq(&self, other: &Vec<f32>) -> bool {
        matches!(self, VectorData::Dense(vector) if vector == other)
    }
}

/// One point to store: an id, a kind-shaped vector payload and a JSON payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorPoint {
    /// Caller-assigned stable id; upserts with the same id overwrite.
    pub id: String,
    /// Vector payload; must match the collection kind and dimension.
    pub vector: VectorData,
    /// JSON metadata, conventionally holding `namespace` and `source`.
    #[serde(default)]
    pub payload: HashMap<String, serde_json::Value>,
}

impl VectorPoint {
    /// Builds a dense point without payload.
    pub fn new(id: impl Into<String>, vector: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            vector: VectorData::Dense(vector),
            payload: HashMap::new(),
        }
    }

    /// Builds a point with an arbitrary vector payload.
    pub fn with_data(id: impl Into<String>, vector: VectorData) -> Self {
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

    /// Validates id and dense dimension/finiteness without touching a backend.
    pub fn validate(&self, expected_dim: usize) -> crate::error::Result<()> {
        if self.id.is_empty() {
            return Err(crate::error::VectorError::InvalidInput(
                "point id must not be empty".to_string(),
            ));
        }
        self.vector.validate(VectorKind::Dense, expected_dim, None)
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
    fn dense_vector_data_keeps_plain_array_json() {
        let data = VectorData::dense(vec![1.0, 2.0]);
        assert_eq!(
            serde_json::to_value(&data).expect("serialize"),
            serde_json::json!([1.0, 2.0])
        );
        let point = VectorPoint::new("a", vec![1.0, 2.0]);
        assert_eq!(point.vector, vec![1.0, 2.0]);
        assert_eq!(point.vector.as_dense().expect("dense"), &[1.0, 2.0]);
    }

    #[test]
    fn vector_data_round_trips_all_kinds() {
        for data in [
            VectorData::dense(vec![1.0, 2.0]),
            VectorData::sparse(vec![1, 4], vec![0.5, 0.25]),
            VectorData::multi(vec![vec![1.0, 0.0], vec![0.0, 1.0]]),
        ] {
            let kind = data.kind();
            let json = serde_json::to_string(&data).expect("serialize");
            let decoded: VectorData = serde_json::from_str(&json).expect("decode");
            assert_eq!(decoded, data);
            assert_eq!(decoded.kind(), kind);
        }
    }

    #[test]
    fn vector_data_validation_enforces_each_shape() {
        assert!(VectorData::dense(vec![1.0, 2.0])
            .validate(VectorKind::Dense, 2, None)
            .is_ok());
        assert!(VectorData::dense(vec![1.0])
            .validate(VectorKind::Dense, 2, None)
            .is_err());
        assert!(VectorData::sparse(vec![2, 2], vec![1.0, 1.0])
            .validate(VectorKind::Sparse, 0, None)
            .is_err());
        assert!(VectorData::sparse(vec![2, 5], vec![1.0, 1.0])
            .validate(VectorKind::Sparse, 0, None)
            .is_ok());
        assert!(VectorData::sparse(vec![1], vec![f32::NAN])
            .validate(VectorKind::Sparse, 0, None)
            .is_err());
        assert!(VectorData::multi(vec![vec![1.0, 0.0], vec![0.0, 1.0]])
            .validate(VectorKind::Multi, 2, Some(2))
            .is_ok());
        assert!(VectorData::multi(vec![vec![1.0, 0.0], vec![0.0, 1.0]])
            .validate(VectorKind::Multi, 2, Some(1))
            .is_err());
        assert!(VectorData::dense(vec![1.0, 2.0])
            .validate(VectorKind::Sparse, 0, None)
            .is_err());
    }

    #[test]
    fn shape_validation_ignores_collection_dimensions() {
        assert!(VectorData::sparse(vec![1, 4], vec![0.5, 1.0])
            .validate_shape()
            .is_ok());
        assert!(VectorData::dense(Vec::new()).validate_shape().is_err());
        assert!(VectorData::multi(vec![vec![1.0], Vec::new()])
            .validate_shape()
            .is_err());
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
