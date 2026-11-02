//! Remote Qdrant backend over the REST API.
//!
//! Deliberately thin: only the calls RAG needs (create, upsert, query,
//! filter delete, count). The official gRPC client is intentionally not
//! used so the crate stays on the shared `reqwest` stack; `ref/rust-client`
//! remains a protocol reference, not a dependency.
//!
//! Point ids are arbitrary strings. Because Qdrant native ids only hold
//! integers or UUIDs, the contracts-level id travels in the `pid` payload
//! field and each upsert mints fresh native UUIDs after deleting previous
//! rows with the same `pid`. Upserts are therefore idempotent at the
//! contracts level at the cost of one extra filter delete per batch.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;

use crate::error::{Result, VectorError};
use crate::filter::SearchFilter;
use crate::store::{CollectionSpec, VectorQuery, VectorStorage};
use crate::types::{DistanceMetric, ScoredPoint, VectorPoint, NAMESPACE_KEY, PID_KEY, SOURCE_KEY};

/// Connection parameters of the remote Qdrant backend.
#[derive(Debug, Clone)]
pub struct QdrantConfig {
    /// Base URL of the REST API, for example `http://127.0.0.1:6333`.
    pub base_url: String,
    /// Optional API key sent as the `api-key` header.
    pub api_key: Option<String>,
    /// Per-request deadline in seconds.
    pub timeout_secs: u64,
}

impl QdrantConfig {
    /// Builds a config with a ten-second per-request deadline.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: None,
            timeout_secs: 10,
        }
    }

    /// Attaches an API key.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }
}

/// Remote Qdrant backend.
#[derive(Debug, Clone)]
pub struct QdrantVectorStore {
    client: reqwest::Client,
    config: QdrantConfig,
}

impl QdrantVectorStore {
    /// Builds the backend; no request is issued until first use.
    pub fn new(config: QdrantConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs.max(1)))
            .build()
            .map_err(|err| VectorError::Transport(err.to_string()))?;
        Ok(Self { client, config })
    }

    fn distance_name(distance: DistanceMetric) -> &'static str {
        match distance {
            DistanceMetric::Cosine => "Cosine",
            DistanceMetric::Dot => "Dot",
            DistanceMetric::Euclid => "Euclid",
        }
    }

    /// Builds the Qdrant filter object for a contracts filter.
    pub fn build_filter(filter: &SearchFilter) -> Option<serde_json::Value> {
        let mut must = Vec::new();
        if let Some(namespace) = &filter.namespace {
            must.push(json!({"key": NAMESPACE_KEY, "match": {"value": namespace}}));
        }
        if let Some(source) = &filter.source {
            must.push(json!({"key": SOURCE_KEY, "match": {"value": source}}));
        }
        for (key, value) in &filter.fields {
            must.push(json!({"key": key, "match": {"value": value}}));
        }
        if must.is_empty() {
            None
        } else {
            Some(json!({"must": must}))
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url.trim_end_matches('/'), path)
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let mut request = request;
        if let Some(api_key) = &self.config.api_key {
            request = request.header("api-key", api_key.clone());
        }
        request
            .send()
            .await
            .map_err(|err| VectorError::Transport(err.to_string()))
    }

    fn check_status(status: reqwest::StatusCode, body: &str, collection: &str) -> Result<()> {
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
        Err(VectorError::Backend(format!(
            "qdrant status {status}: {body}"
        )))
    }
}

#[async_trait]
impl VectorStorage for QdrantVectorStore {
    fn backend_name(&self) -> &'static str {
        "qdrant"
    }

    async fn ensure_collection(&self, spec: &CollectionSpec) -> Result<bool> {
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
            .send(
                self.client
                    .get(self.url(&format!("/collections/{}", spec.name))),
            )
            .await?;
        if response.status().is_success() {
            return Ok(false);
        }
        if response.status() != reqwest::StatusCode::NOT_FOUND {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(VectorError::Backend(format!(
                "qdrant describe failed with status {status}: {body}"
            )));
        }
        let response = self
            .send(self.client.put(self.url(&format!("/collections/{}", spec.name))).json(
                &json!({"vectors": {"size": spec.dimension, "distance": Self::distance_name(spec.distance)}}),
            ))
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Self::check_status(status, &body, &spec.name)?;
        Ok(true)
    }

    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()> {
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
        for chunk in points.chunks(256) {
            let ids: Vec<String> = chunk.iter().map(|point| point.id.clone()).collect();
            self.delete_by_id(collection, &ids).await?;
            let payload: Vec<serde_json::Value> = chunk
                .iter()
                .map(|point| {
                    let mut fields: HashMap<String, serde_json::Value> = point.payload.clone();
                    fields.insert(PID_KEY.to_string(), json!(point.id));
                    json!({
                        "id": uuid::Uuid::now_v7().to_string(),
                        "vector": point.vector,
                        "payload": fields,
                    })
                })
                .collect();
            let response = self
                .send(
                    self.client
                        .put(self.url(&format!("/collections/{collection}/points")))
                        .json(&json!({"points": payload})),
                )
                .await?;
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Self::check_status(status, &body, collection)?;
        }
        Ok(())
    }

    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredPoint>> {
        if query.vector.iter().any(|v| !v.is_finite()) {
            return Err(VectorError::InvalidInput(
                "query vector contains non-finite elements".to_string(),
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
            if let Some(mapped) = Self::build_filter(filter) {
                body["filter"] = mapped;
            }
        }
        let response = self
            .send(
                self.client
                    .post(self.url(&format!("/collections/{collection}/points/query")))
                    .json(&body),
            )
            .await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        Self::check_status(status, &text, collection)?;
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| VectorError::Decode(err.to_string()))?;
        let points = parsed
            .pointer("/result/points")
            .and_then(|value| value.as_array())
            .ok_or_else(|| {
                VectorError::Decode("qdrant query response has no result.points".to_string())
            })?;
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
            hits.push(ScoredPoint {
                id,
                score: point
                    .get("score")
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.0) as f32,
                payload,
            });
        }
        Ok(hits)
    }

    async fn fetch(&self, collection: &str, ids: &[String]) -> Result<Vec<VectorPoint>> {
        let mut found = Vec::with_capacity(ids.len());
        for id in ids {
            // Scroll (not query) avoids sending a dummy vector whose
            // dimension would fail server-side validation.
            let body = json!({
                "filter": {"must": [{"key": PID_KEY, "match": {"value": id}}]},
                "limit": 1,
                "with_payload": true,
                "with_vector": true,
            });
            let response = self
                .send(
                    self.client
                        .post(self.url(&format!("/collections/{collection}/points/scroll")))
                        .json(&body),
                )
                .await?;
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Self::check_status(status, &text, collection)?;
            let parsed: serde_json::Value =
                serde_json::from_str(&text).map_err(|err| VectorError::Decode(err.to_string()))?;
            let point = parsed
                .pointer("/result/points")
                .and_then(|value| value.as_array())
                .and_then(|points| points.first())
                .or_else(|| {
                    parsed
                        .pointer("/result")
                        .and_then(|value| value.as_array())
                        .and_then(|points| points.first())
                });
            if let Some(point) = point {
                let vector = point
                    .get("vector")
                    .and_then(|value| value.as_array())
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_f64().map(|v| v as f32))
                            .collect::<Vec<f32>>()
                    })
                    .unwrap_or_default();
                let payload = point
                    .get("payload")
                    .and_then(|value| value.as_object())
                    .map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();
                found.push(VectorPoint {
                    id: id.clone(),
                    vector,
                    payload,
                });
            }
        }
        Ok(found)
    }

    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut removed = 0;
        for chunk in ids.chunks(256) {
            let conditions: Vec<serde_json::Value> = chunk
                .iter()
                .map(|id| json!({"key": PID_KEY, "match": {"value": id}}))
                .collect();
            let response = self
                .send(
                    self.client
                        .post(self.url(&format!("/collections/{collection}/points/delete")))
                        .json(&json!({"filter": {"should": conditions}})),
                )
                .await?;
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Self::check_status(status, &body, collection)?;
            removed += chunk.len() as u64;
        }
        Ok(removed)
    }

    async fn delete_by_source(
        &self,
        collection: &str,
        namespace: Option<&str>,
        source: &str,
    ) -> Result<u64> {
        let mut filter = SearchFilter::new().with_source(source);
        if let Some(namespace) = namespace {
            filter = filter.with_namespace(namespace);
        }
        let existing = self.count(collection, Some(&filter)).await?;
        let mapped = Self::build_filter(&filter)
            .ok_or_else(|| VectorError::InvalidInput("source must not be empty".to_string()))?;
        let response = self
            .send(
                self.client
                    .post(self.url(&format!("/collections/{collection}/points/delete")))
                    .json(&json!({"filter": mapped})),
            )
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Self::check_status(status, &body, collection)?;
        Ok(existing)
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64> {
        let mut body = json!({"exact": true});
        if let Some(filter) = filter {
            if let Some(mapped) = Self::build_filter(filter) {
                body["filter"] = mapped;
            }
        }
        let response = self
            .send(
                self.client
                    .post(self.url(&format!("/collections/{collection}/points/count")))
                    .json(&body),
            )
            .await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        Self::check_status(status, &text, collection)?;
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| VectorError::Decode(err.to_string()))?;
        parsed
            .pointer("/result/count")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| {
                VectorError::Decode("qdrant count response has no result.count".to_string())
            })
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
        let built = QdrantVectorStore::build_filter(&filter).expect("filter");
        let must = built
            .get("must")
            .and_then(|value| value.as_array())
            .expect("must");
        assert_eq!(must.len(), 3);
        assert!(QdrantVectorStore::build_filter(&SearchFilter::new()).is_none());
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
