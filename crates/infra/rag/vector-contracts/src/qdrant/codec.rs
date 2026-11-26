use super::*;

impl QdrantVectorStore {
    pub(crate) fn distance_name(distance: DistanceMetric) -> &'static str {
        match distance {
            DistanceMetric::Cosine => "Cosine",
            DistanceMetric::Dot => "Dot",
            DistanceMetric::Euclid => "Euclid",
        }
    }

    pub(crate) fn parse_distance(name: &str) -> Result<DistanceMetric> {
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
    #[cfg(test)]
    pub(crate) fn parse_collection_vectors(
        vectors: &serde_json::Value,
    ) -> Result<(usize, DistanceMetric)> {
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

    pub(crate) fn parse_collection_info(
        body: &str,
    ) -> Result<(usize, DistanceMetric, VectorKind, Option<String>)> {
        let parsed: serde_json::Value =
            serde_json::from_str(body).map_err(|err| VectorError::Decode(err.to_string()))?;
        let params = parsed
            .pointer("/result/config/params")
            .or_else(|| parsed.pointer("/result/config"))
            .or_else(|| parsed.pointer("/result"));
        let vectors = parsed
            .pointer("/result/config/params/vectors")
            .or_else(|| parsed.pointer("/result/vectors"))
            .or_else(|| parsed.pointer("/result/config/vectors"));
        let sparse = parsed
            .pointer("/result/config/params/sparse_vectors")
            .or_else(|| parsed.pointer("/result/sparse_vectors"))
            .or_else(|| parsed.pointer("/result/config/sparse_vectors"));
        if sparse.is_some() && vectors.is_none() {
            let name = sparse
                .and_then(|v| v.as_object())
                .and_then(|map| map.keys().next().cloned())
                .unwrap_or_default();
            return Ok((0, DistanceMetric::Cosine, VectorKind::Sparse, Some(name)));
        }
        let vectors = vectors.ok_or_else(|| {
            VectorError::Decode("qdrant collection response has no vectors".to_string())
        })?;
        let (dimension, distance, kind, name) = Self::parse_vector_config(vectors);
        let _ = params;
        Ok((dimension, distance, kind, name))
    }

    /// Parses Qdrant's dense `vectors` and sparse `sparse_vectors` fields
    /// into one named-space map. A single unnamed collection stays on the
    /// legacy single-space path; mixed or multi-name layouts are named.
    pub(crate) fn parse_collection_spaces(
        body: &str,
    ) -> Result<Option<BTreeMap<String, VectorSpaceSpec>>> {
        let parsed: serde_json::Value =
            serde_json::from_str(body).map_err(|err| VectorError::Decode(err.to_string()))?;
        let vectors = parsed
            .pointer("/result/config/params/vectors")
            .or_else(|| parsed.pointer("/result/vectors"))
            .or_else(|| parsed.pointer("/result/config/vectors"));
        let sparse = parsed
            .pointer("/result/config/params/sparse_vectors")
            .or_else(|| parsed.pointer("/result/sparse_vectors"))
            .or_else(|| parsed.pointer("/result/config/sparse_vectors"));
        let mut spaces = BTreeMap::new();
        let mut named_dense = 0usize;
        if let Some(map) = vectors.and_then(serde_json::Value::as_object) {
            if !map.contains_key("size") {
                for (name, config) in map {
                    let size = config
                        .get("size")
                        .and_then(|value| value.as_u64())
                        .ok_or_else(|| {
                            VectorError::Decode(format!("qdrant named vector '{name}' has no size"))
                        })? as usize;
                    let metric = config
                        .get("distance")
                        .and_then(|value| value.as_str())
                        .ok_or_else(|| {
                            VectorError::Decode(format!(
                                "qdrant named vector '{name}' has no distance"
                            ))
                        })?;
                    if config.get("multivector_config").is_some() {
                        return Err(VectorError::Decode(format!(
                            "named vector '{name}' uses the unsupported multi kind"
                        )));
                    }
                    spaces.insert(
                        name.clone(),
                        VectorSpaceSpec::new(
                            VectorKind::Dense,
                            size,
                            Self::parse_distance(metric)?,
                        ),
                    );
                    named_dense += 1;
                }
            }
        }
        let sparse_count = sparse
            .and_then(serde_json::Value::as_object)
            .map(|map| map.len())
            .unwrap_or(0);
        if sparse_count > 0 && (named_dense > 0 || sparse_count > 1) {
            let map = sparse
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| {
                    VectorError::Decode("qdrant sparse vectors is not an object".to_string())
                })?;
            for name in map.keys() {
                if spaces.contains_key(name) {
                    return Err(VectorError::Decode(format!(
                        "qdrant vector name '{name}' is declared twice"
                    )));
                }
                spaces.insert(
                    name.clone(),
                    VectorSpaceSpec::new(
                        VectorKind::Sparse,
                        u32::MAX as usize + 1,
                        DistanceMetric::Cosine,
                    ),
                );
            }
        }
        if spaces.is_empty() {
            Ok(None)
        } else {
            Ok(Some(spaces))
        }
    }

    /// Parses a dense or multi vector config into size, distance, kind and
    /// the named slot when the config is a name-to-params map.
    pub(crate) fn parse_vector_config(
        vectors: &serde_json::Value,
    ) -> (usize, DistanceMetric, VectorKind, Option<String>) {
        if vectors.get("size").is_some() {
            let dimension = vectors.get("size").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let distance = vectors
                .get("distance")
                .and_then(|v| v.as_str())
                .and_then(|name| Self::parse_distance(name).ok())
                .unwrap_or(DistanceMetric::Cosine);
            let kind = if vectors.get("multivector_config").is_some() {
                VectorKind::Multi
            } else {
                VectorKind::Dense
            };
            return (dimension, distance, kind, None);
        }
        if let Some(map) = vectors.as_object() {
            let (name, candidate) = map
                .get("default")
                .map(|value| ("default".to_string(), value))
                .or_else(|| map.iter().next().map(|(key, value)| (key.clone(), value)))
                .unwrap_or_default();
            let (dimension, distance, kind, _) = Self::parse_vector_config(candidate);
            return (dimension, distance, kind, Some(name));
        }
        (0, DistanceMetric::Cosine, VectorKind::Dense, None)
    }

    /// Converts a contracts vector payload into the `vector` field of a
    /// Qdrant point. Dense keeps the bare array; sparse and multi vectors
    /// are wrapped by the declared slot name when one is configured.
    pub(crate) fn point_vector_body(data: &VectorData, name: Option<&str>) -> serde_json::Value {
        match data {
            VectorData::Dense(vector) => match name {
                Some(name) => json!({name: vector}),
                None => json!(vector),
            },
            VectorData::Sparse { indices, values } => {
                let sparse = json!({"indices": indices, "values": values});
                match name {
                    Some(name) => json!({name: sparse}),
                    None => json!({"sparse": sparse}),
                }
            }
            VectorData::Multi(vectors) => match name {
                Some(name) => json!({name: vectors}),
                None => json!(vectors),
            },
        }
    }

    /// Serializes a named point payload as Qdrant's vector-name map.
    pub(crate) fn named_point_vector_body(
        named: &BTreeMap<String, VectorData>,
    ) -> serde_json::Value {
        named
            .iter()
            .map(|(name, data)| {
                let value = match data {
                    VectorData::Dense(vector) => json!(vector),
                    VectorData::Sparse { indices, values } => {
                        json!({"indices": indices, "values": values})
                    }
                    VectorData::Multi(vectors) => json!(vectors),
                };
                (name.clone(), value)
            })
            .collect::<serde_json::Map<_, _>>()
            .into()
    }

    /// Converts a contracts query payload into the `query` field of a Qdrant
    /// query request and the optional `using` slot name.
    pub(crate) fn query_body(data: &VectorData) -> (serde_json::Value, Option<String>) {
        match data {
            VectorData::Dense(vector) => (json!(vector), None),
            VectorData::Sparse { indices, values } => {
                (json!({"indices": indices, "values": values}), None)
            }
            VectorData::Multi(vectors) => (json!(vectors), None),
        }
    }

    pub(crate) fn validate_search_params(query: &VectorQuery) -> Result<()> {
        if query.hnsw_ef == Some(0) {
            return Err(VectorError::InvalidInput(
                "hnsw_ef must be positive".to_string(),
            ));
        }
        if let Some(params) = &query.quantization {
            if params
                .oversampling
                .is_some_and(|value| !value.is_finite() || value < 1.0)
            {
                return Err(VectorError::InvalidInput(format!(
                    "quantization oversampling {} must be >= 1.0",
                    params.oversampling.unwrap_or(0.0)
                )));
            }
        }
        if query
            .vector
            .as_dense()?
            .iter()
            .any(|value| !value.is_finite())
        {
            return Err(VectorError::InvalidInput(
                "query vector contains non-finite elements".to_string(),
            ));
        }
        Ok(())
    }

    pub(crate) fn quantization_body(
        params: &crate::types::QuantizationSearchParams,
    ) -> serde_json::Value {
        let mut body = serde_json::Map::new();
        if let Some(ignore) = params.ignore {
            body.insert("ignore".to_string(), json!(ignore));
        }
        if let Some(rescore) = params.rescore {
            body.insert("rescore".to_string(), json!(rescore));
        }
        if let Some(oversampling) = params.oversampling {
            body.insert("oversampling".to_string(), json!(oversampling));
        }
        serde_json::Value::Object(body)
    }

    /// Builds a named-space query. Recall controls are rejected before this
    /// point, so no dense-only `params` are emitted for named spaces.
    pub(crate) fn named_search_body(
        query: &VectorQuery,
        target: &str,
        data: &VectorData,
    ) -> Result<serde_json::Value> {
        let (vector, _) = Self::query_body(data);
        let mut body = json!({
            "query": vector,
            "using": target,
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
        Ok(body)
    }

    /// Builds the REST request body for a query, including the optional
    /// recall-tuning params. Dense queries keep the exact legacy shape.
    pub(crate) fn search_body(
        query: &VectorQuery,
        using: Option<&str>,
    ) -> Result<serde_json::Value> {
        Self::validate_search_params(query)?;
        let (vector, inferred_name) = Self::query_body(&query.vector);
        let mut body = json!({
            "query": vector,
            "limit": query.limit,
            "with_payload": true,
        });
        let slot = using.or(inferred_name.as_deref());
        if let Some(name) = slot {
            body["using"] = json!(name);
        }
        if let Some(threshold) = query.score_threshold {
            body["score_threshold"] = json!(threshold);
        }
        if let Some(filter) = &query.filter {
            if let Some(mapped) = Self::build_filter(filter)? {
                body["filter"] = mapped;
            }
        }
        if query.exact.is_some() || query.hnsw_ef.is_some() || query.quantization.is_some() {
            let mut params = serde_json::Map::new();
            if let Some(exact) = query.exact {
                params.insert("exact".to_string(), json!(exact));
            }
            if let Some(hnsw_ef) = query.hnsw_ef {
                params.insert("hnsw_ef".to_string(), json!(hnsw_ef));
            }
            if let Some(quantization) = &query.quantization {
                params.insert(
                    "quantization".to_string(),
                    Self::quantization_body(quantization),
                );
            }
            body["params"] = serde_json::Value::Object(params);
        }
        Ok(body)
    }

    /// Parses one scroll response into points plus the next cursor.
    pub(crate) fn parse_scroll_page_with_spaces(
        text: &str,
        spaces: Option<&BTreeMap<String, VectorSpaceSpec>>,
    ) -> Result<(Vec<VectorPoint>, Option<String>)> {
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
            let (vector, named) = Self::parse_point_vector(vector_value, spaces)?;
            points.push(VectorPoint {
                id: pid.to_string(),
                vector,
                named,
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
    pub(crate) fn scroll_offset(offset: &str) -> String {
        if uuid::Uuid::parse_str(offset).is_ok() {
            offset.to_string()
        } else {
            Self::native_id(offset)
        }
    }

    /// Parses a stored vector in either dense-array or named-map form.
    pub(crate) fn parse_vector(value: &serde_json::Value) -> Result<Vec<f32>> {
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

    /// Parses a point vector field, preserving named maps when the
    /// collection declares named spaces.
    pub(crate) fn parse_point_vector(
        value: &serde_json::Value,
        spaces: Option<&BTreeMap<String, VectorSpaceSpec>>,
    ) -> Result<(VectorData, Option<BTreeMap<String, VectorData>>)> {
        let Some(map) = value.as_object() else {
            return Ok((Self::parse_vector_data(value)?, None));
        };
        if map.contains_key("indices") && map.contains_key("values") {
            return Ok((Self::parse_vector_data(value)?, None));
        }
        let is_named = spaces
            .map(|spaces| map.len() > 1 && map.keys().all(|name| spaces.contains_key(name)))
            .unwrap_or_else(|| map.len() > 1);
        if !is_named {
            let candidate = map
                .get("default")
                .or_else(|| map.values().next())
                .unwrap_or(value);
            return Ok((Self::parse_vector_data(candidate)?, None));
        }
        let mut named = BTreeMap::new();
        for (name, data) in map {
            named.insert(name.clone(), Self::parse_vector_data(data)?);
        }
        Ok((VectorData::Dense(Vec::new()), Some(named)))
    }

    /// Parses a point's `vector` field into a kind-shaped payload. Sparse
    /// objects are preserved as index/value pairs; nested arrays become
    /// multi vectors; everything else falls back to a dense vector.
    pub(crate) fn parse_vector_data(value: &serde_json::Value) -> Result<VectorData> {
        let candidate = if let Some(map) = value.as_object() {
            if map.contains_key("indices") && map.contains_key("values") {
                let indices = map
                    .get("indices")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        VectorError::Decode("qdrant sparse indices invalid".to_string())
                    })?
                    .iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u32))
                    .collect();
                let values = map
                    .get("values")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| VectorError::Decode("qdrant sparse values invalid".to_string()))?
                    .iter()
                    .filter_map(|v| v.as_f64().map(|n| n as f32))
                    .collect();
                return Ok(VectorData::Sparse { indices, values });
            }
            map.get("default")
                .or_else(|| map.values().next())
                .unwrap_or(value)
        } else {
            value
        };
        if let Some(array) = candidate.as_array() {
            if array.first().is_some_and(|v| v.is_array()) {
                let mut vectors = Vec::with_capacity(array.len());
                for item in array {
                    vectors.push(Self::parse_vector(item)?);
                }
                return Ok(VectorData::Multi(vectors));
            }
        }
        Ok(VectorData::Dense(Self::parse_vector(candidate)?))
    }

    /// Normalizes Euclid scores to the contracts convention (higher is
    /// better, negated distance matching the in-process backend). Cosine
    /// and dot pass through. Positive Euclid raws are assumed to be either
    /// `1/(1+distance)` (at most one) or a raw distance and are converted
    /// to negated distance; non-positive raws already follow the convention.
    pub(crate) fn normalize_score(raw: f32, distance: Option<DistanceMetric>) -> f32 {
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
}
