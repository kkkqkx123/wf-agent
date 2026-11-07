//! SQLite FTS5 keyword backend: the built-in inverted index.
//!
//! Single virtual table holds every collection; only the content column is
//! indexed while collection and scope columns stay unindexed for filtering.
//! Relevance comes from the native BM25 rank; generic field conditions are
//! rechecked in Rust so keyword filtering shares one match semantic.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use vector_contracts::{SearchFilter, VectorPoint};

use crate::error::{RagError, Result};
use crate::keyword_index::KeywordIndex;
use crate::retriever::{chunk_from_hit, RetrievedChunk};

const SCHEMA: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS keyword_fts USING fts5(collection UNINDEXED, chunk_id UNINDEXED, namespace UNINDEXED, source UNINDEXED, content, payload UNINDEXED, tokenize='unicode61')";

/// Built-in keyword backend powered by SQLite FTS5.
pub struct SqliteKeywordIndex {
    conn: Mutex<rusqlite::Connection>,
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
}

impl SqliteKeywordIndex {
    /// Opens an in-memory database for tests and ephemeral knowledge bases.
    pub fn open_in_memory() -> Result<Self> {
        Self::open_in_memory_with(wf_types::config::keyword::KeywordTokenizer::English, None)
    }

    /// Opens an in-memory database with an explicit tokenizer selection.
    pub fn open_in_memory_with(
        tokenizer: wf_types::config::keyword::KeywordTokenizer,
        analyzer_name: Option<String>,
    ) -> Result<Self> {
        let conn = rusqlite::Connection::open_in_memory()
            .map_err(|err| RagError::Keyword(err.to_string()))?;
        let index = Self {
            conn: Mutex::new(conn),
            tokenizer,
            analyzer_name,
        };
        index.ensure_schema()?;
        Ok(index)
    }

    /// Opens or creates a file database.
    pub fn open(path: &str) -> Result<Self> {
        Self::open_with(
            path,
            wf_types::config::keyword::KeywordTokenizer::English,
            None,
        )
    }

    /// Opens or creates a file database with an explicit selection.
    pub fn open_with(
        path: &str,
        tokenizer: wf_types::config::keyword::KeywordTokenizer,
        analyzer_name: Option<String>,
    ) -> Result<Self> {
        if path.trim().is_empty() {
            return Err(RagError::InvalidInput(
                "keyword database path must not be empty".to_string(),
            ));
        }
        let conn =
            rusqlite::Connection::open(path).map_err(|err| RagError::Keyword(err.to_string()))?;
        conn.execute_batch("PRAGMA busy_timeout = 5000;")
            .map_err(|err| RagError::Keyword(err.to_string()))?;
        let _ = conn
            .execute_batch("PRAGMA journal_mode = WAL;")
            .map_err(|err| RagError::Keyword(err.to_string()));
        let index = Self {
            conn: Mutex::new(conn),
            tokenizer,
            analyzer_name,
        };
        index.ensure_schema()?;
        Ok(index)
    }

    fn ensure_schema(&self) -> Result<()> {
        let guard = self
            .conn
            .lock()
            .map_err(|err| RagError::Keyword(err.to_string()))?;
        guard
            .execute_batch(SCHEMA)
            .map_err(|err| RagError::Keyword(err.to_string()))?;
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>> {
        self.conn
            .lock()
            .map_err(|err| RagError::Keyword(err.to_string()))
    }
}

fn payload_string(payload: &HashMap<String, serde_json::Value>, key: &str) -> String {
    payload
        .get(key)
        .and_then(|value| match value {
            serde_json::Value::String(text) => Some(text.clone()),
            other => vector_contracts::payload_text(other),
        })
        .unwrap_or_default()
}

/// Disjunction with an explicit tokenizer; mixed uses dictionary terms
/// quoted the same way so the FTS5 query parser re-splits them into
/// single characters and matches the indexed rows.
fn fts_query_for_with(
    question: &str,
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<&str>,
) -> Option<String> {
    let mut terms: Vec<String> = crate::tokenizer::terms(question, tokenizer, analyzer_name)
        .into_iter()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        return None;
    }
    terms.sort();
    terms.dedup();
    Some(terms.join(" OR "))
}

fn check_analyzer(analyzer_name: Option<&str>) -> Result<()> {
    if let Some(name) = analyzer_name.filter(|name| !name.trim().is_empty()) {
        crate::tokenizer::ensure_analyzer_registered(name).map_err(RagError::InvalidInput)?;
    }
    Ok(())
}

#[async_trait]
impl KeywordIndex for SqliteKeywordIndex {
    fn backend_name(&self) -> &'static str {
        "sqlite-fts"
    }

    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()> {
        if collection.is_empty() {
            return Err(RagError::InvalidInput(
                "collection must not be empty".to_string(),
            ));
        }
        check_analyzer(self.analyzer_name.as_deref())?;
        let mut guard = self.lock()?;
        let tx = guard
            .transaction()
            .map_err(|err| RagError::Keyword(err.to_string()))?;
        for point in points {
            if point.id.is_empty() {
                return Err(RagError::InvalidInput(
                    "point id must not be empty".to_string(),
                ));
            }
            let content = payload_string(&point.payload, crate::document::CONTENT_KEY);
            if content.trim().is_empty() {
                continue;
            }
            let namespace = payload_string(&point.payload, vector_contracts::types::NAMESPACE_KEY);
            let source = payload_string(&point.payload, vector_contracts::types::SOURCE_KEY);
            let payload = serde_json::to_string(&point.payload)
                .map_err(|err| RagError::Keyword(err.to_string()))?;
            use wf_types::config::keyword::KeywordTokenizer as Selection;
            let index_content = match self.tokenizer {
                Selection::English => content,
                Selection::Mixed => crate::tokenizer::pretokenize(
                    &content,
                    self.tokenizer,
                    self.analyzer_name.as_deref(),
                ),
            };
            if index_content.trim().is_empty() {
                continue;
            }
            tx.execute(
                "DELETE FROM keyword_fts WHERE collection = ?1 AND chunk_id = ?2",
                rusqlite::params![collection, point.id],
            )
            .map_err(|err| RagError::Keyword(err.to_string()))?;
            tx.execute(
                "INSERT INTO keyword_fts (collection, chunk_id, namespace, source, content, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![collection, point.id, namespace, source, index_content, payload],
            )
            .map_err(|err| RagError::Keyword(err.to_string()))?;
        }
        tx.commit()
            .map_err(|err| RagError::Keyword(err.to_string()))?;
        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        query: &str,
        filter: Option<&SearchFilter>,
        limit: usize,
    ) -> Result<Vec<RetrievedChunk>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        check_analyzer(self.analyzer_name.as_deref())?;
        let Some(match_query) =
            fts_query_for_with(query, self.tokenizer, self.analyzer_name.as_deref())
        else {
            return Ok(Vec::new());
        };
        let namespace = filter.and_then(|item| item.namespace.clone());
        let source = filter.and_then(|item| item.source.clone());
        let fetch = limit.saturating_mul(3).max(limit).max(10);
        let rows: Vec<(String, f64, String)> = {
            let guard = self.lock()?;
            match (namespace.as_deref(), source.as_deref()) {
                (Some(namespace), Some(source)) => {
                    let sql = format!("SELECT chunk_id, payload, bm25(keyword_fts) AS rank FROM keyword_fts WHERE keyword_fts MATCH ?1 AND collection = ?2 AND namespace = ?3 AND source = ?4 ORDER BY rank LIMIT {fetch}");
                    let mut stmt = guard
                        .prepare(&sql)
                        .map_err(|err| RagError::Keyword(err.to_string()))?;
                    let mut rows = stmt
                        .query_map(
                            rusqlite::params![match_query, collection, namespace, source],
                            |row| {
                                let chunk_id: String = row.get(0)?;
                                let payload: String = row.get(1)?;
                                let rank: f64 = row.get(2)?;
                                Ok((chunk_id, rank, payload))
                            },
                        )
                        .map_err(|err| RagError::Keyword(err.to_string()))?;
                    let mut collected = Vec::new();
                    for row in rows.by_ref() {
                        collected.push(row.map_err(|err| RagError::Keyword(err.to_string()))?);
                        if collected.len() >= fetch {
                            break;
                        }
                    }
                    collected
                }
                (Some(namespace), None) => {
                    let sql = format!("SELECT chunk_id, payload, bm25(keyword_fts) AS rank FROM keyword_fts WHERE keyword_fts MATCH ?1 AND collection = ?2 AND namespace = ?3 ORDER BY rank LIMIT {fetch}");
                    let mut stmt = guard
                        .prepare(&sql)
                        .map_err(|err| RagError::Keyword(err.to_string()))?;
                    let mut rows = stmt
                        .query_map(
                            rusqlite::params![match_query, collection, namespace],
                            |row| {
                                let chunk_id: String = row.get(0)?;
                                let payload: String = row.get(1)?;
                                let rank: f64 = row.get(2)?;
                                Ok((chunk_id, rank, payload))
                            },
                        )
                        .map_err(|err| RagError::Keyword(err.to_string()))?;
                    let mut collected = Vec::new();
                    for row in rows.by_ref() {
                        collected.push(row.map_err(|err| RagError::Keyword(err.to_string()))?);
                        if collected.len() >= fetch {
                            break;
                        }
                    }
                    collected
                }
                (None, Some(source)) => {
                    let sql = format!("SELECT chunk_id, payload, bm25(keyword_fts) AS rank FROM keyword_fts WHERE keyword_fts MATCH ?1 AND collection = ?2 AND source = ?3 ORDER BY rank LIMIT {fetch}");
                    let mut stmt = guard
                        .prepare(&sql)
                        .map_err(|err| RagError::Keyword(err.to_string()))?;
                    let mut rows = stmt
                        .query_map(rusqlite::params![match_query, collection, source], |row| {
                            let chunk_id: String = row.get(0)?;
                            let payload: String = row.get(1)?;
                            let rank: f64 = row.get(2)?;
                            Ok((chunk_id, rank, payload))
                        })
                        .map_err(|err| RagError::Keyword(err.to_string()))?;
                    let mut collected = Vec::new();
                    for row in rows.by_ref() {
                        collected.push(row.map_err(|err| RagError::Keyword(err.to_string()))?);
                        if collected.len() >= fetch {
                            break;
                        }
                    }
                    collected
                }
                (None, None) => {
                    let sql = format!("SELECT chunk_id, payload, bm25(keyword_fts) AS rank FROM keyword_fts WHERE keyword_fts MATCH ?1 AND collection = ?2 ORDER BY rank LIMIT {fetch}");
                    let mut stmt = guard
                        .prepare(&sql)
                        .map_err(|err| RagError::Keyword(err.to_string()))?;
                    let mut rows = stmt
                        .query_map(rusqlite::params![match_query, collection], |row| {
                            let chunk_id: String = row.get(0)?;
                            let payload: String = row.get(1)?;
                            let rank: f64 = row.get(2)?;
                            Ok((chunk_id, rank, payload))
                        })
                        .map_err(|err| RagError::Keyword(err.to_string()))?;
                    let mut collected = Vec::new();
                    for row in rows.by_ref() {
                        collected.push(row.map_err(|err| RagError::Keyword(err.to_string()))?);
                        if collected.len() >= fetch {
                            break;
                        }
                    }
                    collected
                }
            }
        };
        let mut hits = Vec::new();
        for (chunk_id, rank, payload) in rows {
            let payload_map: HashMap<String, serde_json::Value> =
                serde_json::from_str(&payload).map_err(|err| RagError::Keyword(err.to_string()))?;
            if let Some(wanted) = filter {
                if !wanted.matches(&payload_map) {
                    continue;
                }
            }
            let scored = vector_contracts::ScoredPoint {
                id: chunk_id,
                score: (-rank) as f32,
                payload: payload_map,
            };
            if let Ok(chunk) = chunk_from_hit(&scored) {
                hits.push(chunk);
            }
            if hits.len() >= limit {
                break;
            }
        }
        Ok(hits)
    }

    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let guard = self.lock()?;
        let mut removed = 0;
        for id in ids {
            let count = guard
                .execute(
                    "DELETE FROM keyword_fts WHERE collection = ?1 AND chunk_id = ?2",
                    rusqlite::params![collection, id],
                )
                .map_err(|err| RagError::Keyword(err.to_string()))?;
            removed += count as u64;
        }
        Ok(removed)
    }

    async fn delete_by_source(
        &self,
        collection: &str,
        namespace: Option<&str>,
        source: &str,
    ) -> Result<u64> {
        let guard = self.lock()?;
        let removed = if let Some(namespace) = namespace {
            guard
                .execute(
                    "DELETE FROM keyword_fts WHERE collection = ?1 AND namespace = ?2 AND source = ?3",
                    rusqlite::params![collection, namespace, source],
                )
                .map_err(|err| RagError::Keyword(err.to_string()))?
        } else {
            guard
                .execute(
                    "DELETE FROM keyword_fts WHERE collection = ?1 AND source = ?2",
                    rusqlite::params![collection, source],
                )
                .map_err(|err| RagError::Keyword(err.to_string()))?
        };
        Ok(removed as u64)
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64> {
        let guard = self.lock()?;
        let (namespace, source) = match filter {
            Some(item) => (item.namespace.clone(), item.source.clone()),
            None => (None, None),
        };
        let count: i64 = match (namespace, source) {
            (Some(namespace), Some(source)) => guard.query_row("SELECT COUNT(*) FROM keyword_fts WHERE collection = ?1 AND namespace = ?2 AND source = ?3", rusqlite::params![collection, namespace, source], |row| row.get(0)).map_err(|err| RagError::Keyword(err.to_string()))?,
            (Some(namespace), None) => guard.query_row("SELECT COUNT(*) FROM keyword_fts WHERE collection = ?1 AND namespace = ?2", rusqlite::params![collection, namespace], |row| row.get(0)).map_err(|err| RagError::Keyword(err.to_string()))?,
            (None, Some(source)) => guard.query_row("SELECT COUNT(*) FROM keyword_fts WHERE collection = ?1 AND source = ?2", rusqlite::params![collection, source], |row| row.get(0)).map_err(|err| RagError::Keyword(err.to_string()))?,
            (None, None) => guard.query_row("SELECT COUNT(*) FROM keyword_fts WHERE collection = ?1", rusqlite::params![collection], |row| row.get(0)).map_err(|err| RagError::Keyword(err.to_string()))?,
        };
        Ok(count.max(0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(id: &str, content: &str, namespace: &str, source: &str) -> VectorPoint {
        VectorPoint::new(id, vec![0.0; 2]).with_payload(HashMap::from([
            ("content".to_string(), serde_json::json!(content)),
            ("chunk_id".to_string(), serde_json::json!(id)),
            ("document_id".to_string(), serde_json::json!("d")),
            ("namespace".to_string(), serde_json::json!(namespace)),
            ("source".to_string(), serde_json::json!(source)),
        ]))
    }

    #[tokio::test]
    async fn sqlite_roundtrip_filters_and_persists() {
        let dir = std::env::temp_dir().join(format!("rag-fts-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("keyword.db");
        let _ = std::fs::remove_file(&path);
        let index = SqliteKeywordIndex::open(path.to_str().expect("path")).expect("open");
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
        let filter = SearchFilter::new().with_namespace("kb");
        let hits = index
            .search("kb", "rust ownership", Some(&filter), 5)
            .await
            .expect("search");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].chunk_id, "a");
        assert_eq!(index.count("kb", None).await.expect("count"), 2);
        assert_eq!(
            index
                .delete_by_source("kb", Some("kb"), "a.md")
                .await
                .expect("delete"),
            1
        );
        drop(index);
        let reopened = SqliteKeywordIndex::open(path.to_str().expect("path")).expect("reopen");
        assert_eq!(reopened.count("kb", None).await.expect("count"), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn sqlite_mixed_finds_chinese_terms() {
        use wf_types::config::keyword::KeywordTokenizer;
        let index =
            SqliteKeywordIndex::open_in_memory_with(KeywordTokenizer::Mixed, None).expect("open");
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
        let filter = SearchFilter::new().with_namespace("kb");
        let hits = index
            .search("kb", "所有权", Some(&filter), 5)
            .await
            .expect("search");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].chunk_id, "a");
    }

    #[test]
    fn unknown_analyzer_fails_fast() {
        use wf_types::config::keyword::KeywordTokenizer;
        let index = SqliteKeywordIndex::open_in_memory_with(
            KeywordTokenizer::English,
            Some("missing".to_string()),
        )
        .expect("open");
        let filter = SearchFilter::new();
        let err = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt")
            .block_on(index.search("kb", "hello", Some(&filter), 5))
            .expect_err("unknown analyzer must fail");
        assert!(err.to_string().contains("unknown text analyzer"));
    }
}
