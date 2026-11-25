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

use crate::ElasticKeywordConfig;
use async_trait::async_trait;
use keyword_contracts::{
    KeywordDocument, KeywordError, KeywordFormat, KeywordIndex, KeywordQuery,
    Result as KeywordResult,
};
use serde_json::json;
use vector_contracts::{diagnostic_body, ScoredPoint, SearchFilter};

/// Upper bound for automatic read retries on rate limiting and transient faults.
const MAX_READ_RETRIES: usize = 3;

/// Remote Elasticsearch keyword backend.
pub struct ElasticKeywordIndex {
    client: reqwest::Client,
    config: ElasticKeywordConfig,
}

impl ElasticKeywordIndex {
    /// Builds the backend; no request is issued until first use.
    pub fn new(config: &ElasticKeywordConfig) -> KeywordResult<Self> {
        let base_url = config.base_url.trim();
        if base_url.is_empty() {
            return Err(KeywordError::invalid_input(
                "elastic base_url must not be empty".to_string(),
            ));
        }
        if let Some(name) = config
            .analyzer_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            crate::tokenizer::ensure_analyzer_registered(name)
                .map_err(KeywordError::InvalidInput)?;
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs.max(1)))
            .build()
            .map_err(|err| KeywordError::backend(err.to_string()))?;
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

    async fn send(&self, request: reqwest::RequestBuilder) -> KeywordResult<reqwest::Response> {
        let mut request = request;
        if let Some(api_key) = self.config.api_key.as_deref().filter(|key| !key.is_empty()) {
            request = request.header("Authorization", format!("ApiKey {api_key}"));
        }
        request
            .send()
            .await
            .map_err(|err| KeywordError::backend(err.to_string()))
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
    ) -> KeywordResult<reqwest::Response> {
        let mut attempt = 0;
        loop {
            match self.send(build()).await {
                Ok(response) => {
                    if Self::is_retryable_status(response.status()) && attempt < MAX_READ_RETRIES {
                        attempt += 1;
                        tracing::warn!(
                            backend = "elastic",
                            attempt = attempt,
                            status = %response.status(),
                            "retrying transient remote failure"
                        );
                        tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                        continue;
                    }
                    return Ok(response);
                }
                Err(KeywordError::Backend(ref error)) if attempt < MAX_READ_RETRIES => {
                    attempt += 1;
                    tracing::warn!(
                        backend = "elastic",
                        attempt = attempt,
                        error = %error,
                        "retrying transient transport failure"
                    );
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

    fn status_error(context: &str, status: reqwest::StatusCode, body: &str) -> KeywordError {
        KeywordError::backend(format!(
            "elastic {context} failed (status {status}): {}",
            diagnostic_body(body)
        ))
    }

    fn auth_error(status: reqwest::StatusCode, body: &str) -> Option<KeywordError> {
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            Some(KeywordError::backend(format!(
                "elastic unauthorized (status {status}): {}",
                diagnostic_body(body)
            )))
        } else {
            None
        }
    }

    fn bad_request_error(status: reqwest::StatusCode, body: &str) -> Option<KeywordError> {
        if status == reqwest::StatusCode::BAD_REQUEST {
            Some(KeywordError::invalid_input(format!(
                "elastic invalid request (status {status}): {}",
                diagnostic_body(body)
            )))
        } else {
            None
        }
    }

    fn decode_error(err: impl std::fmt::Display) -> KeywordError {
        KeywordError::backend(format!("elastic decode error: {err}"))
    }

    /// Creates the index with the keyword mapping when missing.
    /// Existing indexes pass through after a mapping compatibility check;
    /// mapping conflicts fail explicitly instead of auto-migrating.
    async fn ensure_index(&self) -> KeywordResult<()> {
        let index = self.index_name().to_string();
        let probe = self
            .send_read(|| self.client.get(self.url(&format!("/{index}"))))
            .await?;
        if probe.status().is_success() {
            self.check_format(&index).await?;
            return self.check_mapping(&index).await;
        }
        if probe.status() != reqwest::StatusCode::NOT_FOUND {
            let status = probe.status();
            let body = probe.text().await.unwrap_or_default();
            if let Some(err) = Self::auth_error(status, &body) {
                return Err(err);
            }
            if let Some(err) = Self::bad_request_error(status, &body) {
                return Err(err);
            }
            return Err(Self::status_error("index probe", status, &body));
        }
        let mut mapping = Self::index_mapping_with(self.config.tokenizer);
        mapping["settings"]["index"]["keyword_format"] = json!(self.format_id());
        let response = self
            .send(
                self.client
                    .put(self.url(&format!("/{index}")))
                    .json(&mapping),
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
            return self.check_mapping(&index).await;
        }
        if let Some(err) = Self::auth_error(status, &body) {
            return Err(err);
        }
        if let Some(err) = Self::bad_request_error(status, &body) {
            return Err(err);
        }
        Err(Self::status_error("index create", status, &body))
    }

    fn format_id(&self) -> String {
        KeywordFormat::new(self.config.tokenizer, self.config.analyzer_name.clone()).id()
    }

    /// Reads the persisted format stamp from index settings and compares
    /// it with the resolved format. A missing stamp is treated as legacy
    /// adoption when the configuration resolves to the built-in English
    /// path; any other drift fails or degrades per the policy.
    async fn check_format(&self, index: &str) -> KeywordResult<()> {
        let response = self
            .send_read(|| self.client.get(self.url(&format!("/{index}/_settings"))))
            .await?;
        if !response.status().is_success() {
            return Ok(());
        }
        let text = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&text).map_err(Self::decode_error)?;
        let stamp = parsed
            .pointer(&format!("/{index}/settings/index.keyword_format"))
            .or_else(|| {
                parsed
                    .as_object()
                    .and_then(|obj| obj.values().next())
                    .and_then(|idx| idx.pointer("/settings/index.keyword_format"))
            })
            .and_then(|value| value.as_str().map(String::from));
        let current = self.format_id();
        let legacy_compatible = self.config.tokenizer
            == wf_types::config::keyword::KeywordTokenizer::English
            && self
                .config
                .analyzer_name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .is_none();
        let mismatch = match stamp.as_deref() {
            Some(existing) => existing != current,
            None => !legacy_compatible,
        };
        if !mismatch {
            return Ok(());
        }
        if self.config.format_policy == wf_types::config::keyword::KeywordFormatPolicy::ReadOnly {
            return Ok(());
        }
        Err(KeywordFormat::mismatch_error(
            stamp.as_deref().unwrap_or("legacy-unstamped"),
            &current,
        ))
    }

    fn index_mapping_with(
        tokenizer: wf_types::config::keyword::KeywordTokenizer,
    ) -> serde_json::Value {
        use wf_types::config::keyword::KeywordTokenizer as Selection;
        let mut properties = serde_json::Map::new();
        properties.insert("collection".to_string(), json!({"type": "keyword"}));
        properties.insert("chunk_id".to_string(), json!({"type": "keyword"}));
        properties.insert("namespace".to_string(), json!({"type": "keyword"}));
        properties.insert("source".to_string(), json!({"type": "keyword"}));
        properties.insert("content".to_string(), json!({"type": "text"}));
        if matches!(tokenizer, Selection::Mixed) {
            properties.insert(
                "content_tokens".to_string(),
                json!({"type": "text", "analyzer": "whitespace"}),
            );
        }
        properties.insert(
            "payload".to_string(),
            json!({"type": "text", "index": false}),
        );
        json!({
            "settings": {"number_of_shards": 1, "number_of_replicas": 0},
            "mappings": {"properties": properties},
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

    /// Verifies the live mapping carries every keyword field with a
    /// compatible type; conflicts fail explicitly, missing indexes are
    /// the caller's `ensure_index` path and never reach here.
    async fn check_mapping(&self, index: &str) -> KeywordResult<()> {
        let response = self
            .send_read(|| self.client.get(self.url(&format!("/{index}/_mapping"))))
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Some(err) = Self::auth_error(status, &body) {
                return Err(err);
            }
            return Err(Self::status_error("mapping probe", status, &body));
        }
        let text = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&text).map_err(Self::decode_error)?;
        let properties = parsed
            .pointer(&format!("/{index}/mappings/properties"))
            .or_else(|| parsed.pointer("/_doc/mappings/properties"))
            .or_else(|| {
                parsed.as_object().and_then(|obj| {
                    obj.values()
                        .next()
                        .and_then(|idx| idx.pointer("/mappings/properties"))
                })
            });
        let Some(properties) = properties else {
            return Ok(());
        };
        use wf_types::config::keyword::KeywordTokenizer as Selection;
        let mut expected_fields = vec![
            ("collection", "keyword"),
            ("chunk_id", "keyword"),
            ("namespace", "keyword"),
            ("source", "keyword"),
            ("content", "text"),
        ];
        if matches!(self.config.tokenizer, Selection::Mixed) {
            expected_fields.push(("content_tokens", "text"));
        }
        for (field, expected) in expected_fields {
            if let Some(actual) = properties
                .get(field)
                .and_then(|def| def.get("type"))
                .and_then(|kind| kind.as_str())
            {
                if actual != expected {
                    return Err(KeywordError::backend(format!(
                        "elastic mapping conflict on '{field}': expected {expected}, found {actual}"
                    )));
                }
            }
        }
        Ok(())
    }

    fn search_body_with(
        collection: &str,
        query: &str,
        filter: Option<&SearchFilter>,
        size: usize,
        tokenizer: wf_types::config::keyword::KeywordTokenizer,
        analyzer_name: Option<&str>,
    ) -> serde_json::Value {
        use wf_types::config::keyword::KeywordTokenizer as Selection;
        let must = match tokenizer {
            Selection::English => {
                vec![json!({"match": {"content": {"query": query, "operator": "or"}}})]
            }
            Selection::Mixed => {
                let tokens = crate::tokenizer::pretokenize(query, tokenizer, analyzer_name);
                if tokens.trim().is_empty() {
                    vec![json!({"match_none": {}})]
                } else {
                    vec![json!({
                        "bool": {
                            "should": [
                                {"match": {"content": {"query": query, "operator": "or"}}},
                                {"match": {"content_tokens": {"query": tokens, "operator": "or"}}},
                            ],
                            "minimum_should_match": 1,
                        }
                    })]
                }
            }
        };
        json!({
            "query": {"bool": {
                "must": must,
                "filter": Self::scope_clauses(collection, filter),
            }},
            "size": size,
            "track_total_hits": true,
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

    fn doc_body_with(
        collection: &str,
        document: &KeywordDocument,
        tokenizer: wf_types::config::keyword::KeywordTokenizer,
        analyzer_name: Option<&str>,
    ) -> KeywordResult<(String, serde_json::Value)> {
        if document.id.is_empty() {
            return Err(KeywordError::invalid_input("document id must not be empty"));
        }
        let namespace =
            Self::payload_string(&document.payload, vector_contracts::types::NAMESPACE_KEY);
        let source = Self::payload_string(&document.payload, vector_contracts::types::SOURCE_KEY);
        let payload = serde_json::to_string(&document.payload)
            .map_err(|err| KeywordError::backend(err.to_string()))?;
        let id = Self::doc_id(collection, &document.id);
        use wf_types::config::keyword::KeywordTokenizer as Selection;
        let body = match tokenizer {
            Selection::English => json!({
                "collection": collection,
                "chunk_id": document.id.clone(),
                "namespace": namespace,
                "source": source,
                "content": document.text.clone(),
                "payload": payload,
            }),
            Selection::Mixed => {
                let tokens =
                    crate::tokenizer::pretokenize(&document.text, tokenizer, analyzer_name);
                json!({
                    "collection": collection,
                    "chunk_id": document.id.clone(),
                    "namespace": namespace,
                    "source": source,
                    "content": document.text.clone(),
                    "content_tokens": tokens,
                    "payload": payload,
                })
            }
        };
        Ok((id, body))
    }

    /// Checks server reachability (proprietary extension, not part of the contract).
    pub async fn health_check(&self) -> KeywordResult<()> {
        let response = self.send_read(|| self.client.get(self.url("/"))).await?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if let Some(err) = Self::auth_error(status, &body) {
            return Err(err);
        }
        Err(Self::status_error("health check", status, &body))
    }

    /// Returns the server version string (proprietary extension).
    pub async fn version(&self) -> KeywordResult<String> {
        let response = self.send_read(|| self.client.get(self.url("/"))).await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Some(err) = Self::auth_error(status, &body) {
                return Err(err);
            }
            return Err(Self::status_error("version probe", status, &body));
        }
        let body = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&body).map_err(Self::decode_error)?;
        parsed
            .pointer("/version/number")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .ok_or_else(|| {
                KeywordError::backend("elastic version response has no number".to_string())
            })
    }

    /// Warns on major-version drift without blocking startup: the major
    /// must match the expected line, minors drift by at most one.
    pub fn version_drift_warning(version: &str, expected_major: u64) -> Option<String> {
        let mut parts = version.split('.');
        let major: u64 = parts.next()?.trim().parse().ok()?;
        if major != expected_major {
            return Some(format!(
                "elastic version drift: server {version} major differs from expected {expected_major}"
            ));
        }
        None
    }

    /// Fast count through the search total when exact counting is disabled.
    /// Large collections trade accuracy for speed; the relation flag tells
    /// whether the total is exact or a lower bound.
    async fn approximate_count(
        &self,
        index: &str,
        collection: &str,
        filter: Option<&SearchFilter>,
    ) -> KeywordResult<u64> {
        let response = self
            .send_read(|| {
                self.client
                    .post(self.url(&format!("/{index}/_search")))
                    .json(&json!({
                        "query": {"bool": {"filter": Self::scope_clauses(collection, filter)}},
                        "size": 0,
                        "track_total_hits": false,
                    }))
            })
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(0);
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Self::status_error("approximate count", status, &body));
        }
        let text = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&text).map_err(Self::decode_error)?;
        parsed
            .pointer("/hits/total/value")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| KeywordError::backend("elastic search total has no value".to_string()))
    }
}

#[async_trait]
impl KeywordIndex for ElasticKeywordIndex {
    fn backend_name(&self) -> &'static str {
        "elastic"
    }

    async fn upsert(&self, collection: &str, documents: &[KeywordDocument]) -> KeywordResult<()> {
        if collection.is_empty() {
            return Err(KeywordError::invalid_input("collection must not be empty"));
        }
        if documents.is_empty() {
            return Ok(());
        }
        self.ensure_index().await?;
        for batch in documents.chunks(self.batch_size()) {
            let mut body = String::new();
            let mut actionable = 0;
            for document in batch {
                let (id, doc) = Self::doc_body_with(
                    collection,
                    document,
                    self.config.tokenizer,
                    self.config.analyzer_name.as_deref(),
                )?;
                body.push_str(
                    &serde_json::to_string(&json!({"index": {"_id": id}}))
                        .map_err(|err| KeywordError::backend(err.to_string()))?,
                );
                body.push('\n');
                body.push_str(
                    &serde_json::to_string(&doc)
                        .map_err(|err| KeywordError::backend(err.to_string()))?,
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
                if let Some(err) = Self::auth_error(status, &text) {
                    return Err(err);
                }
                if let Some(err) = Self::bad_request_error(status, &text) {
                    return Err(err);
                }
                return Err(KeywordError::backend(format!(
                    "elastic bulk index failed (status {status}): {text}"
                )));
            }
            let parsed: serde_json::Value =
                serde_json::from_str(&text).map_err(Self::decode_error)?;
            if parsed.get("errors").and_then(|value| value.as_bool()) == Some(true) {
                return Err(KeywordError::backend(format!(
                    "elastic bulk index reported item errors: {}",
                    diagnostic_body(&text)
                )));
            }
        }
        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        query: &KeywordQuery,
    ) -> KeywordResult<Vec<ScoredPoint>> {
        if query.limit == 0 || query.text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let filter = query.filter.as_ref();
        let fetch = query.limit.saturating_mul(3).max(query.limit).max(10);
        let index = self.index_name().to_string();
        let response = self
            .send_read(|| {
                self.client
                    .post(self.url(&format!("/{index}/_search")))
                    .json(&Self::search_body_with(
                        collection,
                        &query.text,
                        filter,
                        fetch,
                        self.config.tokenizer,
                        self.config.analyzer_name.as_deref(),
                    ))
            })
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Some(err) = Self::auth_error(status, &body) {
                return Err(err);
            }
            if let Some(err) = Self::bad_request_error(status, &body) {
                return Err(err);
            }
            return Err(Self::status_error("search", status, &body));
        }
        let text = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&text).map_err(Self::decode_error)?;
        let hits = parsed
            .pointer("/hits/hits")
            .and_then(|value| value.as_array())
            .ok_or_else(|| {
                KeywordError::backend("elastic search response has no hits".to_string())
            })?;
        let mut out = Vec::new();
        for hit in hits {
            let score = hit
                .get("_score")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0) as f32;
            let source = hit
                .get("_source")
                .ok_or_else(|| KeywordError::backend("elastic hit has no _source".to_string()))?;
            let payload_raw = source
                .get("payload")
                .and_then(|value| value.as_str())
                .ok_or_else(|| KeywordError::backend("elastic hit has no payload".to_string()))?;
            let payload_map: HashMap<String, serde_json::Value> = serde_json::from_str(payload_raw)
                .map_err(|err| KeywordError::backend(err.to_string()))?;
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
            out.push(ScoredPoint {
                id: chunk_id,
                score,
                payload: payload_map,
            });
            if out.len() >= query.limit {
                break;
            }
        }
        Ok(out)
    }

    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> KeywordResult<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut body = String::new();
        for id in ids {
            let doc_id = Self::doc_id(collection, id);
            body.push_str(
                &serde_json::to_string(&json!({"delete": {"_id": doc_id}}))
                    .map_err(|err| KeywordError::backend(err.to_string()))?,
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
            if let Some(err) = Self::auth_error(status, &text) {
                return Err(err);
            }
            if let Some(err) = Self::bad_request_error(status, &text) {
                return Err(err);
            }
            return Err(KeywordError::backend(format!(
                "elastic bulk delete failed (status {status}): {text}"
            )));
        }
        let text = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&text).map_err(Self::decode_error)?;
        let items = parsed
            .get("items")
            .and_then(|value| value.as_array())
            .ok_or_else(|| {
                KeywordError::backend("elastic bulk response has no items".to_string())
            })?;
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
    ) -> KeywordResult<u64> {
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
            if let Some(err) = Self::auth_error(status, &body) {
                return Err(err);
            }
            if let Some(err) = Self::bad_request_error(status, &body) {
                return Err(err);
            }
            return Err(Self::status_error("delete by query", status, &body));
        }
        let text = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&text).map_err(Self::decode_error)?;
        Ok(parsed
            .get("deleted")
            .and_then(|value| value.as_u64())
            .unwrap_or(0))
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> KeywordResult<u64> {
        let index = self.index_name().to_string();
        if !self.config.exact_count {
            return self.approximate_count(&index, collection, filter).await;
        }
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
            if let Some(err) = Self::auth_error(status, &body) {
                return Err(err);
            }
            return Err(Self::status_error("count", status, &body));
        }
        let text = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&text).map_err(Self::decode_error)?;
        parsed
            .get("count")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| KeywordError::backend("elastic count response has no count".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(text: &str, filter: Option<&SearchFilter>, limit: usize) -> KeywordQuery {
        let query = KeywordQuery::new(text, limit);
        match filter {
            Some(filter) => query.with_filter(filter.clone()),
            None => query,
        }
    }

    fn test_config() -> ElasticKeywordConfig {
        ElasticKeywordConfig {
            base_url: "http://127.0.0.1:19200".to_string(),
            api_key: None,
            index: "rag-keyword-test".to_string(),
            timeout_secs: 2,
            batch_size: 64,
            exact_count: true,
            tokenizer: wf_types::config::keyword::KeywordTokenizer::English,
            analyzer_name: None,
            format_policy: Default::default(),
        }
    }

    #[test]
    fn status_error_redacts_credentials() {
        let error = ElasticKeywordIndex::status_error(
            "probe",
            reqwest::StatusCode::BAD_GATEWAY,
            r#"{"error":"bad","api_key":"secret-value"}"#,
        );
        let text = error.to_string();
        assert!(!text.contains("secret-value"));
        assert!(text.contains("[redacted]"));
    }

    #[test]
    fn mixed_mapping_carries_sidecar_and_roundtrip_bodies() {
        use wf_types::config::keyword::KeywordTokenizer;
        let mapping = ElasticKeywordIndex::index_mapping_with(KeywordTokenizer::Mixed);
        assert!(
            mapping
                .pointer("/mappings/properties/content_tokens")
                .is_some(),
            "mixed mapping must carry a whitespace sidecar"
        );
        let point = KeywordDocument::new("a", "hello world").with_payload(HashMap::from([(
            "chunk_id".to_string(),
            serde_json::json!("a"),
        )]));
        let (_, doc) =
            ElasticKeywordIndex::doc_body_with("kb", &point, KeywordTokenizer::Mixed, None)
                .expect("doc");
        assert!(doc.get("content_tokens").is_some());
        let body = ElasticKeywordIndex::search_body_with(
            "kb",
            "hello",
            None,
            10,
            KeywordTokenizer::Mixed,
            None,
        );
        assert!(body.pointer("/query/bool/must").is_some());
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
            KeywordDocument::new("live#0", "rust ownership borrowing").with_payload(HashMap::from(
                [
                    ("chunk_id".to_string(), serde_json::json!("live#0")),
                    ("namespace".to_string(), serde_json::json!("kb")),
                    ("source".to_string(), serde_json::json!("live.md")),
                ],
            )),
        ];
        index.upsert("kb", &points).await.expect("upsert");
        let filter = SearchFilter::new().with_namespace("kb");
        let hits = index
            .search("kb", &query("rust ownership", Some(&filter), 5))
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

    #[tokio::test]
    async fn live_concurrent_bulk_when_server_configured() {
        use std::sync::Arc;
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
        let index = Arc::new(ElasticKeywordIndex::new(&config).expect("client"));
        let mut writers = Vec::new();
        for shard in 0..4 {
            let owned = index.clone();
            writers.push(tokio::spawn(async move {
                let points = vec![KeywordDocument::new(
                    format!("concurrent#{shard}"),
                    "concurrent bulk rust ownership",
                )
                .with_payload(HashMap::from([
                    (
                        "chunk_id".to_string(),
                        serde_json::json!(format!("concurrent#{shard}")),
                    ),
                    ("namespace".to_string(), serde_json::json!("kb")),
                    (
                        "source".to_string(),
                        serde_json::json!(format!("load-{shard}.md")),
                    ),
                ]))];
                owned.upsert("kb", &points).await.expect("upsert");
            }));
        }
        for writer in writers {
            writer.await.expect("writer");
        }
        let filter = SearchFilter::new().with_namespace("kb");
        assert_eq!(index.count("kb", Some(&filter)).await.expect("count"), 4);
        for shard in 0..4 {
            index
                .delete_by_source("kb", Some("kb"), &format!("load-{shard}.md"))
                .await
                .expect("delete");
        }
    }
}
