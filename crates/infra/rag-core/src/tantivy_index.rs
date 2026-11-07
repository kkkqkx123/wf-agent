//! Embedded tantivy keyword backend: a persistent inverted index.
//!
//! One tantivy index carries every collection; the content field is the
//! only analyzed field while collection and scope fields stay exact.
//! Relevance comes from the native BM25 scorer; generic field conditions
//! are rechecked in Rust so keyword filtering shares one match semantic.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use tantivy::collector::{Count, TopDocs};
use tantivy::directory::MmapDirectory;
use tantivy::query::{BooleanQuery, Occur, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, Schema, TextOptions, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, TantivyDocument, Term};
use vector_contracts::{SearchFilter, VectorPoint};

use crate::error::{RagError, Result};
use crate::keyword_index::KeywordIndex;
use crate::retriever::{chunk_from_hit, tokenize, RetrievedChunk};

/// Heap budget for the index writer (50 MiB, single embedded knowledge base).
const WRITER_HEAP_BYTES: usize = 50_000_000;
/// Upper bound on query terms; longer queries truncate to this prefix.
const MAX_QUERY_TERMS: usize = 32;

/// Separator joining collection and chunk id into one exact uid term.
/// Unit separators never appear in collection names or chunk ids.
const UID_SEPARATOR: char = '\u{1f}';

fn keyword_error(err: impl std::fmt::Display) -> RagError {
    RagError::Keyword(err.to_string())
}

fn uid_for(collection: &str, chunk_id: &str) -> String {
    format!("{collection}{UID_SEPARATOR}{chunk_id}")
}

struct Fields {
    collection: Field,
    uid: Field,
    chunk_id: Field,
    namespace: Field,
    source: Field,
    content: Field,
    payload: Field,
}

fn build_schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let collection = builder.add_text_field("collection", STRING | STORED);
    let uid = builder.add_text_field("uid", STRING | STORED);
    let chunk_id = builder.add_text_field("chunk_id", STRING | STORED);
    let namespace = builder.add_text_field("namespace", STRING | STORED);
    let source = builder.add_text_field("source", STRING | STORED);
    let content = builder.add_text_field("content", TEXT | STORED);
    let payload = builder.add_text_field("payload", TextOptions::default().set_stored());
    let schema = builder.build();
    (
        schema,
        Fields {
            collection,
            uid,
            chunk_id,
            namespace,
            source,
            content,
            payload,
        },
    )
}

struct State {
    writer: IndexWriter,
    reader: IndexReader,
    fields: Fields,
}

/// Embedded full-text keyword backend powered by tantivy.
pub struct TantivyKeywordIndex {
    state: Mutex<State>,
}

impl TantivyKeywordIndex {
    fn from_index(index: Index, fields: Fields) -> Result<Self> {
        let writer = index.writer(WRITER_HEAP_BYTES).map_err(keyword_error)?;
        let reader = index.reader().map_err(keyword_error)?;
        Ok(Self {
            state: Mutex::new(State {
                writer,
                reader,
                fields,
            }),
        })
    }

    /// Opens an in-RAM database for tests and ephemeral knowledge bases.
    pub fn open_in_memory() -> Result<Self> {
        let (schema, fields) = build_schema();
        let index = Index::create_in_ram(schema);
        Self::from_index(index, fields)
    }

    /// Opens or creates a file database rooted at the directory.
    pub fn open(path: &str) -> Result<Self> {
        if path.trim().is_empty() {
            return Err(RagError::InvalidInput(
                "tantivy keyword path must not be empty".to_string(),
            ));
        }
        std::fs::create_dir_all(path)
            .map_err(|err| RagError::Keyword(format!("create tantivy dir '{path}': {err}")))?;
        let dir = MmapDirectory::open(path).map_err(keyword_error)?;
        let (schema, fields) = build_schema();
        let index = Index::open_or_create(dir, schema).map_err(keyword_error)?;
        Self::from_index(index, fields)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>> {
        self.state
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

fn stored_string(doc: &TantivyDocument, field: Field) -> Option<String> {
    doc.get_first(field)
        .and_then(|value| value.as_str().map(str::to_string))
}

/// Exact-term filter on one scope field.
fn must_term(field: Field, value: &str) -> (Occur, Box<dyn tantivy::query::Query>) {
    let term = Term::from_field_text(field, value);
    let query = TermQuery::new(term, IndexRecordOption::Basic);
    (Occur::Must, Box::new(query))
}

/// Native scope query: collection always, namespace and source when set.
/// Generic field conditions stay out; callers recheck them in Rust.
fn scope_query(fields: &Fields, collection: &str, filter: Option<&SearchFilter>) -> BooleanQuery {
    let mut clauses = vec![must_term(fields.collection, collection)];
    if let Some(filter) = filter {
        if let Some(namespace) = &filter.namespace {
            clauses.push(must_term(fields.namespace, namespace));
        }
        if let Some(source) = &filter.source {
            clauses.push(must_term(fields.source, source));
        }
    }
    BooleanQuery::new(clauses)
}

/// Content query: one scored term per token combined with OR.
/// Empty token lists yield no query so callers return early.
fn content_query(fields: &Fields, question: &str) -> Option<BooleanQuery> {
    let mut terms: Vec<String> = tokenize(question);
    terms.sort();
    terms.dedup();
    if terms.is_empty() {
        return None;
    }
    terms.truncate(MAX_QUERY_TERMS);
    let clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = terms
        .into_iter()
        .map(|term| {
            let term = Term::from_field_text(fields.content, &term);
            let query = TermQuery::new(term, IndexRecordOption::WithFreqs);
            (
                Occur::Should,
                Box::new(query) as Box<dyn tantivy::query::Query>,
            )
        })
        .collect();
    Some(BooleanQuery::new(clauses))
}

#[async_trait]
impl KeywordIndex for TantivyKeywordIndex {
    fn backend_name(&self) -> &'static str {
        "tantivy"
    }

    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()> {
        if collection.is_empty() {
            return Err(RagError::InvalidInput(
                "collection must not be empty".to_string(),
            ));
        }
        let mut guard = self.lock()?;
        let fields = &guard.fields;
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
            guard.writer.delete_term(Term::from_field_text(
                fields.uid,
                &uid_for(collection, &point.id),
            ));
            guard
                .writer
                .add_document(doc!(
                    fields.collection => collection,
                    fields.uid => uid_for(collection, &point.id),
                    fields.chunk_id => point.id.clone(),
                    fields.namespace => namespace,
                    fields.source => source,
                    fields.content => content,
                    fields.payload => payload
                ))
                .map_err(keyword_error)?;
        }
        guard.writer.commit().map_err(keyword_error)?;
        guard.reader.reload().map_err(keyword_error)?;
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
        let Some(content) = content_query(&self.lock()?.fields, query) else {
            return Ok(Vec::new());
        };
        let fetch = limit.saturating_mul(3).max(limit).max(10);
        let rows: Vec<(f32, TantivyDocument)> = {
            let guard = self.lock()?;
            let mut clauses = vec![(
                Occur::Must,
                Box::new(content) as Box<dyn tantivy::query::Query>,
            )];
            let scope = scope_query(&guard.fields, collection, filter);
            clauses.push((
                Occur::Must,
                Box::new(scope) as Box<dyn tantivy::query::Query>,
            ));
            let merged = BooleanQuery::new(clauses);
            let searcher = guard.reader.searcher();
            let top: Vec<(f32, tantivy::DocAddress)> = searcher
                .search(&merged, &TopDocs::with_limit(fetch).order_by_score())
                .map_err(keyword_error)?;
            let mut rows = Vec::with_capacity(top.len());
            for (score, address) in top {
                let doc: TantivyDocument = searcher.doc(address).map_err(keyword_error)?;
                rows.push((score, doc));
            }
            rows
        };
        let payload_field = self.lock()?.fields.payload;
        let mut hits = Vec::new();
        for (score, doc) in rows {
            let Some(payload_raw) = stored_string(&doc, payload_field) else {
                continue;
            };
            let payload_map: HashMap<String, serde_json::Value> =
                serde_json::from_str(&payload_raw)
                    .map_err(|err| RagError::Keyword(err.to_string()))?;
            if let Some(wanted) = filter {
                if !wanted.matches(&payload_map) {
                    continue;
                }
            }
            let Some(chunk_id) = stored_string(&doc, self.lock()?.fields.chunk_id) else {
                continue;
            };
            let scored = vector_contracts::ScoredPoint {
                id: chunk_id,
                score,
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
        let mut guard = self.lock()?;
        let searcher = guard.reader.searcher();
        let mut removed = 0_u64;
        for id in ids {
            let term = Term::from_field_text(guard.fields.uid, &uid_for(collection, id));
            let query = TermQuery::new(term.clone(), IndexRecordOption::Basic);
            let found: usize = searcher.search(&query, &Count).map_err(keyword_error)?;
            guard.writer.delete_term(term);
            removed += found.min(1) as u64;
        }
        guard.writer.commit().map_err(keyword_error)?;
        guard.reader.reload().map_err(keyword_error)?;
        Ok(removed)
    }

    async fn delete_by_source(
        &self,
        collection: &str,
        namespace: Option<&str>,
        source: &str,
    ) -> Result<u64> {
        let mut guard = self.lock()?;
        let filter = SearchFilter::new().with_source(source);
        let filter = match namespace {
            Some(namespace) => filter.with_namespace(namespace),
            None => filter,
        };
        let scope = scope_query(&guard.fields, collection, Some(&filter));
        let searcher = guard.reader.searcher();
        let found: usize = searcher.search(&scope, &Count).map_err(keyword_error)?;
        guard
            .writer
            .delete_query(Box::new(scope))
            .map_err(keyword_error)?;
        guard.writer.commit().map_err(keyword_error)?;
        guard.reader.reload().map_err(keyword_error)?;
        Ok(found as u64)
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64> {
        let guard = self.lock()?;
        let scope = scope_query(&guard.fields, collection, filter);
        let searcher = guard.reader.searcher();
        let found: usize = searcher.search(&scope, &Count).map_err(keyword_error)?;
        Ok(found as u64)
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
        let filter = SearchFilter::new().with_namespace("kb");
        let hits = index
            .search("kb", "rust ownership", Some(&filter), 5)
            .await
            .expect("search");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].chunk_id, "a");
        assert_eq!(index.count("kb", None).await.expect("count"), 2);
        assert!(index
            .search("kb", "   ", Some(&filter), 5)
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
}
