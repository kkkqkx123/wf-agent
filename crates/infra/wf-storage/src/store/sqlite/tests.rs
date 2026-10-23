use super::*;
use crate::domain::indexes::EntityIndexes;
use crate::domain::keys::SCHEMA_VERSION_EXCLUDE_PATTERN;
use crate::domain::store::{
    BatchItem, FilterBindValue, QueryFilter, Store, StoreExt, StoreOperation,
};

#[tokio::test]
async fn test_sqlite_save_load() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    store
        .save(
            "id1",
            b"hello world",
            &serde_json::json!({"entityType": "test"}),
        )
        .await
        .unwrap();
    let (data, meta) = store.load("id1").await.unwrap().unwrap();
    assert_eq!(data, b"hello world");
    assert_eq!(meta["entityType"], "test");
}

#[tokio::test]
async fn test_sqlite_list_filter() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    store
        .save(
            "id1",
            b"data1",
            &serde_json::json!({"entityType": "A", "status": "active"}),
        )
        .await
        .unwrap();
    store
        .save(
            "id2",
            b"data2",
            &serde_json::json!({"entityType": "B", "status": "inactive"}),
        )
        .await
        .unwrap();

    let filter = QueryFilter::new().with_entity_type("A");
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "id1");
}

#[tokio::test]
async fn test_sqlite_list_pushdown_ops() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    for i in 0..5 {
        store
            .save(
                &format!("wf-{}:v{}", i, 1),
                b"data",
                &serde_json::json!({"entityType": "workflow", "timestamp": 1000 + i}),
            )
            .await
            .unwrap();
    }

    let filter = QueryFilter::new().with_id_prefix("wf-");
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 5);

    let filter = QueryFilter::new()
        .with_field("entityType", "workflow")
        .with_order_by("timestamp", true)
        .with_limit(2);
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1["timestamp"], 1004);
    assert_eq!(results[1].1["timestamp"], 1003);

    let filter = QueryFilter::new().with_field_lt("timestamp", 1003);
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 3);

    let counts = store.count_by_field("entityType").await.unwrap();
    assert_eq!(*counts.get("workflow").unwrap(), 5);
}

#[tokio::test]
async fn test_sqlite_batch() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    let items: Vec<BatchItem> = (0..100)
        .map(|i| {
            BatchItem::new(
                format!("id_{}", i),
                vec![i as u8; 50],
                serde_json::json!({"index": i}),
            )
        })
        .collect();
    store.save_batch(&items).await.unwrap();
    assert_eq!(store.list(None).await.unwrap().len(), 100);
}

#[tokio::test]
async fn test_sqlite_filter_semantics_aligned() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    store
        .save(
            "n1",
            b"data",
            &serde_json::json!({"entityType": "wf", "timestamp": 1000, "flag": true}),
        )
        .await
        .unwrap();
    store
        .save(
            "s1",
            b"data",
            &serde_json::json!({"entityType": "wf", "timestamp": "abc", "flag": false}),
        )
        .await
        .unwrap();
    store
        .save(
            "s2",
            b"data",
            &serde_json::json!({"entityType": "wf", "timestamp": "500"}),
        )
        .await
        .unwrap();

    // Eq compares text representations: the canonical decimal form
    // matches, but numeric variants like '1e3' do not.
    let filter = QueryFilter::new().with_field("timestamp", "1000");
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "n1");

    let filter = QueryFilter::new().with_field("timestamp", "1e3");
    assert!(store.list(Some(&filter)).await.unwrap().is_empty());

    // Booleans match their 'true' / 'false' text form like on the other
    // backends; the integer-looking strings '1' / '0' must not match.
    let filter = QueryFilter::new().with_field("flag", "true");
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "n1");

    let filter = QueryFilter::new().with_field("flag", "false");
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "s1");

    let filter = QueryFilter::new().with_field("flag", "1");
    assert!(store.list(Some(&filter)).await.unwrap().is_empty());

    // Numeric predicates only match JSON numbers: the numeric-looking
    // string "500" must be excluded just like "abc".
    let filter = QueryFilter::new().with_field_lt("timestamp", 1000);
    assert!(store.list(Some(&filter)).await.unwrap().is_empty());

    // OrderBy puts numeric values first in both directions.
    let filter = QueryFilter::new().with_order_by("timestamp", true);
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results[0].0, "n1");
    assert_eq!(results.len(), 3);

    let filter = QueryFilter::new().with_order_by("timestamp", false);
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results[0].0, "n1");
}

#[tokio::test]
async fn test_sqlite_prefix_like_matches_literal_percent() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    for id in ["wf-1", "wf-2", "other-1", "100%-x"] {
        store
            .save(id, b"data", &serde_json::json!({"entityType": "wf"}))
            .await
            .unwrap();
    }

    let filter = QueryFilter::new().with_id_prefix("wf-");
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 2);

    // A '%' inside the prefix stays literal instead of wildcarding.
    let filter = QueryFilter::new().with_id_prefix("100%");
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "100%-x");
}

/// A prefix match has to compile to a bounded range: `LIKE` is
/// case-insensitive for ASCII here, so it can neither use the `BINARY`
/// index nor agree with the other backends.
#[test]
fn field_prefix_compiles_to_a_bounded_range() {
    let filter = QueryFilter::new().with_field_prefix("executionPath", "/r/");
    let (sql, params) = build_select_sql(Some(&filter), "execution", "id, metadata");
    assert!(
        !sql.contains("LIKE ?"),
        "prefix must not compile to LIKE: {sql}"
    );
    assert_eq!(
        params,
        vec![
            FilterBindValue::S("/r/".into()),
            FilterBindValue::S("/r0".into()),
        ]
    );
}

#[test]
fn id_prefix_compiles_to_a_bounded_range() {
    let filter = QueryFilter::new().with_id_prefix("wf-");
    let (sql, params) = build_select_sql(Some(&filter), "execution", "id, metadata");
    assert!(
        !sql.contains("LIKE ?"),
        "prefix must not compile to LIKE: {sql}"
    );
    assert_eq!(
        params,
        vec![
            FilterBindValue::S("wf-".into()),
            FilterBindValue::S("wf.".into()),
        ]
    );
}

/// Raising the last code point gives the exclusive upper bound; a code
/// point with no successor is dropped so the one before it carries, and a
/// prefix left with no bound at all still selects exactly its own strings.
#[test]
fn prefix_upper_bound_raises_or_carries_the_last_code_point() {
    assert_eq!(prefix_upper_bound("/"), Some("0".to_string()));
    assert_eq!(prefix_upper_bound("ab"), Some("ac".to_string()));
    assert_eq!(prefix_upper_bound("a\u{10FFFF}"), Some("b".to_string()));
    assert_eq!(prefix_upper_bound("\u{10FFFF}\u{10FFFF}"), None);
    assert_eq!(prefix_upper_bound(""), None);
}

/// The same filter has to select the same rows from either backend, so a
/// hierarchy query behaves identically after a backend change.
#[tokio::test]
async fn prefix_selects_the_same_rows_in_memory_and_sqlite() {
    use crate::store::memory::MemoryStorage;

    let ids = ["wf-1", "wf-2", "WF-3", "other-1", "wf-", "w"];
    let memory = MemoryStorage::new("prefix_equivalence");
    let sqlite = SqliteStorage::new(":memory:", "prefix_equivalence", EntityIndexes::NONE)
        .await
        .unwrap();
    for id in ids {
        let meta = serde_json::json!({
            "entityType": "wf",
            "executionPath": format!("/{}-", id),
        });
        memory.save(id, b"d", &meta).await.unwrap();
        sqlite.save(id, b"d", &meta).await.unwrap();
    }

    let cases = [
        ("id prefix", QueryFilter::new().with_id_prefix("wf-")),
        (
            "exact path prefix",
            QueryFilter::new().with_field_prefix("executionPath", "/wf-1-"),
        ),
        (
            "case-differing prefix",
            QueryFilter::new().with_field_prefix("executionPath", "/W"),
        ),
        (
            "empty prefix",
            QueryFilter::new().with_field_prefix("executionPath", ""),
        ),
    ];
    for (label, filter) in cases {
        let mut from_memory: Vec<String> = memory
            .list(Some(&filter))
            .await
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let mut from_sqlite: Vec<String> = sqlite
            .list(Some(&filter))
            .await
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        from_memory.sort();
        from_sqlite.sort();
        assert_eq!(from_memory, from_sqlite, "backend disagreement on {label}");
    }
}

#[tokio::test]
async fn test_sqlite_count_ignores_pagination() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    for i in 0..5 {
        store
            .save(
                &format!("id-{}", i),
                b"data",
                &serde_json::json!({"entityType": "wf", "timestamp": i}),
            )
            .await
            .unwrap();
    }

    let filter = QueryFilter::new()
        .with_field("entityType", "wf")
        .with_order_by("timestamp", true)
        .with_offset(1)
        .with_limit(2);
    assert_eq!(store.list(Some(&filter)).await.unwrap().len(), 2);
    assert_eq!(store.count(Some(&filter)).await.unwrap(), 5);
}

#[tokio::test]
async fn test_sqlite_count_by_field_excludes_version_row() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    store
        .save("id1", b"data", &serde_json::json!({"kind": "a"}))
        .await
        .unwrap();
    let counts = store.count_by_field("kind").await.unwrap();
    assert_eq!(*counts.get("a").unwrap(), 1);
    assert!(counts.keys().all(|k| !k.contains("__schema")));

    let version_counts = store.count_by_field("version").await.unwrap();
    assert!(version_counts.is_empty());
}

#[tokio::test]
async fn test_sqlite_large_and_empty_batches() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    // 250 rows exceed the old single-statement variable budget; chunking
    // must absorb them.
    let items: Vec<BatchItem> = (0..250)
        .map(|i| {
            BatchItem::new(
                format!("bulk-{}", i),
                vec![i as u8; 10],
                serde_json::json!({"index": i}),
            )
        })
        .collect();
    store.save_batch(&items).await.unwrap();
    assert_eq!(store.count(None).await.unwrap(), 250);

    let ids: Vec<String> = (0..250).map(|i| format!("bulk-{}", i)).collect();
    assert_eq!(store.load_batch(&ids).await.unwrap().len(), 250);

    // Empty batches are no-ops without opening a transaction.
    store.save_batch(&[]).await.unwrap();
    store.load_batch(&[]).await.unwrap();
    store.delete_batch(&[]).await.unwrap();
    assert_eq!(store.count(None).await.unwrap(), 250);

    store.delete_batch(&ids).await.unwrap();
    assert_eq!(store.count(None).await.unwrap(), 0);
}

#[tokio::test]
async fn test_sqlite_update_status() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    store
        .save(
            "exec1",
            b"data",
            &serde_json::json!({"entityType": "execution", "status": "pending"}),
        )
        .await
        .unwrap();
    store.update_status("exec1", "running").await.unwrap();
    let (_, meta) = store.load("exec1").await.unwrap().unwrap();
    assert_eq!(meta["status"], "running");
}

#[tokio::test]
async fn test_sqlite_apply_batch_mixed_ops() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    for i in 0..5 {
        store
            .save(
                &format!("cp-{}", i),
                b"data",
                &serde_json::json!({"entityType": "checkpoint", "index": i}),
            )
            .await
            .unwrap();
    }

    // Atomic batch: delete three checkpoints and write the watermark
    // record in the same transaction.
    let operations = vec![
        StoreOperation::Delete("cp-0".to_string()),
        StoreOperation::Delete("cp-1".to_string()),
        StoreOperation::Delete("cp-2".to_string()),
        StoreOperation::Save(BatchItem::new(
            "__watermark__:exec-1",
            Vec::new(),
            serde_json::json!({"cleanupWatermark": 1000, "cleanupRunCount": 1}),
        )),
    ];
    store.apply_batch(&operations).await.unwrap();

    assert!(!store.exists("cp-0").await.unwrap());
    assert!(!store.exists("cp-2").await.unwrap());
    assert!(store.exists("cp-3").await.unwrap());
    let (_, meta) = store.load("__watermark__:exec-1").await.unwrap().unwrap();
    assert_eq!(meta["cleanupWatermark"], 1000);
    assert_eq!(store.list(None).await.unwrap().len(), 3);
}

#[tokio::test]
async fn test_sqlite_apply_batch_empty_is_noop() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    store.apply_batch(&[]).await.unwrap();
    assert!(store.list(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn test_sqlite_cross_table_rolls_back_on_failure() {
    use crate::domain::store::CrossTableOperation;

    let pool = SqliteStorage::create_pool(":memory:").await.unwrap();
    let first = SqliteStorage::with_pool(pool.clone(), "rollback_a", EntityIndexes::NONE)
        .await
        .unwrap();
    let _second = SqliteStorage::with_pool(pool.clone(), "rollback_b", EntityIndexes::NONE)
        .await
        .unwrap();

    let save = StoreOperation::Save(BatchItem::new(
        "id1",
        b"data".to_vec(),
        serde_json::json!({"entityType": "test"}),
    ));
    let delete_missing_table = StoreOperation::Delete("id1".into());
    let operations = vec![
        CrossTableOperation {
            table: "rollback_a",
            operation: &save,
        },
        CrossTableOperation {
            table: "no_such_table",
            operation: &delete_missing_table,
        },
    ];
    let result = SqliteStorage::apply_cross_table(&pool, &operations).await;
    assert!(result.is_err());
    // The first write must not survive the failed batch: list hides the
    // internal schema version row, so empty means fully rolled back.
    assert!(first.list(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn test_sqlite_save_preserves_created_at() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    store
        .save("id1", b"v1", &serde_json::json!({"entityType": "test"}))
        .await
        .unwrap();
    let sql = format!(
        "SELECT created_at FROM {} WHERE id = ?1",
        store.table_name()
    );
    let first: i64 = sqlx::query_scalar(&sql)
        .bind("id1")
        .fetch_one(store.pool())
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    store
        .save("id1", b"v2", &serde_json::json!({"entityType": "test"}))
        .await
        .unwrap();
    let second: i64 = sqlx::query_scalar(&sql)
        .bind("id1")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(first, second, "overwrite must keep the original created_at");
}

#[tokio::test]
async fn test_sqlite_save_batch_preserves_created_at() {
    let store = SqliteStorage::new(":memory:", "test", EntityIndexes::NONE)
        .await
        .unwrap();
    let items: Vec<BatchItem> = (0..3)
        .map(|i| {
            BatchItem::new(
                format!("id_{}", i),
                vec![i as u8; 10],
                serde_json::json!({"index": i}),
            )
        })
        .collect();
    store.save_batch(&items).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let re_saved: Vec<BatchItem> = (0..3)
        .map(|i| {
            BatchItem::new(
                format!("id_{}", i),
                vec![0xAA; 10],
                serde_json::json!({"index": i}),
            )
        })
        .collect();
    store.save_batch(&re_saved).await.unwrap();

    let sql = format!(
        "SELECT created_at FROM {} WHERE id = ?1",
        store.table_name()
    );
    for i in 0..3 {
        let created: i64 = sqlx::query_scalar(&sql)
            .bind(format!("id_{}", i))
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert!(created > 0);
    }
    let all: Vec<(String, Vec<u8>)> = sqlx::query_as(&format!(
        "SELECT id, data FROM {} WHERE id NOT LIKE '{}'",
        store.table_name(),
        SCHEMA_VERSION_EXCLUDE_PATTERN
    ))
    .fetch_all(store.pool())
    .await
    .unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|(_, data)| data == &vec![0xAA; 10]));
}
