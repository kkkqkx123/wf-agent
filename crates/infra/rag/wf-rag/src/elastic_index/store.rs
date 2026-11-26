use super::*;

impl ElasticKeywordIndex {
    /// Fast count through the search total when exact counting is disabled.
    /// Large collections trade accuracy for speed; the relation flag tells
    /// whether the total is exact or a lower bound.
    pub(crate) async fn approximate_count(
        &self,
        index: &str,
        collection: &str,
        scope: Option<&KeywordScope>,
    ) -> KeywordResult<u64> {
        let response = self
            .send_read(|| {
                self.client
                    .post(self.url(&format!("/{index}/_search")))
                    .json(&json!({
                        "query": {"bool": {"filter": Self::scope_clauses(collection, scope)}},
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
        let scope = query.scope.as_ref();
        let fetch = query.limit;
        let index = self.index_name().to_string();
        let response = self
            .send_read(|| {
                self.client
                    .post(self.url(&format!("/{index}/_search")))
                    .json(&Self::search_body_with(
                        collection,
                        &query.text,
                        scope,
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
        let mut scope = KeywordScope::new().with_source(source);
        if let Some(namespace) = namespace {
            scope = scope.with_namespace(namespace);
        }
        let index = self.index_name().to_string();
        let response = self
            .send(
                self.client
                    .post(self.url(&format!("/{index}/_delete_by_query?refresh=wait_for")))
                    .json(&Self::count_body(collection, Some(&scope))),
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

    async fn count(&self, collection: &str, scope: Option<&KeywordScope>) -> KeywordResult<u64> {
        let index = self.index_name().to_string();
        if !self.config.exact_count {
            return self.approximate_count(&index, collection, scope).await;
        }
        let response = self
            .send_read(|| {
                self.client
                    .post(self.url(&format!("/{index}/_count")))
                    .json(&Self::count_body(collection, scope))
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
