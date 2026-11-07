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
use crate::retriever::{chunk_from_hit, RetrievedChunk};

/// Heap budget for the index writer (50 MiB, single embedded knowledge base).
const WRITER_HEAP_BYTES: usize = 50_000_000;
/// Upper bound on query terms; longer queries truncate to this prefix.
const MAX_QUERY_TERMS: usize = 32;
/// Documents per commit; one batch is one commit so huge ingests do not
/// hold a single oversized transaction.
const UPSERT_BATCH_SIZE: usize = 256;

/// Separator joining collection and chunk id into one exact uid term.
/// Unit separators never appear in collection names or chunk ids.
const UID_SEPARATOR: char = '\u{1f}';

fn keyword_error(err: impl std::fmt::Display) -> RagError {
    RagError::Keyword(err.to_string())
}

fn uid_for(collection: &str, chunk_id: &str) -> String {
    format!("{collection}{UID_SEPARATOR}{chunk_id}")
}

#[derive(Clone, Copy)]
struct Fields {
    collection: Field,
    uid: Field,
    chunk_id: Field,
    namespace: Field,
    source: Field,
    content: Field,
    payload: Field,
}

/// Mixed segmentation tokenizer for the embedded index.
#[derive(Debug, Clone, Default)]
struct MixedTokenizer;

/// Mixed stream carrying owned tantivy tokens.
struct OwnedMixedStream {
    tokens: Vec<tantivy::tokenizer::Token>,
    next: usize,
    current: tantivy::tokenizer::Token,
}

impl tantivy::tokenizer::TokenStream for OwnedMixedStream {
    fn advance(&mut self) -> bool {
        if self.next >= self.tokens.len() {
            return false;
        }
        self.current = self.tokens[self.next].clone();
        self.next += 1;
        true
    }

    fn token(&self) -> &tantivy::tokenizer::Token {
        &self.current
    }

    fn token_mut(&mut self) -> &mut tantivy::tokenizer::Token {
        &mut self.current
    }
}

impl tantivy::tokenizer::Tokenizer for MixedTokenizer {
    type TokenStream<'a> = OwnedMixedStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let tokens = crate::tokenizer::analyze_mixed(text)
            .into_iter()
            .map(|token| tantivy::tokenizer::Token {
                text: token.text,
                offset_from: token.offset_from,
                offset_to: token.offset_to,
                position: token.position as usize,
                position_length: token.position_length as usize,
            })
            .collect();
        OwnedMixedStream {
            tokens,
            next: 0,
            current: tantivy::tokenizer::Token::default(),
        }
    }
}

fn build_schema_with(tokenizer: wf_types::config::keyword::KeywordTokenizer) -> (Schema, Fields) {
    use wf_types::config::keyword::KeywordTokenizer as Selection;
    let mut builder = Schema::builder();
    let collection = builder.add_text_field("collection", STRING | STORED);
    let uid = builder.add_text_field("uid", STRING | STORED);
    let chunk_id = builder.add_text_field("chunk_id", STRING | STORED);
    let namespace = builder.add_text_field("namespace", STRING | STORED);
    let source = builder.add_text_field("source", STRING | STORED);
    let content = match tokenizer {
        Selection::English => builder.add_text_field("content", TEXT | STORED),
        Selection::Mixed => {
            let indexing = tantivy::schema::TextFieldIndexing::default()
                .set_tokenizer("mixed")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions);
            builder.add_text_field(
                "content",
                TextOptions::default()
                    .set_indexing_options(indexing)
                    .set_stored(),
            )
        }
    };
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

/// Embedded full-text keyword backend powered by tantivy.
///
/// Writes hold the writer mutex serially; reads share the reader lock so
/// concurrent queries run in parallel against the latest reloaded snapshot.
/// Field handles are immutable after construction and need no locking.
pub struct TantivyKeywordIndex {
    writer: Mutex<IndexWriter>,
    reader: std::sync::RwLock<IndexReader>,
    fields: Fields,
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
}

impl TantivyKeywordIndex {
    fn from_index(
        index: Index,
        fields: Fields,
        tokenizer: wf_types::config::keyword::KeywordTokenizer,
        analyzer_name: Option<String>,
    ) -> Result<Self> {
        if tokenizer == wf_types::config::keyword::KeywordTokenizer::Mixed {
            index.tokenizers().register("mixed", MixedTokenizer);
        }
        let writer = index.writer(WRITER_HEAP_BYTES).map_err(keyword_error)?;
        let reader = index.reader().map_err(keyword_error)?;
        Ok(Self {
            writer: Mutex::new(writer),
            reader: std::sync::RwLock::new(reader),
            fields,
            tokenizer,
            analyzer_name,
        })
    }

    /// Opens an in-RAM database for tests and ephemeral knowledge bases.
    pub fn open_in_memory() -> Result<Self> {
        Self::open_in_memory_with(wf_types::config::keyword::KeywordTokenizer::English, None)
    }

    /// Opens an in-RAM database with an explicit tokenizer selection.
    pub fn open_in_memory_with(
        tokenizer: wf_types::config::keyword::KeywordTokenizer,
        analyzer_name: Option<String>,
    ) -> Result<Self> {
        let (schema, fields) = build_schema_with(tokenizer);
        let index = Index::create_in_ram(schema);
        Self::from_index(index, fields, tokenizer, analyzer_name)
    }

    /// Opens or creates a file database rooted at the directory.
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
                "tantivy keyword path must not be empty".to_string(),
            ));
        }
        std::fs::create_dir_all(path)
            .map_err(|err| RagError::Keyword(format!("create tantivy dir '{path}': {err}")))?;
        let dir = MmapDirectory::open(path).map_err(keyword_error)?;
        let (schema, fields) = build_schema_with(tokenizer);
        let index = Index::open_or_create(dir, schema).map_err(keyword_error)?;
        // Existing directories keep the stored schema; registering the
        // mixed tokenizer stays harmless when the field uses it.
        Self::from_index(index, fields, tokenizer, analyzer_name)
    }

    fn lock_writer(&self) -> Result<std::sync::MutexGuard<'_, IndexWriter>> {
        self.writer
            .lock()
            .map_err(|err| RagError::Keyword(err.to_string()))
    }

    fn lock_reader(&self) -> Result<std::sync::RwLockReadGuard<'_, IndexReader>> {
        self.reader
            .read()
            .map_err(|err| RagError::Keyword(err.to_string()))
    }

    fn reload(&self) -> Result<()> {
        self.lock_reader()?.reload().map_err(keyword_error)
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

/// Content query with an explicit tokenizer selection.
fn content_query_with(
    fields: &Fields,
    question: &str,
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<&str>,
) -> Option<BooleanQuery> {
    let mut terms: Vec<String> = crate::tokenizer::terms(question, tokenizer, analyzer_name);
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
        if let Some(name) = self
            .analyzer_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            crate::tokenizer::ensure_analyzer_registered(name).map_err(RagError::InvalidInput)?;
        }
        for batch in points.chunks(UPSERT_BATCH_SIZE.max(1)) {
            let mut writer = self.lock_writer()?;
            for point in batch {
                if point.id.is_empty() {
                    return Err(RagError::InvalidInput(
                        "point id must not be empty".to_string(),
                    ));
                }
                let content = payload_string(&point.payload, crate::document::CONTENT_KEY);
                if content.trim().is_empty() {
                    continue;
                }
                let namespace =
                    payload_string(&point.payload, vector_contracts::types::NAMESPACE_KEY);
                let source = payload_string(&point.payload, vector_contracts::types::SOURCE_KEY);
                let payload = serde_json::to_string(&point.payload)
                    .map_err(|err| RagError::Keyword(err.to_string()))?;
                writer.delete_term(Term::from_field_text(
                    self.fields.uid,
                    &uid_for(collection, &point.id),
                ));
                writer
                    .add_document(doc!(
                        self.fields.collection => collection,
                        self.fields.uid => uid_for(collection, &point.id),
                        self.fields.chunk_id => point.id.clone(),
                        self.fields.namespace => namespace,
                        self.fields.source => source,
                        self.fields.content => content,
                        self.fields.payload => payload
                    ))
                    .map_err(keyword_error)?;
            }
            writer.commit().map_err(keyword_error)?;
            drop(writer);
            self.reload()?;
        }
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
        if let Some(name) = self
            .analyzer_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            crate::tokenizer::ensure_analyzer_registered(name).map_err(RagError::InvalidInput)?;
        }
        let Some(content) = content_query_with(
            &self.fields,
            query,
            self.tokenizer,
            self.analyzer_name.as_deref(),
        ) else {
            return Ok(Vec::new());
        };
        let fetch = limit.saturating_mul(3).max(limit).max(10);
        let rows: Vec<(f32, TantivyDocument)> = {
            let reader = self.lock_reader()?;
            let mut clauses = vec![(
                Occur::Must,
                Box::new(content) as Box<dyn tantivy::query::Query>,
            )];
            let scope = scope_query(&self.fields, collection, filter);
            clauses.push((
                Occur::Must,
                Box::new(scope) as Box<dyn tantivy::query::Query>,
            ));
            let merged = BooleanQuery::new(clauses);
            let searcher = reader.searcher();
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
        let mut hits = Vec::new();
        for (score, doc) in rows {
            let Some(payload_raw) = stored_string(&doc, self.fields.payload) else {
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
            let Some(chunk_id) = stored_string(&doc, self.fields.chunk_id) else {
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
        let found: Vec<bool> = {
            let reader = self.lock_reader()?;
            let searcher = reader.searcher();
            let mut found = Vec::with_capacity(ids.len());
            for id in ids {
                let term = Term::from_field_text(self.fields.uid, &uid_for(collection, id));
                let query = TermQuery::new(term, IndexRecordOption::Basic);
                let count: usize = searcher.search(&query, &Count).map_err(keyword_error)?;
                found.push(count > 0);
            }
            found
        };
        let mut writer = self.lock_writer()?;
        let mut removed = 0_u64;
        for (id, present) in ids.iter().zip(found) {
            let term = Term::from_field_text(self.fields.uid, &uid_for(collection, id));
            writer.delete_term(term);
            if present {
                removed += 1;
            }
        }
        writer.commit().map_err(keyword_error)?;
        drop(writer);
        self.reload()?;
        Ok(removed)
    }

    async fn delete_by_source(
        &self,
        collection: &str,
        namespace: Option<&str>,
        source: &str,
    ) -> Result<u64> {
        let filter = SearchFilter::new().with_source(source);
        let filter = match namespace {
            Some(namespace) => filter.with_namespace(namespace),
            None => filter,
        };
        let scope = scope_query(&self.fields, collection, Some(&filter));
        let found: usize = {
            let reader = self.lock_reader()?;
            reader
                .searcher()
                .search(&scope, &Count)
                .map_err(keyword_error)?
        };
        let mut writer = self.lock_writer()?;
        writer
            .delete_query(Box::new(scope))
            .map_err(keyword_error)?;
        writer.commit().map_err(keyword_error)?;
        drop(writer);
        self.reload()?;
        Ok(found as u64)
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64> {
        let scope = scope_query(&self.fields, collection, filter);
        let reader = self.lock_reader()?;
        let found: usize = reader
            .searcher()
            .search(&scope, &Count)
            .map_err(keyword_error)?;
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

    fn tagged_point(id: &str, content: &str, lang: &str) -> VectorPoint {
        VectorPoint::new(id, vec![0.0; 2]).with_payload(HashMap::from([
            ("content".to_string(), serde_json::json!(content)),
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
        let filter = SearchFilter::new().with_namespace("kb");
        let sqlite_hits = sqlite
            .search("kb", "rust ownership", Some(&filter), 5)
            .await
            .expect("search");
        let tantivy_hits = tantivy
            .search("kb", "rust ownership", Some(&filter), 5)
            .await
            .expect("search");
        let sqlite_ids: Vec<&str> = sqlite_hits
            .iter()
            .map(|hit| hit.chunk_id.as_str())
            .collect();
        let tantivy_ids: Vec<&str> = tantivy_hits
            .iter()
            .map(|hit| hit.chunk_id.as_str())
            .collect();
        assert_eq!(sqlite_ids, tantivy_ids);
        assert_eq!(sqlite_ids.first(), Some(&"a"));
        let scoped = SearchFilter::new().with_namespace("kb").with_source("a.md");
        assert_eq!(
            sqlite.count("kb", Some(&scoped)).await.expect("count"),
            tantivy.count("kb", Some(&scoped)).await.expect("count")
        );
        let generic = SearchFilter::new()
            .with_namespace("kb")
            .with_field("lang", "en");
        let sqlite_generic = sqlite
            .search("kb", "rust", Some(&generic), 5)
            .await
            .expect("search");
        let tantivy_generic = tantivy
            .search("kb", "rust", Some(&generic), 5)
            .await
            .expect("search");
        assert_eq!(sqlite_generic.len(), tantivy_generic.len());
        assert!(sqlite_generic.iter().all(|hit| hit.chunk_id != "c"));
        assert!(tantivy_generic.iter().all(|hit| hit.chunk_id != "c"));
        assert!(sqlite
            .search("kb", "   ", Some(&filter), 5)
            .await
            .expect("empty")
            .is_empty());
        assert!(tantivy
            .search("kb", "   ", Some(&filter), 5)
            .await
            .expect("empty")
            .is_empty());
        assert!(sqlite
            .search("kb", "rust", Some(&filter), 0)
            .await
            .expect("zero")
            .is_empty());
        assert!(tantivy
            .search("kb", "rust", Some(&filter), 0)
            .await
            .expect("zero")
            .is_empty());
        let selective = SearchFilter::new()
            .with_namespace("kb")
            .with_source("missing.md");
        assert!(sqlite
            .search("kb", "rust", Some(&selective), 5)
            .await
            .expect("selective")
            .is_empty());
        assert!(tantivy
            .search("kb", "rust", Some(&selective), 5)
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
                let filter = SearchFilter::new().with_namespace("kb");
                owned
                    .search("kb", "concurrent rust", Some(&filter), 5)
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
        let filter = SearchFilter::new().with_namespace("kb");
        let hits = index
            .search("kb", "所有权", Some(&filter), 5)
            .await
            .expect("search");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].chunk_id, "a");
        let english = index
            .search("kb", "rust", Some(&filter), 5)
            .await
            .expect("search");
        assert!(english.iter().any(|hit| hit.chunk_id == "a"));
    }

    #[tokio::test]
    async fn tantivy_mixed_matches_sqlite_on_chinese_corpus() {
        use crate::sqlite_index::SqliteKeywordIndex;
        use wf_types::config::keyword::KeywordTokenizer;
        let sqlite =
            SqliteKeywordIndex::open_in_memory_with(KeywordTokenizer::Mixed, None).expect("sqlite");
        let tantivy = TantivyKeywordIndex::open_in_memory_with(KeywordTokenizer::Mixed, None)
            .expect("tantivy");
        let points = vec![
            point("a", "rust所有权借用检查", "kb", "a.md"),
            point("b", "go并发调度", "kb", "b.md"),
        ];
        sqlite.upsert("kb", &points).await.expect("upsert");
        tantivy.upsert("kb", &points).await.expect("upsert");
        let filter = SearchFilter::new().with_namespace("kb");
        let sqlite_hits = sqlite
            .search("kb", "所有权", Some(&filter), 5)
            .await
            .expect("search");
        let tantivy_hits = tantivy
            .search("kb", "所有权", Some(&filter), 5)
            .await
            .expect("search");
        assert!(!sqlite_hits.is_empty());
        assert!(!tantivy_hits.is_empty());
        assert_eq!(sqlite_hits[0].chunk_id, tantivy_hits[0].chunk_id);
    }
}
