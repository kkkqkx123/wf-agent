use super::*;

/// Sub-vector admission limit used when a caller leaves it unset. Matches the
/// engine's own default so a collection created here and one created by the
/// engine directly stay compatible.
const DEFAULT_MULTI_VECTORS: usize = 16;

impl SimvecVectorStore {
    pub(crate) fn map_distance(distance: crate::types::DistanceMetric) -> simvec::DistanceMetric {
        match distance {
            crate::types::DistanceMetric::Cosine => simvec::DistanceMetric::Cosine,
            crate::types::DistanceMetric::Dot => simvec::DistanceMetric::Dot,
            crate::types::DistanceMetric::Euclid => simvec::DistanceMetric::Euclid,
        }
    }

    /// Maps a contracts payload to the engine's payload enum, field for field.
    pub(crate) fn map_vector_data(data: &VectorData) -> Result<simvec::VectorData> {
        Ok(match data {
            VectorData::Dense(vector) => simvec::VectorData::dense(vector.clone()),
            VectorData::Sparse { indices, values } => {
                simvec::VectorData::sparse(indices.clone(), values.clone())
            }
            VectorData::Multi(vectors) => simvec::VectorData::multi(vectors.clone()),
        })
    }

    /// Maps an engine payload back to the contracts payload enum.
    pub(crate) fn map_engine_data(data: simvec::VectorData) -> VectorData {
        match data {
            simvec::VectorData::Dense(vector) => VectorData::Dense(vector),
            simvec::VectorData::Sparse { indices, values } => {
                VectorData::Sparse { indices, values }
            }
            // simvec keeps multi-vectors in one contiguous buffer to avoid an
            // allocation per sub-vector; the contracts payload keeps them
            // nested, so chunk the buffer back by sub-vector dimension.
            simvec::VectorData::Multi(multi) => {
                VectorData::Multi(multi.vectors().map(<[f32]>::to_vec).collect())
            }
        }
    }

    pub(crate) fn map_compression(
        compression: crate::types::CompressionRatio,
    ) -> simvec::CompressionRatio {
        match compression {
            crate::types::CompressionRatio::X4 => simvec::CompressionRatio::X4,
            crate::types::CompressionRatio::X8 => simvec::CompressionRatio::X8,
            crate::types::CompressionRatio::X16 => simvec::CompressionRatio::X16,
            crate::types::CompressionRatio::X32 => simvec::CompressionRatio::X32,
            crate::types::CompressionRatio::X64 => simvec::CompressionRatio::X64,
        }
    }

    pub(crate) fn map_quantization(
        quantization: &crate::types::QuantizationConfig,
    ) -> simvec::QuantizationConfig {
        match quantization {
            crate::types::QuantizationConfig::Scalar { quantile } => {
                simvec::QuantizationConfig::scalar(quantile.unwrap_or(0.99))
            }
            crate::types::QuantizationConfig::Product { compression } => {
                simvec::QuantizationConfig::product(Self::map_compression(*compression))
            }
            crate::types::QuantizationConfig::Binary => simvec::QuantizationConfig::binary(),
        }
    }

    pub(crate) fn map_hnsw(hnsw: &crate::types::HnswConfig) -> Result<simvec::HnswConfig> {
        if hnsw.m.is_some_and(|m| m == 0) {
            return Err(VectorError::InvalidInput(
                "hnsw m must be positive".to_string(),
            ));
        }
        let mut config = simvec::HnswConfig::default();
        if let Some(m) = hnsw.m {
            config.m = m;
        }
        if let Some(ef_construct) = hnsw.ef_construct {
            config.ef_construct = ef_construct;
        }
        if let Some(on_disk) = hnsw.on_disk {
            config.on_disk = Some(on_disk);
        }
        config.validate().map_err(map_engine_error)?;
        Ok(config)
    }

    /// Maps a contracts kind to the engine kind enum.
    pub(crate) fn map_kind(kind: VectorKind) -> simvec::VectorKind {
        match kind {
            VectorKind::Dense => simvec::VectorKind::Dense,
            VectorKind::Sparse => simvec::VectorKind::Sparse,
            VectorKind::Multi => simvec::VectorKind::Multi,
        }
    }

    /// Maps an engine kind back to the contracts kind enum.
    pub(crate) fn map_engine_kind(kind: simvec::VectorKind) -> VectorKind {
        match kind {
            simvec::VectorKind::Dense => VectorKind::Dense,
            simvec::VectorKind::Sparse => VectorKind::Sparse,
            simvec::VectorKind::Multi => VectorKind::Multi,
        }
    }

    pub(crate) fn map_engine_space(
        &self,
        space: simvec::VectorSpaceConfig,
    ) -> Result<VectorSpaceSpec> {
        let distance = match space.distance {
            simvec::DistanceMetric::Cosine => DistanceMetric::Cosine,
            simvec::DistanceMetric::Dot => DistanceMetric::Dot,
            simvec::DistanceMetric::Euclid => DistanceMetric::Euclid,
            simvec::DistanceMetric::Manhattan => {
                return Err(VectorError::unsupported(
                    self.backend_name(),
                    "named manhattan spaces",
                ))
            }
        };
        Ok(VectorSpaceSpec {
            kind: Self::map_engine_kind(space.kind),
            vector_size: space.vector_size,
            distance,
        })
    }

    pub(crate) fn map_space(space: &VectorSpaceSpec) -> simvec::VectorSpaceConfig {
        // `multi_config` must be present exactly when the kind is multi. Named
        // spaces reject the multi kind during contracts validation, so this
        // only ever fills for a layout that reaches the engine through the
        // single-space path; the width stays at the same default the
        // single-space builder uses.
        let multi_config = match space.kind {
            VectorKind::Multi => Some(simvec::MultiConfig::new(DEFAULT_MULTI_VECTORS)),
            VectorKind::Dense | VectorKind::Sparse => None,
        };
        simvec::VectorSpaceConfig {
            kind: Self::map_kind(space.kind),
            vector_size: space.vector_size,
            distance: Self::map_distance(space.distance),
            multi_config,
        }
    }

    pub(crate) fn named_spaces(
        &self,
        config: &simvec::CollectionConfig,
    ) -> Result<Option<BTreeMap<String, VectorSpaceSpec>>> {
        // simvec keeps every layout in a single space map; a single-space
        // collection is exactly the one whose only key is the default name.
        if !config.is_named_layout() {
            return Ok(None);
        }
        config
            .vectors
            .iter()
            .map(|(name, space)| Ok((name.clone(), self.map_engine_space(space.clone())?)))
            .collect::<Result<BTreeMap<_, _>>>()
            .map(Some)
    }

    /// Converts an engine point, preserving named-space payloads.
    pub(crate) fn map_engine_point(point: simvec::VectorPoint, id_hint: &str) -> VectorPoint {
        let simvec::VectorPoint {
            vectors, payload, ..
        } = point;
        let resolved = payload
            .as_ref()
            .and_then(|payload| payload.get(PID_KEY))
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| id_hint.to_string());
        // A named layout never fills the default slot, so its absence is what
        // separates the two shapes; everything then moves wholesale into the
        // contracts `named` map, leaving the single payload empty.
        if vectors.len() != 1 || !vectors.contains_key(simvec::DEFAULT_VECTOR_NAME) {
            let named = vectors
                .into_iter()
                .map(|(name, data)| (name, Self::map_engine_data(data)))
                .collect();
            return VectorPoint {
                id: resolved,
                vector: VectorData::Dense(Vec::new()),
                named: Some(named),
                payload: payload.unwrap_or_default(),
            };
        }
        let vector = vectors
            .into_iter()
            .next()
            .map(|(_, data)| Self::map_engine_data(data))
            .unwrap_or(VectorData::Dense(Vec::new()));
        VectorPoint {
            id: resolved,
            vector,
            named: None,
            payload: payload.unwrap_or_default(),
        }
    }

    /// Builds the engine config for a non-dense collection spec.
    pub(crate) fn kind_collection_config(
        &self,
        spec: &CollectionSpec,
    ) -> Result<simvec::CollectionConfig> {
        let config = match spec.kind {
            VectorKind::Sparse => {
                // Sparse indices are open `u32` ids: the engine validates them
                // against its own bound and the collection therefore declares
                // no vocabulary size. The contracts-level `sparse_max_index`
                // stays a caller-side admission limit only.
                simvec::CollectionConfig::with_sparse(
                    simvec::SPARSE_DIM_BOUND,
                    Self::map_distance(spec.distance),
                )
            }
            VectorKind::Multi => simvec::CollectionConfig::with_multi(
                spec.dimension,
                Self::map_distance(spec.distance),
                simvec::MultiConfig::new(spec.multi_max_vectors.unwrap_or(DEFAULT_MULTI_VECTORS)),
            ),
            VectorKind::Dense => unreachable!("dense configs use the dense path"),
        };
        Ok(config)
    }
}
