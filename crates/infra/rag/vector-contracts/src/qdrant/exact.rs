use super::*;

impl QdrantVectorStore {
    /// Whether a filter needs client-side exact filtering because its
    /// remote translation is an over-approximation.
    pub(crate) fn filter_needs_exact_match(filter: Option<&SearchFilter>) -> bool {
        let Some(filter) = filter else {
            return false;
        };
        fn condition_needs_exact(condition: &FieldFilter) -> bool {
            matches!(
                condition.operator,
                FilterOperator::IsNull | FilterOperator::Contains
            )
        }
        filter.conditions.iter().any(condition_needs_exact)
            || filter.should.iter().any(condition_needs_exact)
            || filter
                .min_should
                .as_ref()
                .is_some_and(|clause| clause.conditions.iter().any(condition_needs_exact))
    }

    /// Scores a filtered page with the contracts scoring convention so an
    /// exact client-side search can reuse the paginated scroll path.
    pub(crate) fn exact_hits(
        points: Vec<VectorPoint>,
        query: &VectorQuery,
        info: &CachedCollection,
    ) -> Result<Vec<ScoredPoint>> {
        if let Some(spaces) = &info.spaces {
            return Self::exact_named_hits(points, query, spaces);
        }
        let query_vector = match &query.vector {
            VectorData::Dense(vector) => vector.as_slice(),
            other => {
                return Err(VectorError::unsupported(
                    "qdrant exact filter rescore",
                    other.kind().as_name(),
                ))
            }
        };
        let mut hits = Vec::with_capacity(points.len());
        for point in points {
            if query
                .filter
                .as_ref()
                .is_some_and(|filter| !filter.matches_point(&point.id, &point.payload))
            {
                continue;
            }
            let point_vector = match &point.vector {
                VectorData::Dense(vector) => vector.as_slice(),
                other => {
                    return Err(VectorError::unsupported(
                        "qdrant exact filter rescore",
                        other.kind().as_name(),
                    ))
                }
            };
            if point_vector.len() != info.dimension {
                return Err(VectorError::DimensionMismatch {
                    expected: info.dimension,
                    actual: point_vector.len(),
                });
            }
            let score = InMemoryVectorStore::similarity(info.distance, query_vector, point_vector);
            if query
                .score_threshold
                .is_some_and(|threshold| score < threshold)
            {
                continue;
            }
            hits.push(ScoredPoint {
                id: point.id,
                score,
                payload: point.payload,
            });
        }
        Ok(hits)
    }

    pub(crate) fn exact_named_hits(
        points: Vec<VectorPoint>,
        query: &VectorQuery,
        spaces: &BTreeMap<String, VectorSpaceSpec>,
    ) -> Result<Vec<ScoredPoint>> {
        query.validate_named(spaces)?;
        let target = query.target_space.as_deref().ok_or_else(|| {
            VectorError::InvalidInput("named query has no target space".to_string())
        })?;
        let space = spaces.get(target).ok_or_else(|| {
            VectorError::InvalidInput(format!("unknown named vector space '{target}'"))
        })?;
        let query_data = query
            .named_vector
            .as_ref()
            .ok_or_else(|| VectorError::InvalidInput("named query has no vector".to_string()))?;
        let mut hits = Vec::with_capacity(points.len());
        for point in points {
            if query
                .filter
                .as_ref()
                .is_some_and(|filter| !filter.matches_point(&point.id, &point.payload))
            {
                continue;
            }
            let named = point.named.as_ref().ok_or_else(|| {
                VectorError::Decode(format!("qdrant point '{}' has no named vectors", point.id))
            })?;
            let stored = named.get(target).ok_or_else(|| {
                VectorError::Decode(format!(
                    "qdrant point '{}' is missing space '{target}'",
                    point.id
                ))
            })?;
            let score = match (query_data, stored) {
                (VectorData::Dense(left), VectorData::Dense(right)) => {
                    InMemoryVectorStore::similarity(space.distance, left, right)
                }
                (
                    VectorData::Sparse {
                        indices: left,
                        values: left_values,
                    },
                    VectorData::Sparse {
                        indices: right,
                        values: right_values,
                    },
                ) => {
                    let mut score = 0.0f32;
                    let (mut i, mut j) = (0usize, 0usize);
                    while i < left.len() && j < right.len() {
                        match left[i].cmp(&right[j]) {
                            std::cmp::Ordering::Less => i += 1,
                            std::cmp::Ordering::Greater => j += 1,
                            std::cmp::Ordering::Equal => {
                                score += left_values[i] * right_values[j];
                                i += 1;
                                j += 1;
                            }
                        }
                    }
                    score
                }
                _ => {
                    return Err(VectorError::InvalidInput(format!(
                        "space '{target}' query and stored vector kinds differ"
                    )))
                }
            };
            if query
                .score_threshold
                .is_some_and(|threshold| score < threshold)
            {
                continue;
            }
            hits.push(ScoredPoint {
                id: point.id,
                score,
                payload: point.payload,
            });
        }
        Ok(hits)
    }

    /// Exact search path for filters whose remote translation can only
    /// over-approximate. It walks the collection instead of relying on a
    /// bounded server-side candidate limit.
    pub(crate) async fn search_exact(
        &self,
        collection: &str,
        query: &VectorQuery,
        info: CachedCollection,
    ) -> Result<Vec<ScoredPoint>> {
        let mut offset: Option<String> = None;
        let mut hits = Vec::new();
        loop {
            let mut scroll_query = ScrollQuery::new(256);
            if let Some(filter) = &query.filter {
                scroll_query = scroll_query.with_filter(filter.clone());
            }
            if let Some(cursor) = &offset {
                scroll_query = scroll_query.with_offset(cursor.clone());
            }
            let page = self.scroll(collection, &scroll_query).await?;
            hits.extend(Self::exact_hits(page.points, query, &info)?);
            match page.next_offset {
                Some(next) => offset = Some(next),
                None => break,
            }
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(query.limit);
        Ok(hits)
    }
}
