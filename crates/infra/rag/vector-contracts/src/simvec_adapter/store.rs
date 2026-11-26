use super::*;

impl SimvecVectorStore {
    pub(crate) fn point_id(id: &str) -> simvec::PointId {
        simvec::PointId::from(id.to_string())
    }

    /// Resolves a contracts id to its engine id, failing on missing
    /// collections and missing points like the other payload operations.
    pub(crate) async fn require_point(
        &self,
        collection: &str,
        id: &str,
    ) -> Result<simvec::PointId> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        if id.is_empty() {
            return Err(VectorError::InvalidInput(
                "point id must not be empty".to_string(),
            ));
        }
        let target = Self::point_id(id);
        let stored = self
            .engine
            .get(collection, &target)
            .map_err(map_engine_error)?;
        if stored.is_none() {
            return Err(VectorError::InvalidInput(format!(
                "point '{id}' not found in collection '{collection}'"
            )));
        }
        Ok(target)
    }

    pub(crate) fn id_string(id: &simvec::PointId) -> String {
        match id {
            simvec::PointId::Num(n) => n.to_string(),
            simvec::PointId::Uuid(s) => s.clone(),
        }
    }
}

#[async_trait]
impl VectorStorage for SimvecVectorStore {
    fn backend_name(&self) -> &'static str {
        "simvec"
    }

    fn supported_kinds(&self) -> Vec<VectorKind> {
        self.engine
            .supported_kinds()
            .into_iter()
            .map(Self::map_engine_kind)
            .collect()
    }

    fn supports_named_spaces(&self) -> bool {
        self.engine.supports_named_spaces()
    }

    async fn ensure_collection(&self, spec: &CollectionSpec) -> Result<bool> {
        if spec.name.is_empty() {
            return Err(VectorError::InvalidInput(
                "collection name must not be empty".to_string(),
            ));
        }
        if let Some(spaces) = &spec.spaces {
            spec.validate()?;
            if !self.supports_named_spaces() {
                return Err(VectorError::unsupported(
                    self.backend_name(),
                    "named vector spaces",
                ));
            }
            if !self.engine.collection_exists(&spec.name) {
                let mapped = spaces
                    .iter()
                    .map(|(name, space)| (name.clone(), Self::map_space(space)))
                    .collect();
                let config = simvec::CollectionConfig::with_named(mapped);
                self.engine
                    .create_collection(&spec.name, &config)
                    .map_err(map_engine_error)?;
                return Ok(true);
            }
            let existing = self
                .engine
                .collection_config(&spec.name)
                .map_err(map_engine_error)?
                .ok_or_else(|| VectorError::CollectionMissing(spec.name.clone()))?;
            let existing = self.named_spaces(&existing)?.ok_or_else(|| {
                VectorError::InvalidInput(format!(
                    "collection '{}' is not a named-layout collection",
                    spec.name
                ))
            })?;
            if &existing != spaces {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses different named spaces, recreate it",
                    spec.name
                )));
            }
            return Ok(false);
        }
        if spec.dimension == 0 {
            return Err(VectorError::InvalidInput(
                "collection dimension must be positive".to_string(),
            ));
        }
        if spec.kind != VectorKind::Dense {
            if spec.quantization.is_some() || spec.hnsw.is_some() {
                return Err(VectorError::InvalidInput(format!(
                    "{} collections do not support quantization or hnsw tuning",
                    spec.kind.as_name()
                )));
            }
            if spec.vector_name.is_some() {
                return Err(VectorError::unsupported(
                    self.backend_name(),
                    "named vectors",
                ));
            }
            if !self.engine.collection_exists(&spec.name) {
                let config = self.kind_collection_config(spec)?;
                self.engine
                    .create_collection(&spec.name, &config)
                    .map_err(map_engine_error)?;
                return Ok(true);
            }
            let existing = self
                .engine
                .collection_config(&spec.name)
                .map_err(map_engine_error)?
                .ok_or_else(|| VectorError::CollectionMissing(spec.name.clone()))?;
            if existing.kind != Self::map_kind(spec.kind) {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses {:?} kind, requested {:?}",
                    spec.name, existing.kind, spec.kind
                )));
            }
            match spec.kind {
                VectorKind::Sparse => {
                    let vocab = existing
                        .sparse_config
                        .map(|config| config.vocab_size)
                        .unwrap_or(existing.vector_size);
                    let requested = spec
                        .sparse_max_index
                        .map(|max| max as usize + 1)
                        .unwrap_or(spec.dimension);
                    if vocab != requested {
                        return Err(VectorError::DimensionMismatch {
                            expected: vocab,
                            actual: requested,
                        });
                    }
                }
                VectorKind::Multi => {
                    if existing.vector_size != spec.dimension {
                        return Err(VectorError::DimensionMismatch {
                            expected: existing.vector_size,
                            actual: spec.dimension,
                        });
                    }
                    let limit = existing
                        .multi_config
                        .map(|config| config.max_vectors_per_point)
                        .unwrap_or(16);
                    let requested = spec.multi_max_vectors.unwrap_or(16);
                    if limit != requested {
                        return Err(VectorError::InvalidInput(format!(
                            "collection '{}' allows {} sub-vectors per point, requested {}",
                            spec.name, limit, requested
                        )));
                    }
                }
                VectorKind::Dense => unreachable!("dense configs use the dense path"),
            }
            if existing.distance != Self::map_distance(spec.distance) {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses {:?} distance, requested {:?}",
                    spec.name, existing.distance, spec.distance
                )));
            }
            return Ok(false);
        }
        if spec.quantization.is_some() && spec.dimension == 0 {
            return Err(VectorError::InvalidInput(
                "collection dimension must be positive".to_string(),
            ));
        }
        if let Some(quantization) = &spec.quantization {
            Self::map_quantization(quantization)
                .validate(spec.dimension)
                .map_err(map_engine_error)?;
        }
        if let Some(hnsw) = &spec.hnsw {
            Self::map_hnsw(hnsw)?;
        }
        if !self.engine.collection_exists(&spec.name) {
            let mut config =
                simvec::CollectionConfig::new(spec.dimension, Self::map_distance(spec.distance));
            if let Some(quantization) = &spec.quantization {
                config = config.with_quantization(Self::map_quantization(quantization));
            }
            if let Some(hnsw) = &spec.hnsw {
                config = config.with_hnsw(Self::map_hnsw(hnsw)?);
            }
            if let Some(on_disk_payload) = spec.on_disk_payload {
                config = config.with_on_disk_payload(on_disk_payload);
            }
            self.engine
                .create_collection(&spec.name, &config)
                .map_err(map_engine_error)?;
            if spec.quantization.is_some() {
                self.engine
                    .build_quantization(&spec.name)
                    .map_err(map_engine_error)?;
            }
            return Ok(true);
        }
        let existing = self
            .engine
            .collection_config(&spec.name)
            .map_err(map_engine_error)?
            .ok_or_else(|| VectorError::CollectionMissing(spec.name.clone()))?;
        if existing.vector_size != spec.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: existing.vector_size,
                actual: spec.dimension,
            });
        }
        if existing.distance != Self::map_distance(spec.distance) {
            return Err(VectorError::InvalidInput(format!(
                "collection '{}' uses {:?} distance, requested {:?}",
                spec.name, existing.distance, spec.distance
            )));
        }
        if let Some(quantization) = &spec.quantization {
            let requested = Self::map_quantization(quantization);
            if existing.quantization_config.as_ref() != Some(&requested) {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses different quantization, recreate it",
                    spec.name
                )));
            }
        }
        if let Some(hnsw) = &spec.hnsw {
            let requested = Self::map_hnsw(hnsw)?;
            if let simvec::IndexConfig::Hnsw(current) = &existing.index {
                if current.m != requested.m
                    || current.ef_construct != requested.ef_construct
                    || current.on_disk != requested.on_disk
                {
                    return Err(VectorError::InvalidInput(format!(
                        "collection '{}' uses different hnsw tuning, recreate it",
                        spec.name
                    )));
                }
            } else {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses a different index tier, recreate it",
                    spec.name
                )));
            }
        }
        Ok(false)
    }

    async fn delete_collection(&self, collection: &str) -> Result<()> {
        if collection.is_empty() {
            return Err(VectorError::InvalidInput(
                "collection name must not be empty".to_string(),
            ));
        }
        self.engine
            .delete_collection(collection)
            .map_err(|err| match err {
                simvec::VectorSearchError::CollectionNotFound(name) => {
                    VectorError::CollectionMissing(name)
                }
                other => map_engine_error(other),
            })
    }

    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        let config = self
            .engine
            .collection_config(collection)
            .map_err(map_engine_error)?
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        if let Some(spaces) = self.named_spaces(&config)? {
            let mut mapped = Vec::with_capacity(points.len());
            for point in points {
                point.validate_named(&spaces)?;
                let named = point
                    .named
                    .as_ref()
                    .ok_or_else(|| {
                        VectorError::InvalidInput(
                            "named points must carry named vectors".to_string(),
                        )
                    })?
                    .iter()
                    .map(|(name, data)| Ok((name.clone(), Self::map_vector_data(data)?)))
                    .collect::<Result<BTreeMap<_, _>>>()?;
                let mut payload = point.payload.clone();
                payload.insert(PID_KEY.to_string(), serde_json::json!(point.id));
                mapped.push(simvec::VectorPoint {
                    id: Self::point_id(&point.id),
                    vector: Vec::new(),
                    data: None,
                    named,
                    payload: Some(payload),
                });
            }
            self.engine
                .upsert_batch(collection, &mapped)
                .map_err(map_engine_error)?;
            return Ok(());
        }
        let dimension = config.vector_size;
        let max_vectors = config.multi_config.map(|c| c.max_vectors_per_point);
        let mut mapped = Vec::with_capacity(points.len());
        for point in points {
            point
                .vector
                .validate(Self::map_engine_kind(config.kind), dimension, max_vectors)?;
            if point.id.is_empty() {
                return Err(VectorError::InvalidInput(
                    "point id must not be empty".to_string(),
                ));
            }
            let mut payload: HashMap<String, serde_json::Value> = point.payload.clone();
            payload.insert(PID_KEY.to_string(), serde_json::json!(point.id));
            let engine_point = match &point.vector {
                VectorData::Dense(dense) => {
                    simvec::VectorPoint::new(Self::point_id(&point.id), dense.to_vec())
                        .with_payload(payload)
                }
                other => simvec::VectorPoint::with_data(
                    Self::point_id(&point.id),
                    Self::map_vector_data(other)?,
                )
                .with_payload(payload),
            };
            mapped.push(engine_point);
        }
        self.engine
            .upsert_batch(collection, &mapped)
            .map_err(map_engine_error)?;
        Ok(())
    }

    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredPoint>> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        // Shared result shaping: scores pass through untouched and the point
        // id is recovered from the pid payload key.
        let shape = |hits: Vec<simvec::SearchResult>| -> Vec<ScoredPoint> {
            hits.into_iter()
                .map(|hit| {
                    let id = hit
                        .payload
                        .as_ref()
                        .and_then(|payload| payload.get(PID_KEY))
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| Self::id_string(&hit.id));
                    ScoredPoint {
                        id,
                        score: hit.score,
                        payload: hit.payload.unwrap_or_default(),
                    }
                })
                .collect()
        };
        let config = self
            .engine
            .collection_config(collection)
            .map_err(map_engine_error)?
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        if let Some(spaces) = self.named_spaces(&config)? {
            query.validate_named(&spaces)?;
            let target = query.target_space.as_deref().ok_or_else(|| {
                VectorError::InvalidInput("named query has no target".to_string())
            })?;
            let data = query.named_vector.as_ref().ok_or_else(|| {
                VectorError::InvalidInput("named query has no vector".to_string())
            })?;
            let mut request =
                simvec::SearchQuery::with_named(target, Self::map_vector_data(data)?, query.limit);
            if let Some(threshold) = query.score_threshold {
                request = request.with_score_threshold(threshold);
            }
            if let Some(filter) = &query.filter {
                if let Some(mapped) = Self::map_filter(filter)? {
                    request = request.with_filter(mapped);
                }
            }
            let hits = self
                .engine
                .search(collection, &request)
                .map_err(map_engine_error)?;
            return Ok(shape(hits));
        }
        let kind = Self::map_engine_kind(config.kind);
        if query.vector.kind() != kind {
            return Err(VectorError::InvalidInput(format!(
                "collection '{collection}' expects {} vectors, got {}",
                kind.as_name(),
                query.vector.kind().as_name()
            )));
        }
        query.vector.validate(
            kind,
            config.vector_size,
            config
                .multi_config
                .map(|config| config.max_vectors_per_point),
        )?;
        if kind != VectorKind::Dense
            && (query.exact.is_some() || query.hnsw_ef.is_some() || query.quantization.is_some())
        {
            return Err(VectorError::InvalidInput(
                "dense recall controls only apply to dense collections".to_string(),
            ));
        }
        let request = match &query.vector {
            VectorData::Dense(dense) => {
                Self::validate_search_params(query)?;
                let mut request = simvec::SearchQuery::new(dense.to_vec(), query.limit);
                if let Some(threshold) = query.score_threshold {
                    request = request.with_score_threshold(threshold);
                }
                if let Some(filter) = &query.filter {
                    if let Some(mapped) = Self::map_filter(filter)? {
                        request = request.with_filter(mapped);
                    }
                }
                if let Some(hnsw_ef) = query.hnsw_ef {
                    request = request.with_search_mode(simvec::SearchMode::KNN {
                        k: query.limit,
                        ef_search: Some(hnsw_ef as usize),
                    });
                }
                if let Some(params) = &query.quantization {
                    request = request.with_quantization_params(simvec::QuantizationSearchParams {
                        ignore: params.ignore.unwrap_or(false),
                        exact: query.exact.unwrap_or(false),
                        rescore: params.rescore,
                        oversampling: params.oversampling,
                    });
                } else if query.exact == Some(true) {
                    request = request.with_exact(true);
                }
                request
            }
            other => {
                let mut request =
                    simvec::SearchQuery::with_data(Self::map_vector_data(other)?, query.limit);
                if let Some(threshold) = query.score_threshold {
                    request = request.with_score_threshold(threshold);
                }
                if let Some(filter) = &query.filter {
                    if let Some(mapped) = Self::map_filter(filter)? {
                        request = request.with_filter(mapped);
                    }
                }
                request
            }
        };
        let hits = self
            .engine
            .search(collection, &request)
            .map_err(map_engine_error)?;
        Ok(shape(hits))
    }

    async fn fetch(&self, collection: &str, ids: &[String]) -> Result<Vec<VectorPoint>> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        let mut found = Vec::with_capacity(ids.len());
        for id in ids {
            let stored = self
                .engine
                .get(collection, &Self::point_id(id))
                .map_err(map_engine_error)?;
            if let Some(point) = stored {
                found.push(Self::map_engine_point(point, id));
            }
        }
        Ok(found)
    }

    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> Result<u64> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        if ids.is_empty() {
            return Ok(0);
        }
        let mut existing = 0u64;
        for id in ids {
            let stored = self
                .engine
                .get(collection, &Self::point_id(id))
                .map_err(map_engine_error)?;
            if stored.is_some() {
                existing += 1;
            }
        }
        let points: Vec<simvec::PointId> = ids.iter().map(|id| Self::point_id(id)).collect();
        self.engine
            .delete_batch(collection, &points)
            .map_err(map_engine_error)?;
        Ok(existing)
    }

    async fn delete_by_source(
        &self,
        collection: &str,
        namespace: Option<&str>,
        source: &str,
    ) -> Result<u64> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        let mut filter = SearchFilter::new().with_source(source);
        if let Some(namespace) = namespace {
            filter = filter.with_namespace(namespace);
        }
        let mapped = Self::map_filter(&filter)?
            .ok_or_else(|| VectorError::InvalidInput("source must not be empty".to_string()))?;
        self.engine
            .delete_by_filter(collection, &mapped)
            .map_err(map_engine_error)
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        match filter {
            None => self.engine.count(collection).map_err(map_engine_error),
            Some(filter) => match Self::map_filter(filter)? {
                None => self.engine.count(collection).map_err(map_engine_error),
                Some(mapped) => self
                    .engine
                    .count_by_filter(collection, &mapped)
                    .map_err(map_engine_error),
            },
        }
    }

    async fn scroll(&self, collection: &str, query: &ScrollQuery) -> Result<ScrollPage> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        let limit = query.limit.max(1);
        let offset = query.offset.as_deref().map(simvec::PointId::from);
        let (points, next) = match &query.filter {
            None => self
                .engine
                .scroll(collection, limit, offset.as_ref(), Some(true), Some(true))
                .map_err(map_engine_error)?,
            Some(filter) => match Self::map_filter(filter)? {
                None => self
                    .engine
                    .scroll(collection, limit, offset.as_ref(), Some(true), Some(true))
                    .map_err(map_engine_error)?,
                Some(mapped) => self
                    .engine
                    .scroll_by_filter(
                        collection,
                        &mapped,
                        limit,
                        offset.as_ref(),
                        Some(true),
                        Some(true),
                    )
                    .map_err(map_engine_error)?,
            },
        };
        let mut converted = Vec::with_capacity(points.len());
        for point in points {
            let id_hint = Self::id_string(&point.id);
            converted.push(Self::map_engine_point(point, &id_hint));
        }
        let next_offset = next.as_ref().map(Self::id_string);
        let next_offset = if converted.len() < limit {
            None
        } else {
            next_offset
        };
        Ok(ScrollPage {
            points: converted,
            next_offset,
        })
    }

    async fn overwrite_payload(
        &self,
        collection: &str,
        id: &str,
        payload: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let target = self.require_point(collection, id).await?;
        let mut full = payload;
        full.insert(PID_KEY.to_string(), serde_json::json!(id));
        self.engine
            .set_payload(collection, &target, full)
            .map_err(|err| map_point_error(err, collection, id))
    }

    async fn merge_payload(
        &self,
        collection: &str,
        id: &str,
        fields: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let target = self.require_point(collection, id).await?;
        let mut merged = fields;
        merged.remove(PID_KEY);
        if merged.is_empty() {
            return Ok(());
        }
        self.engine
            .set_payload_fields(collection, &target, merged)
            .map_err(|err| map_point_error(err, collection, id))
    }

    async fn delete_payload_keys(&self, collection: &str, id: &str, keys: &[String]) -> Result<()> {
        let target = self.require_point(collection, id).await?;
        let kept: Vec<String> = keys
            .iter()
            .filter(|key| key.as_str() != PID_KEY)
            .cloned()
            .collect();
        if kept.is_empty() {
            return Ok(());
        }
        self.engine
            .delete_payload(collection, &target, kept)
            .map_err(|err| map_point_error(err, collection, id))
    }
}
