//! Remote Elasticsearch backend over the REST API.
//!
//! Deliberately thin: only the calls keyword retrieval needs (create,
//! bulk index, match search, delete by query, count) plus a minimal
//! operations set (health, version). No official client dependency; the
//! crate stays on the shared `reqwest` stack, mirroring the Qdrant backend
//! in `vector-contracts`.
//!
//! One index carries every collection; the document id is the deterministic
//! `collection#chunk_id` string so upserts overwrite in one request with no
//! delete-before-insert window. Writes request refresh so a write followed
//! by a read observes the write. Only idempotent reads retry on rate
//! limiting and transient server errors; writes never retry implicitly.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use vector_contracts::{SearchFilter, VectorPoint};

use crate::error::{RagError, Result};
use crate::keyword_index::KeywordIndex;
use crate::retriever::{chunk_from_hit, RetrievedChunk};
use crate::ElasticKeywordConfig;

/// Upper bound for automatic read retries on rate limiting and transient faults.
const MAX_READ_RETRIES: usize = 3;

/// Remote Elasticsearch keyword backend.
pub struct ElasticKeywordIndex {
    client: reqwest::Client,
    config: ElasticKeywordConfig,
}

impl ElasticKeywordIndex {
    /// Builds the backend; no request is issued until first use.
    pub fn new(config: &ElasticKeywordConfig) -> Result<Self> {
        let base_url = config.base_url.trim();
        if base_url.is_empty() {
            return Err(RagError::InvalidInput(
                "elastic base_url must not be empty".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs.max(1)))
            .build()
            .map_err(|err| RagError::Keyword(err.to_string()))?;
        Ok(Self {
            client,
            config: config.clone(),
        })
    }

    /// Effective config (used by diagnostics and tests).
    pub fn config(&self) -> &ElasticKeywordConfig {
        &self.config
    }

    /// Deterministic document id for a contracts id pair.
    pub fn doc_id(collection: &str, chunk_id: &str) -> String {
        format!("{collection}#{chunk_id}")
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.base_url.trim_end_matches('/'))
    }

    fn batch_size(&self) -> usize {
        self.config.batch_size.max(1)
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let mut request = request;
        if let Some(api_key) = self.config.api_key.as_deref().filter(|key| !key.is_empty()) {
            request = request.header("Authorization", format!("ApiKey {api_key}"));
        }
        request
            .send()
            .await
            .map_err(|err| RagError::Keyword(err.to_string()))
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
                Err(RagError::Keyword(_)) if attempt < MAX_READ_RETRIES => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                    continue;
                }
                Err(other) => return Err(other),
            }
        }
    }

    fn index_name(&self) -> &str {
        self.config.index.trim()
    }

    /// Creates the index with the keyword mapping when missing.
    /// Existing indexes pass through; mapping conflicts fail explicitly.
    async fn ensure_index(&self) -> Result<()> {
        let index = self.index_name().to_string();
        let probe = self
            .send_read(|| self.client.get(self.url(&format!("/{index}"))))
            .await?;
        if probe.status().is_success() {
            return Ok(());
        }
        if probe.status() != reqwest::StatusCode::NOT_FOUND {
            let status = probe.status();
            let body = probe.text().await.unwrap_or_default();
            return Err(RagError::Keyword(format!(
                "elastic index probe failed (status {status}): {body}"
            )));
        }
        let response = self
            .send(
                self.client
                    .put(self.url(&format!("/{index}")))
                    .json(&Self::index_mapping()),
            )
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_success() {
            return Ok(());
        }
        if status == reqwest::StatusCode::BAD_REQUEST
            && body.contains("resource_already_exists_exception")
        {
            return Ok(());
        }
        Err(RagError::Keyword(format!(
            "elastic index create failed (status {status}): {body}"
        )))
    }

    fn index_mapping() -> serde_json::Value {
        json!({
            "settings": {"number_of_shards": 1, "number_of_replicas": 0},
            "mappings": {"properties": {
                "collection": {"type": "keyword"},
                "chunk_id": {"type": "keyword"},
                "namespace": {"type": "keyword"},
                "source": {"type": "keyword"},
                "content": {"type": "text"},
                "payload": {"type": "text", "index": false},
            }},
        })
    }

    /// Native scope filter: collection always, namespace and source when set.
    /// Generic field conditions stay out; callers recheck them in Rust.
    pub fn scope_clauses(
        collection: &str,
        filter: Option<&SearchFilter>,
    ) -> Vec<serde_json::Value> {
        let mut clauses = vec![json!({"term": {"collection": collection}})];
        if let Some(filter) = filter {
            if let Some(namespace) = &filter.namespace {
                clauses.push(json!({"term": {"namespace": namespace}}));
            }
            if let Some(source) = &filter.source {
                clauses.push(json!({"term": {"source": source}}));
            }
        }
        clauses
    }

    fn search_body(
        collection: &str,
        query: &str,
        filter: Option<&SearchFilter>,
        size: usize,
    ) -> serde_json::Value {
        json!({
            "query": {"bool": {
                "must": [{"match": {"content": {"query": query, "operator": "or"}}}],
                "filter": Self::scope_clauses(collection, filter),
            }},
            "size": size,
            "_source": ["collection", "chunk_id", "namespace", "source", "content", "payload"],
        })
    }

    fn count_body(collection: &str, filter: Option<&SearchFilter>) -> serde_json::Value {
        json!({"query": {"bool": {"filter": Self::scope_clauses(collection, filter)}}})
    }

    fn payload_string(payload: &HashMap<String, serde_json::Value>, key: &str) -> String {
        payload
            .get(key)
            .and_then(|value| match value {
                serde_json::Value::String(text) => Some(text.clone()),
                other => vector_contracts::payload_text(other),
            })
            .unwrap_or_default()
    }

    fn doc_body(collection: &str, point: &VectorPoint) -> Result<(String, serde_json::Value)> {
        if point.id.is_empty() {
            return Err(RagError::InvalidInput(
                "point id must not be empty".to_string(),
            ));
        }
        let content = Self::payload_string(&point.payload, crate::document::CONTENT_KEY);
        if content.trim().is_empty() {
            return Err(RagError::InvalidInput(format!(
                "point '{}' has no indexable content",
                point.id
            )));
        }
        let namespace =
            Self::payload_string(&point.payload, vector_contracts::types::NAMESPACE_KEY);
        let source = Self::payload_string(&point.payload, vector_contracts::types::SOURCE_KEY);
        let payload = serde_json::to_string(&point.payload)
            .map_err(|err| RagError::Keyword(err.to_string()))?;
        let id = Self::doc_id(collection, &point.id);
        let body = json!({
            "collection": collection,
            "chunk_id": point.id,
            "namespace": namespace,
            "source": source,
            "content": content,
            "payload": payload,
        });
        Ok((id, body))
    }

    /// Checks server reachability (proprietary extension, not part of the contract).
    pub async fn health_check(&self) -> Result<()> {
        let response = self.send_read(|| self.client.get(self.url("/"))).await?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(RagError::Keyword(format!(
            "elastic health check failed (status {status}): {body}"
        )))
    }

    /// Returns the server version string (proprietary extension).
    pub async fn version(&self) -> Result<String> {
        let response = self.send_read(|| self.client.get(self.url("/"))).await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(RagError::Keyword(format!(
                "elastic version probe failed (status {status}): {body}"
            )));
        }
        let body = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|err| RagError::Keyword(err.to_string()))?;
        parsed
            .pointer("/version/number")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .ok_or_else(|| RagError::Keyword("elastic version response has no number".to_string()))
    }
}

#[async_trait]
impl KeywordIndex for ElasticKeywordIndex {
    fn backend_name(&self) -> &'static str {
        "elastic"
    }

    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()> {
        if collection.is_empty() {
            return Err(RagError::InvalidInput(
                "collection must not be empty".to_string(),
            ));
        }
        if points.is_empty() {
            return Ok(());
        }
        self.ensure_index().await?;
        for batch in points.chunks(self.batch_size()) {
            let mut body = String::new();
            let mut actionable = 0;
            for point in batch {
                let content = Self::payload_string(&point.payload, crate::document::CONTENT_KEY);
                if content.trim().is_empty() {
                    continue;
                }
                let (id, doc) = Self::doc_body(collection, point)?;
                body.push_str(
                    &serde_json::to_string(&json!({"index": {"_id": id}}))
                        .map_err(|err| RagError::Keyword(err.to_string()))?,
                );
                body.push('\n');
                body.push_str(
                    &serde_json::to_string(&doc)
                        .map_err(|err| RagError::Keyword(err.to_string()))?,
                );
                body.push('\n');
                actionable += 1;
            }
            if actionable == 0 {
                continue;
            }
            let response = self
                .send(
                    self.client
                        .post(self.url("/_bulk?refresh=wait_for"))
                        .header("Content-Type", "application/x-ndjson")
                        .body(body),
                )
                .await?;
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(RagError::Keyword(format!(
                    "elastic bulk index failed (status {status}): {text}"
                )));
            }
            let parsed: serde_json::Value =
                serde_json::from_str(&text).map_err(|err| RagError::Keyword(err.to_string()))?;
            if parsed.get("errors").and_then(|value| value.as_bool()) == Some(true) {
                return Err(RagError::Keyword(format!(
                    "elastic bulk index reported item errors: {text}"
                )));
            }
        }
        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        query: &str,
        filter: Option<&SearchFilter>,
        limit: usize,
    ) -> Result<Vec<RetrievedChunk>> {
        if limit == 0 || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let fetch = limit.saturating_mul(3).max(limit).max(10);
        let index = self.index_name().to_string();
        let response = self
            .send_read(|| {
                self.client
                    .post(self.url(&format!("/{index}/_search")))
                    .json(&Self::search_body(collection, query, filter, fetch))
            })
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(RagError::Keyword(format!(
                "elastic search failed (status {status}): {body}"
            )));
        }
        let text = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| RagError::Keyword(err.to_string()))?;
        let hits = parsed
            .pointer("/hits/hits")
            .and_then(|value| value.as_array())
            .ok_or_else(|| RagError::Keyword("elastic search response has no hits".to_string()))?;
        let mut out = Vec::new();
        for hit in hits {
            let score = hit
                .get("_score")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0) as f32;
            let source = hit
                .get("_source")
                .ok_or_else(|| RagError::Keyword("elastic hit has no _source".to_string()))?;
            let payload_raw = source
                .get("payload")
                .and_then(|value| value.as_str())
                .ok_or_else(|| RagError::Keyword("elastic hit has no payload".to_string()))?;
            let payload_map: HashMap<String, serde_json::Value> = serde_json::from_str(payload_raw)
                .map_err(|err| RagError::Keyword(err.to_string()))?;
            if let Some(wanted) = filter {
                if !wanted.matches(&payload_map) {
                    continue;
                }
            }
            let chunk_id = source
                .get("chunk_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let scored = vector_contracts::ScoredPoint {
                id: chunk_id,
                score,
                payload: payload_map,
            };
            if let Ok(chunk) = chunk_from_hit(&scored) {
                out.push(chunk);
            }
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut body = String::new();
        for id in ids {
            let doc_id = Self::doc_id(collection, id);
            body.push_str(
                &serde_json::to_string(&json!({"delete": {"_id": doc_id}}))
                    .map_err(|err| RagError::Keyword(err.to_string()))?,
            );
            body.push('\n');
        }
        let response = self
            .send(
                self.client
                    .post(self.url("/_bulk?refresh=wait_for"))
                    .header("Content-Type", "application/x-ndjson")
                    .body(body),
            )
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(0);
        }
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(RagError::Keyword(format!(
                "elastic bulk delete failed (status {status}): {text}"
            )));
        }
        let text = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| RagError::Keyword(err.to_string()))?;
        let items = parsed
            .get("items")
            .and_then(|value| value.as_array())
            .ok_or_else(|| RagError::Keyword("elastic bulk response has no items".to_string()))?;
        let mut removed = 0_u64;
        for item in items {
            let status = item
                .get("delete")
                .and_then(|op| op.get("status"))
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            if status == 200 || status == 201 {
                removed += 1;
            }
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
        let index = self.index_name().to_string();
        let response = self
            .send(
                self.client
                    .post(self.url(&format!("/{index}/_delete_by_query?refresh=wait_for")))
                    .json(&Self::count_body(collection, Some(&filter))),
            )
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(0);
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(RagError::Keyword(format!(
                "elastic delete by query failed (status {status}): {body}"
            )));
        }
        let text = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| RagError::Keyword(err.to_string()))?;
        Ok(parsed
            .get("deleted")
            .and_then(|value| value.as_u64())
            .unwrap_or(0))
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64> {
        let index = self.index_name().to_string();
        let response = self
            .send_read(|| {
                self.client
                    .post(self.url(&format!("/{index}/_count")))
                    .json(&Self::count_body(collection, filter))
            })
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(0);
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(RagError::Keyword(format!(
                "elastic count failed (status {status}): {body}"
            )));
        }
        let text = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| RagError::Keyword(err.to_string()))?;
        parsed
            .get("count")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| RagError::Keyword("elastic count response has no count".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ElasticKeywordConfig {
        ElasticKeywordConfig {
            base_url: "http://127.0.0.1:19200".to_string(),
            api_key: None,
            index: "rag-keyword-test".to_string(),
            timeout_secs: 2,
            batch_size: 64,
        }
    }

    #[test]
    fn empty_base_url_fails_fast() {
        let config = ElasticKeywordConfig {
            base_url: "  ".to_string(),
            ..test_config()
        };
        assert!(ElasticKeywordIndex::new(&config).is_err());
    }

    #[test]
    fn scope_clauses_cover_collection_namespace_source() {
        let filter = SearchFilter::new().with_namespace("kb").with_source("a.md");
        let clauses = ElasticKeywordIndex::scope_clauses("kb", Some(&filter));
        assert_eq!(clauses.len(), 3);
        assert!(ElasticKeywordIndex::scope_clauses("kb", None).len() == 1);
    }

    #[test]
    fn doc_id_is_deterministic() {
        assert_eq!(ElasticKeywordIndex::doc_id("kb", "a.md#0"), "kb#a.md#0");
    }

    #[tokio::test]
    async fn live_roundtrip_when_server_configured() {
        let Some(url) = std::env::var("WF_ES_TEST_URL")
            .ok()
            .filter(|v| !v.is_empty())
        else {
            return;
        };
        let config = ElasticKeywordConfig {
            base_url: url,
            ..test_config()
        };
        let index = ElasticKeywordIndex::new(&config).expect("client");
        index.health_check().await.expect("health");
        let points = vec![
            VectorPoint::new("live#0", vec![0.0; 2]).with_payload(HashMap::from([
                (
                    "content".to_string(),
                    serde_json::json!("rust ownership borrowing"),
                ),
                ("chunk_id".to_string(), serde_json::json!("live#0")),
                ("namespace".to_string(), serde_json::json!("kb")),
                ("source".to_string(), serde_json::json!("live.md")),
            ])),
        ];
        index.upsert("kb", &points).await.expect("upsert");
        let filter = SearchFilter::new().with_namespace("kb");
        let hits = index
            .search("kb", "rust ownership", Some(&filter), 5)
            .await
            .expect("search");
        assert!(!hits.is_empty());
        assert_eq!(index.count("kb", Some(&filter)).await.expect("count"), 1);
        assert_eq!(
            index
                .delete_by_source("kb", Some("kb"), "live.md")
                .await
                .expect("delete"),
            1
        );
    }
}
