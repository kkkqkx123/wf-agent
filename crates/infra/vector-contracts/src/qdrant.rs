//! Remote Qdrant backend over the REST API.
//!
//! Deliberately thin: only the calls retrieval needs (create, upsert,
//! query, filter delete, count) plus a minimal operations set (delete
//! collection, list, health, version). The official gRPC client stays a
//! protocol reference, never a dependency, so the crate remains on the
//! shared `reqwest` stack.
//!
//! Deployment notes (also repeated on `QdrantConfig`):
//! REST serves on `6333`, gRPC on `6334`; this backend requires the REST
//! base URL (for example `http://127.0.0.1:6333`). Server versions with the
//! same major and at most one minor apart are accepted; drift only warns.
//!
//! Point ids are arbitrary strings. Qdrant native ids only hold integers
//! or UUIDs, so each contracts id maps to one deterministic native UUID
//! (UUID v5 over the `pid` string) and the contracts id additionally
//! travels in the `pid` payload field. Upserts overwrite in one request
//! with no delete-before-insert window. Old rows written with random UUIDs
//! are not migrated; rewrite the source to converge.
//!
//! Writes and deletes request `wait=true` by default so a write followed
//! by a read observes the write. Counting is exact by default; large
//! collections may disable it. Payload indexes on `namespace`, `source`
//! and `pid` are created idempotently after ensuring the collection.
//! Only idempotent reads retry on rate limiting and transient server
//! errors; writes never retry implicitly.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::json;

use crate::error::{Result, VectorError};
use crate::filter::SearchFilter;
use crate::store::{CollectionSpec, ScrollPage, ScrollQuery, VectorQuery, VectorStorage};
use crate::types::{DistanceMetric, ScoredPoint, VectorPoint, NAMESPACE_KEY, PID_KEY, SOURCE_KEY};

/// Payload index fields ensured after every collection creation.
const INDEXED_FIELDS: [&str; 3] = [NAMESPACE_KEY, SOURCE_KEY, PID_KEY];
/// Upper bound for automatic read retries on rate limiting and transient faults.
const MAX_READ_RETRIES: usize = 3;

/// Connection parameters of the remote Qdrant backend.
#[derive(Debug, Clone)]
pub struct QdrantConfig {
    /// Base URL of the REST API, for example `http://127.0.0.1:6333`.
    /// Must use the REST port (`6333`); the gRPC port (`6334`) is rejected
    /// with a readable error.
    pub base_url: String,
    /// Optional API key sent as the `api-key` header.
    pub api_key: Option<String>,
    /// Per-request deadline in seconds.
    pub timeout_secs: u64,
    /// Optional TCP connect deadline in seconds; unset inherits the request deadline.
    pub connect_timeout_secs: Option<u64>,
    /// Extra headers sent with every request.
    pub headers: Vec<(String, String)>,
    /// Points per upsert and delete batch.
    pub batch_size: usize,
    /// Request server-side durability before acknowledging writes.
    pub wait: bool,
    /// Request exact counting; disable for cheaper counts on large collections.
    pub exact_count: bool,
}

impl QdrantConfig {
    /// Builds a config with a ten-second per-request deadline.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: None,
            timeout_secs: 10,
            connect_timeout_secs: None,
            headers: Vec::new(),
            batch_size: 256,
            wait: true,
            exact_count: true,
        }
    }

    /// Attaches an API key.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Overrides the per-request deadline in seconds.
    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs.max(1);
        self
    }

    /// Overrides the TCP connect deadline in seconds.
    pub fn with_connect_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.connect_timeout_secs = Some(timeout_secs.max(1));
        self
    }

    /// Adds a custom header sent with every request.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    /// Overrides the batch size for upserts and deletes.
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    /// Toggles server-side wait on writes and deletes.
    pub fn with_wait(mut self, wait: bool) -> Self {
        self.wait = wait;
        self
    }

    /// Toggles exact counting.
    pub fn with_exact_count(mut self, exact: bool) -> Self {
        self.exact_count = exact;
        self
    }
}

#[derive(Debug, Clone, Copy)]
struct CachedCollection {
    dimension: usize,
    distance: DistanceMetric,
}

/// Remote Qdrant backend.
#[derive(Debug, Clone)]
pub struct QdrantVectorStore {
    client: reqwest::Client,
    config: QdrantConfig,
    collections: Arc<parking_lot::RwLock<HashMap<String, CachedCollection>>>,
}

impl QdrantVectorStore {
    /// Builds the backend; no request is issued until first use.
    pub fn new(config: QdrantConfig) -> Result<Self> {
        if config.base_url.trim().is_empty() {
            return Err(VectorError::InvalidInput(
                "qdrant base_url must not be empty (REST, e.g. http://127.0.0.1:6333)".to_string(),
            ));
        }
        if config.base_url.contains(":6334") {
            return Err(VectorError::InvalidInput(
                "qdrant base_url uses the gRPC port 6334; use the REST port 6333".to_string(),
            ));
        }
        let mut builder =
            reqwest::Client::builder().timeout(Duration::from_secs(config.timeout_secs.max(1)));
        if let Some(connect) = config.connect_timeout_secs {
            builder = builder.connect_timeout(Duration::from_secs(connect.max(1)));
        }
        let client = builder
            .build()
            .map_err(|err| VectorError::Transport(err.to_string()))?;
        Ok(Self {
            client,
            config,
            collections: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        })
    }

    /// Returns the effective config (used by diagnostics and tests).
    pub fn config(&self) -> &QdrantConfig {
        &self.config
    }

    fn distance_name(distance: DistanceMetric) -> &'static str {
        match distance {
            DistanceMetric::Cosine => "Cosine",
            DistanceMetric::Dot => "Dot",
            DistanceMetric::Euclid => "Euclid",
        }
    }

    fn parse_distance(name: &str) -> Result<DistanceMetric> {
        match name.to_ascii_lowercase().as_str() {
            "cosine" => Ok(DistanceMetric::Cosine),
            "dot" => Ok(DistanceMetric::Dot),
            "euclid" | "euclidean" => Ok(DistanceMetric::Euclid),
            other => Err(VectorError::Decode(format!(
                "unknown qdrant distance '{other}'"
            ))),
        }
    }

    /// Deterministic native id for a contracts id (UUID v5, stable across restarts).
    pub fn native_id(pid: &str) -> String {
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, pid.as_bytes()).to_string()
    }

    /// Parses the `vectors` section of a collection description into size and distance.
    fn parse_collection_vectors(vectors: &serde_json::Value) -> Result<(usize, DistanceMetric)> {
        if let Some(size) = vectors.get("size").and_then(|v| v.as_u64()) {
            let distance = vectors
                .get("distance")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    VectorError::Decode("qdrant collection has no distance".to_string())
                })?;
            return Ok((size as usize, Self::parse_distance(distance)?));
        }
        if let Some(map) = vectors.as_object() {
            let candidate = map
                .get("default")
                .or_else(|| map.values().next())
                .ok_or_else(|| {
                    VectorError::Decode("qdrant collection has no vector config".to_string())
                })?;
            return Self::parse_collection_vectors(candidate);
        }
        Err(VectorError::Decode(
            "qdrant collection has no vector size".to_string(),
        ))
    }

    fn parse_collection_info(body: &str) -> Result<(usize, DistanceMetric)> {
        let parsed: serde_json::Value =
            serde_json::from_str(body).map_err(|err| VectorError::Decode(err.to_string()))?;
        let vectors = parsed
            .pointer("/result/config/params/vectors")
            .or_else(|| parsed.pointer("/result/vectors"))
            .or_else(|| parsed.pointer("/result/config/vectors"))
            .ok_or_else(|| {
                VectorError::Decode("qdrant collection response has no vectors".to_string())
            })?;
        Self::parse_collection_vectors(vectors)
    }

    /// Builds the Qdrant filter object for a contracts filter.
    pub fn build_filter(filter: &SearchFilter) -> Result<Option<serde_json::Value>> {
        let mut must = Vec::new();
        let mut must_not = Vec::new();
        if !filter.has_ids.is_empty() {
            let native: Vec<serde_json::Value> = filter
                .has_ids
                .iter()
                .map(|id| serde_json::Value::String(Self::native_id(id)))
                .collect();
            must.push(json!({"has_id": native}));
        }
        if let Some(namespace) = &filter.namespace {
            must.push(Self::text_match_condition(NAMESPACE_KEY, namespace));
        }
        if let Some(source) = &filter.source {
            must.push(Self::text_match_condition(SOURCE_KEY, source));
        }
        for (key, value) in &filter.fields {
            must.push(Self::text_match_condition(key, value));
        }
        for condition in &filter.conditions {
            let (clause, negated) = Self::build_condition(condition)?;
            if negated {
                must_not.push(clause);
            } else {
                must.push(clause);
            }
        }
        let mut filter_obj = serde_json::Map::new();
        if !must.is_empty() {
            filter_obj.insert("must".to_string(), json!(must));
        }
        if !must_not.is_empty() {
            filter_obj.insert("must_not".to_string(), json!(must_not));
        }
        if !filter.should.is_empty() {
            let mut branch = Vec::with_capacity(filter.should.len());
            for condition in &filter.should {
                let (clause, negated) = Self::build_condition(condition)?;
                if negated {
                    branch.push(json!({"must_not": [clause]}));
                } else {
                    branch.push(clause);
                }
            }
            filter_obj.insert("should".to_string(), json!(branch));
        }
        if let Some(min_should) = &filter.min_should {
            let mut branch = Vec::with_capacity(min_should.conditions.len());
            for condition in &min_should.conditions {
                let (clause, negated) = Self::build_condition(condition)?;
                if negated {
                    branch.push(json!({"must_not": [clause]}));
                } else {
                    branch.push(clause);
                }
            }
            filter_obj.insert(
                "min_should".to_string(),
                json!({"conditions": branch, "min_count": min_should.min_count}),
            );
        }
        if filter_obj.is_empty() {
            return Ok(None);
        }
        Ok(Some(serde_json::Value::Object(filter_obj)))
    }

    /// Text equality follows the contracts text form: a string condition
    /// also matches numeric and boolean payloads whose canonical text
    /// equals the condition. Typed variants are ORed inside one `should`
    /// group; plain strings stay a single match.
    fn text_match_condition(key: &str, text: &str) -> serde_json::Value {
        let mut variants = vec![json!({"key": key, "match": {"value": text}})];
        if let Ok(int_value) = text.parse::<i64>() {
            if int_value.to_string() == text {
                variants.push(json!({"key": key, "match": {"value": int_value}}));
            }
        } else if let Some(float_value) = text.parse::<f64>().ok().filter(|v| v.is_finite()) {
            let canonical = serde_json::Number::from_f64(float_value)
                .map(|n| n.to_string())
                .unwrap_or_default();
            if canonical == text {
                variants.push(json!({"key": key, "match": {"value": float_value}}));
            }
        }
        if text == "true" || text == "false" {
            variants.push(json!({"key": key, "match": {"value": text == "true"}}));
        }
        if variants.len() == 1 {
            variants.pop().expect("single variant")
        } else {
            json!({"should": variants})
        }
    }

    fn build_condition(
        condition: &crate::filter::FieldFilter,
    ) -> Result<(serde_json::Value, bool)> {
        use crate::filter::FilterOperator::*;
        let key = &condition.key;
        match (&condition.operator, &condition.value) {
            (Equal, crate::filter::FilterValue::Text(text)) => {
                Ok((Self::text_match_condition(key, text), false))
            }
            (Equal, crate::filter::FilterValue::Number(number)) => {
                Ok((json!({"key": key, "match": {"value": number}}), false))
            }
            (Equal, crate::filter::FilterValue::Bool(flag)) => {
                Ok((json!({"key": key, "match": {"value": flag}}), false))
            }
            (NotEqual, crate::filter::FilterValue::Text(text)) => {
                Ok((Self::text_match_condition(key, text), true))
            }
            (NotEqual, crate::filter::FilterValue::Number(number)) => {
                Ok((json!({"key": key, "match": {"value": number}}), true))
            }
            (NotEqual, crate::filter::FilterValue::Bool(flag)) => {
                Ok((json!({"key": key, "match": {"value": flag}}), true))
            }
            (GreaterThan, crate::filter::FilterValue::Number(bound)) => {
                Ok((json!({"key": key, "range": {"gt": bound}}), false))
            }
            (GreaterEqual, crate::filter::FilterValue::Number(bound)) => {
                Ok((json!({"key": key, "range": {"gte": bound}}), false))
            }
            (LessThan, crate::filter::FilterValue::Number(bound)) => {
                Ok((json!({"key": key, "range": {"lt": bound}}), false))
            }
            (LessEqual, crate::filter::FilterValue::Number(bound)) => {
                Ok((json!({"key": key, "range": {"lte": bound}}), false))
            }
            (Prefix, crate::filter::FilterValue::Text(prefix)) => {
                Ok((json!({"key": key, "match": {"prefix": prefix}}), false))
            }
            (Contains, crate::filter::FilterValue::Text(part)) => {
                Ok((json!({"key": key, "match": {"substring": part}}), false))
            }
            (IsEmpty, _) => Ok((json!({"is_empty": {"key": key}}), false)),
            (IsNull, _) => Ok((json!({"is_null": {"key": key}}), false)),
            (ValuesEqual, crate::filter::FilterValue::Number(bound)) => {
                let count = Self::count_bound(*bound)?;
                Ok((
                    json!({"key": key, "values_count": {"gte": count, "lte": count}}),
                    false,
                ))
            }
            (ValuesGreaterThan, crate::filter::FilterValue::Number(bound)) => {
                let count = Self::count_bound_floor(*bound)?;
                Ok((json!({"key": key, "values_count": {"gt": count}}), false))
            }
            (ValuesGreaterEqual, crate::filter::FilterValue::Number(bound)) => {
                let count = Self::count_bound_ceil(*bound)?;
                Ok((json!({"key": key, "values_count": {"gte": count}}), false))
            }
            (ValuesLessThan, crate::filter::FilterValue::Number(bound)) => {
                let count = Self::count_bound_ceil(*bound)?;
                Ok((json!({"key": key, "values_count": {"lt": count}}), false))
            }
            (ValuesLessEqual, crate::filter::FilterValue::Number(bound)) => {
                let count = Self::count_bound_floor(*bound)?;
                Ok((json!({"key": key, "values_count": {"lte": count}}), false))
            }
            _ => Err(VectorError::InvalidInput(format!(
                "unsupported filter condition on '{key}'"
            ))),
        }
    }

    /// Integer bound for an exact length comparison.
    fn count_bound(bound: f64) -> Result<u64> {
        if !bound.is_finite() || bound < 0.0 || bound.fract() != 0.0 {
            return Err(VectorError::InvalidInput(format!(
                "array length bound must be a non-negative integer, got {bound}"
            )));
        }
        Ok(bound as u64)
    }

    /// Integer bound for a strict lower length comparison.
    fn count_bound_floor(bound: f64) -> Result<u64> {
        if !bound.is_finite() || bound < 0.0 {
            return Err(VectorError::InvalidInput(format!(
                "array length bound must be non-negative, got {bound}"
            )));
        }
        Ok(bound.floor() as u64)
    }

    /// Integer bound for a lower-or-equal length comparison.
    fn count_bound_ceil(bound: f64) -> Result<u64> {
        if !bound.is_finite() || bound < 0.0 {
            return Err(VectorError::InvalidInput(format!(
                "array length bound must be non-negative, got {bound}"
            )));
        }
        Ok(bound.ceil() as u64)
    }

    /// Builds the REST request body for a dense query, including the
    /// optional recall-tuning params. Memory and simvec ignore the params
    /// but reject a zero `hnsw_ef` the same way.
    fn search_body(query: &VectorQuery) -> Result<serde_json::Value> {
        if query.hnsw_ef == Some(0) {
            return Err(VectorError::InvalidInput(
                "hnsw_ef must be positive".to_string(),
            ));
        }
        let mut body = json!({
            "query": query.vector,
            "limit": query.limit,
            "with_payload": true,
        });
        if let Some(threshold) = query.score_threshold {
            body["score_threshold"] = json!(threshold);
        }
        if let Some(filter) = &query.filter {
            if let Some(mapped) = Self::build_filter(filter)? {
                body["filter"] = mapped;
            }
        }
        if query.exact.is_some() || query.hnsw_ef.is_some() {
            let mut params = serde_json::Map::new();
            if let Some(exact) = query.exact {
                params.insert("exact".to_string(), json!(exact));
            }
            if let Some(hnsw_ef) = query.hnsw_ef {
                params.insert("hnsw_ef".to_string(), json!(hnsw_ef));
            }
            body["params"] = serde_json::Value::Object(params);
        }
        Ok(body)
    }

    /// Parses one scroll response into points plus the next cursor.
    fn parse_scroll_page(text: &str) -> Result<(Vec<VectorPoint>, Option<String>)> {
        let parsed: serde_json::Value =
            serde_json::from_str(text).map_err(|err| VectorError::Decode(err.to_string()))?;
        let result = parsed.get("result").ok_or_else(|| {
            VectorError::Decode("qdrant scroll response has no result".to_string())
        })?;
        let raw_points = result
            .get("points")
            .and_then(|value| value.as_array())
            .ok_or_else(|| {
                VectorError::Decode("qdrant scroll response has no result.points".to_string())
            })?;
        let mut points = Vec::with_capacity(raw_points.len());
        for point in raw_points {
            let payload: HashMap<String, serde_json::Value> = point
                .get("payload")
                .and_then(|value| value.as_object())
                .map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            let Some(pid) = payload.get(PID_KEY).and_then(|value| value.as_str()) else {
                continue;
            };
            let Some(vector_value) = point.get("vector") else {
                return Err(VectorError::Decode(format!(
                    "qdrant point '{pid}' has no vector"
                )));
            };
            let vector = Self::parse_vector(vector_value)?;
            points.push(VectorPoint {
                id: pid.to_string(),
                vector,
                payload,
            });
        }
        let next_offset = result
            .get("next_page_offset")
            .and_then(|value| match value {
                serde_json::Value::Null => None,
                serde_json::Value::String(text) => Some(text.clone()),
                serde_json::Value::Number(number) => Some(number.to_string()),
                _ => None,
            });
        Ok((points, next_offset))
    }

    /// Scroll offset for the REST call: server cursors pass through,
    /// contracts ids convert to their deterministic native form.
    fn scroll_offset(offset: &str) -> String {
        if uuid::Uuid::parse_str(offset).is_ok() {
            offset.to_string()
        } else {
            Self::native_id(offset)
        }
    }

    /// Parses a stored vector in either dense-array or named-map form.
    fn parse_vector(value: &serde_json::Value) -> Result<Vec<f32>> {
        if let Some(array) = value.as_array() {
            return array
                .iter()
                .map(|v| {
                    v.as_f64().map(|n| n as f32).ok_or_else(|| {
                        VectorError::Decode("qdrant vector holds non-numeric elements".to_string())
                    })
                })
                .collect();
        }
        if let Some(map) = value.as_object() {
            let candidate = map
                .get("default")
                .or_else(|| map.values().next())
                .ok_or_else(|| VectorError::Decode("qdrant named vector is empty".to_string()))?;
            return Self::parse_vector(candidate);
        }
        Err(VectorError::Decode(
            "qdrant point has no vector".to_string(),
        ))
    }

    /// Normalizes Euclid scores to the contracts convention (higher is
    /// better, negated distance matching the in-process backend). Cosine
    /// and dot pass through. Positive Euclid raws are assumed to be either
    /// `1/(1+distance)` (at most one) or a raw distance and are converted
    /// to negated distance; non-positive raws already follow the convention.
    fn normalize_score(raw: f32, distance: Option<DistanceMetric>) -> f32 {
        if !matches!(distance, Some(DistanceMetric::Euclid)) {
            return raw;
        }
        if raw <= 0.0 {
            return raw;
        }
        if raw <= 1.0 {
            if raw <= f32::EPSILON {
                return f32::NEG_INFINITY;
            }
            return -((1.0 / raw) - 1.0);
        }
        -raw
    }

    /// Official compatibility rule: same major, minor drift at most one.
    pub fn is_compatible_version(client: &str, server: &str) -> bool {
        fn parts(version: &str) -> Option<(u64, u64)> {
            let core = version.split('-').next()?.split('+').next()?;
            let mut iter = core.split('.');
            let major = iter.next()?.trim().parse::<u64>().ok()?;
            let minor = iter
                .next()
                .map(str::trim)
                .unwrap_or("0")
                .parse::<u64>()
                .ok()?;
            Some((major, minor))
        }
        match (parts(client), parts(server)) {
            (Some((major_a, minor_a)), Some((major_b, minor_b))) => {
                major_a == major_b && minor_a.abs_diff(minor_b) <= 1
            }
            _ => false,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url.trim_end_matches('/'), path)
    }

    fn wait_suffix(&self) -> &'static str {
        if self.config.wait {
            "?wait=true"
        } else {
            "?wait=false"
        }
    }

    fn batch_size(&self) -> usize {
        self.config.batch_size.max(1)
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let mut request = request;
        if let Some(api_key) = &self.config.api_key {
            request = request.header("api-key", api_key.clone());
        }
        for (key, value) in &self.config.headers {
            request = request.header(key.clone(), value.clone());
        }
        request
            .send()
            .await
            .map_err(|err| VectorError::Transport(err.to_string()))
    }

    fn is_retryable_status(status: reqwest::StatusCode) -> bool {
        matches!(
            status,
            reqwest::StatusCode::TOO_MANY_REQUESTS
                | reqwest::StatusCode::BAD_GATEWAY
                | reqwest::StatusCode::SERVICE_UNAVAILABLE
                | reqwest::StatusCode::GATEWAY_TIMEOUT
        )
    }

    /// Sends one idempotent read with bounded retries on rate limiting,
    /// transient server errors and transport failures.
    async fn send_read(
        &self,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response> {
        let mut attempt = 0;
        loop {
            match self.send(build()).await {
                Ok(response) => {
                    if Self::is_retryable_status(response.status()) && attempt < MAX_READ_RETRIES {
                        attempt += 1;
                        tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                        continue;
                    }
                    return Ok(response);
                }
                Err(VectorError::Transport(_)) if attempt < MAX_READ_RETRIES => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                    continue;
                }
                Err(other) => return Err(other),
            }
        }
    }

    fn map_status(status: reqwest::StatusCode, body: &str, collection: &str) -> Result<()> {
        if status.is_success() {
            return Ok(());
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(VectorError::Unauthorized(format!(
                "qdrant rejected credentials (status {status})"
            )));
        }
        if status == reqwest::StatusCode::BAD_REQUEST
            || status == reqwest::StatusCode::UNPROCESSABLE_ENTITY
        {
            return Err(VectorError::InvalidInput(format!(
                "qdrant rejected request (status {status}): {body}"
            )));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return Err(VectorError::Transport(format!(
                "qdrant transient failure (status {status}): {body}"
            )));
        }
        Err(VectorError::Backend(format!(
            "qdrant status {status}: {body}"
        )))
    }

    fn is_dimension_body(body: &str) -> bool {
        let lower = body.to_ascii_lowercase();
        lower.contains("dimension")
            || lower.contains("vector size")
            || lower.contains("size") && lower.contains("vector")
    }

    fn cached(&self, collection: &str) -> Option<CachedCollection> {
        self.collections.read().get(collection).copied()
    }

    fn remember(&self, collection: &str, dimension: usize, distance: DistanceMetric) {
        self.collections.write().insert(
            collection.to_string(),
            CachedCollection {
                dimension,
                distance,
            },
        );
    }

    async fn collection_config(&self, collection: &str) -> Result<CachedCollection> {
        if let Some(cached) = self.cached(collection) {
            return Ok(cached);
        }
        let response = self
            .send_read(|| {
                self.client
                    .get(self.url(&format!("/collections/{collection}")))
            })
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Self::map_status(status, &body, collection)?;
        let (dimension, distance) = Self::parse_collection_info(&body)?;
        self.remember(collection, dimension, distance);
        Ok(CachedCollection {
            dimension,
            distance,
        })
    }

    async fn ensure_payload_indexes(&self, collection: &str) {
        for field in INDEXED_FIELDS {
            let response = self
                .send(
                    self.client
                        .put(self.url(&format!(
                            "/collections/{collection}/index{}",
                            self.wait_suffix()
                        )))
                        .json(&json!({"field_name": field, "field_schema": "keyword"})),
                )
                .await;
            match response {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() || status == reqwest::StatusCode::CONFLICT {
                        continue;
                    }
                    let body = response.text().await.unwrap_or_default();
                    tracing::warn!(
                        collection = collection,
                        field = field,
                        status = %status,
                        "qdrant index creation reported {status}: {body}"
                    );
                }
                Err(err) => {
                    tracing::warn!(
                        collection = collection,
                        field = field,
                        error = %err,
                        "qdrant index creation failed"
                    );
                }
            }
        }
    }

    /// Validates a payload-update target id shared by the three payload ops.
    fn check_payload_target(id: &str) -> Result<()> {
        if id.is_empty() {
            return Err(VectorError::InvalidInput(
                "point id must not be empty".to_string(),
            ));
        }
        Ok(())
    }

    /// Fails on missing collections and missing points so payload updates
    /// report the same not-found errors on every backend.
    async fn require_point(&self, collection: &str, id: &str) -> Result<()> {
        if self.fetch(collection, &[id.to_string()]).await?.is_empty() {
            return Err(VectorError::InvalidInput(format!(
                "point '{id}' not found in collection '{collection}'"
            )));
        }
        Ok(())
    }

    /// Collection dimensions and distance (专有扩展, not part of the contract).
    pub async fn collection_info(&self, collection: &str) -> Result<(usize, DistanceMetric)> {
        let info = self.collection_config(collection).await?;
        Ok((info.dimension, info.distance))
    }

    /// Lists collection names (专有扩展 for operations and tests).
    pub async fn list_collections(&self) -> Result<Vec<String>> {
        let response = self
            .send_read(|| self.client.get(self.url("/collections")))
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(VectorError::Backend(format!(
                "qdrant list failed with status {status}: {body}"
            )));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|err| VectorError::Decode(err.to_string()))?;
        let names = parsed
            .pointer("/result/collections")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        item.get("name")
                            .and_then(|n| n.as_str())
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>()
            })
            .ok_or_else(|| {
                VectorError::Decode("qdrant list response has no result.collections".to_string())
            })?;
        Ok(names)
    }

    /// Checks server reachability (专有扩展).
    pub async fn health_check(&self) -> Result<()> {
        let started = Instant::now();
        let response = self
            .send_read(|| self.client.get(self.url("/healthz")))
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        tracing::debug!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant health check"
        );
        if status.is_success() {
            return Ok(());
        }
        Err(VectorError::Transport(format!(
            "qdrant health check failed with status {status}: {body}"
        )))
    }

    /// Returns the server version string (专有扩展).
    pub async fn version(&self) -> Result<String> {
        let response = self.send_read(|| self.client.get(self.url("/"))).await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(VectorError::Backend(format!(
                "qdrant version probe failed with status {status}: {body}"
            )));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|err| VectorError::Decode(err.to_string()))?;
        parsed
            .get("version")
            .and_then(|v| v.as_str())
            .or_else(|| parsed.pointer("/result/version").and_then(|v| v.as_str()))
            .map(str::to_string)
            .ok_or_else(|| {
                VectorError::Decode("qdrant version response has no version".to_string())
            })
    }

    /// Warns on version drift without blocking (major must match, minor drift at most one).
    pub async fn check_compatibility(&self, client_version: &str) -> Result<bool> {
        let server = self.version().await?;
        let compatible = Self::is_compatible_version(client_version, &server);
        if !compatible {
            tracing::warn!(
                client_version = client_version,
                server_version = %server,
                "qdrant version drift detected"
            );
        }
        Ok(compatible)
    }
}

#[async_trait]
impl VectorStorage for QdrantVectorStore {
    fn backend_name(&self) -> &'static str {
        "qdrant"
    }

    async fn ensure_collection(&self, spec: &CollectionSpec) -> Result<bool> {
        let started = Instant::now();
        tracing::debug!(collection = %spec.name, dimension = spec.dimension, "qdrant ensure collection");
        if spec.name.is_empty() {
            return Err(VectorError::InvalidInput(
                "collection name must not be empty".to_string(),
            ));
        }
        if spec.dimension == 0 {
            return Err(VectorError::InvalidInput(
                "collection dimension must be positive".to_string(),
            ));
        }
        let response = self
            .send_read(|| {
                self.client
                    .get(self.url(&format!("/collections/{}", spec.name)))
            })
            .await?;
        if response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            let (dimension, distance) = Self::parse_collection_info(&body)?;
            if dimension != spec.dimension {
                return Err(VectorError::DimensionMismatch {
                    expected: dimension,
                    actual: spec.dimension,
                });
            }
            if distance != spec.distance {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses {:?} distance, requested {:?}",
                    spec.name, distance, spec.distance
                )));
            }
            self.remember(&spec.name, dimension, distance);
            self.ensure_payload_indexes(&spec.name).await;
            tracing::debug!(
                collection = %spec.name,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "qdrant ensure collection hit"
            );
            return Ok(false);
        }
        if response.status() != reqwest::StatusCode::NOT_FOUND {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Self::map_status(status, &body, &spec.name)?;
            return Err(VectorError::Backend(format!(
                "qdrant describe failed with status {status}: {body}"
            )));
        }
        let response = self
            .send(
                self.client
                    .put(self.url(&format!(
                        "/collections/{}{}",
                        spec.name,
                        self.wait_suffix()
                    )))
                    .json(
                    &json!({"vectors": {"size": spec.dimension, "distance": Self::distance_name(spec.distance)}}),
                ),
            )
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Self::map_status(status, &body, &spec.name)?;
        self.remember(&spec.name, spec.dimension, spec.distance);
        self.ensure_payload_indexes(&spec.name).await;
        tracing::debug!(
            collection = %spec.name,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant ensure collection created"
        );
        Ok(true)
    }

    async fn delete_collection(&self, collection: &str) -> Result<()> {
        if collection.is_empty() {
            return Err(VectorError::InvalidInput(
                "collection name must not be empty".to_string(),
            ));
        }
        let started = Instant::now();
        tracing::debug!(collection = %collection, "qdrant delete collection");
        let response = self
            .send(
                self.client
                    .delete(self.url(&format!("/collections/{collection}{}", self.wait_suffix()))),
            )
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Self::map_status(status, &body, collection)?;
        self.collections.write().remove(collection);
        tracing::debug!(
            collection = %collection,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant delete collection done"
        );
        Ok(())
    }

    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()> {
        let started = Instant::now();
        tracing::debug!(collection = %collection, count = points.len(), "qdrant upsert");
        for point in points {
            if point.id.is_empty() {
                return Err(VectorError::InvalidInput(
                    "point id must not be empty".to_string(),
                ));
            }
            if point.vector.iter().any(|v| !v.is_finite()) {
                return Err(VectorError::InvalidInput(format!(
                    "point '{}' contains non-finite vector elements",
                    point.id
                )));
            }
        }
        if points.is_empty() {
            return Ok(());
        }
        let info = self.collection_config(collection).await?;
        for point in points {
            if point.vector.len() != info.dimension {
                return Err(VectorError::DimensionMismatch {
                    expected: info.dimension,
                    actual: point.vector.len(),
                });
            }
        }
        let batch = self.batch_size();
        for chunk in points.chunks(batch) {
            let payload: Vec<serde_json::Value> = chunk
                .iter()
                .map(|point| {
                    let mut fields: HashMap<String, serde_json::Value> = point.payload.clone();
                    fields.insert(PID_KEY.to_string(), json!(point.id));
                    json!({
                        "id": Self::native_id(&point.id),
                        "vector": point.vector,
                        "payload": fields,
                    })
                })
                .collect();
            let response = self
                .send(
                    self.client
                        .put(self.url(&format!(
                            "/collections/{collection}/points{}",
                            self.wait_suffix()
                        )))
                        .json(&json!({"points": payload})),
                )
                .await?;
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::BAD_REQUEST && Self::is_dimension_body(&body) {
                let actual = chunk.first().map(|p| p.vector.len()).unwrap_or(0);
                return Err(VectorError::DimensionMismatch {
                    expected: info.dimension,
                    actual,
                });
            }
            Self::map_status(status, &body, collection)?;
        }
        tracing::debug!(
            collection = %collection,
            count = points.len(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant upsert done"
        );
        Ok(())
    }

    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredPoint>> {
        let started = Instant::now();
        tracing::debug!(collection = %collection, limit = query.limit, "qdrant search");
        if query.vector.iter().any(|v| !v.is_finite()) {
            return Err(VectorError::InvalidInput(
                "query vector contains non-finite elements".to_string(),
            ));
        }
        let info = self.collection_config(collection).await.ok();
        if let Some(info) = info {
            if query.vector.len() != info.dimension {
                return Err(VectorError::DimensionMismatch {
                    expected: info.dimension,
                    actual: query.vector.len(),
                });
            }
        }
        let body = Self::search_body(query)?;
        let response = self
            .send_read(|| {
                self.client
                    .post(self.url(&format!("/collections/{collection}/points/query")))
                    .json(&body)
            })
            .await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::BAD_REQUEST && Self::is_dimension_body(&text) {
            if let Some(info) = info {
                return Err(VectorError::DimensionMismatch {
                    expected: info.dimension,
                    actual: query.vector.len(),
                });
            }
        }
        Self::map_status(status, &text, collection)?;
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| VectorError::Decode(err.to_string()))?;
        let points = parsed
            .pointer("/result/points")
            .and_then(|value| value.as_array())
            .ok_or_else(|| {
                VectorError::Decode("qdrant query response has no result.points".to_string())
            })?;
        let distance = info.map(|info| info.distance);
        let mut hits = Vec::with_capacity(points.len());
        for point in points {
            let payload = point
                .get("payload")
                .and_then(|value| value.as_object())
                .map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            let id = point
                .get("payload")
                .and_then(|payload| payload.get(PID_KEY))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    point
                        .get("id")
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                });
            let raw = point
                .get("score")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0) as f32;
            hits.push(ScoredPoint {
                id,
                score: Self::normalize_score(raw, distance),
                payload,
            });
        }
        tracing::debug!(
            collection = %collection,
            hits = hits.len(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant search done"
        );
        Ok(hits)
    }

    async fn fetch(&self, collection: &str, ids: &[String]) -> Result<Vec<VectorPoint>> {
        let started = Instant::now();
        tracing::debug!(collection = %collection, count = ids.len(), "qdrant fetch");
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conditions: Vec<serde_json::Value> = ids
            .iter()
            .map(|id| json!({"key": PID_KEY, "match": {"value": id}}))
            .collect();
        let body = json!({
            "filter": {"should": conditions},
            "limit": ids.len(),
            "with_payload": true,
            "with_vector": true,
        });
        let response = self
            .send_read(|| {
                self.client
                    .post(self.url(&format!("/collections/{collection}/points/scroll")))
                    .json(&body)
            })
            .await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        Self::map_status(status, &text, collection)?;
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| VectorError::Decode(err.to_string()))?;
        let points = parsed
            .pointer("/result/points")
            .and_then(|value| value.as_array())
            .or_else(|| parsed.pointer("/result").and_then(|value| value.as_array()))
            .ok_or_else(|| {
                VectorError::Decode("qdrant scroll response has no result.points".to_string())
            })?;
        let mut by_pid: HashMap<String, VectorPoint> = HashMap::with_capacity(points.len());
        for point in points {
            let Some(pid) = point
                .get("payload")
                .and_then(|payload| payload.get(PID_KEY))
                .and_then(|value| value.as_str())
            else {
                continue;
            };
            let Some(vector_value) = point.get("vector") else {
                return Err(VectorError::Decode(format!(
                    "qdrant point '{pid}' has no vector"
                )));
            };
            let vector = Self::parse_vector(vector_value)?;
            let payload = point
                .get("payload")
                .and_then(|value| value.as_object())
                .map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            by_pid.insert(
                pid.to_string(),
                VectorPoint {
                    id: pid.to_string(),
                    vector,
                    payload,
                },
            );
        }
        let ordered = ids
            .iter()
            .filter_map(|id| by_pid.remove(id))
            .collect::<Vec<_>>();
        tracing::debug!(
            collection = %collection,
            requested = ids.len(),
            found = ordered.len(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant fetch done"
        );
        Ok(ordered)
    }

    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> Result<u64> {
        let started = Instant::now();
        tracing::debug!(collection = %collection, count = ids.len(), "qdrant delete by id");
        if ids.is_empty() {
            return Ok(0);
        }
        let mut counted: u64 = 0;
        for chunk in ids.chunks(self.batch_size()) {
            let filter_conditions: Vec<serde_json::Value> = chunk
                .iter()
                .map(|id| json!({"key": PID_KEY, "match": {"value": id}}))
                .collect();
            let count_body = json!({"filter": {"should": filter_conditions}, "exact": true});
            let count_response = self
                .send_read(|| {
                    self.client
                        .post(self.url(&format!("/collections/{collection}/points/count")))
                        .json(&count_body)
                })
                .await?;
            let count_status = count_response.status();
            let count_text = count_response.text().await.unwrap_or_default();
            Self::map_status(count_status, &count_text, collection)?;
            let parsed: serde_json::Value = serde_json::from_str(&count_text)
                .map_err(|err| VectorError::Decode(err.to_string()))?;
            counted += parsed
                .pointer("/result/count")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| {
                    VectorError::Decode("qdrant count response has no result.count".to_string())
                })?;
            let native: Vec<String> = chunk.iter().map(|id| Self::native_id(id)).collect();
            let response = self
                .send(
                    self.client
                        .post(self.url(&format!(
                            "/collections/{collection}/points/delete{}",
                            self.wait_suffix()
                        )))
                        .json(&json!({"points": native})),
                )
                .await?;
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Self::map_status(status, &body, collection)?;
        }
        tracing::debug!(
            collection = %collection,
            removed = counted,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant delete by id done"
        );
        Ok(counted)
    }

    async fn delete_by_source(
        &self,
        collection: &str,
        namespace: Option<&str>,
        source: &str,
    ) -> Result<u64> {
        let started = Instant::now();
        tracing::debug!(collection = %collection, source = %source, "qdrant delete by source");
        let mut filter = SearchFilter::new().with_source(source);
        if let Some(namespace) = namespace {
            filter = filter.with_namespace(namespace);
        }
        let existing = self.count(collection, Some(&filter)).await?;
        let mapped = Self::build_filter(&filter)?
            .ok_or_else(|| VectorError::InvalidInput("source must not be empty".to_string()))?;
        let response = self
            .send(
                self.client
                    .post(self.url(&format!(
                        "/collections/{collection}/points/delete{}",
                        self.wait_suffix()
                    )))
                    .json(&json!({"filter": mapped})),
            )
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Self::map_status(status, &body, collection)?;
        tracing::debug!(
            collection = %collection,
            removed = existing,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant delete by source done"
        );
        Ok(existing)
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64> {
        let started = Instant::now();
        let mut body = json!({"exact": self.config.exact_count});
        if let Some(filter) = filter {
            if let Some(mapped) = Self::build_filter(filter)? {
                body["filter"] = mapped;
            }
        }
        let response = self
            .send_read(|| {
                self.client
                    .post(self.url(&format!("/collections/{collection}/points/count")))
                    .json(&body)
            })
            .await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        Self::map_status(status, &text, collection)?;
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| VectorError::Decode(err.to_string()))?;
        let count = parsed
            .pointer("/result/count")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| {
                VectorError::Decode("qdrant count response has no result.count".to_string())
            })?;
        tracing::debug!(
            collection = %collection,
            count = count,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant count done"
        );
        Ok(count)
    }

    async fn scroll(&self, collection: &str, query: &ScrollQuery) -> Result<ScrollPage> {
        let started = Instant::now();
        tracing::debug!(collection = %collection, limit = query.limit, "qdrant scroll");
        let limit = query.limit.max(1);
        let mut body = json!({
            "limit": limit,
            "with_payload": true,
            "with_vector": true,
        });
        if let Some(filter) = &query.filter {
            if let Some(mapped) = Self::build_filter(filter)? {
                body["filter"] = mapped;
            }
        }
        if let Some(offset) = &query.offset {
            body["offset"] = json!(Self::scroll_offset(offset));
        }
        let response = self
            .send_read(|| {
                self.client
                    .post(self.url(&format!("/collections/{collection}/points/scroll")))
                    .json(&body)
            })
            .await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        Self::map_status(status, &text, collection)?;
        let (mut points, next_offset) = Self::parse_scroll_page(&text)?;
        points.truncate(limit);
        let next_offset = if points.is_empty() || points.len() < limit {
            None
        } else {
            next_offset
        };
        tracing::debug!(
            collection = %collection,
            points = points.len(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant scroll done"
        );
        Ok(ScrollPage {
            points,
            next_offset,
        })
    }

    async fn overwrite_payload(
        &self,
        collection: &str,
        id: &str,
        payload: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let started = Instant::now();
        tracing::debug!(collection = %collection, "qdrant overwrite payload");
        Self::check_payload_target(id)?;
        self.require_point(collection, id).await?;
        let mut full = payload;
        full.insert(PID_KEY.to_string(), json!(id));
        let response = self
            .send(
                self.client
                    .put(self.url(&format!(
                        "/collections/{collection}/points/payload{}",
                        self.wait_suffix()
                    )))
                    .json(&json!({"payload": full, "points": [Self::native_id(id)]})),
            )
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Self::map_status(status, &body, collection)?;
        tracing::debug!(
            collection = %collection,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant overwrite payload done"
        );
        Ok(())
    }

    async fn merge_payload(
        &self,
        collection: &str,
        id: &str,
        fields: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let started = Instant::now();
        tracing::debug!(collection = %collection, "qdrant merge payload");
        Self::check_payload_target(id)?;
        self.require_point(collection, id).await?;
        let mut merged = fields;
        merged.remove(PID_KEY);
        if merged.is_empty() {
            return Ok(());
        }
        let response = self
            .send(
                self.client
                    .post(self.url(&format!(
                        "/collections/{collection}/points/payload{}",
                        self.wait_suffix()
                    )))
                    .json(&json!({"payload": merged, "points": [Self::native_id(id)]})),
            )
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Self::map_status(status, &body, collection)?;
        tracing::debug!(
            collection = %collection,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant merge payload done"
        );
        Ok(())
    }

    async fn delete_payload_keys(&self, collection: &str, id: &str, keys: &[String]) -> Result<()> {
        let started = Instant::now();
        tracing::debug!(collection = %collection, "qdrant delete payload keys");
        Self::check_payload_target(id)?;
        self.require_point(collection, id).await?;
        let kept: Vec<String> = keys
            .iter()
            .filter(|key| key.as_str() != PID_KEY)
            .cloned()
            .collect();
        if kept.is_empty() {
            return Ok(());
        }
        let response = self
            .send(
                self.client
                    .post(self.url(&format!(
                        "/collections/{collection}/points/payload/delete{}",
                        self.wait_suffix()
                    )))
                    .json(&json!({"keys": kept, "points": [Self::native_id(id)]})),
            )
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Self::map_status(status, &body, collection)?;
        tracing::debug!(
            collection = %collection,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant delete payload keys done"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_builder_covers_namespace_source_and_fields() {
        let filter = SearchFilter::new()
            .with_namespace("kb")
            .with_source("doc.md")
            .with_field("lang", "en");
        let built = QdrantVectorStore::build_filter(&filter)
            .expect("filter")
            .expect("some");
        let must = built
            .get("must")
            .and_then(|value| value.as_array())
            .expect("must");
        assert_eq!(must.len(), 3);
        assert!(QdrantVectorStore::build_filter(&SearchFilter::new())
            .expect("empty")
            .is_none());
    }

    #[test]
    fn filter_builder_covers_conditions() {
        use crate::filter::{FieldFilter, FilterOperator};
        let filter = SearchFilter::new()
            .with_condition(FieldFilter::new("page", FilterOperator::GreaterEqual, 3.0))
            .with_condition(FieldFilter::new("lang", FilterOperator::NotEqual, "zh"));
        let built = QdrantVectorStore::build_filter(&filter)
            .expect("filter")
            .expect("some");
        assert_eq!(
            built
                .get("must")
                .and_then(|value| value.as_array())
                .expect("must")
                .len(),
            1
        );
        assert_eq!(
            built
                .get("must_not")
                .and_then(|value| value.as_array())
                .expect("must_not")
                .len(),
            1
        );
        let bad = SearchFilter::new().with_condition(FieldFilter::new(
            "lang",
            FilterOperator::GreaterThan,
            "a",
        ));
        assert!(QdrantVectorStore::build_filter(&bad).is_err());
    }

    #[test]
    fn filter_builder_covers_branch_and_id_groups() {
        use crate::filter::{FieldFilter, FilterOperator};
        let filter = SearchFilter::new()
            .with_should(FieldFilter::new("lang", FilterOperator::Equal, "en"))
            .with_should(FieldFilter::new("lang", FilterOperator::NotEqual, "zh"))
            .with_min_should(
                vec![FieldFilter::new("page", FilterOperator::GreaterEqual, 3.0)],
                1,
            )
            .with_has_id("doc.md#0");
        let built = QdrantVectorStore::build_filter(&filter)
            .expect("filter")
            .expect("some");
        assert_eq!(
            built
                .get("must")
                .and_then(|value| value.as_array())
                .expect("must")
                .len(),
            1
        );
        let should = built
            .get("should")
            .and_then(|value| value.as_array())
            .expect("should");
        assert_eq!(should.len(), 2);
        assert!(should[1].get("must_not").is_some());
        let min_should = built.get("min_should").expect("min_should");
        assert_eq!(
            min_should.get("min_count").and_then(|value| value.as_u64()),
            Some(1)
        );
        assert!(QdrantVectorStore::build_filter(&SearchFilter::new())
            .expect("empty")
            .is_none());
    }

    #[test]
    fn filter_builder_covers_match_and_length_operators() {
        use crate::filter::{FieldFilter, FilterOperator, FilterValue};
        let cases = vec![
            (
                FieldFilter::new("city", FilterOperator::Prefix, "ber"),
                "prefix",
            ),
            (
                FieldFilter::new("desc", FilterOperator::Contains, "lo wo"),
                "substring",
            ),
            (
                FieldFilter::new("nick", FilterOperator::IsEmpty, true),
                "is_empty",
            ),
            (
                FieldFilter::new("gone", FilterOperator::IsNull, true),
                "is_null",
            ),
            (
                FieldFilter::new("tags", FilterOperator::ValuesEqual, 2.0),
                "values_count",
            ),
            (
                FieldFilter::new("tags", FilterOperator::ValuesGreaterThan, 1.0),
                "values_count",
            ),
        ];
        for (condition, marker) in cases {
            let filter = SearchFilter::new().with_condition(condition);
            let built = QdrantVectorStore::build_filter(&filter)
                .expect("filter")
                .expect("some");
            let text = serde_json::to_string(&built).expect("json");
            assert!(text.contains(marker), "{text}");
        }
        let bad = SearchFilter::new().with_condition(FieldFilter::new(
            "city",
            FilterOperator::Prefix,
            FilterValue::Number(1.0),
        ));
        assert!(QdrantVectorStore::build_filter(&bad).is_err());
        let bad = SearchFilter::new().with_condition(FieldFilter::new(
            "tags",
            FilterOperator::ValuesEqual,
            2.5,
        ));
        assert!(QdrantVectorStore::build_filter(&bad).is_err());
    }

    #[test]
    fn search_body_carries_optional_recall_params() {
        let plain = QdrantVectorStore::search_body(&VectorQuery::new(vec![1.0], 5)).expect("body");
        assert!(plain.get("params").is_none());
        let tuned = QdrantVectorStore::search_body(
            &VectorQuery::new(vec![1.0], 5)
                .with_exact(true)
                .with_hnsw_ef(64),
        )
        .expect("body");
        let params = tuned.get("params").expect("params");
        assert_eq!(
            params.get("exact").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            params.get("hnsw_ef").and_then(|value| value.as_u64()),
            Some(64)
        );
        let bad = QdrantVectorStore::search_body(&VectorQuery::new(vec![1.0], 5).with_hnsw_ef(0));
        assert!(matches!(bad, Err(VectorError::InvalidInput(_))));
    }

    #[test]
    fn scroll_page_parses_points_and_cursor() {
        let text = serde_json::json!({
            "result": {
                "points": [
                    {
                        "id": "11111111-1111-1111-1111-111111111111",
                        "vector": [1.0, 0.0],
                        "payload": {"pid": "p0", "lang": "en"},
                    },
                    {
                        "id": "22222222-2222-2222-2222-222222222222",
                        "payload": {"other": 1},
                    }
                ],
                "next_page_offset": "22222222-2222-2222-2222-222222222222",
            }
        })
        .to_string();
        let (points, next) = QdrantVectorStore::parse_scroll_page(&text).expect("parsed");
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].id, "p0");
        assert_eq!(points[0].vector, vec![1.0, 0.0]);
        assert_eq!(
            next.as_deref(),
            Some("22222222-2222-2222-2222-222222222222")
        );
        let done =
            serde_json::json!({"result": {"points": [], "next_page_offset": serde_json::Value::Null}})
                .to_string();
        let (points, next) = QdrantVectorStore::parse_scroll_page(&done).expect("parsed");
        assert!(points.is_empty());
        assert!(next.is_none());
        assert!(QdrantVectorStore::parse_scroll_page("{}").is_err());
    }

    #[test]
    fn scroll_offset_passes_server_cursors_through() {
        assert_eq!(
            QdrantVectorStore::scroll_offset("11111111-1111-1111-1111-111111111111"),
            "11111111-1111-1111-1111-111111111111"
        );
        assert_eq!(
            QdrantVectorStore::scroll_offset("doc.md#0"),
            QdrantVectorStore::native_id("doc.md#0")
        );
    }

    #[test]
    fn text_match_covers_numeric_and_bool_payloads() {
        let numeric = QdrantVectorStore::text_match_condition("page", "3");
        assert!(numeric.get("should").is_some());
        let plain = QdrantVectorStore::text_match_condition("lang", "en");
        assert!(plain.get("match").is_some());
        let flag = QdrantVectorStore::text_match_condition("archived", "true");
        assert!(flag.get("should").is_some());
    }

    #[test]
    fn vector_parser_covers_named_and_dense_forms() {
        let dense = serde_json::json!([1.0, 0.0]);
        assert_eq!(
            QdrantVectorStore::parse_vector(&dense).expect("dense"),
            vec![1.0, 0.0]
        );
        let named = serde_json::json!({"default": [0.0, 1.0]});
        assert_eq!(
            QdrantVectorStore::parse_vector(&named).expect("named"),
            vec![0.0, 1.0]
        );
        let other = serde_json::json!({"image": [1.0, 2.0]});
        assert_eq!(
            QdrantVectorStore::parse_vector(&other).expect("first"),
            vec![1.0, 2.0]
        );
        assert!(QdrantVectorStore::parse_vector(&serde_json::json!({})).is_err());
    }

    #[test]
    fn native_ids_are_deterministic() {
        assert_eq!(
            QdrantVectorStore::native_id("doc.md#0"),
            QdrantVectorStore::native_id("doc.md#0")
        );
        assert_ne!(
            QdrantVectorStore::native_id("doc.md#0"),
            QdrantVectorStore::native_id("doc.md#1")
        );
    }

    #[test]
    fn version_compatibility_follows_official_rule() {
        assert!(QdrantVectorStore::is_compatible_version("1.2.0", "1.2.0"));
        assert!(QdrantVectorStore::is_compatible_version("1.1.0", "1.2.9"));
        assert!(!QdrantVectorStore::is_compatible_version("1.2.0", "1.4.0"));
        assert!(!QdrantVectorStore::is_compatible_version("1.9.0", "2.8.0"));
        assert!(!QdrantVectorStore::is_compatible_version("bad", "1.2.0"));
    }

    #[test]
    fn collection_vectors_parse_single_and_named() {
        let single = serde_json::json!({"size": 3, "distance": "Cosine"});
        let (dim, metric) = QdrantVectorStore::parse_collection_vectors(&single).expect("single");
        assert_eq!(dim, 3);
        assert_eq!(metric, DistanceMetric::Cosine);
        let named = serde_json::json!({"default": {"size": 4, "distance": "Dot"}});
        let (dim, metric) = QdrantVectorStore::parse_collection_vectors(&named).expect("named");
        assert_eq!(dim, 4);
        assert_eq!(metric, DistanceMetric::Dot);
    }

    #[test]
    fn euclid_scores_normalize_to_negated_distance() {
        assert_eq!(
            QdrantVectorStore::normalize_score(-2.0, Some(DistanceMetric::Euclid)),
            -2.0
        );
        let converted = QdrantVectorStore::normalize_score(0.5, Some(DistanceMetric::Euclid));
        assert!((converted + 1.0).abs() < 1e-5);
        assert_eq!(
            QdrantVectorStore::normalize_score(0.8, Some(DistanceMetric::Cosine)),
            0.8
        );
    }

    #[test]
    fn config_rejects_grpc_port_with_readable_error() {
        let result = QdrantVectorStore::new(QdrantConfig::new("http://127.0.0.1:6334"));
        assert!(matches!(result, Err(VectorError::InvalidInput(_))));
        let result = QdrantVectorStore::new(QdrantConfig::new(""));
        assert!(matches!(result, Err(VectorError::InvalidInput(_))));
    }

    /// Live-server contract round-trip. Runs only when `QDRANT_URL` is set,
    /// so default test runs stay dependency-free (pure-function unit tests
    /// above). Exercises the same behavior group as the other backends:
    /// ensure, upsert, search, fetch, count, filtered deletes.
    #[tokio::test]
    async fn remote_contract_roundtrip_when_server_configured() {
        let Some(base_url) = std::env::var("QDRANT_URL")
            .ok()
            .filter(|url| !url.trim().is_empty())
        else {
            return;
        };
        let store = QdrantVectorStore::new(QdrantConfig::new(base_url)).expect("client builds");
        let collection = format!("rag_contract_{}", std::process::id());
        store
            .ensure_collection(&CollectionSpec::new(&collection, 3))
            .await
            .expect("ensure");
        let point = VectorPoint {
            id: "doc.md#0".to_string(),
            vector: vec![1.0, 0.0, 0.0],
            payload: HashMap::from([
                (NAMESPACE_KEY.to_string(), serde_json::json!("kb")),
                (SOURCE_KEY.to_string(), serde_json::json!("doc.md")),
            ]),
        };
        store.upsert(&collection, &[point]).await.expect("upsert");
        let query = VectorQuery::new(vec![1.0, 0.0, 0.0], 5)
            .with_filter(SearchFilter::new().with_namespace("kb"));
        let hits = store.search(&collection, &query).await.expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "doc.md#0");
        let fetched = store
            .fetch(&collection, &["doc.md#0".to_string()])
            .await
            .expect("fetch");
        assert_eq!(fetched.len(), 1);
        let counted = store
            .count(&collection, Some(&SearchFilter::new().with_namespace("kb")))
            .await
            .expect("count");
        assert!(counted >= 1);
        let removed = store
            .delete_by_source(&collection, Some("kb"), "doc.md")
            .await
            .expect("delete by source");
        assert!(removed >= 1);
        store
            .upsert(
                &collection,
                &[VectorPoint {
                    id: "doc.md#9".to_string(),
                    vector: vec![0.0, 1.0, 0.0],
                    payload: HashMap::from([
                        (NAMESPACE_KEY.to_string(), serde_json::json!("kb")),
                        (SOURCE_KEY.to_string(), serde_json::json!("doc.md")),
                    ]),
                }],
            )
            .await
            .expect("upsert");
        store
            .overwrite_payload(
                &collection,
                "doc.md#9",
                HashMap::from([("lang".to_string(), serde_json::json!("en"))]),
            )
            .await
            .expect("overwrite");
        store
            .merge_payload(
                &collection,
                "doc.md#9",
                HashMap::from([("page".to_string(), serde_json::json!(3))]),
            )
            .await
            .expect("merge");
        let fetched = store
            .fetch(&collection, &["doc.md#9".to_string()])
            .await
            .expect("fetch");
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].vector, vec![0.0, 1.0, 0.0]);
        assert_eq!(
            fetched[0].payload.get("lang"),
            Some(&serde_json::json!("en"))
        );
        store
            .delete_payload_keys(&collection, "doc.md#9", &["lang".to_string()])
            .await
            .expect("delete keys");
        let missing = store
            .overwrite_payload(&collection, "nope", HashMap::new())
            .await;
        assert!(matches!(missing, Err(VectorError::InvalidInput(_))));
        let page = store
            .scroll(&collection, &ScrollQuery::new(10))
            .await
            .expect("scroll");
        assert!(!page.points.is_empty());
        assert!(page.points.iter().any(|point| point.id == "doc.md#9"));
        assert!(page.points.iter().all(|point| !point.vector.is_empty()));
        let tuned = VectorQuery::new(vec![0.0, 1.0, 0.0], 5)
            .with_exact(true)
            .with_hnsw_ef(32);
        let hits = store.search(&collection, &tuned).await.expect("search");
        assert!(!hits.is_empty());
        let removed = store
            .delete_by_id(&collection, &["doc.md#9".to_string()])
            .await
            .expect("delete");
        assert_eq!(removed, 1);
        store
            .delete_collection(&collection)
            .await
            .expect("cleanup collection");
    }

    #[test]
    fn invalid_inputs_fail_before_any_request() {
        let store =
            QdrantVectorStore::new(QdrantConfig::new("http://127.0.0.1:9")).expect("client builds");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let result = store.ensure_collection(&CollectionSpec::new("", 8)).await;
            assert!(matches!(result, Err(VectorError::InvalidInput(_))));
        });
    }
}
