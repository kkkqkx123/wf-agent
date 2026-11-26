use super::*;

impl ElasticKeywordIndex {
    /// Creates the index with the keyword mapping when missing.
    /// Existing indexes pass through after a mapping compatibility check;
    /// mapping conflicts fail explicitly instead of auto-migrating.
    pub(crate) async fn ensure_index(&self) -> KeywordResult<()> {
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

    pub(crate) fn format_id(&self) -> String {
        KeywordFormat::new(self.config.tokenizer, self.config.analyzer_name.clone()).id()
    }

    /// Reads the persisted format stamp from index settings and compares
    /// it with the resolved format. A missing stamp is treated as legacy
    /// adoption when the configuration resolves to the built-in English
    /// path; any other drift fails or degrades per the policy.
    pub(crate) async fn check_format(&self, index: &str) -> KeywordResult<()> {
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

    pub(crate) fn index_mapping_with(
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
    /// Generic payload conditions stay out; the RAG adapter rechecks them.
    pub fn scope_clauses(collection: &str, scope: Option<&KeywordScope>) -> Vec<serde_json::Value> {
        let mut clauses = vec![json!({"term": {"collection": collection}})];
        if let Some(scope) = scope {
            if let Some(namespace) = &scope.namespace {
                clauses.push(json!({"term": {"namespace": namespace}}));
            }
            if let Some(source) = &scope.source {
                clauses.push(json!({"term": {"source": source}}));
            }
        }
        clauses
    }

    /// Verifies the live mapping carries every keyword field with a
    /// compatible type; conflicts fail explicitly, missing indexes are
    /// the caller's `ensure_index` path and never reach here.
    pub(crate) async fn check_mapping(&self, index: &str) -> KeywordResult<()> {
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

    pub(crate) fn search_body_with(
        collection: &str,
        query: &str,
        scope: Option<&KeywordScope>,
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
                "filter": Self::scope_clauses(collection, scope),
            }},
            "size": size,
            "track_total_hits": true,
            "_source": ["collection", "chunk_id", "namespace", "source", "content", "payload"],
        })
    }

    pub(crate) fn count_body(collection: &str, scope: Option<&KeywordScope>) -> serde_json::Value {
        json!({"query": {"bool": {"filter": Self::scope_clauses(collection, scope)}}})
    }

    pub(crate) fn payload_string(
        payload: &HashMap<String, serde_json::Value>,
        key: &str,
    ) -> String {
        payload
            .get(key)
            .and_then(|value| match value {
                serde_json::Value::String(text) => Some(text.clone()),
                other => vector_contracts::payload_text(other),
            })
            .unwrap_or_default()
    }

    pub(crate) fn doc_body_with(
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
}
