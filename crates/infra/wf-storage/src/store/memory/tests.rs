    use super::*;
    use crate::domain::store::StoreExt;

    #[tokio::test]
    async fn test_save_load_roundtrip() {
        let store = MemoryStorage::new("test");
        store
            .save("id1", b"hello", &serde_json::json!({"type": "test"}))
            .await
            .unwrap();
        let (data, meta) = store.load("id1").await.unwrap().unwrap();
        assert_eq!(data, b"hello");
        assert_eq!(meta, serde_json::json!({"type": "test"}));
    }

    #[tokio::test]
    async fn test_delete() {
        let store = MemoryStorage::new("test");
        store
            .save("id1", b"data", &serde_json::json!({}))
            .await
            .unwrap();
        assert!(store.exists("id1").await.unwrap());
        store.delete("id1").await.unwrap();
        assert!(!store.exists("id1").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_with_filter() {
        let store = MemoryStorage::new("test");
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
        store
            .save(
                "id3",
                b"data3",
                &serde_json::json!({"entityType": "A", "status": "inactive"}),
            )
            .await
            .unwrap();

        let filter = QueryFilter::new().with_entity_type("A");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 2);

        let filter = QueryFilter::new().with_status("inactive");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_list_with_pushdown_ops() {
        let store = MemoryStorage::new("test");
        for i in 0..5 {
            store
                .save(
                    &format!("wf-{}:v{}", i, 1),
                    b"data",
                    &serde_json::json!({"entityType": "workflow", "timestamp": 1000 + i}),
                )
                .await
                .unwrap();
            store
                .save(
                    &format!("other-{}", i),
                    b"data",
                    &serde_json::json!({"entityType": "other", "timestamp": 2000 + i}),
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
    }

    #[tokio::test]
    async fn test_filter_semantics_aligned() {
        let store = MemoryStorage::new("test");
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

        // Eq matches the text representation of numbers and booleans.
        let filter = QueryFilter::new().with_field("timestamp", "1000");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "n1");

        let filter = QueryFilter::new().with_field("flag", "true");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "n1");

        // Numeric variants of the same number do not match text equality.
        let filter = QueryFilter::new().with_field("timestamp", "1e3");
        assert!(store.list(Some(&filter)).await.unwrap().is_empty());

        // Numeric predicates only match JSON numbers.
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
    async fn test_batch_operations() {
        let store = MemoryStorage::new("test");
        let items: Vec<BatchItem> = (0..100)
            .map(|i| {
                BatchItem::new(
                    format!("id_{}", i),
                    vec![i as u8; 100],
                    serde_json::json!({"index": i}),
                )
            })
            .collect();
        store.save_batch(&items).await.unwrap();
        assert_eq!(store.list(None).await.unwrap().len(), 100);

        let ids: Vec<String> = (0..50).map(|i| format!("id_{}", i)).collect();
        store.delete_batch(&ids).await.unwrap();
        assert_eq!(store.list(None).await.unwrap().len(), 50);
    }

    #[tokio::test]
    async fn test_close_does_not_clear_data() {
        let store = MemoryStorage::new("test");
        store
            .save("id1", b"data", &serde_json::json!({}))
            .await
            .unwrap();
        // MemoryStorage has no close() — data persists for the lifetime of the struct
        assert!(store.exists("id1").await.unwrap());
    }

    #[tokio::test]
    async fn test_apply_batch_mixed_ops_atomic() {
        let store = MemoryStorage::new("test");
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
    async fn test_apply_batch_empty_is_noop() {
        let store = MemoryStorage::new("test");
        store.apply_batch(&[]).await.unwrap();
        assert!(store.list(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_count_ignores_pagination() {
        let store = MemoryStorage::new("test");
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
    async fn test_repeated_order_by_resolves_to_last() {
        let store = MemoryStorage::new("test");
        store
            .save("a", b"data", &serde_json::json!({"x": 1, "y": 30}))
            .await
            .unwrap();
        store
            .save("b", b"data", &serde_json::json!({"x": 2, "y": 10}))
            .await
            .unwrap();
        store
            .save("c", b"data", &serde_json::json!({"x": 3, "y": 20}))
            .await
            .unwrap();
        // Two orderings: only the last one takes effect, like the SQL
        // backends' compiled plan.
        let mut filter = QueryFilter::new().with_order_by("x", false);
        filter.add_op(crate::domain::store::FilterOp::OrderBy("y".into(), false));
        let results = store.list(Some(&filter)).await.unwrap();
        let ids: Vec<&str> = results.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["b", "c", "a"]);
    }

    #[tokio::test]
    async fn test_float_metadata_comparable() {
        let store = MemoryStorage::new("test");
        store
            .save("f1", b"data", &serde_json::json!({"score": 1.5}))
            .await
            .unwrap();
        store
            .save("f2", b"data", &serde_json::json!({"score": "high"}))
            .await
            .unwrap();
        let filter = QueryFilter::new().with_field_gt("score", 1);
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "f1");

        let filter = QueryFilter::new().with_order_by("score", false);
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results[0].0, "f1");
        assert_eq!(results.len(), 2);
    }
