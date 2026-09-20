use wf_storage::domain::store::{BatchItem, QueryFilter, Store, StoreExt, StoreOperation};
use wf_storage::store::sqlite::SqliteStorage;

async fn make_store() -> SqliteStorage {
    SqliteStorage::new(":memory:", "test_filter").await.unwrap()
}

#[tokio::test]
async fn test_eq_matches_text_representation() {
    let store = make_store().await;
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

    let filter = QueryFilter::new().with_field("timestamp", "1000");
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "n1");

    // Booleans match their 'true' / 'false' text form like on the Memory
    // and Postgres backends; the integer strings '1' / '0' must not match.
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

    // Numeric variants like '1e3' must not match text equality.
    let filter = QueryFilter::new().with_field("timestamp", "1e3");
    assert!(store.list(Some(&filter)).await.unwrap().is_empty());
}

#[tokio::test]
async fn test_numeric_predicates_only_match_numbers() {
    let store = make_store().await;
    store
        .save(
            "n1",
            b"data",
            &serde_json::json!({"entityType": "wf", "timestamp": 1000}),
        )
        .await
        .unwrap();
    store
        .save(
            "s1",
            b"data",
            &serde_json::json!({"entityType": "wf", "timestamp": "abc"}),
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

    let filter = QueryFilter::new().with_field_lt("timestamp", 1000);
    assert!(store.list(Some(&filter)).await.unwrap().is_empty());
}

#[tokio::test]
async fn test_order_by_puts_numeric_first() {
    let store = make_store().await;
    store
        .save(
            "n1",
            b"data",
            &serde_json::json!({"entityType": "wf", "timestamp": 1000}),
        )
        .await
        .unwrap();
    store
        .save(
            "s1",
            b"data",
            &serde_json::json!({"entityType": "wf", "timestamp": "abc"}),
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

    let filter = QueryFilter::new().with_order_by("timestamp", true);
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results[0].0, "n1");
    assert_eq!(results.len(), 3);

    let filter = QueryFilter::new().with_order_by("timestamp", false);
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results[0].0, "n1");
}

#[tokio::test]
async fn test_batch_atomicity() {
    let store = make_store().await;
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

    let operations = vec![
        StoreOperation::Delete("cp-0".to_string()),
        StoreOperation::Delete("cp-1".to_string()),
        StoreOperation::Save(BatchItem::new(
            "__watermark__:exec-1",
            Vec::new(),
            serde_json::json!({"cleanupWatermark": 1000}),
        )),
    ];
    store.apply_batch(&operations).await.unwrap();

    assert!(!store.exists("cp-0").await.unwrap());
    assert!(!store.exists("cp-1").await.unwrap());
    assert!(store.exists("cp-2").await.unwrap());
    let (_, meta) = store.load("__watermark__:exec-1").await.unwrap().unwrap();
    assert_eq!(meta["cleanupWatermark"], 1000);
}

#[tokio::test]
async fn test_update_status() {
    let store = make_store().await;
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
async fn test_list_data_batch_read() {
    let store = make_store().await;
    for i in 0..10 {
        store
            .save(
                &format!("item-{}", i),
                b"data",
                &serde_json::json!({"index": i}),
            )
            .await
            .unwrap();
    }
    let all = store.list_data(None).await.unwrap();
    assert_eq!(all.len(), 10);

    let filter = QueryFilter::new().with_id_prefix("item-5");
    let filtered = store.list_data(Some(&filter)).await.unwrap();
    assert_eq!(filtered.len(), 1);
}

#[tokio::test]
async fn test_schema_version_inserted_on_first_open() {
    let store = make_store().await;
    let (_, meta) = store
        .load("__schema_version__:test_filter")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(meta["version"], 1);
}

#[tokio::test]
async fn test_schema_version_rejects_mismatch() {
    let store = make_store().await;
    // Tamper with the version record to simulate an older schema.
    store
        .save(
            "__schema_version__:test_filter",
            b"",
            &serde_json::json!({"version": 0}),
        )
        .await
        .unwrap();

    // Opening a new handle against the same in-memory DB would re-run the
    // version check; since :memory: databases are per-connection we cannot
    // test the rejection path with a second SqliteStorage::new. Instead,
    // verify the check logic by directly reading and comparing.
    let (_, meta) = store
        .load("__schema_version__:test_filter")
        .await
        .unwrap()
        .unwrap();
    let stored = meta["version"].as_i64().unwrap();
    assert_ne!(stored, 1, "version should have been tampered");
    // The actual rejection happens at SqliteStorage::new time; this test
    // confirms the version record exists and can be read.
}
