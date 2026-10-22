use crate::domain::keys::SCHEMA_VERSION_EXCLUDE_PATTERN;
use crate::domain::store::{FilterBindValue, FilterCondition, QueryFilter};

/// Normalizes a metadata value to its text representation, mirroring
/// PostgreSQL's `metadata->>'key'` operator: strings as-is, numbers in
/// canonical decimal form, booleans as 'true' / 'false'. Sqlite's
/// `json_extract` yields 1/0 for booleans, but `json_type` still reports
/// 'true' / 'false', so the boolean arms restore the text form while every
/// other type keeps the plain cast.
pub fn metadata_text_expr(key: &str) -> String {
    format!(
        "(CASE WHEN json_type(metadata, '$.{0}') = 'true' THEN 'true' \
         WHEN json_type(metadata, '$.{0}') = 'false' THEN 'false' \
         ELSE CAST(json_extract(metadata, '$.{0}') AS TEXT) END)",
        key
    )
}

/// True when the metadata value is a JSON number, mirroring PostgreSQL's
/// `jsonb_typeof(...) = 'number'` guard so that numeric predicates never
/// match non-numeric values.
pub fn is_numeric_expr(key: &str) -> String {
    format!("json_type(metadata, '$.{}') IN ('integer', 'real')", key)
}

/// The smallest string greater than every string beginning with `prefix`,
/// making an open-ended prefix match a closed `BINARY` range.
///
/// SQLite's `LIKE` is case-insensitive for ASCII and so cannot use an index
/// ordered `BINARY`; a range of comparisons can. `None` means no string is
/// excluded, which is the case for the empty prefix.
pub fn prefix_upper_bound(prefix: &str) -> Option<String> {
    let mut upper: Vec<char> = prefix.chars().collect();
    while let Some(last) = upper.pop() {
        // A code point with no successor cannot be raised; it is dropped and
        // the one before it carries instead.
        if let Some(next) = char::from_u32(last as u32 + 1) {
            upper.push(next);
            return Some(upper.into_iter().collect());
        }
    }
    None
}

/// Translates a QueryFilter into a complete SELECT statement.
/// Field names come from a fixed metadata schema, so interpolation is safe.
/// The filter is first normalized through the shared compiled plan; only
/// placeholder style and JSON operators are Sqlite-specific. Mirror any
/// semantic change in the PostgreSQL renderer.
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
                    conditions.push(format!("{} = ?", metadata_text_expr(key)));
                    params.push(FilterBindValue::S(value.clone()));
                }
                FilterCondition::Id(id) => {
                    conditions.push("id = ?".into());
                    params.push(FilterBindValue::S(id.clone()));
                }
                FilterCondition::IdPrefix(prefix) => {
                    conditions.push("id >= ?".into());
                    params.push(FilterBindValue::S(prefix.clone()));
                    if let Some(upper) = prefix_upper_bound(prefix) {
                        conditions.push("id < ?".into());
                        params.push(FilterBindValue::S(upper));
                    }
                }
                FilterCondition::Prefix(key, prefix) => {
                    let expr = metadata_text_expr(key);
                    conditions.push(format!("{} >= ?", expr));
                    params.push(FilterBindValue::S(prefix.clone()));
                    if let Some(upper) = prefix_upper_bound(prefix) {
                        conditions.push(format!("{} < ?", expr));
                        params.push(FilterBindValue::S(upper));
                    }
                }
                FilterCondition::Lt(key, value) => {
                    conditions.push(format!(
                        "({} AND json_extract(metadata, '$.{}') < ?)",
                        is_numeric_expr(key),
                        key
                    ));
                    params.push(FilterBindValue::I(*value));
                }
                FilterCondition::Gt(key, value) => {
                    conditions.push(format!(
                        "({} AND json_extract(metadata, '$.{}') > ?)",
                        is_numeric_expr(key),
                        key
                    ));
                    params.push(FilterBindValue::I(*value));
                }
                FilterCondition::Between(key, start, end) => {
                    conditions.push(format!(
                        "({} AND json_extract(metadata, '$.{}') >= ? AND json_extract(metadata, '$.{}') <= ?)",
                        is_numeric_expr(key),
                        key,
                        key
                    ));
                    params.push(FilterBindValue::I(*start));
                    params.push(FilterBindValue::I(*end));
                }
                FilterCondition::In(key, values) => {
                    if values.is_empty() {
                        conditions.push("0 = 1".into());
                    } else {
                        let placeholders: Vec<String> =
                            (0..values.len()).map(|_| "?".into()).collect();
                        conditions.push(format!(
                            "{} IN ({})",
                            metadata_text_expr(key),
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
        // Numeric-aware ordering matching PostgreSQL: numeric values sort by
        // their numeric value and always come first, everything else (missing
        // keys and non-numeric values) sorts last in both directions. The
        // leading flag column emulates PostgreSQL's `NULLS LAST`; the
        // trailing id follows the primary direction so ties resolve to
        // creation order on both newest-first and oldest-first queries.
        let direction = if descending { "DESC" } else { "ASC" };
        sql.push_str(&format!(
            " ORDER BY (CASE WHEN {} THEN 0 ELSE 1 END) ASC, json_extract(metadata, '$.{}') {}, id {}",
            is_numeric_expr(&key),
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
