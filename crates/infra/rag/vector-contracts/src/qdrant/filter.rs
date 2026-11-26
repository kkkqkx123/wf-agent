use super::*;

impl QdrantVectorStore {
    /// Builds the Qdrant filter object for a contracts filter.
    pub fn build_filter(filter: &SearchFilter) -> Result<Option<serde_json::Value>> {
        let mut must = Vec::new();
        let mut must_not = Vec::new();
        if !filter.has_ids.is_empty() {
            let native: Vec<serde_json::Value> = filter
                .has_ids
                .iter()
                .map(|id| serde_json::Value::String(Self::native_id(id)))
                .collect();
            must.push(json!({"has_id": native}));
        }
        if let Some(namespace) = &filter.namespace {
            must.push(Self::text_match_condition(NAMESPACE_KEY, namespace));
        }
        if let Some(source) = &filter.source {
            must.push(Self::text_match_condition(SOURCE_KEY, source));
        }
        for (key, value) in &filter.fields {
            must.push(Self::text_match_condition(key, value));
        }
        for condition in &filter.conditions {
            let (clause, negated) = Self::build_condition(condition)?;
            if negated {
                must_not.push(clause);
            } else {
                must.push(clause);
            }
        }
        let mut filter_obj = serde_json::Map::new();
        if !must.is_empty() {
            filter_obj.insert("must".to_string(), json!(must));
        }
        if !must_not.is_empty() {
            filter_obj.insert("must_not".to_string(), json!(must_not));
        }
        if !filter.should.is_empty() {
            let mut branch = Vec::with_capacity(filter.should.len());
            for condition in &filter.should {
                let (clause, negated) = Self::build_condition(condition)?;
                if negated {
                    branch.push(json!({"must_not": [clause]}));
                } else {
                    branch.push(clause);
                }
            }
            filter_obj.insert("should".to_string(), json!(branch));
        }
        if let Some(min_should) = &filter.min_should {
            let mut branch = Vec::with_capacity(min_should.conditions.len());
            for condition in &min_should.conditions {
                let (clause, negated) = Self::build_condition(condition)?;
                if negated {
                    branch.push(json!({"must_not": [clause]}));
                } else {
                    branch.push(clause);
                }
            }
            filter_obj.insert(
                "min_should".to_string(),
                json!({"conditions": branch, "min_count": min_should.min_count}),
            );
        }
        if filter_obj.is_empty() {
            return Ok(None);
        }
        Ok(Some(serde_json::Value::Object(filter_obj)))
    }

    /// Text equality follows the contracts text form: a string condition
    /// also matches numeric and boolean payloads whose canonical text
    /// equals the condition. Typed variants are ORed inside one `should`
    /// group; plain strings stay a single match.
    pub(crate) fn text_match_condition(key: &str, text: &str) -> serde_json::Value {
        let mut variants = vec![json!({"key": key, "match": {"value": text}})];
        if let Ok(int_value) = text.parse::<i64>() {
            if int_value.to_string() == text {
                variants.push(json!({"key": key, "match": {"value": int_value}}));
            }
        } else if let Some(float_value) = text.parse::<f64>().ok().filter(|v| v.is_finite()) {
            let canonical = serde_json::Number::from_f64(float_value)
                .map(|n| n.to_string())
                .unwrap_or_default();
            if canonical == text {
                variants
                    .push(json!({"key": key, "range": {"gte": float_value, "lte": float_value}}));
            }
        }
        if text == "true" || text == "false" {
            variants.push(json!({"key": key, "match": {"value": text == "true"}}));
        }
        if variants.len() == 1 {
            variants.pop().expect("single variant")
        } else {
            json!({"should": variants})
        }
    }

    pub(crate) fn build_condition(
        condition: &crate::filter::FieldFilter,
    ) -> Result<(serde_json::Value, bool)> {
        use crate::filter::FilterOperator::*;
        let key = &condition.key;
        match (&condition.operator, &condition.value) {
            (Equal, crate::filter::FilterValue::Text(text)) => {
                Ok((Self::text_match_condition(key, text), false))
            }
            (Equal, crate::filter::FilterValue::Number(number)) => {
                if number.fract() == 0.0 {
                    Ok((
                        json!({"key": key, "match": {"value": *number as i64}}),
                        false,
                    ))
                } else {
                    Ok((
                        json!({"key": key, "range": {"gte": number, "lte": number}}),
                        false,
                    ))
                }
            }
            (Equal, crate::filter::FilterValue::Bool(flag)) => {
                Ok((json!({"key": key, "match": {"value": flag}}), false))
            }
            (NotEqual, crate::filter::FilterValue::Text(text)) => {
                Ok((Self::text_match_condition(key, text), true))
            }
            (NotEqual, crate::filter::FilterValue::Number(number)) => {
                if number.fract() == 0.0 {
                    Ok((
                        json!({"key": key, "match": {"value": *number as i64}}),
                        true,
                    ))
                } else {
                    Ok((
                        json!({"key": key, "range": {"gte": number, "lte": number}}),
                        true,
                    ))
                }
            }
            (NotEqual, crate::filter::FilterValue::Bool(flag)) => {
                Ok((json!({"key": key, "match": {"value": flag}}), true))
            }
            (GreaterThan, crate::filter::FilterValue::Number(bound)) => {
                Ok((json!({"key": key, "range": {"gt": bound}}), false))
            }
            (GreaterEqual, crate::filter::FilterValue::Number(bound)) => {
                Ok((json!({"key": key, "range": {"gte": bound}}), false))
            }
            (LessThan, crate::filter::FilterValue::Number(bound)) => {
                Ok((json!({"key": key, "range": {"lt": bound}}), false))
            }
            (LessEqual, crate::filter::FilterValue::Number(bound)) => {
                Ok((json!({"key": key, "range": {"lte": bound}}), false))
            }
            (Prefix, crate::filter::FilterValue::Text(prefix)) => {
                Ok((json!({"key": key, "match": {"prefix": prefix}}), false))
            }
            (Contains, crate::filter::FilterValue::Text(part)) => {
                Ok((json!({"key": key, "match": {"substring": part}}), false))
            }
            (IsEmpty, _) => Ok((
                json!({
                    "should": [
                        {"key": key, "match": {"value": ""}},
                        {"must": [{"is_empty": {"key": key}}], "must_not": [{"is_null": {"key": key}}]}
                    ]
                }),
                false,
            )),
            (IsNull, _) => Ok((
                json!({"should": [{"is_null": {"key": key}}, {"is_empty": {"key": key}}]}),
                false,
            )),
            (ValuesEqual, crate::filter::FilterValue::Number(bound)) => {
                let count = Self::count_bound(*bound)?;
                Ok((
                    json!({"key": key, "values_count": {"gte": count, "lte": count}}),
                    false,
                ))
            }
            (ValuesGreaterThan, crate::filter::FilterValue::Number(bound)) => {
                let count = Self::count_bound_floor(*bound)?;
                Ok((json!({"key": key, "values_count": {"gt": count}}), false))
            }
            (ValuesGreaterEqual, crate::filter::FilterValue::Number(bound)) => {
                let count = Self::count_bound_ceil(*bound)?;
                Ok((json!({"key": key, "values_count": {"gte": count}}), false))
            }
            (ValuesLessThan, crate::filter::FilterValue::Number(bound)) => {
                let count = Self::count_bound_ceil(*bound)?;
                Ok((json!({"key": key, "values_count": {"lt": count}}), false))
            }
            (ValuesLessEqual, crate::filter::FilterValue::Number(bound)) => {
                let count = Self::count_bound_floor(*bound)?;
                Ok((json!({"key": key, "values_count": {"lte": count}}), false))
            }
            _ => Err(VectorError::InvalidInput(format!(
                "unsupported filter condition on '{key}'"
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

    /// Integer bound for a strict lower length comparison.
    pub(crate) fn count_bound_floor(bound: f64) -> Result<u64> {
        if !bound.is_finite() || bound < 0.0 {
            return Err(VectorError::InvalidInput(format!(
                "array length bound must be non-negative, got {bound}"
            )));
        }
        Ok(bound.floor() as u64)
    }

    /// Integer bound for a lower-or-equal length comparison.
    pub(crate) fn count_bound_ceil(bound: f64) -> Result<u64> {
        if !bound.is_finite() || bound < 0.0 {
            return Err(VectorError::InvalidInput(format!(
                "array length bound must be non-negative, got {bound}"
            )));
        }
        Ok(bound.ceil() as u64)
    }
}
