use crate::domain::keys::SCHEMA_VERSION_EXCLUDE_PATTERN;
use crate::domain::store::{prefix_like_pattern, FilterBindValue, FilterCondition, QueryFilter};
pub fn build_select_sql(
    filter: Option<&QueryFilter>,
    table_name: &str,
    select_columns: &str,
) -> (String, Vec<FilterBindValue>) {
    let plan = filter.map(|f| f.compile());
    let mut sql = format!("SELECT {} FROM {}", select_columns, table_name);
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<FilterBindValue> = Vec::new();

    // Exclude internal schema version records from application queries.
    conditions.push(format!("id NOT LIKE '{}'", SCHEMA_VERSION_EXCLUDE_PATTERN));

    if let Some(p) = plan.as_ref() {
        for op in &p.conditions {
            match op {
                FilterCondition::Eq(key, value) => {
                    conditions.push(format!("metadata->>'{}' = ${}", key, params.len() + 1));
                    params.push(FilterBindValue::S(value.clone()));
                }
                FilterCondition::Id(id) => {
                    conditions.push(format!("id = ${}", params.len() + 1));
                    params.push(FilterBindValue::S(id.clone()));
                }
                FilterCondition::IdPrefix(prefix) => {
                    conditions.push(format!("id LIKE ${} ESCAPE '\\'", params.len() + 1));
                    params.push(FilterBindValue::S(prefix_like_pattern(prefix)));
                }
                FilterCondition::Prefix(key, prefix) => {
                    conditions.push(format!(
                        "metadata->>'{}' LIKE ${} ESCAPE '\\'",
                        key,
                        params.len() + 1
                    ));
                    params.push(FilterBindValue::S(prefix_like_pattern(prefix)));
                }
                FilterCondition::Lt(key, value) => {
                    conditions.push(format!(
                        "(jsonb_typeof(metadata->'{}') = 'number' AND (metadata->>'{}')::float8 < ${})",
                        key,
                        key,
                        params.len() + 1
                    ));
                    params.push(FilterBindValue::I(*value));
                }
                FilterCondition::Gt(key, value) => {
                    conditions.push(format!(
                        "(jsonb_typeof(metadata->'{}') = 'number' AND (metadata->>'{}')::float8 > ${})",
                        key,
                        key,
                        params.len() + 1
                    ));
                    params.push(FilterBindValue::I(*value));
                }
                FilterCondition::Between(key, start, end) => {
                    conditions.push(format!(
                        "(jsonb_typeof(metadata->'{}') = 'number' AND (metadata->>'{}')::float8 >= ${} AND (metadata->>'{}')::float8 <= ${})",
                        key,
                        key,
                        params.len() + 1,
                        key,
                        params.len() + 2
                    ));
                    params.push(FilterBindValue::I(*start));
                    params.push(FilterBindValue::I(*end));
                }
                FilterCondition::In(key, values) => {
                    if values.is_empty() {
                        conditions.push("false".into());
                    } else {
                        let placeholders: Vec<String> = (0..values.len())
                            .map(|i| format!("${}", params.len() + i + 1))
                            .collect();
                        conditions.push(format!(
                            "metadata->>'{}' IN ({})",
                            key,
                            placeholders.join(", ")
                        ));
                        params.extend(values.iter().cloned().map(FilterBindValue::S));
                    }
                }
            }
        }
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    if let Some((key, descending)) = plan.as_ref().and_then(|p| p.order_by.clone()) {
        // Numeric-aware ordering: numeric metadata values sort by their
        // numeric value, everything else falls back to NULL (excluded from
        // numeric comparison) so a cast can never fail the query. NULLS LAST
        // keeps the direction predictable for both ASC and DESC; the trailing
        // id follows the primary direction so ties resolve to creation order
        // on both newest-first and oldest-first queries.
        let direction = if descending { "DESC" } else { "ASC" };
        sql.push_str(&format!(
            " ORDER BY (CASE WHEN jsonb_typeof(metadata->'{}') = 'number' THEN (metadata->>'{}')::float8 END) {} NULLS LAST, id {}",
            key,
            key,
            direction,
            direction
        ));
    }
    if let Some(limit) = plan.as_ref().and_then(|p| p.limit) {
        sql.push_str(&format!(" LIMIT {}", limit));
    }
    if let Some(offset) = plan.as_ref().and_then(|p| p.offset) {
        sql.push_str(&format!(" OFFSET {}", offset));
    }

    (sql, params)
}
