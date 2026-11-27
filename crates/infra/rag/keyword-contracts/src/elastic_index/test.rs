use super::*;

fn query(text: &str, scope: Option<&KeywordScope>, limit: usize) -> KeywordQuery {
    let query = KeywordQuery::new(text, limit);
    match scope {
        Some(scope) => query.with_scope(scope.clone()),
        None => query,
    }
}

fn test_config() -> ElasticKeywordConfig {
    ElasticKeywordConfig {
        base_url: "http://127.0.0.1:19200".to_string(),
        api_key: None,
        index: "rag-keyword-test".to_string(),
        timeout_secs: 2,
        batch_size: 64,
        exact_count: true,
        tokenizer: wf_types::config::keyword::KeywordTokenizer::English,
        analyzer_name: None,
        format_policy: Default::default(),
    }
}

#[test]
fn status_error_redacts_credentials() {
    let error = ElasticKeywordIndex::status_error(
        "probe",
        reqwest::StatusCode::BAD_GATEWAY,
        r#"{"error":"bad","api_key":"secret-value"}"#,
    );
    let text = error.to_string();
    assert!(!text.contains("secret-value"));
    assert!(text.contains("[redacted]"));
}

#[test]
fn mixed_mapping_carries_sidecar_and_roundtrip_bodies() {
    use wf_types::config::keyword::KeywordTokenizer;
    let mapping = ElasticKeywordIndex::index_mapping_with(KeywordTokenizer::Mixed);
    assert!(
        mapping
            .pointer("/mappings/properties/content_tokens")
            .is_some(),
        "mixed mapping must carry a whitespace sidecar"
    );
    let point = KeywordDocument::new("a", "hello world").with_payload(HashMap::from([(
        "chunk_id".to_string(),
        serde_json::json!("a"),
    )]));
    let (_, doc) = ElasticKeywordIndex::doc_body_with("kb", &point, KeywordTokenizer::Mixed, None)
        .expect("doc");
    assert!(doc.get("content_tokens").is_some());
    let body = ElasticKeywordIndex::search_body_with(
        "kb",
        "hello",
        None,
        10,
        KeywordTokenizer::Mixed,
        None,
    );
    assert!(body.pointer("/query/bool/must").is_some());
}

#[test]
fn empty_base_url_fails_fast() {
    let config = ElasticKeywordConfig {
        base_url: "  ".to_string(),
        ..test_config()
    };
    assert!(ElasticKeywordIndex::new(&config).is_err());
}

#[test]
fn scope_clauses_cover_collection_namespace_source() {
    let scope = KeywordScope::new().with_namespace("kb").with_source("a.md");
    let clauses = ElasticKeywordIndex::scope_clauses("kb", Some(&scope));
    assert_eq!(clauses.len(), 3);
    assert!(ElasticKeywordIndex::scope_clauses("kb", None).len() == 1);
}

#[test]
fn doc_id_is_deterministic() {
    assert_eq!(ElasticKeywordIndex::doc_id("kb", "a.md#0"), "kb#a.md#0");
}

#[tokio::test]
async fn live_roundtrip_when_server_configured() {
    let Some(url) = std::env::var("WF_ES_TEST_URL")
        .ok()
        .filter(|v| !v.is_empty())
    else {
        return;
    };
    let config = ElasticKeywordConfig {
        base_url: url,
        ..test_config()
    };
    let index = ElasticKeywordIndex::new(&config).expect("client");
    index.health_check().await.expect("health");
    let points = vec![
        KeywordDocument::new("live#0", "rust ownership borrowing").with_payload(HashMap::from([
            ("chunk_id".to_string(), serde_json::json!("live#0")),
            ("namespace".to_string(), serde_json::json!("kb")),
            ("source".to_string(), serde_json::json!("live.md")),
        ])),
    ];
    index.upsert("kb", &points).await.expect("upsert");
    let scope = KeywordScope::new().with_namespace("kb");
    let hits = index
        .search("kb", &query("rust ownership", Some(&scope), 5))
        .await
        .expect("search");
    assert!(!hits.is_empty());
    assert_eq!(index.count("kb", Some(&scope)).await.expect("count"), 1);
    assert_eq!(
        index
            .delete_by_source("kb", Some("kb"), "live.md")
            .await
            .expect("delete"),
        1
    );
}

#[tokio::test]
async fn live_concurrent_bulk_when_server_configured() {
    use std::sync::Arc;
    let Some(url) = std::env::var("WF_ES_TEST_URL")
        .ok()
        .filter(|v| !v.is_empty())
    else {
        return;
    };
    let config = ElasticKeywordConfig {
        base_url: url,
        ..test_config()
    };
    let index = Arc::new(ElasticKeywordIndex::new(&config).expect("client"));
    let mut writers = Vec::new();
    for shard in 0..4 {
        let owned = index.clone();
        writers.push(tokio::spawn(async move {
            let points = vec![KeywordDocument::new(
                format!("concurrent#{shard}"),
                "concurrent bulk rust ownership",
            )
            .with_payload(HashMap::from([
                (
                    "chunk_id".to_string(),
                    serde_json::json!(format!("concurrent#{shard}")),
                ),
                ("namespace".to_string(), serde_json::json!("kb")),
                (
                    "source".to_string(),
                    serde_json::json!(format!("load-{shard}.md")),
                ),
            ]))];
            owned.upsert("kb", &points).await.expect("upsert");
        }));
    }
    for writer in writers {
        writer.await.expect("writer");
    }
    let scope = KeywordScope::new().with_namespace("kb");
    assert_eq!(index.count("kb", Some(&scope)).await.expect("count"), 4);
    for shard in 0..4 {
        index
            .delete_by_source("kb", Some("kb"), &format!("load-{shard}.md"))
            .await
            .expect("delete");
    }
}
