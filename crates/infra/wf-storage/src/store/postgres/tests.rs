use super::*;
use crate::domain::indexes::EntityIndexes;
use crate::domain::store::QueryFilter;
use crate::util::pool::sanitize_connection_string;

#[tokio::test]
async fn test_postgres_pool_sanitization() {
    let conn = "postgres://user:secret@localhost:5432/testdb";
    let sanitized = sanitize_connection_string(conn);
    assert!(!sanitized.contains("secret"));
    assert!(sanitized.contains("***"));
    assert!(sanitized.contains("localhost"));
    assert!(sanitized.contains("testdb"));
}

#[tokio::test]
async fn test_postgres_new_with_invalid_url() {
    let result = PostgresStorage::new("not-a-url", "test", EntityIndexes::NONE).await;
    assert!(result.is_err());
}

#[test]
fn test_build_select_sql_numeric_guards() {
    let filter = QueryFilter::new()
        .with_field_lt("timestamp", 100)
        .with_order_by("timestamp", true)
        .with_limit(10);
    let (sql, params) = build_select_sql(Some(&filter), "checkpoint", "id, metadata");

    // Numeric predicates must not fail on non-numeric values.
    assert!(sql.contains("jsonb_typeof(metadata->'timestamp') = 'number'"));
    // float8 keeps integer and floating-point metadata comparable.
    assert!(sql.contains("::float8 <"));
    assert!(!sql.contains("::bigint"));
    // Ordering must fall back safely instead of casting blindly.
    assert!(sql.contains("CASE WHEN jsonb_typeof"));
    assert!(sql.contains("NULLS LAST"));
    // The id tie-break follows the primary direction so ties resolve to
    // creation order on both newest-first and oldest-first queries.
    assert!(sql.contains(", id DESC"));
    assert_eq!(params.len(), 1);

    let filter = QueryFilter::new()
        .with_field_lt("timestamp", 100)
        .with_order_by("timestamp", false)
        .with_limit(10);
    let (sql, _) = build_select_sql(Some(&filter), "checkpoint", "id, metadata");
    assert!(sql.contains(", id ASC"));

    let filter = QueryFilter::new().with_field_in("entityId", vec!["a".into()]);
    let (sql, params) = build_select_sql(Some(&filter), "checkpoint", "id, metadata");
    assert!(sql.contains("metadata->>'entityId' IN"));
    assert_eq!(params.len(), 1);
}

#[test]
fn test_build_select_sql_prefix_uses_like() {
    let filter = QueryFilter::new().with_id_prefix("wf-");
    let (sql, params) = build_select_sql(Some(&filter), "checkpoint", "id, metadata");
    assert!(sql.contains("id LIKE $1 ESCAPE"));
    assert!(!sql.contains("substr"));
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0],
        crate::domain::store::FilterBindValue::S("wf-%".into())
    );

    let filter = QueryFilter::new().with_field_prefix("name", "100%");
    let (sql, params) = build_select_sql(Some(&filter), "checkpoint", "id, metadata");
    assert!(sql.contains("metadata->>'name' LIKE $1 ESCAPE"));
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0],
        crate::domain::store::FilterBindValue::S("100\\%%".into())
    );
}
