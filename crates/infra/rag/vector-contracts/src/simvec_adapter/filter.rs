use super::*;

impl SimvecVectorStore {
    pub(crate) fn validate_search_params(query: &VectorQuery) -> Result<()> {
        if query.hnsw_ef == Some(0) {
            return Err(VectorError::InvalidInput(
                "hnsw_ef must be positive".to_string(),
            ));
        }
        if let Some(params) = &query.quantization {
            if params
                .oversampling
                .is_some_and(|v| !v.is_finite() || v < 1.0)
            {
                return Err(VectorError::InvalidInput(format!(
                    "quantization oversampling {} must be >= 1.0",
                    params.oversampling.unwrap_or(0.0)
                )));
            }
        }
        if query.vector.as_dense()?.iter().any(|v| !v.is_finite()) {
            return Err(VectorError::InvalidInput(
                "query vector contains non-finite elements".to_string(),
            ));
        }
        Ok(())
    }

    pub(crate) fn map_filter(filter: &SearchFilter) -> Result<Option<simvec::VectorFilter>> {
        let mut mapped = simvec::VectorFilter::new();
        let mut empty = true;
        if let Some(namespace) = &filter.namespace {
            mapped = mapped.must(simvec::FilterCondition::match_value(
                NAMESPACE_KEY,
                namespace.clone(),
            ));
            empty = false;
        }
        if let Some(source) = &filter.source {
            mapped = mapped.must(simvec::FilterCondition::match_value(
                SOURCE_KEY,
                source.clone(),
            ));
            empty = false;
        }
        for (key, value) in &filter.fields {
            mapped = mapped.must(simvec::FilterCondition::match_value(
                key.clone(),
                value.clone(),
            ));
            empty = false;
        }
        for condition in &filter.conditions {
            let (clause, negated) = Self::map_condition(condition)?;
            mapped = if negated {
                mapped.must_not(clause)
            } else {
                mapped.must(clause)
            };
            empty = false;
        }
        for condition in &filter.should {
            mapped = mapped.should(Self::map_branch_condition(condition)?);
            empty = false;
        }
        if let Some(min_should) = &filter.min_should {
            let conditions = min_should
                .conditions
                .iter()
                .map(Self::map_branch_condition)
                .collect::<Result<Vec<_>>>()?;
            mapped.min_should = Some(simvec::MinShouldCondition {
                conditions,
                min_count: min_should.min_count,
            });
            empty = false;
        }
        if !filter.has_ids.is_empty() {
            let ids = filter.has_ids.iter().map(|id| Self::point_id(id)).collect();
            mapped = mapped.must(simvec::FilterCondition::has_id(ids));
            empty = false;
        }
        if empty {
            Ok(None)
        } else {
            Ok(Some(mapped))
        }
    }

    /// Maps one branch condition, wrapping negations so OR groups keep
    /// their meaning (`must_not` would AND with the group instead).
    pub(crate) fn map_branch_condition(condition: &FieldFilter) -> Result<simvec::FilterCondition> {
        let (clause, negated) = Self::map_condition(condition)?;
        if negated {
            Ok(simvec::FilterCondition::new(
                condition.key.clone(),
                simvec::ConditionType::Not {
                    condition: Box::new(clause),
                },
            ))
        } else {
            Ok(clause)
        }
    }

    pub(crate) fn map_condition(
        condition: &FieldFilter,
    ) -> Result<(simvec::FilterCondition, bool)> {
        use FilterOperator::*;
        match (&condition.operator, &condition.value) {
            (Equal, value) => {
                let text = value.canonical_text().ok_or_else(|| {
                    VectorError::InvalidInput(format!(
                        "unsupported filter value on '{}'",
                        condition.key
                    ))
                })?;
                Ok((
                    simvec::FilterCondition::match_value(condition.key.clone(), text),
                    false,
                ))
            }
            (NotEqual, value) => {
                let text = value.canonical_text().ok_or_else(|| {
                    VectorError::InvalidInput(format!(
                        "unsupported filter value on '{}'",
                        condition.key
                    ))
                })?;
                Ok((
                    simvec::FilterCondition::match_value(condition.key.clone(), text),
                    true,
                ))
            }
            (GreaterThan, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::range(
                    condition.key.clone(),
                    simvec::RangeCondition::new().gt(*bound),
                ),
                false,
            )),
            (GreaterEqual, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::range(
                    condition.key.clone(),
                    simvec::RangeCondition::new().gte(*bound),
                ),
                false,
            )),
            (LessThan, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::range(
                    condition.key.clone(),
                    simvec::RangeCondition::new().lt(*bound),
                ),
                false,
            )),
            (LessEqual, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::range(
                    condition.key.clone(),
                    simvec::RangeCondition::new().lte(*bound),
                ),
                false,
            )),
            (Prefix, crate::filter::FilterValue::Text(prefix)) => Ok((
                simvec::FilterCondition::new(
                    condition.key.clone(),
                    simvec::ConditionType::StartsWith {
                        prefix: prefix.clone(),
                    },
                ),
                false,
            )),
            (Contains, crate::filter::FilterValue::Text(part)) => Ok((
                simvec::FilterCondition::new(
                    condition.key.clone(),
                    simvec::ConditionType::Contains {
                        value: part.clone(),
                    },
                ),
                false,
            )),
            (IsEmpty, _) => Ok((
                simvec::FilterCondition::is_empty(condition.key.clone()),
                false,
            )),
            (IsNull, _) => Ok((
                simvec::FilterCondition::is_null(condition.key.clone()),
                false,
            )),
            (ValuesEqual, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::values_count(
                    condition.key.clone(),
                    simvec::ValuesCountCondition::new()
                        .gte(Self::count_bound(*bound)?)
                        .lte(Self::count_bound(*bound)?),
                ),
                false,
            )),
            (ValuesGreaterThan, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::values_count(
                    condition.key.clone(),
                    simvec::ValuesCountCondition::new().gt(Self::count_bound_floor(*bound)?),
                ),
                false,
            )),
            (ValuesGreaterEqual, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::values_count(
                    condition.key.clone(),
                    simvec::ValuesCountCondition::new().gte(Self::count_bound_ceil(*bound)?),
                ),
                false,
            )),
            (ValuesLessThan, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::values_count(
                    condition.key.clone(),
                    simvec::ValuesCountCondition::new().lt(Self::count_bound_ceil(*bound)?),
                ),
                false,
            )),
            (ValuesLessEqual, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::values_count(
                    condition.key.clone(),
                    simvec::ValuesCountCondition::new().lte(Self::count_bound_floor(*bound)?),
                ),
                false,
            )),
            _ => Err(VectorError::InvalidInput(format!(
                "unsupported filter condition on '{}'",
                condition.key
            ))),
        }
    }

    /// Integer bound for an exact length comparison.
    pub(crate) fn count_bound(bound: f64) -> Result<u64> {
        if !bound.is_finite() || bound < 0.0 || bound.fract() != 0.0 {
            return Err(VectorError::InvalidInput(format!(
                "array length bound must be a non-negative integer, got {bound}"
            )));
        }
        Ok(bound as u64)
    }

    /// Integer bound for a strict lower length comparison (`len > bound`
    /// over integers equals `len > floor(bound)`).
    pub(crate) fn count_bound_floor(bound: f64) -> Result<u64> {
        if !bound.is_finite() || bound < 0.0 {
            return Err(VectorError::InvalidInput(format!(
                "array length bound must be non-negative, got {bound}"
            )));
        }
        Ok(bound.floor() as u64)
    }

    /// Integer bound for a lower-or-equal length comparison (`len >= bound`
    /// over integers equals `len >= ceil(bound)`).
    pub(crate) fn count_bound_ceil(bound: f64) -> Result<u64> {
        if !bound.is_finite() || bound < 0.0 {
            return Err(VectorError::InvalidInput(format!(
                "array length bound must be non-negative, got {bound}"
            )));
        }
        Ok(bound.ceil() as u64)
    }
}
