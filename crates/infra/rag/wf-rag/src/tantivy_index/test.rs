use super::*;

fn point(id: &str, content: &str, namespace: &str, source: &str) -> KeywordDocument {
    KeywordDocument::new(id, content).with_payload(HashMap::from([
        ("chunk_id".to_string(), serde_json::json!(id)),
        ("document_id".to_string(), serde_json::json!("d")),
        ("namespace".to_string(), serde_json::json!(namespace)),
        ("source".to_string(), serde_json::json!(source)),
    ]))
}

fn query(text: &str, scope: Option<&KeywordScope>, limit: usize) -> KeywordQuery {
    let query = KeywordQuery::new(text, limit);
    match scope {
        Some(scope) => query.with_scope(scope.clone()),
        None => query,
    }
}

#[tokio::test]
async fn tantivy_roundtrip_filters_and_counts() {
    let index = TantivyKeywordIndex::open_in_memory().expect("open");
    index
        .upsert(
            "kb",
            &[
                point("a", "rust ownership borrowing", "kb", "a.md"),
                point("b", "go routines channels", "kb", "b.md"),
            ],
        )
        .await
        .expect("upsert");
    let scope = KeywordScope::new().with_namespace("kb");
    let hits = index
        .search("kb", &query("rust ownership", Some(&scope), 5))
        .await
        .expect("search");
    assert!(!hits.is_empty());
    assert_eq!(hits[0].id, "a");
    assert_eq!(index.count("kb", None).await.expect("count"), 2);
    assert!(index
        .search("kb", &query("   ", Some(&scope), 5))
        .await
        .expect("empty")
        .is_empty());
    assert_eq!(
        index
            .delete_by_source("kb", Some("kb"), "a.md")
            .await
            .expect("delete"),
        1
    );
    assert_eq!(index.count("kb", None).await.expect("count"), 1);
}

#[tokio::test]
async fn tantivy_file_index_survives_reopen() {
    let dir = std::env::temp_dir().join(format!("rag-tantivy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.to_string_lossy().into_owned();
    let index = TantivyKeywordIndex::open(&path).expect("open");
    index
        .upsert(
            "kb",
            &[point("a", "rust ownership borrowing", "kb", "a.md")],
        )
        .await
        .expect("upsert");
    drop(index);
    let reopened = TantivyKeywordIndex::open(&path).expect("reopen");
    assert_eq!(reopened.count("kb", None).await.expect("count"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

fn tagged_point(id: &str, content: &str, lang: &str) -> KeywordDocument {
    KeywordDocument::new(id, content).with_payload(HashMap::from([
        ("chunk_id".to_string(), serde_json::json!(id)),
        ("document_id".to_string(), serde_json::json!("d")),
        ("namespace".to_string(), serde_json::json!("kb")),
        ("source".to_string(), serde_json::json!("a.md")),
        ("lang".to_string(), serde_json::json!(lang)),
    ]))
}

/// Three-backend parity on English: same corpus and query, same order
/// and filter outcome; scores are never compared across engines.
#[tokio::test]
async fn tantivy_matches_sqlite_on_english_corpus() {
    use crate::sqlite_index::SqliteKeywordIndex;
    let sqlite = SqliteKeywordIndex::open_in_memory().expect("sqlite");
    let tantivy = TantivyKeywordIndex::open_in_memory().expect("tantivy");
    let points = vec![
        tagged_point("a", "rust ownership borrowing", "en"),
        tagged_point("b", "rust async runtime tokio", "en"),
        tagged_point("c", "go routines channels", "zh"),
    ];
    sqlite.upsert("kb", &points).await.expect("upsert");
    tantivy.upsert("kb", &points).await.expect("upsert");
    let scope = KeywordScope::new().with_namespace("kb");
    let sqlite_hits = sqlite
        .search("kb", &query("rust ownership", Some(&scope), 5))
        .await
        .expect("search");
    let tantivy_hits = tantivy
        .search("kb", &query("rust ownership", Some(&scope), 5))
        .await
        .expect("search");
    let sqlite_ids: Vec<&str> = sqlite_hits.iter().map(|hit| hit.id.as_str()).collect();
    let tantivy_ids: Vec<&str> = tantivy_hits.iter().map(|hit| hit.id.as_str()).collect();
    assert_eq!(sqlite_ids, tantivy_ids);
    assert_eq!(sqlite_ids.first(), Some(&"a"));
    let scoped = KeywordScope::new().with_namespace("kb").with_source("a.md");
    assert_eq!(
        sqlite.count("kb", Some(&scoped)).await.expect("count"),
        tantivy.count("kb", Some(&scoped)).await.expect("count")
    );
    assert!(sqlite
        .search("kb", &query("   ", Some(&scope), 5))
        .await
        .expect("empty")
        .is_empty());
    assert!(tantivy
        .search("kb", &query("   ", Some(&scope), 5))
        .await
        .expect("empty")
        .is_empty());
    assert!(sqlite
        .search("kb", &query("rust", Some(&scope), 0))
        .await
        .expect("zero")
        .is_empty());
    assert!(tantivy
        .search("kb", &query("rust", Some(&scope), 0))
        .await
        .expect("zero")
        .is_empty());
    let selective = KeywordScope::new()
        .with_namespace("kb")
        .with_source("missing.md");
    assert!(sqlite
        .search("kb", &query("rust", Some(&selective), 5))
        .await
        .expect("selective")
        .is_empty());
    assert!(tantivy
        .search("kb", &query("rust", Some(&selective), 5))
        .await
        .expect("selective")
        .is_empty());
}

#[tokio::test]
async fn tantivy_concurrent_writes_and_reads_stay_consistent() {
    use std::sync::Arc;
    let index = Arc::new(TantivyKeywordIndex::open_in_memory().expect("open"));
    let mut writers = Vec::new();
    for shard in 0..4 {
        let owned = index.clone();
        writers.push(tokio::spawn(async move {
            let mut batch = Vec::new();
            for seq in 0..8 {
                let id = format!("w{shard}#{seq}");
                batch.push(point(
                    &id,
                    &format!("concurrent shard {shard} token seq {seq} rust"),
                    "kb",
                    "load.md",
                ));
            }
            owned.upsert("kb", &batch).await.expect("upsert");
        }));
    }
    for writer in writers {
        writer.await.expect("writer");
    }
    assert_eq!(index.count("kb", None).await.expect("count"), 32);
    let mut readers = Vec::new();
    for _ in 0..8 {
        let owned = index.clone();
        readers.push(tokio::spawn(async move {
            let scope = KeywordScope::new().with_namespace("kb");
            owned
                .search("kb", &query("concurrent rust", Some(&scope), 5))
                .await
                .expect("search")
        }));
    }
    for reader in readers {
        assert!(!reader.await.expect("reader").is_empty());
    }
}

#[tokio::test]
async fn tantivy_mixed_finds_chinese_and_keeps_english() {
    use wf_types::config::keyword::KeywordTokenizer;
    let index =
        TantivyKeywordIndex::open_in_memory_with(KeywordTokenizer::Mixed, None).expect("open");
    index
        .upsert(
            "kb",
            &[
                point("a", "rust所有权借用检查", "kb", "a.md"),
                point("b", "go并发调度", "kb", "b.md"),
            ],
        )
        .await
        .expect("upsert");
    let scope = KeywordScope::new().with_namespace("kb");
    let hits = index
        .search("kb", &query("所有权", Some(&scope), 5))
        .await
        .expect("search");
    assert!(!hits.is_empty());
    assert_eq!(hits[0].id, "a");
    let english = index
        .search("kb", &query("rust", Some(&scope), 5))
        .await
        .expect("search");
    assert!(english.iter().any(|hit| hit.id == "a"));
}

#[tokio::test]
async fn tantivy_mixed_matches_sqlite_on_chinese_corpus() {
    use crate::sqlite_index::SqliteKeywordIndex;
    use wf_types::config::keyword::KeywordTokenizer;
    let sqlite =
        SqliteKeywordIndex::open_in_memory_with(KeywordTokenizer::Mixed, None).expect("sqlite");
    let tantivy =
        TantivyKeywordIndex::open_in_memory_with(KeywordTokenizer::Mixed, None).expect("tantivy");
    let points = vec![
        point("a", "rust所有权借用检查", "kb", "a.md"),
        point("b", "go并发调度", "kb", "b.md"),
    ];
    sqlite.upsert("kb", &points).await.expect("upsert");
    tantivy.upsert("kb", &points).await.expect("upsert");
    let scope = KeywordScope::new().with_namespace("kb");
    let sqlite_hits = sqlite
        .search("kb", &query("所有权", Some(&scope), 5))
        .await
        .expect("search");
    let tantivy_hits = tantivy
        .search("kb", &query("所有权", Some(&scope), 5))
        .await
        .expect("search");
    assert!(!sqlite_hits.is_empty());
    assert!(!tantivy_hits.is_empty());
    assert_eq!(sqlite_hits[0].id, tantivy_hits[0].id);
}
