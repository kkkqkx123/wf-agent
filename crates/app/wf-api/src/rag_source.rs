//! Source registry and ingestion service for RAG.
//!
//! Readers produce normalized documents and a revision. This module owns
//! source state, change detection and the write path; readers never touch
//! indexes and pipelines never touch readers.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use source_contracts::{SourceDescriptor, SourceReader};
use tracing::{info, warn};
use vector_contracts::VectorError;
use wf_storage::backend::StorageBackend;
use wf_storage::domain::store::{QueryFilter, Store};

use crate::infra::error::{ApiError, ApiResult};
use crate::rag::RagHandles;

/// Lifecycle state of one indexed source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RagSourceStatus {
    #[default]
    Active,
    Failed,
}

fn default_vector_kind() -> String {
    "dense".to_string()
}

/// Persisted state of one source in one collection and namespace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagSourceState {
    pub collection: String,
    pub namespace: String,
    pub source: String,
    pub kind: String,
    pub uri: String,
    #[serde(default)]
    pub converter: Option<String>,
    /// Reader options captured at ingest time. Persisted so a rebuild
    /// replays the same selection, focus and limits instead of the defaults.
    #[serde(default)]
    pub options: serde_json::Value,
    #[serde(default = "default_vector_kind")]
    pub vector_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder_schema: Option<String>,
    #[serde(default)]
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
    #[serde(default)]
    pub documents: usize,
    #[serde(default)]
    pub chunks: usize,
    #[serde(default)]
    pub points: usize,
    #[serde(default)]
    pub status: RagSourceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl RagSourceState {
    /// Builds state from a descriptor and a successful/failed ingest.
    pub fn from_descriptor(
        descriptor: &SourceDescriptor,
        content_hash: String,
        revision: source_contracts::SourceRevision,
    ) -> Self {
        Self {
            collection: descriptor.collection.clone(),
            namespace: descriptor.namespace.clone(),
            source: descriptor.source.clone(),
            kind: descriptor.kind.clone(),
            uri: descriptor.uri.clone(),
            converter: descriptor.converter.clone(),
            options: descriptor.options.clone(),
            vector_kind: default_vector_kind(),
            encoder: None,
            encoder_fingerprint: None,
            encoder_schema: None,
            content_hash,
            etag: revision.etag,
            last_modified: revision.last_modified,
            fetched_at: revision.fetched_at,
            documents: 0,
            chunks: 0,
            points: 0,
            status: RagSourceStatus::Active,
            error: None,
        }
    }

    /// Rebuilds the descriptor this state was ingested from.
    ///
    /// The record carries the reader kind, URI, converter and options, so a
    /// rebuild replays the original read instead of guessing at it.
    pub fn descriptor(&self) -> SourceDescriptor {
        let mut descriptor = SourceDescriptor::new(
            self.kind.clone(),
            self.source.clone(),
            self.uri.clone(),
            self.collection.clone(),
            self.namespace.clone(),
        );
        descriptor.converter = self.converter.clone();
        descriptor.options = self.options.clone();
        descriptor
    }

    /// Storage key shared by every state backend.
    pub fn key(collection: &str, namespace: &str, source: &str) -> String {
        format!("{collection}\u{1f}{namespace}\u{1f}{source}")
    }

    fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "collection": self.collection,
            "namespace": self.namespace,
            "source": self.source,
            "status": self.status.as_str(),
        })
    }
}

impl RagSourceStatus {
    /// Stable text form stored in metadata.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Failed => "failed",
        }
    }
}

/// Persistence port for source states.
#[async_trait]
pub trait RagSourceStateStore: Send + Sync {
    async fn save(&self, state: RagSourceState) -> ApiResult<()>;
    async fn get(
        &self,
        collection: &str,
        namespace: &str,
        source: &str,
    ) -> ApiResult<Option<RagSourceState>>;
    async fn list(
        &self,
        collection: &str,
        namespace: &str,
        source: Option<&str>,
    ) -> ApiResult<Vec<RagSourceState>>;
    async fn delete(&self, collection: &str, namespace: &str, source: &str) -> ApiResult<()>;
}

/// Source state stored in `wf-storage`, joined with durable entity storage.
pub struct StorageRagSourceStateStore {
    backend: Arc<StorageBackend>,
}

impl StorageRagSourceStateStore {
    pub fn new(backend: Arc<StorageBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl RagSourceStateStore for StorageRagSourceStateStore {
    async fn save(&self, state: RagSourceState) -> ApiResult<()> {
        let bytes = serde_json::to_vec(&state)
            .map_err(|err| ApiError::execution(format!("serialize rag source state: {err}")))?;
        self.backend
            .save(
                &RagSourceState::key(&state.collection, &state.namespace, &state.source),
                &bytes,
                &state.metadata(),
            )
            .await
            .map_err(|err| ApiError::execution(format!("save rag source state: {err}")))
    }

    async fn get(
        &self,
        collection: &str,
        namespace: &str,
        source: &str,
    ) -> ApiResult<Option<RagSourceState>> {
        let Some((bytes, _)) = self
            .backend
            .load(&RagSourceState::key(collection, namespace, source))
            .await
            .map_err(|err| ApiError::execution(format!("load rag source state: {err}")))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|err| ApiError::execution(format!("decode rag source state: {err}")))
    }

    async fn list(
        &self,
        collection: &str,
        namespace: &str,
        source: Option<&str>,
    ) -> ApiResult<Vec<RagSourceState>> {
        let mut filter = QueryFilter::new()
            .with_field("collection", collection)
            .with_field("namespace", namespace);
        if let Some(source) = source {
            filter = filter.with_field("source", source);
        }
        let rows = self
            .backend
            .list_data(Some(&filter))
            .await
            .map_err(|err| ApiError::execution(format!("list rag source states: {err}")))?;
        let mut states: Vec<RagSourceState> = Vec::with_capacity(rows.len());
        for (bytes, _) in rows {
            states.push(
                serde_json::from_slice(&bytes).map_err(|err| {
                    ApiError::execution(format!("decode rag source state: {err}"))
                })?,
            );
        }
        states.sort_by(|left, right| left.source.cmp(&right.source));
        Ok(states)
    }

    async fn delete(&self, collection: &str, namespace: &str, source: &str) -> ApiResult<()> {
        self.backend
            .delete(&RagSourceState::key(collection, namespace, source))
            .await
            .map_err(|err| ApiError::execution(format!("delete rag source state: {err}")))
    }
}

/// Process-local source state used by library hosts and tests.
#[derive(Clone, Default)]
pub struct MemoryRagSourceStateStore {
    states: Arc<DashMap<String, RagSourceState>>,
}

#[async_trait]
impl RagSourceStateStore for MemoryRagSourceStateStore {
    async fn save(&self, state: RagSourceState) -> ApiResult<()> {
        self.states.insert(
            RagSourceState::key(&state.collection, &state.namespace, &state.source),
            state,
        );
        Ok(())
    }

    async fn get(
        &self,
        collection: &str,
        namespace: &str,
        source: &str,
    ) -> ApiResult<Option<RagSourceState>> {
        Ok(self
            .states
            .get(&RagSourceState::key(collection, namespace, source))
            .map(|entry| entry.clone()))
    }

    async fn list(
        &self,
        collection: &str,
        namespace: &str,
        source: Option<&str>,
    ) -> ApiResult<Vec<RagSourceState>> {
        let mut states = self
            .states
            .iter()
            .filter(|entry| {
                let state = entry.value();
                state.collection == collection
                    && state.namespace == namespace
                    && source.map_or(true, |value| state.source == value)
            })
            .map(|entry| entry.clone())
            .collect::<Vec<_>>();
        states.sort_by(|left, right| left.source.cmp(&right.source));
        Ok(states)
    }

    async fn delete(&self, collection: &str, namespace: &str, source: &str) -> ApiResult<()> {
        self.states
            .remove(&RagSourceState::key(collection, namespace, source));
        Ok(())
    }
}

/// Named registry of source readers.
#[derive(Clone, Default)]
pub struct SourceReaderRegistry {
    readers: Arc<DashMap<String, Arc<dyn SourceReader>>>,
}

impl SourceReaderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a reader, rejecting duplicate kinds.
    pub fn register(&self, reader: Arc<dyn SourceReader>) -> ApiResult<()> {
        let kind = reader.kind();
        if kind.trim().is_empty() {
            return Err(ApiError::Validation(
                "source reader kind must not be empty".to_string(),
            ));
        }
        let replaced = self.readers.insert(kind.to_string(), reader);
        if replaced.is_some() {
            return Err(ApiError::Validation(format!(
                "source reader '{kind}' is already registered"
            )));
        }
        Ok(())
    }

    pub fn get(&self, kind: &str) -> Option<Arc<dyn SourceReader>> {
        self.readers.get(kind).map(|entry| entry.clone())
    }

    pub fn kinds(&self) -> Vec<String> {
        let mut kinds = self
            .readers
            .iter()
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();
        kinds.sort();
        kinds
    }
}

/// Outcome of one source ingest run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagSourceIngestResult {
    pub source: String,
    pub action: RagSourceSyncAction,
    pub documents: usize,
    pub chunks: usize,
    pub points: usize,
    pub skipped: usize,
    pub content_hash: String,
    pub revision: source_contracts::SourceRevision,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Change decision for one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RagSourceSyncAction {
    Insert,
    Update,
    Skip,
}

/// Upper bound on reader pages per source. A reader that keeps handing out
/// cursors past this is reported instead of silently truncated.
const MAX_SOURCE_PAGES: usize = 64;

/// One completed read pass over a source, ready to be written.
struct FetchedSource {
    documents: Vec<source_contracts::SourceDocument>,
    revision: source_contracts::SourceRevision,
    content_hash: String,
}

/// Host-side source ingestion over injected readers and conversion registry.
pub struct RagSourceService {
    handles: RagHandles,
    readers: Arc<SourceReaderRegistry>,
    converters: Arc<wf_rag::ConverterRegistry>,
    states: Arc<dyn RagSourceStateStore>,
    index_defaults: wf_rag::RagIndexConfig,
}

impl RagSourceService {
    pub fn new(
        handles: RagHandles,
        readers: Arc<SourceReaderRegistry>,
        states: Arc<dyn RagSourceStateStore>,
        index_defaults: wf_rag::RagIndexConfig,
    ) -> Self {
        Self {
            handles,
            readers,
            converters: Arc::new(wf_rag::ConverterRegistry::with_defaults()),
            states,
            index_defaults,
        }
    }

    /// Builds a service with the built-in URL and file readers.
    pub fn with_builtin_readers(
        handles: RagHandles,
        fetch: Arc<dyn wf_tools::predefined::web::WebFetchBackend>,
        states: Arc<dyn RagSourceStateStore>,
        index_defaults: wf_rag::RagIndexConfig,
        max_content_chars: usize,
        source_roots: Vec<std::path::PathBuf>,
    ) -> Self {
        let readers = Arc::new(SourceReaderRegistry::new());
        readers
            .register(Arc::new(crate::rag_source_readers::UrlSourceReader::new(
                fetch,
                max_content_chars,
            )))
            .expect("new reader registry cannot contain a duplicate");
        readers
            .register(Arc::new(crate::rag_source_readers::FileSourceReader::new(
                source_roots,
                max_content_chars,
            )))
            .expect("file reader kind cannot collide with url");
        Self::new(handles, readers, states, index_defaults)
    }

    /// Replaces the converter registry with a host-extended one.
    pub fn with_converters(mut self, converters: Arc<wf_rag::ConverterRegistry>) -> Self {
        self.converters = converters;
        self
    }

    /// Reads, normalizes and indexes one source.
    pub async fn ingest(
        &self,
        descriptor: SourceDescriptor,
        force: bool,
    ) -> ApiResult<RagSourceIngestResult> {
        let fetched = self.fetch(&descriptor).await?;
        self.write(descriptor, fetched, force).await
    }

    /// Reads one source into memory without touching either index.
    ///
    /// Splitting the read from the write lets a rebuild fail on a transient
    /// fetch error while the previously indexed content stays in place.
    async fn fetch(&self, descriptor: &SourceDescriptor) -> ApiResult<FetchedSource> {
        descriptor
            .validate()
            .map_err(|err| ApiError::Validation(err.to_string()))?;
        let reader = self.readers.get(&descriptor.kind).ok_or_else(|| {
            ApiError::Validation(format!("unknown source reader '{}'", descriptor.kind))
        })?;
        let mut cursor: Option<String> = None;
        let mut documents = Vec::new();
        let mut revision: Option<source_contracts::SourceRevision>;
        let mut pages = 0;
        loop {
            if pages >= MAX_SOURCE_PAGES {
                return Err(ApiError::execution(format!(
                    "source '{}' exceeded {MAX_SOURCE_PAGES} reader pages; split the source or raise the page cap",
                    descriptor.source
                )));
            }
            let page = reader
                .read(descriptor, cursor.as_deref())
                .await
                .map_err(|err| ApiError::execution(format!("source read failed: {err}")))?;
            pages += 1;
            revision = Some(page.revision);
            documents.extend(page.documents);
            match page.next_cursor {
                None => break,
                Some(next) if Some(&next) == cursor.as_ref() => {
                    return Err(ApiError::execution("source reader repeated a page cursor"));
                }
                Some(next) => cursor = Some(next),
            }
        }
        let mut revision = revision.unwrap_or_default();
        let combined = documents
            .iter()
            .map(|document| document.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let content_hash = if revision.content_hash.is_empty() {
            wf_rag::content_hash(&combined)
        } else {
            revision.content_hash.clone()
        };
        revision.content_hash = content_hash.clone();
        if revision.fetched_at.is_none() {
            revision.fetched_at = Some(Utc::now().to_rfc3339());
        }
        Ok(FetchedSource {
            documents,
            revision,
            content_hash,
        })
    }

    /// Writes one already-read source through the ingest pipeline.
    async fn write(
        &self,
        descriptor: SourceDescriptor,
        fetched: FetchedSource,
        force: bool,
    ) -> ApiResult<RagSourceIngestResult> {
        let FetchedSource {
            documents,
            revision,
            content_hash,
        } = fetched;
        let mut index = self.index_defaults.clone();
        index.namespace = descriptor.namespace.clone();
        index.duplicate_policy = wf_rag::DuplicatePolicy::Skip;
        let fingerprint = wf_rag::vector_encoding_fingerprint(&index);
        let known = self
            .states
            .get(
                &descriptor.collection,
                &descriptor.namespace,
                &descriptor.source,
            )
            .await?;
        if let Some(previous) = &known {
            if !force && previous.encoder_fingerprint.as_deref() != fingerprint.as_deref() {
                return Err(ApiError::Validation(format!(
                    "vector encoding of '{}' changed; explicit reindex is required",
                    descriptor.source
                )));
            }
        }
        let action = if force || known.is_none() {
            RagSourceSyncAction::Insert
        } else if revision_unchanged(known.as_ref().expect("known state"), &revision) {
            RagSourceSyncAction::Skip
        } else {
            RagSourceSyncAction::Update
        };
        let mut state =
            RagSourceState::from_descriptor(&descriptor, content_hash.clone(), revision.clone());
        state.vector_kind = index.vector_kind.as_name().to_string();
        state.encoder = index.encoder.clone();
        state.encoder_fingerprint = fingerprint.clone();
        state.encoder_schema = index
            .encoder
            .as_deref()
            .map(|_| wf_rag::VECTOR_ENCODING_SCHEMA.to_string());
        if action == RagSourceSyncAction::Skip {
            if let Some(previous) = known {
                state.documents = previous.documents;
                state.chunks = previous.chunks;
                state.points = previous.points;
            }
            self.states.save(state).await?;
            return Ok(skipped_result(&descriptor, content_hash, revision));
        }
        if action == RagSourceSyncAction::Update {
            delete_source_scope(
                self.handles.store.as_ref(),
                self.handles.keyword.as_deref(),
                &descriptor.collection,
                Some(&descriptor.namespace),
                &descriptor.source,
            )
            .await?;
        }
        let mut rag_documents = Vec::with_capacity(documents.len());
        let mut source_metadata = HashMap::new();
        for document in documents {
            let converted = self
                .converters
                .convert(
                    &descriptor.uri,
                    descriptor.converter.as_deref(),
                    document.content.as_bytes(),
                )
                .await;
            let converted = match converted {
                Ok(value) => value,
                Err(err) => {
                    state.status = RagSourceStatus::Failed;
                    state.error = Some(err.to_string());
                    self.states.save(state).await?;
                    return Err(ApiError::execution(format!(
                        "source conversion failed: {err}"
                    )));
                }
            };
            let mut metadata = document.metadata;
            for (key, value) in converted.metadata {
                metadata.entry(key).or_insert(value);
            }
            metadata.insert("content_hash".to_string(), content_hash.clone());
            metadata.insert("source_uri".to_string(), descriptor.uri.clone());
            if let Some(fetched_at) = &revision.fetched_at {
                metadata.insert("fetched_at".to_string(), fetched_at.clone());
            }
            if let Some(etag) = &revision.etag {
                metadata.insert("etag".to_string(), etag.clone());
            }
            if let Some(last_modified) = &revision.last_modified {
                metadata.insert("last_modified".to_string(), last_modified.clone());
            }
            if source_metadata.is_empty() {
                source_metadata = metadata.clone();
            }
            rag_documents.push(
                wf_rag::RagDocument::new(document.id, converted.content, descriptor.source.clone())
                    .with_metadata_map(metadata),
            );
        }
        let stats = match wf_rag::pipeline::ingest_documents(
            self.handles.store.as_ref(),
            self.handles.keyword.as_deref(),
            self.handles.embedder.as_ref(),
            &descriptor.collection,
            &rag_documents,
            &index,
        )
        .await
        {
            Ok(stats) => stats,
            Err(err) => {
                state.status = RagSourceStatus::Failed;
                state.error = Some(err.to_string());
                self.states.save(state).await?;
                return Err(ApiError::execution(format!("source ingest failed: {err}")));
            }
        };
        state.documents = stats.documents;
        state.chunks = stats.chunks;
        state.points = stats.points;
        self.states.save(state).await?;
        info!(source = %descriptor.source, points = stats.points, "rag source ingested");
        Ok(RagSourceIngestResult {
            source: descriptor.source,
            action,
            documents: stats.documents,
            chunks: stats.chunks,
            points: stats.points,
            skipped: stats.skipped,
            content_hash,
            revision,
            metadata: source_metadata,
        })
    }

    /// Deletes a source from both indexes and the state store.
    pub async fn delete(&self, collection: &str, namespace: &str, source: &str) -> ApiResult<u64> {
        let removed = delete_source_scope(
            self.handles.store.as_ref(),
            self.handles.keyword.as_deref(),
            collection,
            Some(namespace),
            source,
        )
        .await?;
        self.states.delete(collection, namespace, source).await?;
        Ok(removed)
    }

    /// Rebuilds a source without ever leaving the index empty on failure.
    ///
    /// The read happens first: a transient fetch or conversion error aborts
    /// before anything is deleted, so the previously indexed content and its
    /// state record survive. Only a successful read clears the old points.
    pub async fn reindex(&self, descriptor: SourceDescriptor) -> ApiResult<RagSourceIngestResult> {
        let fetched = self.fetch(&descriptor).await?;
        self.delete(
            &descriptor.collection,
            &descriptor.namespace,
            &descriptor.source,
        )
        .await?;
        self.write(descriptor, fetched, true).await
    }

    /// Rebuilds one known source from its persisted descriptor.
    ///
    /// Callers only supply the identity triple; reader kind, URI, converter
    /// and reader options come from the state record, so a rebuild cannot
    /// silently change how the source is read.
    pub async fn reindex_known(
        &self,
        collection: &str,
        namespace: &str,
        source: &str,
    ) -> ApiResult<RagSourceIngestResult> {
        let state = self
            .states
            .get(collection, namespace, source)
            .await?
            .ok_or_else(|| ApiError::NotFound {
                entity_type: "rag_source".to_string(),
                id: RagSourceState::key(collection, namespace, source),
            })?;
        self.reindex(state.descriptor()).await
    }

    /// Lists known sources with their per-side point counts.
    pub async fn status(
        &self,
        collection: &str,
        namespace: &str,
        source: Option<&str>,
    ) -> ApiResult<Vec<RagSourceStatusView>> {
        let states = self.states.list(collection, namespace, source).await?;
        let mut views = Vec::with_capacity(states.len());
        for state in states {
            let (points, keyword_points) = wf_rag::pipeline::source_counts(
                self.handles.store.as_ref(),
                self.handles.keyword.as_deref(),
                collection,
                namespace,
                &state.source,
            )
            .await
            .map_err(|err| ApiError::execution(format!("source counts failed: {err}")))?;
            views.push(RagSourceStatusView {
                state,
                point_count: points,
                keyword_point_count: keyword_points,
            });
        }
        Ok(views)
    }
}

/// Source state paired with live index counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagSourceStatusView {
    pub state: RagSourceState,
    pub point_count: u64,
    pub keyword_point_count: Option<u64>,
}

/// Counts for the most recent source run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RagSourceRunStats {
    pub documents: usize,
    pub chunks: usize,
    pub points: usize,
    pub skipped: usize,
}

fn skipped_result(
    descriptor: &SourceDescriptor,
    content_hash: String,
    revision: source_contracts::SourceRevision,
) -> RagSourceIngestResult {
    RagSourceIngestResult {
        source: descriptor.source.clone(),
        action: RagSourceSyncAction::Skip,
        documents: 0,
        chunks: 0,
        points: 0,
        skipped: 1,
        content_hash,
        revision,
        metadata: HashMap::new(),
    }
}

fn revision_unchanged(state: &RagSourceState, revision: &source_contracts::SourceRevision) -> bool {
    if state.status == RagSourceStatus::Failed {
        return false;
    }
    let http_match = (revision.etag.is_some() && revision.etag == state.etag)
        || (revision.last_modified.is_some() && revision.last_modified == state.last_modified);
    http_match || (!revision.content_hash.is_empty() && revision.content_hash == state.content_hash)
}

async fn delete_source_scope(
    store: &dyn vector_contracts::VectorStorage,
    keyword: Option<&dyn keyword_contracts::KeywordIndex>,
    collection: &str,
    namespace: Option<&str>,
    source: &str,
) -> ApiResult<u64> {
    let removed = match wf_rag::pipeline::delete_source(
        store, keyword, collection, namespace, source,
    )
    .await
    {
        Ok(removed) => removed,
        Err(wf_rag::RagError::Store(VectorError::CollectionMissing(_))) => 0,
        Err(err) => {
            return Err(ApiError::execution(format!(
                "delete source scope failed: {err}"
            )))
        }
    };
    Ok(removed)
}

/// Shared converter map used by URL and file readers.
pub type SourceMetadata = HashMap<String, String>;

#[allow(dead_code)]
fn warn_stale_source(source: &str) {
    warn!(source = %source, "source state has no indexed points");
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use source_contracts::{
        SourceDescriptor, SourceError, SourcePage, SourceReader, SourceRevision,
    };
    use vector_contracts::{InMemoryVectorStore, VectorStorage as _};

    use super::{
        MemoryRagSourceStateStore, RagSourceService, RagSourceStatus, SourceReaderRegistry,
    };
    use crate::rag::RagHandles;

    struct UnitEmbedder {
        dimension: usize,
    }

    #[async_trait::async_trait]
    impl llm_embedding::provider::EmbeddingProvider for UnitEmbedder {
        async fn embed(
            &self,
            texts: &[String],
        ) -> llm_embedding::error::Result<llm_embedding::provider::EmbeddingResult> {
            Ok(llm_embedding::provider::EmbeddingResult {
                embeddings: texts.iter().map(|text| self.vector_for(text)).collect(),
                prompt_tokens: 0,
                total_tokens: 0,
            })
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        fn model_name(&self) -> &str {
            "test-unit"
        }
    }

    impl UnitEmbedder {
        fn vector_for(&self, text: &str) -> Vec<f32> {
            let mut vector = vec![0.0; self.dimension];
            for (index, byte) in text.bytes().enumerate() {
                vector[index % self.dimension] += f32::from(byte) / 255.0;
            }
            let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
            if norm > 0.0 {
                for value in &mut vector {
                    *value /= norm;
                }
            }
            vector
        }
    }

    /// Reader that records the options it was called with and can be flipped
    /// into a failing state, standing in for a page fetch that times out.
    struct ScriptedReader {
        content: Mutex<String>,
        seen_options: Mutex<Vec<serde_json::Value>>,
        fail: AtomicBool,
    }

    impl ScriptedReader {
        fn new(content: &str) -> Self {
            Self {
                content: Mutex::new(content.to_string()),
                seen_options: Mutex::new(Vec::new()),
                fail: AtomicBool::new(false),
            }
        }

        fn set_content(&self, content: &str) {
            *self.content.lock().expect("content lock") = content.to_string();
        }

        fn fail_next(&self) {
            self.fail.store(true, Ordering::SeqCst);
        }

        fn observed_options(&self) -> Vec<serde_json::Value> {
            self.seen_options.lock().expect("options lock").clone()
        }
    }

    #[async_trait::async_trait]
    impl SourceReader for ScriptedReader {
        fn kind(&self) -> &'static str {
            "scripted"
        }

        async fn read(
            &self,
            descriptor: &SourceDescriptor,
            _cursor: Option<&str>,
        ) -> source_contracts::Result<SourcePage> {
            self.seen_options
                .lock()
                .expect("options lock")
                .push(descriptor.options.clone());
            if self.fail.load(Ordering::SeqCst) {
                return Err(SourceError::Read("reader is offline".to_string()));
            }
            let content = self.content.lock().expect("content lock").clone();
            Ok(SourcePage {
                documents: vec![source_contracts::SourceDocument::new("d1", content)],
                revision: SourceRevision::default(),
                next_cursor: None,
            })
        }
    }

    fn service(reader: Arc<ScriptedReader>, store: Arc<InMemoryVectorStore>) -> RagSourceService {
        let readers = Arc::new(SourceReaderRegistry::new());
        readers
            .register(reader as Arc<dyn SourceReader>)
            .expect("register scripted reader");
        RagSourceService::new(
            RagHandles::new(
                store as Arc<dyn vector_contracts::VectorStorage>,
                Arc::new(UnitEmbedder { dimension: 8 }),
                None,
            ),
            readers,
            Arc::new(MemoryRagSourceStateStore::default()),
            wf_rag::RagIndexConfig::new("kb"),
        )
    }

    #[tokio::test]
    async fn rebuild_replays_persisted_options() {
        let reader = Arc::new(ScriptedReader::new("alpha passage about ownership"));
        let store = Arc::new(InMemoryVectorStore::new());
        let service = service(reader.clone(), store.clone());

        let mut descriptor = SourceDescriptor::new("scripted", "doc", "inline://doc", "kb", "kb");
        // No extension in the URI: only an explicit converter resolves, which
        // also proves the rebuild replays the persisted converter.
        descriptor.converter = Some("plain".to_string());
        descriptor.options = serde_json::json!({ "selector": "#main", "min_words": 10 });
        service.ingest(descriptor, false).await.expect("ingest");

        let listed = service
            .status("kb", "kb", Some("doc"))
            .await
            .expect("status");
        assert_eq!(listed[0].state.options["selector"], "#main");

        reader.set_content("beta passage about borrowing");
        service
            .reindex_known("kb", "kb", "doc")
            .await
            .expect("rebuild");

        // The rebuild must replay the persisted reader options, not defaults.
        let observed = reader.observed_options();
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[1]["selector"], "#main");
    }

    #[tokio::test]
    async fn failed_rebuild_keeps_indexed_content() {
        let reader = Arc::new(ScriptedReader::new("alpha passage about ownership"));
        let store = Arc::new(InMemoryVectorStore::new());
        let service = service(reader.clone(), store.clone());

        let mut descriptor = SourceDescriptor::new("scripted", "doc", "inline://doc", "kb", "kb");
        descriptor.converter = Some("plain".to_string());
        service.ingest(descriptor, false).await.expect("ingest");
        let before = store.count("kb", None).await.expect("count");
        assert!(before > 0);

        reader.fail_next();
        assert!(service.reindex_known("kb", "kb", "doc").await.is_err());

        let after = store.count("kb", None).await.expect("count");
        assert_eq!(after, before, "a failed read must not clear the index");
        let listed = service
            .status("kb", "kb", Some("doc"))
            .await
            .expect("status");
        assert_eq!(listed[0].state.status, RagSourceStatus::Active);
    }

    #[tokio::test]
    async fn rebuild_rejects_unknown_source() {
        let reader = Arc::new(ScriptedReader::new("unused"));
        let store = Arc::new(InMemoryVectorStore::new());
        let service = service(reader, store);
        assert!(service.reindex_known("kb", "kb", "missing").await.is_err());
    }
}
