//! Embedded tantivy keyword backend: a persistent inverted index.
//!
//! One tantivy index carries every collection; the content field is the
//! only analyzed field while collection and scope fields stay exact.
//! Relevance comes from the native BM25 scorer; native scopes are applied
//! in the index while generic payload conditions stay with the RAG adapter.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{
    KeywordDocument, KeywordError, KeywordFormat, KeywordIndex, KeywordQuery, KeywordScope,
    Result as KeywordResult,
};
use async_trait::async_trait;
use tantivy::collector::{Count, TopDocs};
use tantivy::directory::MmapDirectory;
use tantivy::query::{BooleanQuery, Occur, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, Schema, TextOptions, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, TantivyDocument, Term};
use vector_contracts::ScoredPoint;

/// Heap budget for the index writer (50 MiB, single embedded knowledge base).
const WRITER_HEAP_BYTES: usize = 50_000_000;

/// Sidecar file inside the index directory carrying the format stamp.
const FORMAT_SIDECAR: &str = "keyword-format.stamp";
/// Upper bound on query terms; longer queries truncate to this prefix.
const MAX_QUERY_TERMS: usize = 32;
/// Documents per commit; one batch is one commit so huge ingests do not
/// hold a single oversized transaction.
const UPSERT_BATCH_SIZE: usize = 256;

/// Separator joining collection and chunk id into one exact uid term.
/// Unit separators never appear in collection names or chunk ids.
const UID_SEPARATOR: char = '\u{1f}';

fn keyword_error(err: impl std::fmt::Display) -> KeywordError {
    KeywordError::backend(err.to_string())
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
    analyzers: std::sync::Arc<crate::tokenizer::AnalyzerRegistry>,
    /// Set when the index opened under the read-only degrade policy after
    /// a format mismatch; every write path fails with this flag set.
    readonly: std::sync::atomic::AtomicBool,
}

impl TantivyKeywordIndex {
    fn from_index(
        index: Index,
        fields: Fields,
        tokenizer: wf_types::config::keyword::KeywordTokenizer,
        analyzer_name: Option<String>,
        writer_heap_bytes: usize,
    ) -> KeywordResult<Self> {
        if tokenizer == wf_types::config::keyword::KeywordTokenizer::Mixed {
            index.tokenizers().register("mixed", MixedTokenizer);
        }
        let writer = index.writer(writer_heap_bytes).map_err(keyword_error)?;
        let reader = index.reader().map_err(keyword_error)?;
        Ok(Self {
            writer: Mutex::new(writer),
            reader: std::sync::RwLock::new(reader),
            fields,
            tokenizer,
            analyzer_name,
            analyzers: std::sync::Arc::new(crate::tokenizer::AnalyzerRegistry::new()),
            readonly: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Opens an in-RAM database for tests and ephemeral knowledge bases.
    pub fn open_in_memory() -> KeywordResult<Self> {
        Self::open_in_memory_with(wf_types::config::keyword::KeywordTokenizer::English, None)
    }

    /// Opens an in-RAM database with an explicit tokenizer selection.
    pub fn open_in_memory_with(
        tokenizer: wf_types::config::keyword::KeywordTokenizer,
        analyzer_name: Option<String>,
    ) -> KeywordResult<Self> {
        let (schema, fields) = build_schema_with(tokenizer);
        let index = Index::create_in_ram(schema);
        Self::from_index(index, fields, tokenizer, analyzer_name, WRITER_HEAP_BYTES)
    }

    /// Attaches the host scope's analyzer registry.
    pub fn with_analyzers(
        mut self,
        analyzers: std::sync::Arc<crate::tokenizer::AnalyzerRegistry>,
    ) -> Self {
        self.analyzers = analyzers;
        self
    }

    fn check_analyzer(&self) -> KeywordResult<()> {
        if let Some(name) = self
            .analyzer_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            self.analyzers
                .ensure(name)
                .map_err(KeywordError::invalid_input)?;
        }
        Ok(())
    }

    /// Opens or creates a file database rooted at the directory.
    pub fn open(path: &str) -> KeywordResult<Self> {
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
    ) -> KeywordResult<Self> {
        let format = KeywordFormat::new(tokenizer, analyzer_name);
        Self::open_with_format(path, format, None)
    }

    /// Opens or creates a file database with an explicit format stamp and
    /// writer heap budget. An existing directory whose stamp differs from
    /// the resolved format fails under the reject policy or opens
    /// read-only under the degrade policy, so mixed tokenization never
    /// coexists inside one index.
    pub fn open_with_format(
        path: &str,
        format: KeywordFormat,
        writer_heap_bytes: Option<usize>,
    ) -> KeywordResult<Self> {
        if path.trim().is_empty() {
            return Err(KeywordError::invalid_input(
                "tantivy keyword path must not be empty",
            ));
        }
        std::fs::create_dir_all(path)
            .map_err(|err| KeywordError::backend(format!("create tantivy dir '{path}': {err}")))?;
        let stamp_path = std::path::Path::new(path).join(FORMAT_SIDECAR);
        let existing = std::fs::read_to_string(&stamp_path)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let current = format.id();
        let mut readonly = false;
        match existing {
            None => {}
            Some(existing) if existing == current => {}
            Some(existing) => {
                if format.policy == wf_types::config::keyword::KeywordFormatPolicy::ReadOnly {
                    readonly = true;
                } else {
                    return Err(KeywordFormat::mismatch_error(&existing, &current));
                }
            }
        }
        let dir = MmapDirectory::open(path).map_err(keyword_error)?;
        let (schema, fields) = build_schema_with(format.tokenizer);
        let index = Index::open_or_create(dir, schema).map_err(keyword_error)?;
        // Existing directories keep the stored schema; registering the
        // mixed tokenizer stays harmless when the field uses it.
        let index = Self::from_index(
            index,
            fields,
            format.tokenizer,
            format.analyzer_name.clone(),
            writer_heap_bytes.unwrap_or(WRITER_HEAP_BYTES),
        )?;
        if !readonly {
            std::fs::write(&stamp_path, &current).map_err(|err| {
                KeywordError::backend(format!("write tantivy format stamp: {err}"))
            })?;
        } else {
            index
                .readonly
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(index)
    }

    fn ensure_writable(&self) -> KeywordResult<()> {
        if self.readonly.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(KeywordError::backend(
                "tantivy keyword index opened read-only after a format mismatch; rebuild before writing",
            ));
        }
        Ok(())
    }

    fn lock_writer(&self) -> KeywordResult<std::sync::MutexGuard<'_, IndexWriter>> {
        self.writer.lock().map_err(keyword_error)
    }

    fn lock_reader(&self) -> KeywordResult<std::sync::RwLockReadGuard<'_, IndexReader>> {
        self.reader.read().map_err(keyword_error)
    }

    fn reload(&self) -> KeywordResult<()> {
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
/// Generic payload conditions stay out; the RAG adapter rechecks them.
fn scope_query(fields: &Fields, collection: &str, scope: Option<&KeywordScope>) -> BooleanQuery {
    let mut clauses = vec![must_term(fields.collection, collection)];
    if let Some(scope) = scope {
        if let Some(namespace) = &scope.namespace {
            clauses.push(must_term(fields.namespace, namespace));
        }
        if let Some(source) = &scope.source {
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
    analyzers: &crate::tokenizer::AnalyzerRegistry,
) -> Option<BooleanQuery> {
    let mut terms: Vec<String> =
        crate::tokenizer::terms(question, tokenizer, analyzer_name, analyzers);
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

    fn supported_tokenizers(&self) -> Vec<wf_types::config::keyword::KeywordTokenizer> {
        vec![
            wf_types::config::keyword::KeywordTokenizer::English,
            wf_types::config::keyword::KeywordTokenizer::Mixed,
        ]
    }

    fn supports_custom_analyzer(&self) -> bool {
        true
    }

    async fn ensure_collection(&self, collection: &str) -> KeywordResult<bool> {
        if collection.is_empty() {
            return Err(KeywordError::invalid_input("collection must not be empty"));
        }
        Ok(false)
    }

    async fn delete_collection(&self, collection: &str) -> KeywordResult<()> {
        if collection.is_empty() {
            return Err(KeywordError::invalid_input("collection must not be empty"));
        }
        self.ensure_writable()?;
        let scope = scope_query(&self.fields, collection, None);
        let mut writer = self.lock_writer()?;
        writer
            .delete_query(Box::new(scope))
            .map_err(keyword_error)?;
        writer.commit().map_err(keyword_error)?;
        drop(writer);
        self.reload()?;
        Ok(())
    }

    async fn upsert(&self, collection: &str, documents: &[KeywordDocument]) -> KeywordResult<()> {
        if collection.is_empty() {
            return Err(KeywordError::invalid_input("collection must not be empty"));
        }
        self.check_analyzer()?;
        self.ensure_writable()?;
        for batch in documents.chunks(UPSERT_BATCH_SIZE.max(1)) {
            let mut writer = self.lock_writer()?;
            for document in batch {
                if document.id.is_empty() {
                    return Err(KeywordError::invalid_input("document id must not be empty"));
                }
                let namespace =
                    payload_string(&document.payload, vector_contracts::types::NAMESPACE_KEY);
                let source = payload_string(&document.payload, vector_contracts::types::SOURCE_KEY);
                let payload = serde_json::to_string(&document.payload).map_err(keyword_error)?;
                writer.delete_term(Term::from_field_text(
                    self.fields.uid,
                    &uid_for(collection, &document.id),
                ));
                writer
                    .add_document(doc!(
                        self.fields.collection => collection,
                        self.fields.uid => uid_for(collection, &document.id),
                        self.fields.chunk_id => document.id.clone(),
                        self.fields.namespace => namespace,
                        self.fields.source => source,
                        self.fields.content => document.text.clone(),
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
        query: &KeywordQuery,
    ) -> KeywordResult<Vec<ScoredPoint>> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }
        self.check_analyzer()?;
        let Some(content) = content_query_with(
            &self.fields,
            &query.text,
            self.tokenizer,
            self.analyzer_name.as_deref(),
            &self.analyzers,
        ) else {
            return Ok(Vec::new());
        };
        let scope = query.scope.as_ref();
        let fetch = query.limit;
        let rows: Vec<(f32, TantivyDocument)> = {
            let reader = self.lock_reader()?;
            let mut clauses = vec![(
                Occur::Must,
                Box::new(content) as Box<dyn tantivy::query::Query>,
            )];
            let scope_clause = scope_query(&self.fields, collection, scope);
            clauses.push((
                Occur::Must,
                Box::new(scope_clause) as Box<dyn tantivy::query::Query>,
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
                serde_json::from_str(&payload_raw).map_err(keyword_error)?;
            let Some(chunk_id) = stored_string(&doc, self.fields.chunk_id) else {
                continue;
            };
            hits.push(ScoredPoint {
                id: chunk_id,
                score,
                payload: payload_map,
            });
            if hits.len() >= query.limit {
                break;
            }
        }
        Ok(hits)
    }

    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> KeywordResult<u64> {
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
        self.ensure_writable()?;
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
    ) -> KeywordResult<u64> {
        self.ensure_writable()?;
        let mut keyword_scope = KeywordScope::new().with_source(source);
        if let Some(namespace) = namespace {
            keyword_scope = keyword_scope.with_namespace(namespace);
        }
        let scope = scope_query(&self.fields, collection, Some(&keyword_scope));
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

    async fn count(&self, collection: &str, scope: Option<&KeywordScope>) -> KeywordResult<u64> {
        let scope = scope_query(&self.fields, collection, scope);
        let reader = self.lock_reader()?;
        let found: usize = reader
            .searcher()
            .search(&scope, &Count)
            .map_err(keyword_error)?;
        Ok(found as u64)
    }
}

#[cfg(test)]
mod test;
