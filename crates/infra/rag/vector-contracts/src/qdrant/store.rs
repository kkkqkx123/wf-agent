use super::*;

impl QdrantVectorStore {
    /// Validates a payload-update target id shared by the three payload ops.
    pub(crate) fn check_payload_target(id: &str) -> Result<()> {
        if id.is_empty() {
            return Err(VectorError::InvalidInput(
                "point id must not be empty".to_string(),
            ));
        }
        Ok(())
    }

    /// Fails on missing collections and missing points so payload updates
    /// report the same not-found errors on every backend.
    pub(crate) async fn require_point(&self, collection: &str, id: &str) -> Result<()> {
        if self.fetch(collection, &[id.to_string()]).await?.is_empty() {
            return Err(VectorError::InvalidInput(format!(
                "point '{id}' not found in collection '{collection}'"
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl VectorStorage for QdrantVectorStore {
    fn backend_name(&self) -> &'static str {
        "qdrant"
    }

    fn supported_kinds(&self) -> Vec<VectorKind> {
        vec![VectorKind::Dense, VectorKind::Sparse, VectorKind::Multi]
    }

    fn supports_named_spaces(&self) -> bool {
        true
    }

    async fn ensure_collection(&self, spec: &CollectionSpec) -> Result<bool> {
        let started = Instant::now();
        tracing::debug!(backend = "qdrant", collection = %spec.name, dimension = spec.dimension, "qdrant ensure collection");
        if !self.supported_kinds().contains(&spec.kind) {
            return Err(VectorError::unsupported(
                self.backend_name(),
                spec.kind.as_name(),
            ));
        }
        spec.validate()?;
        if spec.spaces.is_some() {
            if !self.supports_named_spaces() {
                return Err(VectorError::unsupported(
                    self.backend_name(),
                    "named vector spaces",
                ));
            }
            if !self.check_named_space_support().await? {
                return Err(VectorError::unsupported(
                    self.backend_name(),
                    "named vector spaces",
                ));
            }
        }
        if spec.kind != VectorKind::Dense && (spec.quantization.is_some() || spec.hnsw.is_some()) {
            return Err(VectorError::InvalidInput(format!(
                "{} vectors do not accept quantization or hnsw settings",
                spec.kind.as_name()
            )));
        }
        if spec.spaces.is_none() && !self.check_kind_support(spec.kind).await? {
            return Err(VectorError::unsupported(
                self.backend_name(),
                spec.kind.as_name(),
            ));
        }
        if spec.name.is_empty() {
            return Err(VectorError::InvalidInput(
                "collection name must not be empty".to_string(),
            ));
        }
        if spec.spaces.is_none() && spec.dimension == 0 && spec.kind != VectorKind::Sparse {
            return Err(VectorError::InvalidInput(
                "collection dimension must be positive".to_string(),
            ));
        }
        Self::validate_collection_spec(spec)?;
        let response = self
            .send_read(|| {
                self.client
                    .get(self.url(&format!("/collections/{}", spec.name)))
            })
            .await?;
        if response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            let (dimension, distance, kind, name) = Self::parse_collection_info(&body)?;
            let spaces = Self::parse_collection_spaces(&body)?;
            if let Some(requested) = &spec.spaces {
                let parsed = spaces.ok_or_else(|| {
                    VectorError::InvalidInput(format!(
                        "collection '{}' is not a named-layout collection",
                        spec.name
                    ))
                })?;
                if !Self::named_spaces_compatible(requested, &parsed) {
                    return Err(VectorError::InvalidInput(format!(
                        "collection '{}' uses different named spaces, recreate it",
                        spec.name
                    )));
                }
                self.remember(
                    &spec.name,
                    spec.dimension,
                    spec.distance,
                    spec.kind,
                    None,
                    Some(parsed),
                );
                self.ensure_payload_indexes(&spec.name).await;
                return Ok(false);
            }
            if dimension != spec.dimension && spec.kind != VectorKind::Sparse {
                return Err(VectorError::DimensionMismatch {
                    expected: dimension,
                    actual: spec.dimension,
                });
            }
            if distance != spec.distance && spec.kind != VectorKind::Sparse {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses {:?} distance, requested {:?}",
                    spec.name, distance, spec.distance
                )));
            }
            if kind != spec.kind {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' is {} but {} was requested",
                    spec.name,
                    kind.as_name(),
                    spec.kind.as_name()
                )));
            }
            let remembered_name = spec.vector_name.clone().or(name);
            self.remember(&spec.name, dimension, distance, kind, remembered_name, None);
            self.ensure_payload_indexes(&spec.name).await;
            tracing::debug!(backend = "qdrant",
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
                "qdrant describe failed with status {status}: {}",
                diagnostic_body(&body)
            )));
        }
        let response = self
            .send(
                self.client
                    .put(self.url(&format!("/collections/{}{}", spec.name, self.wait_suffix())))
                    .json(&Self::collection_create_body(spec)),
            )
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Self::map_status(status, &body, &spec.name)?;
        self.remember(
            &spec.name,
            spec.dimension,
            spec.distance,
            spec.kind,
            spec.vector_name.clone(),
            spec.spaces.clone(),
        );
        self.ensure_payload_indexes(&spec.name).await;
        tracing::debug!(backend = "qdrant",
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
        tracing::debug!(backend = "qdrant", collection = %collection, "qdrant delete collection");
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
        tracing::debug!(backend = "qdrant",
            collection = %collection,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant delete collection done"
        );
        Ok(())
    }

    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()> {
        let started = Instant::now();
        tracing::debug!(backend = "qdrant", collection = %collection, count = points.len(), "qdrant upsert");
        for point in points {
            if point.id.is_empty() {
                return Err(VectorError::InvalidInput(
                    "point id must not be empty".to_string(),
                ));
            }
        }
        if points.is_empty() {
            return Ok(());
        }
        let info = self.collection_config(collection).await?;
        if let Some(spaces) = &info.spaces {
            for point in points {
                point.validate_named(spaces).map_err(|err| match err {
                    VectorError::InvalidInput(message) => {
                        VectorError::InvalidInput(format!("point '{}': {}", point.id, message))
                    }
                    other => other,
                })?;
            }
        } else {
            for point in points {
                point
                    .vector
                    .validate(info.kind, info.dimension, None)
                    .map_err(|err| match err {
                        VectorError::InvalidInput(message) => {
                            VectorError::InvalidInput(format!("point '{}': {}", point.id, message))
                        }
                        other => other,
                    })?;
            }
        }
        let vector_name = info.vector_name.as_deref();
        let batch = self.batch_size();
        for chunk in points.chunks(batch) {
            let payload: Vec<serde_json::Value> = chunk
                .iter()
                .map(|point| {
                    let mut fields: HashMap<String, serde_json::Value> = point.payload.clone();
                    fields.insert(PID_KEY.to_string(), json!(point.id));
                    let vector = match (&info.spaces, &point.named) {
                        (Some(_), Some(named)) => Self::named_point_vector_body(named),
                        _ => Self::point_vector_body(&point.vector, vector_name),
                    };
                    json!({
                        "id": Self::native_id(&point.id),
                        "vector": vector,
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
                    expected: info
                        .spaces
                        .as_ref()
                        .and_then(|spaces| {
                            spaces
                                .values()
                                .find(|space| space.kind == VectorKind::Dense)
                        })
                        .map(|space| space.vector_size)
                        .unwrap_or(info.dimension),
                    actual,
                });
            }
            Self::map_status(status, &body, collection)?;
        }
        tracing::debug!(backend = "qdrant",
            collection = %collection,
            count = points.len(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant upsert done"
        );
        Ok(())
    }

    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredPoint>> {
        let started = Instant::now();
        tracing::debug!(backend = "qdrant", collection = %collection, limit = query.limit, "qdrant search");
        let info = self.collection_config(collection).await.ok();
        let named_spaces = info.as_ref().and_then(|info| info.spaces.clone());
        let named_target = if let Some(spaces) = &named_spaces {
            query.validate_named(spaces)?;
            let target = query.target_space.clone().ok_or_else(|| {
                VectorError::InvalidInput("named query has no target space".to_string())
            })?;
            Some(target)
        } else {
            if let Some(info) = &info {
                query
                    .vector
                    .validate(info.kind, info.dimension, None)
                    .map_err(|err| match err {
                        VectorError::InvalidInput(message) => VectorError::InvalidInput(message),
                        other => other,
                    })?;
            }
            None
        };
        let using = info.as_ref().and_then(|info| {
            info.vector_name.clone().or_else(|| {
                if info.kind == VectorKind::Sparse {
                    Some("sparse".to_string())
                } else {
                    None
                }
            })
        });
        if Self::filter_needs_exact_match(query.filter.as_ref()) {
            let info =
                info.ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
            return self.search_exact(collection, query, info).await;
        }
        let body = match (&named_target, &named_spaces) {
            (Some(target), Some(_)) => {
                let data = query.named_vector.as_ref().ok_or_else(|| {
                    VectorError::InvalidInput("named query has no vector".to_string())
                })?;
                Self::named_search_body(query, target, data)?
            }
            _ => Self::search_body(query, using.as_deref())?,
        };
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
                let actual = query
                    .named_vector
                    .as_ref()
                    .map(|data| data.len())
                    .unwrap_or_else(|| query.vector.len());
                return Err(VectorError::DimensionMismatch {
                    expected: named_spaces
                        .as_ref()
                        .and_then(|spaces| named_target.as_ref().and_then(|name| spaces.get(name)))
                        .map(|space| space.vector_size)
                        .unwrap_or(info.dimension),
                    actual,
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
        let distance = named_target
            .as_ref()
            .and_then(|name| named_spaces.as_ref()?.get(name))
            .map(|space| space.distance)
            .or_else(|| info.map(|info| info.distance));
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
        tracing::debug!(backend = "qdrant",
            collection = %collection,
            hits = hits.len(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant search done"
        );
        Ok(hits)
    }

    async fn fetch(&self, collection: &str, ids: &[String]) -> Result<Vec<VectorPoint>> {
        let started = Instant::now();
        tracing::debug!(backend = "qdrant", collection = %collection, count = ids.len(), "qdrant fetch");
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let spaces = self
            .collection_config(collection)
            .await
            .ok()
            .and_then(|info| info.spaces);
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
            let (vector, named) = Self::parse_point_vector(vector_value, spaces.as_ref())?;
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
                    named,
                    payload,
                },
            );
        }
        let ordered = ids
            .iter()
            .filter_map(|id| by_pid.remove(id))
            .collect::<Vec<_>>();
        tracing::debug!(backend = "qdrant",
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
        tracing::debug!(backend = "qdrant", collection = %collection, count = ids.len(), "qdrant delete by id");
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
        tracing::debug!(backend = "qdrant",
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
        tracing::debug!(backend = "qdrant", collection = %collection, source = %source, "qdrant delete by source");
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
        tracing::debug!(backend = "qdrant",
            collection = %collection,
            removed = existing,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant delete by source done"
        );
        Ok(existing)
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64> {
        let started = Instant::now();
        if Self::filter_needs_exact_match(filter) {
            let mut total = 0u64;
            let mut offset: Option<String> = None;
            loop {
                let mut query = ScrollQuery::new(256);
                if let Some(filter) = filter {
                    query = query.with_filter(filter.clone());
                }
                if let Some(cursor) = &offset {
                    query = query.with_offset(cursor.clone());
                }
                let page = self.scroll(collection, &query).await?;
                total += page.points.len() as u64;
                match page.next_offset {
                    Some(cursor) => offset = Some(cursor),
                    None => break,
                }
            }
            tracing::debug!(backend = "qdrant",
                collection = %collection,
                count = total,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "qdrant count done"
            );
            return Ok(total);
        }
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
        tracing::debug!(backend = "qdrant",
            collection = %collection,
            count = count,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant count done"
        );
        Ok(count)
    }

    async fn scroll(&self, collection: &str, query: &ScrollQuery) -> Result<ScrollPage> {
        let started = Instant::now();
        tracing::debug!(backend = "qdrant", collection = %collection, limit = query.limit, "qdrant scroll");
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
        let spaces = self
            .collection_config(collection)
            .await
            .ok()
            .and_then(|info| info.spaces);
        let (mut points, next_offset) =
            Self::parse_scroll_page_with_spaces(&text, spaces.as_ref())?;
        let raw_count = points.len();
        points.truncate(limit);
        let needs_exact = query
            .filter
            .as_ref()
            .map(|f| Self::filter_needs_exact_match(Some(f)))
            .unwrap_or(false);
        if needs_exact {
            if let Some(filter) = &query.filter {
                points.retain(|point| filter.matches_point(&point.id, &point.payload));
            }
        }
        let next_offset = if raw_count == 0 || raw_count < limit {
            None
        } else {
            next_offset
        };
        tracing::debug!(backend = "qdrant",
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
        tracing::debug!(backend = "qdrant", collection = %collection, "qdrant overwrite payload");
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
        tracing::debug!(backend = "qdrant",
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
        tracing::debug!(backend = "qdrant", collection = %collection, "qdrant merge payload");
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
        tracing::debug!(backend = "qdrant",
            collection = %collection,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant merge payload done"
        );
        Ok(())
    }

    async fn delete_payload_keys(&self, collection: &str, id: &str, keys: &[String]) -> Result<()> {
        let started = Instant::now();
        tracing::debug!(backend = "qdrant", collection = %collection, "qdrant delete payload keys");
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
        tracing::debug!(backend = "qdrant",
            collection = %collection,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant delete payload keys done"
        );
        Ok(())
    }
}
