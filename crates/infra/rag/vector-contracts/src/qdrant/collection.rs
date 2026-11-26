use super::*;

impl QdrantVectorStore {
    pub(crate) fn cached(&self, collection: &str) -> Option<CachedCollection> {
        self.collections.read().get(collection).cloned()
    }

    pub(crate) fn remember(
        &self,
        collection: &str,
        dimension: usize,
        distance: DistanceMetric,
        kind: VectorKind,
        vector_name: Option<String>,
        spaces: Option<BTreeMap<String, VectorSpaceSpec>>,
    ) {
        self.collections.write().insert(
            collection.to_string(),
            CachedCollection {
                dimension,
                distance,
                kind,
                vector_name,
                spaces,
            },
        );
    }

    pub(crate) async fn collection_config(&self, collection: &str) -> Result<CachedCollection> {
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
        let (dimension, distance, kind, vector_name) = Self::parse_collection_info(&body)?;
        let spaces = Self::parse_collection_spaces(&body)?;
        self.remember(
            collection,
            dimension,
            distance,
            kind,
            vector_name.clone(),
            spaces.clone(),
        );
        Ok(CachedCollection {
            dimension,
            distance,
            kind,
            vector_name,
            spaces,
        })
    }

    pub(crate) async fn ensure_payload_indexes(&self, collection: &str) {
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
                    tracing::warn!(backend = "qdrant",
                        collection = collection,
                        field = field,
                        status = %status,
                        body = %diagnostic_body(&body),
                        "qdrant index creation reported {status}"
                    );
                }
                Err(err) => {
                    tracing::warn!(backend = "qdrant",
                        collection = collection,
                        field = field,
                        error = %err,
                        "qdrant index creation failed"
                    );
                }
            }
        }
    }

    /// Qdrant does not expose sparse distances in its REST config, so
    /// sparse spaces compare by name and kind only.
    pub(crate) fn named_spaces_compatible(
        requested: &BTreeMap<String, VectorSpaceSpec>,
        parsed: &BTreeMap<String, VectorSpaceSpec>,
    ) -> bool {
        if requested.len() != parsed.len() {
            return false;
        }
        requested.iter().all(|(name, expected)| {
            parsed.get(name).is_some_and(|actual| match expected.kind {
                VectorKind::Dense => actual == expected,
                VectorKind::Sparse => actual.kind == VectorKind::Sparse && actual.vector_size > 0,
                VectorKind::Multi => false,
            })
        })
    }

    pub(crate) fn validate_collection_spec(spec: &CollectionSpec) -> Result<()> {
        if let Some(quantization) = &spec.quantization {
            match quantization {
                crate::types::QuantizationConfig::Scalar { quantile } => {
                    if let Some(quantile) = quantile {
                        if !quantile.is_finite() || *quantile <= 0.0 || *quantile > 1.0 {
                            return Err(VectorError::InvalidInput(format!(
                                "scalar quantization quantile {quantile} must be in (0,1]"
                            )));
                        }
                    }
                }
                crate::types::QuantizationConfig::Product { .. } => {}
                crate::types::QuantizationConfig::Binary => {}
            }
        }
        if let Some(hnsw) = &spec.hnsw {
            if hnsw.m.is_some_and(|m| m == 0) {
                return Err(VectorError::InvalidInput(
                    "hnsw m must be positive".to_string(),
                ));
            }
            if let (Some(m), Some(ef_construct)) = (hnsw.m, hnsw.ef_construct) {
                let floor = m.saturating_mul(2).max(4);
                if ef_construct < floor {
                    return Err(VectorError::InvalidInput(format!(
                        "hnsw ef_construct {ef_construct} must be >= max(2*m, 4) = {floor} (m = {m})"
                    )));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn collection_quantization_body(
        quantization: &crate::types::QuantizationConfig,
    ) -> serde_json::Value {
        match quantization {
            crate::types::QuantizationConfig::Scalar { quantile } => json!({
                "scalar": {
                    "type": "int8",
                    "quantile": quantile.unwrap_or(0.99),
                    "always_ram": true,
                }
            }),
            crate::types::QuantizationConfig::Product { compression } => json!({
                "product": {
                    "compression": compression.as_name(),
                    "always_ram": true,
                }
            }),
            crate::types::QuantizationConfig::Binary => {
                json!({"binary": {"always_ram": true}})
            }
        }
    }

    pub(crate) fn collection_hnsw_body(hnsw: &crate::types::HnswConfig) -> serde_json::Value {
        let mut body = serde_json::Map::new();
        if let Some(m) = hnsw.m {
            body.insert("m".to_string(), json!(m));
        }
        if let Some(ef_construct) = hnsw.ef_construct {
            body.insert("ef_construct".to_string(), json!(ef_construct));
        }
        if let Some(on_disk) = hnsw.on_disk {
            body.insert("on_disk".to_string(), json!(on_disk));
        }
        serde_json::Value::Object(body)
    }

    pub(crate) fn collection_create_body(spec: &CollectionSpec) -> serde_json::Value {
        let mut body = serde_json::Map::new();
        if let Some(spaces) = &spec.spaces {
            let mut dense = serde_json::Map::new();
            let mut sparse = serde_json::Map::new();
            for (name, space) in spaces {
                match space.kind {
                    VectorKind::Dense => {
                        dense.insert(
                            name.clone(),
                            json!({
                                "size": space.vector_size,
                                "distance": Self::distance_name(space.distance),
                            }),
                        );
                    }
                    VectorKind::Sparse => {
                        sparse.insert(name.clone(), json!({"modifier": "idf"}));
                    }
                    VectorKind::Multi => {
                        // Rejected by `CollectionSpec::validate`.
                        continue;
                    }
                }
            }
            if !dense.is_empty() {
                body.insert("vectors".to_string(), serde_json::Value::Object(dense));
            }
            if !sparse.is_empty() {
                body.insert(
                    "sparse_vectors".to_string(),
                    serde_json::Value::Object(sparse),
                );
            }
            if let Some(on_disk_payload) = spec.on_disk_payload {
                body.insert("on_disk_payload".to_string(), json!(on_disk_payload));
            }
            return serde_json::Value::Object(body);
        }
        match spec.kind {
            VectorKind::Dense | VectorKind::Multi => {
                let mut vector = serde_json::Map::new();
                vector.insert("size".to_string(), json!(spec.dimension));
                vector.insert(
                    "distance".to_string(),
                    json!(Self::distance_name(spec.distance)),
                );
                if spec.kind == VectorKind::Multi {
                    vector.insert(
                        "multivector_config".to_string(),
                        json!({"comparator": "max_sim"}),
                    );
                }
                let vectors = match &spec.vector_name {
                    Some(name) => json!({name: vector}),
                    None => serde_json::Value::Object(vector),
                };
                body.insert("vectors".to_string(), vectors);
            }
            VectorKind::Sparse => {
                let name = spec.vector_name.as_deref().unwrap_or("sparse");
                body.insert(
                    "sparse_vectors".to_string(),
                    json!({name: {"modifier": "idf"}}),
                );
            }
        }
        if let Some(hnsw) = &spec.hnsw {
            body.insert("hnsw_config".to_string(), Self::collection_hnsw_body(hnsw));
        } else if spec.kind == VectorKind::Multi {
            body.insert("hnsw_config".to_string(), json!({"m": 0}));
        }
        if let Some(quantization) = &spec.quantization {
            body.insert(
                "quantization_config".to_string(),
                Self::collection_quantization_body(quantization),
            );
        }
        if let Some(on_disk_payload) = spec.on_disk_payload {
            body.insert("on_disk_payload".to_string(), json!(on_disk_payload));
        }
        serde_json::Value::Object(body)
    }
}
