//! Read-only index management and query troubleshooting for RAG.
//!
//! Transports render these shapes; full vectors never leave the backend.
//! Source enumeration stays caller-maintained (a small registry of known
//! sources per namespace) because the storage contract has no scan call;
//! every function documents which inputs it needs.

use serde::{Deserialize, Serialize};

use std::path::PathBuf;
use std::sync::Arc;

use tracing::info;
use wf_types::config::keyword::KeywordIndexConfig;
use wf_types::config::storage::{StorageConfig, StorageType};

use crate::infra::error::{ApiError, ApiResult};
use rag_core::KeywordIndex;

/// Shared retrieval backend injected by the host runtime.
///
/// Transports read through this handle; `None` keeps the routes
/// registered but reporting service unavailable. A missing keyword
/// index keeps hybrid retrieval off while dense retrieval keeps working.
#[derive(Clone)]
pub struct RagHandles {
    /// Vector backend shared with tools and workflow nodes.
    pub store: Arc<dyn vector_contracts::VectorStorage>,
    /// Embedding provider shared with tools and workflow nodes.
    pub embedder: Arc<dyn llm_embedding::provider::EmbeddingProvider>,
    /// Keyword index shared with tools and workflow nodes; `None`
    /// disables hybrid retrieval and dual writes.
    pub keyword: Option<Arc<dyn rag_core::KeywordIndex>>,
}

impl RagHandles {
    /// Builds the shared retrieval handle.
    pub fn new(
        store: Arc<dyn vector_contracts::VectorStorage>,
        embedder: Arc<dyn llm_embedding::provider::EmbeddingProvider>,
        keyword: Option<Arc<dyn rag_core::KeywordIndex>>,
    ) -> Self {
        Self {
            store,
            embedder,
            keyword,
        }
    }
}

impl std::fmt::Debug for RagHandles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RagHandles")
            .field("keyword_configured", &self.keyword.is_some())
            .finish()
    }
}

/// Resolved keyword index placement: the three explicit branches.
/// `Disabled` is the only legitimate source of an empty keyword slot;
/// bootstrap must fail fast on any open error instead of degrading here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeywordIndexLocation {
    /// Keyword explicitly disabled; the slot stays empty.
    Disabled,
    /// Ephemeral index for memory storage; dies with the process.
    Memory,
    /// File-backed index at the resolved path.
    File(PathBuf),
}

/// Environment variable overriding the database path.
pub const KEYWORD_DB_PATH_ENV: &str = "WF_KEYWORD_DB_PATH";
/// Environment variable overriding the master switch (`0`/`false`/`no`/
/// `off` disable, case-insensitive).
pub const KEYWORD_ENABLED_ENV: &str = "WF_KEYWORD_ENABLED";

/// Resolves where the keyword index lives. Environment overrides win over
/// the programmatic config; an explicit path wins over derived placement.
/// Memory storage resolves to an ephemeral index, every other storage
/// type to a file sibling of the storage database.
pub fn resolve_keyword_location(
    storage: &StorageConfig,
    config: &KeywordIndexConfig,
) -> KeywordIndexLocation {
    if env_flag_off(KEYWORD_ENABLED_ENV) || !config.enabled {
        return KeywordIndexLocation::Disabled;
    }
    if let Some(path) = env_path(KEYWORD_DB_PATH_ENV) {
        return KeywordIndexLocation::File(path);
    }
    if let Some(path) = config.db_path.as_deref().filter(|path| !path.trim().is_empty()) {
        return KeywordIndexLocation::File(PathBuf::from(path));
    }
    if storage.storage_type == StorageType::Memory {
        return KeywordIndexLocation::Memory;
    }
    KeywordIndexLocation::File(keyword_db_path(storage))
}

/// Derives the keyword database path from the storage configuration:
/// the storage database's sibling named after the application. Mirrors
/// the runtime storage path rule, so both databases move together.
pub fn keyword_db_path(storage: &StorageConfig) -> PathBuf {
    let app_name = storage.app_name.as_deref().unwrap_or("app");
    let dir = storage
        .sqlite
        .as_ref()
        .map(|sqlite| sqlite.db_path.as_str())
        .filter(|path| !path.is_empty())
        .map(|path| {
            PathBuf::from(path)
                .parent()
                .map(|parent| parent.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("./storage"))
        })
        .unwrap_or_else(|| PathBuf::from("./storage"));
    dir.join(format!("{app_name}-keyword.db"))
}

fn env_flag_off(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => matches!(
            value.trim().to_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        ),
        Err(_) => false,
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Shared keyword index handle for hosts that must not depend on the
/// retrieval core directly (e.g. the runtime bootstrap). Consumers that
/// already depend on the core unwrap the inner handle.
#[derive(Clone)]
pub struct SharedKeywordIndex {
    inner: Arc<dyn rag_core::KeywordIndex>,
}

impl SharedKeywordIndex {
    /// Wraps an already opened index.
    pub fn new(inner: Arc<dyn rag_core::KeywordIndex>) -> Self {
        Self { inner }
    }

    /// Borrows the inner handle for retrieval calls.
    pub fn as_arc(&self) -> &Arc<dyn rag_core::KeywordIndex> {
        &self.inner
    }
}

impl std::fmt::Debug for SharedKeywordIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedKeywordIndex")
            .field("backend", &self.inner.backend_name())
            .finish()
    }
}

/// Opens the keyword index for the resolved location. Blocking: callers
/// run it on a blocking thread pool, never on an async worker. `Disabled`
/// yields `None`; any open failure is an explicit error so bootstrap
/// fails fast instead of silently running without hybrid retrieval.
pub fn open_keyword_index(
    location: &KeywordIndexLocation,
) -> Result<Option<SharedKeywordIndex>, ApiError> {
    match location {
        KeywordIndexLocation::Disabled => {
            info!("keyword index disabled; hybrid retrieval stays off");
            Ok(None)
        }
        KeywordIndexLocation::Memory => {
            let index = rag_core::SqliteKeywordIndex::open_in_memory().map_err(|err| {
                ApiError::execution(format!("Failed to open in-memory keyword index: {err}"))
            })?;
            info!(
                backend = index.backend_name(),
                "keyword index opened in memory"
            );
            Ok(Some(SharedKeywordIndex::new(Arc::new(index))))
        }
        KeywordIndexLocation::File(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|err| {
                        ApiError::execution(format!(
                            "Failed to create keyword index directory '{}': {err}",
                            parent.display()
                        ))
                    })?;
                }
            }
            let raw = path.to_string_lossy().into_owned();
            let index =
                rag_core::SqliteKeywordIndex::open(&raw).map_err(|err| {
                    ApiError::execution(format!(
                        "Failed to open keyword index at '{}': {err}",
                        path.display()
                    ))
                })?;
            info!(
                backend = index.backend_name(),
                path = %path.display(),
                "keyword index opened"
            );
            Ok(Some(SharedKeywordIndex::new(Arc::new(index))))
        }
    }
}

/// Requires retrieval handles or reports service unavailable.
pub fn require_rag_handles(handles: Option<&RagHandles>) -> ApiResult<RagHandles> {
    handles.cloned().ok_or_else(|| {
        ApiError::execution_categorized(
            "Retrieval service is not configured",
            crate::infra::error::ApiErrorCategory::ServiceUnavailable,
        )
    })
}

/// Point counts for one collection/namespace scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStats {
    /// Collection queried.
    pub collection: String,
    /// Namespace scope.
    pub namespace: String,
    /// Points matching the scope.
    pub point_count: u64,
    /// Keyword index points matching the scope; `None` when the keyword
    /// index is not enabled. Compare against `point_count` to spot
    /// dual-write drift.
    pub keyword_point_count: Option<u64>,
}

/// Counts points in a collection/namespace scope on both indexes.
/// A missing keyword index leaves its count empty while dense stats
/// keep working.
pub async fn collection_stats(
    store: &dyn vector_contracts::VectorStorage,
    keyword: Option<&dyn rag_core::KeywordIndex>,
    collection: &str,
    namespace: &str,
) -> ApiResult<CollectionStats> {
    if collection.trim().is_empty() || namespace.trim().is_empty() {
        return Err(ApiError::Validation(
            "collection and namespace must not be empty".to_string(),
        ));
    }
    let filter = vector_contracts::SearchFilter::new().with_namespace(namespace);
    let point_count = store
        .count(collection, Some(&filter))
        .await
        .map_err(|err| ApiError::Validation(err.to_string()))?;
    let keyword_point_count = match keyword {
        Some(index) => Some(
            index
                .count(collection, Some(&filter))
                .await
                .map_err(|err| ApiError::Validation(err.to_string()))?,
        ),
        None => None,
    };
    Ok(CollectionStats {
        collection: collection.to_string(),
        namespace: namespace.to_string(),
        point_count,
        keyword_point_count,
    })
}

/// Counts points for one source inside a namespace scope on both indexes.
pub async fn source_stats(
    store: &dyn vector_contracts::VectorStorage,
    keyword: Option<&dyn rag_core::KeywordIndex>,
    collection: &str,
    namespace: &str,
    source: &str,
) -> ApiResult<CollectionStats> {
    if source.trim().is_empty() {
        return Err(ApiError::Validation("source must not be empty".to_string()));
    }
    let stats = collection_stats(store, keyword, collection, namespace).await?;
    let filter = vector_contracts::SearchFilter::new()
        .with_namespace(namespace)
        .with_source(source);
    let point_count = store
        .count(collection, Some(&filter))
        .await
        .map_err(|err| ApiError::Validation(err.to_string()))?;
    let keyword_point_count = match keyword {
        Some(index) => Some(
            index
                .count(collection, Some(&filter))
                .await
                .map_err(|err| ApiError::Validation(err.to_string()))?,
        ),
        None => None,
    };
    Ok(CollectionStats {
        collection: stats.collection,
        namespace: format!("{} / source {source}", stats.namespace),
        point_count,
        keyword_point_count,
    })
}

/// One preview hit: excerpt and score only, no vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewHit {
    /// Chunk id used as citation locator.
    pub chunk_id: String,
    /// Source label.
    pub source: String,
    /// Truncated passage text.
    pub excerpt: String,
    /// Recall score.
    pub score: f32,
}

/// Previews ranked hits for a question without exposing vectors.
/// Runs the standard read path with a caller-controlled excerpt budget.
pub async fn preview_search(
    store: &dyn vector_contracts::VectorStorage,
    embedder: &dyn llm_embedding::provider::EmbeddingProvider,
    collection: &str,
    namespace: &str,
    query_text: &str,
    top_k: usize,
    excerpt_chars: usize,
) -> ApiResult<Vec<PreviewHit>> {
    if query_text.trim().is_empty() {
        return Err(ApiError::Validation("query must not be empty".to_string()));
    }
    let config = rag_core::RagQueryConfig {
        top_k: top_k.clamp(1, 50),
        ..Default::default()
    };
    let hits =
        rag_core::pipeline::query(store, embedder, collection, namespace, query_text, &config)
            .await
            .map_err(|err| ApiError::Validation(err.to_string()))?;
    Ok(hits
        .into_iter()
        .map(|hit| PreviewHit {
            chunk_id: hit.chunk_id,
            source: hit.source,
            excerpt: truncate_excerpt(&hit.content, excerpt_chars),
            score: hit.score,
        })
        .collect())
}

/// One histogram bucket over hit scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBucket {
    /// Inclusive lower bound.
    pub lower: f32,
    /// Exclusive upper bound.
    pub upper: f32,
    /// Hits falling in the bucket.
    pub count: usize,
}

/// Buckets hit scores into `buckets` equal-width bins over 0..=1.
/// Scores outside the range clamp into the edge buckets.
pub fn score_histogram(scores: &[f32], buckets: usize) -> Vec<ScoreBucket> {
    let buckets = buckets.clamp(1, 20);
    let width = 1.0 / buckets as f32;
    let mut counts = vec![0_usize; buckets];
    for score in scores {
        let clamped = score.clamp(0.0, 1.0);
        let mut index = (clamped / width).floor() as usize;
        if index >= buckets {
            index = buckets - 1;
        }
        counts[index] += 1;
    }
    counts
        .into_iter()
        .enumerate()
        .map(|(index, count)| ScoreBucket {
            lower: index as f32 * width,
            upper: (index + 1) as f32 * width,
            count,
        })
        .collect()
}

/// Likely causes when a query returns no hits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoHitDiagnosis {
    /// Human-readable likely causes ordered by check cost.
    pub likely_causes: Vec<String>,
}

/// Suggests likely causes for an empty result from the query shape and
/// the keyword switch. Callers append backend facts (namespace point
/// counts from [`collection_stats`]) before showing this to operators.
pub fn diagnose_no_hits(
    query_text: &str,
    top_k: usize,
    filter_fields: usize,
    score_threshold: Option<f32>,
    keyword_enabled: bool,
) -> NoHitDiagnosis {
    let mut likely_causes = Vec::new();
    if query_text.trim().is_empty() {
        likely_causes.push("query is empty; nothing was searched".to_string());
    }
    if keyword_enabled {
        likely_causes.push(
            "keyword index is enabled but returned no matches; compare exact terms against the indexed text"
                .to_string(),
        );
    } else {
        likely_causes.push(
            "keyword index is not enabled; hybrid retrieval is off and only dense matches apply"
                .to_string(),
        );
    }
    if top_k == 0 {
        likely_causes.push("top_k is zero; no hits can be returned".to_string());
    }
    if filter_fields > 0 {
        likely_causes
            .push("metadata filters are set; an over-narrow filter removes candidates".to_string());
    }
    if let Some(threshold) = score_threshold {
        likely_causes.push(format!(
            "score threshold {threshold} drops low-similarity candidates; lower it to inspect recall"
        ));
    }
    likely_causes.push(
        "chunking may be too coarse for the question; compare a finer target length".to_string(),
    );
    likely_causes.push(
        "embedding dimensions may mismatch the collection; re-index with one provider".to_string(),
    );
    NoHitDiagnosis { likely_causes }
}

fn truncate_excerpt(content: &str, max_chars: usize) -> String {
    let budget = if max_chars == 0 { 200 } else { max_chars };
    if content.chars().count() <= budget {
        return content.trim().to_string();
    }
    let kept: String = content.chars().take(budget).collect();
    format!("{}...", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use vector_contracts::VectorStorage as _;

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let id = TEST_DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("{prefix}-{}-{id}", std::process::id()))
    }

    fn memory_storage() -> StorageConfig {
        StorageConfig {
            storage_type: StorageType::Memory,
            ..Default::default()
        }
    }

    fn sqlite_storage(db_path: &str) -> StorageConfig {
        StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: db_path.to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

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
            let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                for value in &mut vector {
                    *value /= norm;
                }
            }
            vector
        }
    }

    #[test]
    fn histogram_buckets_cover_scores() {
        let buckets = score_histogram(&[0.1, 0.15, 0.9, 1.5], 4);
        assert_eq!(buckets.len(), 4);
        assert_eq!(buckets.iter().map(|bucket| bucket.count).sum::<usize>(), 4);
        assert_eq!(buckets[0].count, 2);
        assert_eq!(buckets[3].count, 2);
    }

    #[test]
    fn diagnosis_names_filter_and_threshold() {
        let diagnosis = diagnose_no_hits("hello", 5, 2, Some(0.8), false);
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("filter")));
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("threshold")));
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("not enabled")));
    }

    #[test]
    fn diagnosis_flags_empty_query_and_zero_top_k() {
        let diagnosis = diagnose_no_hits("   ", 0, 0, None, true);
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("empty")));
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("zero")));
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("no matches")));
    }

    #[test]
    fn memory_storage_resolves_to_ephemeral_index() {
        let location = resolve_keyword_location(&memory_storage(), &Default::default());
        assert_eq!(location, KeywordIndexLocation::Memory);
    }

    #[test]
    fn explicit_disable_wins_over_everything() {
        let dir = unique_temp_dir("wf-keyword-disabled");
        let db = dir.join("app.db");
        let storage = sqlite_storage(&db.to_string_lossy());
        let config = KeywordIndexConfig {
            enabled: false,
            db_path: Some("/tmp/should-not-win.db".to_string()),
        };
        std::env::set_var(KEYWORD_DB_PATH_ENV, "/tmp/env-should-not-win.db");
        let location = resolve_keyword_location(&storage, &config);
        std::env::remove_var(KEYWORD_DB_PATH_ENV);
        assert_eq!(location, KeywordIndexLocation::Disabled);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn explicit_path_wins_over_derived_placement() {
        let dir = unique_temp_dir("wf-keyword-explicit");
        let db = dir.join("app.db");
        let storage = sqlite_storage(&db.to_string_lossy());
        let explicit = dir.join("custom-keyword.db");
        let config = KeywordIndexConfig {
            enabled: true,
            db_path: Some(explicit.to_string_lossy().into_owned()),
        };
        let location = resolve_keyword_location(&storage, &config);
        assert_eq!(location, KeywordIndexLocation::File(explicit));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_path_override_wins_over_config() {
        let dir = unique_temp_dir("wf-keyword-env");
        let db = dir.join("app.db");
        let storage = sqlite_storage(&db.to_string_lossy());
        let env_path = dir.join("env-keyword.db");
        std::env::set_var(KEYWORD_DB_PATH_ENV, env_path.to_string_lossy().to_string());
        let location = resolve_keyword_location(&storage, &Default::default());
        std::env::remove_var(KEYWORD_DB_PATH_ENV);
        assert_eq!(location, KeywordIndexLocation::File(env_path));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_flag_off_disables_index() {
        std::env::set_var(KEYWORD_ENABLED_ENV, "off");
        let location = resolve_keyword_location(&memory_storage(), &Default::default());
        std::env::remove_var(KEYWORD_ENABLED_ENV);
        assert_eq!(location, KeywordIndexLocation::Disabled);
    }

    #[test]
    fn file_storage_derives_sibling_keyword_path() {
        let storage = sqlite_storage("/data/app.db");
        assert_eq!(
            keyword_db_path(&storage),
            PathBuf::from("/data/app-keyword.db")
        );
        let storage = StorageConfig {
            storage_type: StorageType::Sqlite,
            app_name: Some("kb".to_string()),
            ..Default::default()
        };
        assert_eq!(
            keyword_db_path(&storage),
            PathBuf::from("./storage/kb-keyword.db")
        );
    }

    #[test]
    fn disabled_location_opens_to_empty_slot() {
        let opened = open_keyword_index(&KeywordIndexLocation::Disabled).expect("open");
        assert!(opened.is_none());
    }

    #[test]
    fn memory_location_opens_usable_index() {
        let opened = open_keyword_index(&KeywordIndexLocation::Memory).expect("open");
        let index = opened.expect("memory index");
        assert_eq!(index.as_arc().backend_name(), "sqlite-fts");
    }

    #[test]
    fn file_location_creates_parent_and_persists() {
        let dir = unique_temp_dir("wf-keyword-file");
        let path = dir.join("nested").join("keyword.db");
        let location = KeywordIndexLocation::File(path.clone());
        let opened = open_keyword_index(&location).expect("open");
        assert!(opened.is_some());
        assert!(path.exists());
        drop(opened);
        let reopened = open_keyword_index(&location).expect("reopen");
        assert!(reopened.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_location_pointing_at_directory_fails_explicitly() {
        let dir = unique_temp_dir("wf-keyword-dir-fail");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let location = KeywordIndexLocation::File(dir.clone());
        assert!(open_keyword_index(&location).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn memory_index_dual_write_and_hybrid_hit() {
        let store = vector_contracts::InMemoryVectorStore::new();
        let keyword = rag_core::SqliteKeywordIndex::open_in_memory().expect("keyword");
        let embedder = UnitEmbedder { dimension: 8 };
        rag_core::pipeline::ingest_documents(
            &store,
            Some(&keyword),
            &embedder,
            "kb",
            &[rag_core::RagDocument::new(
                "d1",
                "rust ownership borrowing rules",
                "rust.md",
            )],
            &rag_core::RagIndexConfig::new("kb"),
        )
        .await
        .expect("ingest");
        let filter = vector_contracts::SearchFilter::new().with_namespace("kb");
        let dense = store.count("kb", Some(&filter)).await.expect("dense count");
        let sparse = keyword
            .count("kb", Some(&filter))
            .await
            .expect("keyword count");
        assert_eq!(dense, sparse);
        assert!(dense > 0);
        let config = rag_core::RagQueryConfig {
            keyword_candidates: 5,
            ..Default::default()
        };
        let hits = rag_core::pipeline::query_hybrid(
            &store,
            &keyword,
            &embedder,
            "kb",
            "kb",
            "ownership",
            &config,
        )
        .await
        .expect("hybrid");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].source, "rust.md");
    }

    #[tokio::test]
    async fn host_tool_service_stays_dense_until_opt_in() {
        use wf_tools::predefined::rag::{RagSearchBackend, RagSearchQuery, RagService};
        let store = std::sync::Arc::new(vector_contracts::InMemoryVectorStore::new());
        let embedder = std::sync::Arc::new(UnitEmbedder { dimension: 8 });
        let keyword =
            std::sync::Arc::new(rag_core::SqliteKeywordIndex::open_in_memory().expect("keyword"));
        rag_core::pipeline::ingest_documents(
            store.as_ref(),
            Some(keyword.as_ref()),
            embedder.as_ref(),
            "kb",
            &[rag_core::RagDocument::new(
                "d1",
                "rust ownership borrowing rules",
                "rust.md",
            )],
            &rag_core::RagIndexConfig::new("kb"),
        )
        .await
        .expect("ingest");
        let dense_service = RagService::new(store.clone(), embedder.clone());
        let dense_hits = dense_service
            .search(RagSearchQuery {
                query: "ownership".to_string(),
                collection: "kb".to_string(),
                namespace: "kb".to_string(),
                top_k: 5,
                score_threshold: None,
                fields: Default::default(),
            })
            .await
            .expect("dense search");
        assert!(!dense_hits.is_empty());
        let hybrid_service = RagService::new(store, embedder)
            .with_keyword(keyword)
            .with_query_defaults(rag_core::RagQueryConfig {
                keyword_candidates: 5,
                ..Default::default()
            });
        let hybrid_hits = hybrid_service
            .search(RagSearchQuery {
                query: "ownership".to_string(),
                collection: "kb".to_string(),
                namespace: "kb".to_string(),
                top_k: 5,
                score_threshold: None,
                fields: Default::default(),
            })
            .await
            .expect("hybrid search");
        assert!(!hybrid_hits.is_empty());
    }

    #[tokio::test]
    async fn host_management_and_workflow_wiring_share_singleton() {
        let store = std::sync::Arc::new(vector_contracts::InMemoryVectorStore::new());
        let embedder = std::sync::Arc::new(UnitEmbedder { dimension: 8 });
        let keyword: std::sync::Arc<dyn rag_core::KeywordIndex> =
            std::sync::Arc::new(rag_core::SqliteKeywordIndex::open_in_memory().expect("keyword"));
        rag_core::pipeline::ingest_documents(
            store.as_ref(),
            Some(keyword.as_ref()),
            embedder.as_ref(),
            "kb",
            &[rag_core::RagDocument::new(
                "d1",
                "rust ownership borrowing rules",
                "rust.md",
            )],
            &rag_core::RagIndexConfig::new("kb"),
        )
        .await
        .expect("ingest");
        let handles = RagHandles::new(store.clone(), embedder.clone(), Some(keyword.clone()));
        let stats = collection_stats(
            handles.store.as_ref(),
            handles.keyword.as_deref(),
            "kb",
            "kb",
        )
        .await
        .expect("stats");
        assert_eq!(stats.keyword_point_count, Some(stats.point_count));
        let ctx = crate::ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            std::sync::Arc::new(wf_resource::registry::ResourceRegistries::new()),
        )
        .with_rag_handles(handles)
        .with_keyword_index(keyword);
        let node_handles = ctx.rag_node_handles("kb", "kb").expect("node handles");
        let mut registry = wf_workflow::handler::HandlerRegistry::new();
        wf_workflow::handler::rag::register_rag_handlers(&mut registry, node_handles);
        for node_type in [
            wf_workflow::handler::rag::RAG_QUERY_NODE_TYPE,
            wf_workflow::handler::rag::RAG_INGEST_NODE_TYPE,
        ] {
            assert!(
                registry.has_handler(&wf_types::node::StaticNodeType::Custom(
                    node_type.to_string()
                ))
            );
        }
    }
}
