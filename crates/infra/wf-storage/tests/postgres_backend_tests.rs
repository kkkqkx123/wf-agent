use wf_storage::domain::store::{BatchItem, QueryFilter, Store, StoreExt, StoreOperation};
use wf_storage::store::postgres::PostgresStorage;

fn database_url() -> Option<String> {
    std::env::var("POSTGRES_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| Some("postgres://postgres@127.0.0.1:5432/wf_storage_test".to_string()))
}

async fn make_store(table: &str) -> Option<PostgresStorage> {
    let url = database_url()?;
    // Drop any prior table so tests start clean.
    let pool = wf_storage::util::pool::create_pg_pool(&url).await.ok()?;
    let _ = sqlx::query(&format!("DROP TABLE IF EXISTS {}", table))
        .execute(&pool)
        .await;
    PostgresStorage::new(&url, table, wf_storage::domain::EntityIndexes::NONE)
        .await
        .ok()
}

#[tokio::test]
async fn test_postgres_eq_matches_text_representation() {
    let Some(store) = make_store("pg_eq").await else {
        return;
    };
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

    let filter = QueryFilter::new().with_field("timestamp", "1e3");
    assert!(store.list(Some(&filter)).await.unwrap().is_empty());
}

#[tokio::test]
async fn test_postgres_numeric_predicates_only_match_numbers() {
    let Some(store) = make_store("pg_num").await else {
        return;
    };
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
async fn test_postgres_order_by_puts_numeric_first() {
    let Some(store) = make_store("pg_order").await else {
        return;
    };
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
async fn test_postgres_batch_atomicity() {
    let Some(store) = make_store("pg_batch").await else {
        return;
    };
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
async fn test_postgres_update_status() {
    let Some(store) = make_store("pg_status").await else {
        return;
    };
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
async fn test_postgres_list_data_batch_read() {
    let Some(store) = make_store("pg_list").await else {
        return;
    };
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
async fn test_postgres_schema_version_inserted_on_first_open() {
    let Some(store) = make_store("pg_version").await else {
        return;
    };
    let (_, meta) = store
        .load("__schema_version__:pg_version")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(meta["version"], 1);
}

#[tokio::test]
async fn test_postgres_prefix_like_escape() {
    let Some(store) = make_store("pg_prefix").await else {
        return;
    };
    store
        .save("a%b", b"data", &serde_json::json!({"name": "100%"}))
        .await
        .unwrap();
    store
        .save("aXb", b"data", &serde_json::json!({"name": "100X"}))
        .await
        .unwrap();
    let filter = QueryFilter::new().with_id_prefix("a%");
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "a%b");
    let filter = QueryFilter::new().with_field_prefix("name", "100%");
    let results = store.list(Some(&filter)).await.unwrap();
    assert_eq!(results.len(), 1);
}
