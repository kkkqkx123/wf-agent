//! Remote Qdrant backend over the REST API.
//!
//! Deliberately thin: only the calls retrieval needs (create, upsert,
//! query, filter delete, count) plus a minimal operations set (delete
//! collection, list, health, version). The official gRPC client stays a
//! protocol reference, never a dependency, so the crate remains on the
//! shared `reqwest` stack.
//!
//! Deployment notes (also repeated on `QdrantConfig`):
//! REST serves on `6333`, gRPC on `6334`; this backend requires the REST
//! base URL (for example `http://127.0.0.1:6333`). Server versions with the
//! same major and at most one minor apart are accepted; drift only warns.
//!
//! Point ids are arbitrary strings. Qdrant native ids only hold integers
//! or UUIDs, so each contracts id maps to one deterministic native UUID
//! (UUID v5 over the `pid` string) and the contracts id additionally
//! travels in the `pid` payload field. Upserts overwrite in one request
//! with no delete-before-insert window. Old rows written with random UUIDs
//! are not migrated; rewrite the source to converge.
//!
//! Writes and deletes request `wait=true` by default so a write followed
//! by a read observes the write. Counting is exact by default; large
//! collections may disable it. Payload indexes on `namespace`, `source`
//! and `pid` are created idempotently after ensuring the collection.
//! Only idempotent reads retry on rate limiting and transient server
//! errors; writes never retry implicitly.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::json;

use crate::error::{diagnostic_body, Result, VectorError};
use crate::filter::{FieldFilter, FilterOperator, SearchFilter};
use crate::memory::InMemoryVectorStore;
use crate::store::{CollectionSpec, ScrollPage, ScrollQuery, VectorQuery, VectorStorage};
use crate::types::{
    DistanceMetric, ScoredPoint, VectorData, VectorKind, VectorPoint, VectorSpaceSpec,
    NAMESPACE_KEY, PID_KEY, SOURCE_KEY,
};

/// Payload index fields ensured after every collection creation.
const INDEXED_FIELDS: [&str; 3] = [NAMESPACE_KEY, SOURCE_KEY, PID_KEY];
/// Upper bound for automatic read retries on rate limiting and transient faults.
const MAX_READ_RETRIES: usize = 3;

mod codec;
mod collection;
mod exact;
mod filter;
mod operations;
mod store;
#[cfg(test)]
mod test;
mod transport;

/// Connection parameters of the remote Qdrant backend.
#[derive(Debug, Clone)]
pub struct QdrantConfig {
    /// Base URL of the REST API, for example `http://127.0.0.1:6333`.
    /// Must use the REST port (`6333`); the gRPC port (`6334`) is rejected
    /// with a readable error.
    pub base_url: String,
    /// Optional API key sent as the `api-key` header.
    pub api_key: Option<String>,
    /// Per-request deadline in seconds.
    pub timeout_secs: u64,
    /// Optional TCP connect deadline in seconds; unset inherits the request deadline.
    pub connect_timeout_secs: Option<u64>,
    /// Extra headers sent with every request.
    pub headers: Vec<(String, String)>,
    /// Points per upsert and delete batch.
    pub batch_size: usize,
    /// Request server-side durability before acknowledging writes.
    pub wait: bool,
    /// Request exact counting; disable for cheaper counts on large collections.
    pub exact_count: bool,
}

impl QdrantConfig {
    /// Builds a config with a ten-second per-request deadline.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: None,
            timeout_secs: 10,
            connect_timeout_secs: None,
            headers: Vec::new(),
            batch_size: 256,
            wait: true,
            exact_count: true,
        }
    }

    /// Attaches an API key.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Overrides the per-request deadline in seconds.
    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs.max(1);
        self
    }

    /// Overrides the TCP connect deadline in seconds.
    pub fn with_connect_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.connect_timeout_secs = Some(timeout_secs.max(1));
        self
    }

    /// Adds a custom header sent with every request.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    /// Overrides the batch size for upserts and deletes.
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    /// Toggles server-side wait on writes and deletes.
    pub fn with_wait(mut self, wait: bool) -> Self {
        self.wait = wait;
        self
    }

    /// Toggles exact counting.
    pub fn with_exact_count(mut self, exact: bool) -> Self {
        self.exact_count = exact;
        self
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CachedCollection {
    dimension: usize,
    distance: DistanceMetric,
    kind: VectorKind,
    vector_name: Option<String>,
    spaces: Option<BTreeMap<String, VectorSpaceSpec>>,
}

/// Remote Qdrant backend.
#[derive(Debug, Clone)]
pub struct QdrantVectorStore {
    client: reqwest::Client,
    config: QdrantConfig,
    collections: Arc<parking_lot::RwLock<HashMap<String, CachedCollection>>>,
}

impl QdrantVectorStore {
    /// Builds the backend; no request is issued until first use.
    pub fn new(config: QdrantConfig) -> Result<Self> {
        if config.base_url.trim().is_empty() {
            return Err(VectorError::InvalidInput(
                "qdrant base_url must not be empty (REST, e.g. http://127.0.0.1:6333)".to_string(),
            ));
        }
        if config.base_url.contains(":6334") {
            return Err(VectorError::InvalidInput(
                "qdrant base_url uses the gRPC port 6334; use the REST port 6333".to_string(),
            ));
        }
        let mut builder =
            reqwest::Client::builder().timeout(Duration::from_secs(config.timeout_secs.max(1)));
        if let Some(connect) = config.connect_timeout_secs {
            builder = builder.connect_timeout(Duration::from_secs(connect.max(1)));
        }
        let client = builder
            .build()
            .map_err(|err| VectorError::Transport(err.to_string()))?;
        Ok(Self {
            client,
            config,
            collections: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        })
    }

    /// Returns the effective config (used by diagnostics and tests).
    pub fn config(&self) -> &QdrantConfig {
        &self.config
    }
}
